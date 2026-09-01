//! Combining the two capture streams into one aligned signal.
//!
//! See `docs/spec/mixing-and-timeline.md`. The hard part is not the summing —
//! it is that the two streams come from devices with independent hardware
//! clocks. Measured on this machine, they disagree by about 240 ppm, roughly
//! 860 ms per hour. Left uncorrected the microphone slides against the remote
//! voices, and because the mix is written at capture time the error is
//! permanent (P1).
//!
//! Discord is the timeline master. The microphone is continuously resampled to
//! match it, with the ratio steered by how full the microphone buffer is —
//! buffer level is a direct proxy for accumulated clock error.

use std::collections::VecDeque;

/// Microphone frames to keep buffered.
///
/// 100 ms. Latency is irrelevant here — this is a recorder, not a live
/// monitor, and nobody hears the buffer. What the depth actually buys is
/// headroom: the controller settles at an offset below target proportional to
/// the drift it is correcting (see `CONTROLLER_GAIN`), and if that offset ever
/// exceeds the target the buffer runs dry. At this depth the design tolerates
/// about 3300 ppm of drift before underrunning, against ~240 ppm measured.
const TARGET_FILL_FRAMES: usize = 4800;

/// Loop time constant, in seconds. Everything else about the controller is
/// derived from this.
const LOOP_TAU_SECS: f64 = 30.0;

/// How hard the controller pulls the resample ratio toward the target depth.
///
/// **Derived, not tuned by feel.** Buffer depth is the integral of rate error,
/// so the closed loop is first-order:
///
/// ```text
///   dB/dt  = r_mic - 48000 * ratio
///   ratio  = 1 + K * (B_smoothed - Bt) / Bt
///   =>  de/dt = d - (48000*K/Bt) * e        with e = B - Bt
/// ```
///
/// giving rate constant `a = 48000*K/Bt`, i.e. `tau_loop = Bt/(48000*K)`.
/// Solving for the time constant above yields the value below.
///
/// The first version used K = 0.05, which is `tau_loop = 1 s` — *five times
/// faster* than the 5 s smoothing filter sitting inside the same loop. A lag
/// that dominates the loop it is part of oscillates, and it did: a 30-minute
/// soak saw the ratio hunt between -6493 and +10000 ppm (the clamp), buffer
/// depth swing from 0 to 6378 frames, and 35,628 underruns accumulate.
///
/// The loop must be slower than its own sensor. This is six times slower.
const CONTROLLER_GAIN: f64 = TARGET_FILL_FRAMES as f64 / (48_000.0 * LOOP_TAU_SECS);

/// Smoothing applied to the measured buffer depth before the controller sees
/// it.
///
/// This is not optional. Frames from the two streams arrive interleaved and in
/// bursts, so the *instantaneous* buffer depth swings by hundreds of frames
/// between calls. Steering on that directly made the ratio wander across a
/// 10,000 ppm range in testing — against a real hardware drift of ~240 ppm —
/// which is pitch warble, not correction.
///
/// `mix` is called roughly every 10 ms, so this gives a time constant of about
/// five seconds: slow enough to ignore burstiness, fast enough to track a
/// clock that is genuinely running away.
///
/// Note the constraint this places on `CONTROLLER_GAIN`: a filter inside a
/// feedback loop must be *faster* than the loop, or the loop oscillates. This
/// filter is fixed at 5 s, so the loop is set to 30 s.
const FILL_SMOOTHING: f64 = 0.002;

/// Integral time, in seconds.
///
/// A proportional-only controller settles with a standing buffer offset rather
/// than at target, because it needs a persistent error to hold a persistent
/// correction. That offset is harmless in itself, but it also means the loop
/// never fully nulls a slowly-changing drift — and crystal oscillators do
/// wander with temperature over minutes.
///
/// Set well above the loop time constant so the integral acts as slow trim
/// rather than a second thing for the loop to fight.
const INTEGRAL_TAU_SECS: f64 = 150.0;

/// Ratio is clamped to this much either side of 1.0. Real drift is measured in
/// hundreds of ppm; anything approaching a percent means something else is
/// wrong, and silently resampling that far would hide the fault.
const MAX_RATIO_DEVIATION: f64 = 0.01;

/// Above this amplitude the sum is progressively softened rather than clipped.
const LIMITER_THRESHOLD: f32 = 0.85;

