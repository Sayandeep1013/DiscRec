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

/// Samples of microphone audio to keep buffered, per channel. Large enough to
/// absorb scheduling jitter, small enough that the added latency is invisible.
const TARGET_FILL_FRAMES: usize = 2400; // 50 ms at 48 kHz

/// How hard the controller pulls the resample ratio toward the target fill.
/// Deliberately weak: it must correct drift over seconds, not chase jitter.
/// Too high and the microphone audibly wobbles in pitch.
const CONTROLLER_GAIN: f64 = 0.05;

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
const FILL_SMOOTHING: f64 = 0.002;

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
        self.steer_ratio();

        let frames = discord.len() / self.channels;
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
                if channel == 0 {
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
    fn steer_ratio(&mut self) {
        let fill = self.buffered_frames() as f64;
        let target = TARGET_FILL_FRAMES as f64;

        // Smooth before steering. See FILL_SMOOTHING — acting on the raw depth
        // makes the controller chase burstiness rather than drift.
        self.smoothed_fill += (fill - self.smoothed_fill) * FILL_SMOOTHING;

        let error = (self.smoothed_fill - target) / target;

        // Proportional only. An integral term would eventually null the error
        // but also wind up during the startup transient, and the steady-state
        // offset here is far below audibility.
        self.ratio = (1.0 + CONTROLLER_GAIN * error)
            .clamp(1.0 - MAX_RATIO_DEVIATION, 1.0 + MAX_RATIO_DEVIATION);
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
        assert_eq!(out.len(), 960);
        assert!(
            m.mic_underruns > 0,
            "underrun should be counted, not hidden"
        );
        // Remote audio must still be present at full level.
        assert!((out[0] - 0.3).abs() < 1e-6);
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