pub struct Mixer {
    channels: usize,
    /// Interleaved microphone samples awaiting mixing.
    mic: VecDeque<f32>,
    /// Fractional read position within `mic`, in frames.
    read_pos: f64,
    /// Microphone frames consumed per output frame. 1.0 means the clocks agree.
    ratio: f64,
    /// Smoothed buffer depth, in frames. Seeded at the target so the startup
    /// transient does not slam the controller to a limit.
    smoothed_fill: f64,
    /// False until the buffer first reaches its target depth. Underruns before
    /// that are the startup transient, not a fault, and counting them makes
    /// the metric useless for spotting a real one.
    primed: bool,
    /// Integrated buffer error, in frame-seconds. Nulls the standing offset a
    /// proportional-only controller leaves behind.
    integral: f64,
    /// Times the ratio hit its limit. Non-zero means the controller is
    /// saturating, which is a fault: real drift never needs that much
    /// correction.
    pub clamp_hits: u64,
    /// Frames of microphone audio dropped because the buffer overran.
    pub mic_overruns: u64,
    /// Output frames produced with no microphone audio available.
    pub mic_underruns: u64,
    /// Output frames written.
    pub frames_out: u64,
    /// Output frames the limiter acted on.
    pub limited: u64,
}

impl Mixer {
    pub fn new(channels: u16) -> Self {
        Self {
            channels: channels.max(1) as usize,
            mic: VecDeque::with_capacity(TARGET_FILL_FRAMES * 4 * channels.max(1) as usize),
            read_pos: 0.0,
            ratio: 1.0,
            smoothed_fill: TARGET_FILL_FRAMES as f64,
            primed: false,
            integral: 0.0,
            clamp_hits: 0,
            mic_overruns: 0,
            mic_underruns: 0,
            frames_out: 0,
            limited: 0,
        }
    }

    /// Current microphone resample ratio, expressed as parts per million away
    /// from 1.0. This is the measured clock disagreement, live.
    pub fn drift_ppm(&self) -> f64 {
        (self.ratio - 1.0) * 1.0e6
    }

    pub fn buffered_frames(&self) -> usize {
        self.mic.len() / self.channels
    }

    /// The depth the controller actually steers on.
    ///
    /// Prefer this for telemetry. The raw depth jumps by a whole packet
    /// (~480 frames) depending on whether it is read just before or just after
    /// a microphone push, and sampling that on a fixed interval produces two
    /// alternating branches that look like a trend but are only phase.
    pub fn smoothed_frames(&self) -> f64 {
        self.smoothed_fill
    }

    /// Accumulated integral term, in frames. Exposed so a soak can see whether
    /// it is settling or winding toward its limit.
    pub fn integral_frames(&self) -> f64 {
        self.integral
    }

    /// Accept microphone audio. Interleaved, same channel count as the mix.
    pub fn push_mic(&mut self, samples: &[f32]) {
        self.mic.extend(samples.iter().copied());

        // Guard against unbounded growth if the Discord side stalls. Dropping
        // is preferable to an ever-growing delay, but it is a real fault and
        // is counted rather than hidden.
        let cap = TARGET_FILL_FRAMES * 8;
        while self.buffered_frames() > cap {
            for _ in 0..self.channels {
                self.mic.pop_front();
            }
            self.read_pos = (self.read_pos - 1.0).max(0.0);
            self.mic_overruns += 1;
        }
    }

    /// Mix a block of Discord audio with the buffered microphone audio.
    ///
    /// Discord defines the output timeline: `discord.len()` samples in,
    /// `discord.len()` samples out, always. The microphone is stretched or
    /// compressed to fit.
    pub fn mix(&mut self, discord: &[f32]) -> Vec<f32> {
        let frames = discord.len() / self.channels;
        self.steer_ratio(frames as f64 / 48_000.0);

        let mut out = Vec::with_capacity(discord.len());

        for f in 0..frames {
            for c in 0..self.channels {
                let remote = discord[f * self.channels + c];
                let local = self.sample_mic(c);
                out.push(self.limit(remote + local));
            }
            self.read_pos += self.ratio;
        }

        self.consume_read();
        self.frames_out += frames as u64;
        out
    }

    /// Linear interpolation between the two microphone frames straddling
    /// `read_pos`. Cheap, and at these ratios the error is far below the noise
    /// floor of the microphone itself.
    fn sample_mic(&mut self, channel: usize) -> f32 {
        let base = self.read_pos.floor() as usize;
        let frac = (self.read_pos - self.read_pos.floor()) as f32;

        let a = self.mic.get(base * self.channels + channel).copied();
        let b = self.mic.get((base + 1) * self.channels + channel).copied();

        match (a, b) {
            (Some(a), Some(b)) => a + (b - a) * frac,
            (Some(a), None) => a,
            _ => {
                if channel == 0 && self.primed {
                    self.mic_underruns += 1;
                }
                0.0
            }
        }
    }

    /// Drop microphone frames that have been fully consumed.
    fn consume_read(&mut self) {
        let consumed = self.read_pos.floor() as usize;
        if consumed == 0 {
            return;
        }
        let drain = (consumed * self.channels).min(self.mic.len());
        self.mic.drain(..drain);
        self.read_pos -= consumed as f64;
    }

    /// Nudge the resample ratio so the microphone buffer stays near its target
    /// depth. A buffer that is filling means the microphone clock is running
    /// fast relative to Discord's, and vice versa.
    fn steer_ratio(&mut self, dt: f64) {
        let fill = self.buffered_frames() as f64;
        let target = TARGET_FILL_FRAMES as f64;

        if !self.primed && fill >= target {
            self.primed = true;
        }

        // Smooth before steering. See FILL_SMOOTHING — acting on the raw depth
        // makes the controller chase burstiness rather than drift.
        self.smoothed_fill += (fill - self.smoothed_fill) * FILL_SMOOTHING;

        let error = (self.smoothed_fill - target) / target;

        // Integrate only once primed, so the startup transient - when the
        // buffer is legitimately empty - cannot wind the term up.
        if self.primed {
            self.integral += error * dt;
            // Anti-windup: cap the integral's authority at the same magnitude
            // as a full-scale proportional term. Without this a long stall
            // banks correction it then has to unwind, overshooting badly.
            let limit = INTEGRAL_TAU_SECS;
            self.integral = self.integral.clamp(-limit, limit);
        }

        let wanted = 1.0 + CONTROLLER_GAIN * (error + self.integral / INTEGRAL_TAU_SECS);
        self.ratio = wanted.clamp(1.0 - MAX_RATIO_DEVIATION, 1.0 + MAX_RATIO_DEVIATION);
        if (wanted - self.ratio).abs() > f64::EPSILON {
            self.clamp_hits += 1;
        }
    }

    /// Soft-knee limiter. Two voices summed routinely exceed full scale, and
    /// hard clipping is far more audible than a little compression.
    fn limit(&mut self, x: f32) -> f32 {
        let mag = x.abs();
        if mag <= LIMITER_THRESHOLD {
            return x;
        }
        self.limited += 1;
        let over = (mag - LIMITER_THRESHOLD) / (1.0 - LIMITER_THRESHOLD);
        x.signum() * (LIMITER_THRESHOLD + (1.0 - LIMITER_THRESHOLD) * over.tanh())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn block(n: usize, v: f32) -> Vec<f32> {
        vec![v; n * 2]
    }

    #[test]
    fn output_length_always_matches_discord() {
        let mut m = Mixer::new(2);
        m.push_mic(&block(1000, 0.1));
        for _ in 0..10 {
            let out = m.mix(&block(480, 0.2));
            assert_eq!(out.len(), 960, "Discord defines the output timeline");
        }
    }

    #[test]
    fn missing_mic_audio_does_not_stall_the_mix() {
        let mut m = Mixer::new(2);
        let out = m.mix(&block(480, 0.3));

        assert_eq!(out.len(), 960, "output must not stall on a starved mic");
        // Remote audio must still be present at full level.
        assert!((out[0] - 0.3).abs() < 1e-6);
    }

    #[test]
    fn startup_starvation_is_not_reported_as_a_fault() {
        // Before the buffer has ever filled, a dry microphone is the startup
        // transient. Counting it makes the metric useless for spotting a real
        // fault later.
        let mut m = Mixer::new(2);
        for _ in 0..50 {
            m.mix(&block(480, 0.3));
        }
        assert_eq!(m.mic_underruns, 0, "startup should not count as underrun");
    }

    #[test]
    fn starvation_after_priming_is_reported() {
        let mut m = Mixer::new(2);

        // Prime: fill past target and let the controller observe it.
        m.push_mic(&block(TARGET_FILL_FRAMES + 480, 0.1));
        m.mix(&block(480, 0.1));

        // Now starve it, with no further microphone audio arriving.
        for _ in 0..200 {
            m.mix(&block(480, 0.1));
        }

        assert!(
            m.mic_underruns > 0,
            "a genuine underrun after priming must be counted, not hidden"
        );
    }

    #[test]
    fn both_sources_are_summed() {
        let mut m = Mixer::new(2);
        m.push_mic(&block(5000, 0.25));
        let out = m.mix(&block(480, 0.25));
        assert!(
            out[0] > 0.4,
            "expected both sides in the sum, got {}",
            out[0]
        );
    }

    #[test]
    fn limiter_prevents_full_scale_clipping() {
        let mut m = Mixer::new(2);
        m.push_mic(&block(5000, 0.9));
        let out = m.mix(&block(480, 0.9));
        assert!(
            out.iter().all(|s| s.abs() < 1.0),
            "limiter must hold below full scale"
        );
        assert!(m.limited > 0);
    }

    #[test]
    fn a_sustained_backlog_speeds_the_microphone_up() {
        let mut m = Mixer::new(2);
        // Keep the buffer persistently deep: the microphone clock is fast.
        for _ in 0..2000 {
            m.push_mic(&block(600, 0.1));
            m.mix(&block(480, 0.1));
        }
        assert!(
            m.drift_ppm() > 0.0,
            "ratio should rise to drain a sustained backlog, got {} ppm",
            m.drift_ppm()
        );
    }

    #[test]
    fn a_single_burst_does_not_move_the_ratio() {
        // The bug this guards against: steering on instantaneous buffer depth
        // made the ratio wander across ~10,000 ppm against a real drift of
        // ~240 ppm, which is pitch warble rather than correction.
        let mut m = Mixer::new(2);
        m.push_mic(&block(TARGET_FILL_FRAMES * 4, 0.1));
        m.mix(&block(480, 0.1));
        assert!(
            m.drift_ppm().abs() < 500.0,
            "one burst should barely move the ratio, got {} ppm",
            m.drift_ppm()
        );
    }

    /// The stability condition, asserted directly rather than left as a
    /// comment. A filter inside a feedback loop must be faster than the loop;
    /// violating this is what made the 30-minute soak oscillate into the
    /// clamps with 35,628 underruns.
    #[test]
    fn control_loop_is_slower_than_its_own_filter() {
        // mix() runs about every 10 ms, so the EMA's time constant in seconds
        // is one sample period divided by alpha.
        let filter_tau = 0.010 / FILL_SMOOTHING;
        let loop_tau = TARGET_FILL_FRAMES as f64 / (48_000.0 * CONTROLLER_GAIN);

        assert!(
            loop_tau >= filter_tau * 3.0,
            "loop tau {loop_tau:.1}s must be well above filter tau {filter_tau:.1}s, \
             or the loop oscillates"
        );
    }

    /// The buffer must be deep enough that the controller's steady-state
    /// offset cannot drain it. Offset is drift x loop time constant.
    #[test]
    fn buffer_tolerates_far_more_drift_than_hardware_produces() {
        let loop_tau = TARGET_FILL_FRAMES as f64 / (48_000.0 * CONTROLLER_GAIN);
        // Frames per second of error the buffer can absorb before running dry.
        let max_drift_fps = TARGET_FILL_FRAMES as f64 / loop_tau;
        let max_drift_ppm = max_drift_fps / 48_000.0 * 1.0e6;

        assert!(
            max_drift_ppm > 2000.0,
            "only tolerates {max_drift_ppm:.0} ppm; measured hardware drift is ~240 ppm \
             and headroom this thin will underrun"
        );
    }

    /// The reason the integral term exists: proportional-only control settles
    /// at a standing offset, not at target. Simulate a microphone clock running
    /// 240 ppm fast — the rate measured on real hardware — and require the
    /// buffer to come back to target rather than parking below it.
    #[test]
    fn integral_returns_the_buffer_to_target_under_constant_drift() {
        let mut m = Mixer::new(2);
        m.push_mic(&block(TARGET_FILL_FRAMES + 960, 0.1));

        // 10 ms per iteration; 60_000 iterations is ~10 minutes of audio, or
        // four integral time constants.
        let mut owed = 0.0f64;
        for _ in 0..60_000 {
            owed += 480.0 * (1.0 + 240.0e-6);
            let whole = owed.floor() as usize;
            owed -= whole as f64;
            m.push_mic(&block(whole, 0.1));
            m.mix(&block(480, 0.1));
        }

        let target = TARGET_FILL_FRAMES as f64;
        let offset = (m.smoothed_frames() - target).abs();
        assert!(
            offset < target * 0.10,
            "buffer parked {offset:.0} frames from target ({target:.0}); \
             the integral term is not nulling the standing offset"
        );
        assert_eq!(m.clamp_hits, 0, "controller should not saturate at 240 ppm");
        assert_eq!(m.mic_underruns, 0, "buffer should never run dry at 240 ppm");
    }

    #[test]
    fn ratio_never_leaves_sane_bounds() {
        let mut m = Mixer::new(2);
        for _ in 0..5000 {
            m.push_mic(&block(2000, 0.1));
            m.mix(&block(480, 0.1));
        }
        assert!(m.ratio <= 1.0 + MAX_RATIO_DEVIATION);
        assert!(m.ratio >= 1.0 - MAX_RATIO_DEVIATION);
    }
}
