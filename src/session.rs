//! Live capture session: preview meters, then mix/write on Record.
//!
//! Both the window and the CLI drive this. Discord is the timeline master;
//! the microphone is converted to the mix format here so the mixer never sees
//! a channel or rate mismatch.

use crate::capture::{self, CaptureError, Frame, Source, StreamFormat};
use crate::mixer::Mixer;
use crate::writer::OpusWriter;

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Receiver, RecvTimeoutError, Sender};
use std::sync::Arc;
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const SIGNAL_FLOOR: f32 = 1.0e-6;
const MIX_RATE: u32 = 48_000;

pub fn is_dead_air(peak: f32, packets: u64) -> bool {
    packets == 0 || peak < SIGNAL_FLOOR
}

/// Convert a capture block to interleaved stereo at 48 kHz.
pub fn to_mix_format(samples: &[f32], channels: u16, sample_rate: u32) -> Vec<f32> {
    let ch = channels.max(1) as usize;
    let frames_in = samples.len() / ch;
    if frames_in == 0 {
        return Vec::new();
    }

    let stereo: Vec<f32> = match ch {
        1 => samples.iter().flat_map(|&s| [s, s]).collect(),
        _ => {
            let mut out = Vec::with_capacity(frames_in * 2);
            for f in 0..frames_in {
                out.push(samples[f * ch]);
                out.push(samples[f * ch + 1]);
            }
            out
        }
    };

    if sample_rate == 0 || sample_rate == MIX_RATE {
        return stereo;
    }

    let frames_out = ((frames_in as u64 * MIX_RATE as u64) / sample_rate as u64) as usize;
    let frames_out = frames_out.max(1);
    let mut out = Vec::with_capacity(frames_out * 2);
    for i in 0..frames_out {
        let src = i as f64 * (frames_in as f64 - 1.0) / (frames_out as f64 - 1.0).max(1.0);
        let base = src.floor() as usize;
        let frac = (src - src.floor()) as f32;
        let n = (base + 1).min(frames_in - 1);
        for c in 0..2 {
            let a = stereo[base * 2 + c];
            let b = stereo[n * 2 + c];
            out.push(a + (b - a) * frac);
        }
    }
    out
}

#[derive(Default)]
pub struct Meters {
    discord: AtomicU32,
    mic: AtomicU32,
    elapsed_ms: AtomicU64,
}

impl Meters {
    pub fn discord(&self) -> f32 {
        f32::from_bits(self.discord.load(Ordering::Relaxed))
    }
    pub fn mic(&self) -> f32 {
        f32::from_bits(self.mic.load(Ordering::Relaxed))
    }
    pub fn elapsed_ms(&self) -> u64 {
        self.elapsed_ms.load(Ordering::Relaxed)
    }
    fn set_discord(&self, v: f32) {
        self.discord.store(v.to_bits(), Ordering::Relaxed);
    }
    fn set_mic(&self, v: f32) {
        self.mic.store(v.to_bits(), Ordering::Relaxed);
    }
    fn set_elapsed_ms(&self, v: u64) {
        self.elapsed_ms.store(v, Ordering::Relaxed);
    }
}

pub enum Command {
    Begin { path: PathBuf },
    End,
    Shutdown,
}

#[derive(Debug)]
pub enum Event {
    Previewing,
    Recording,
    Saved { path: PathBuf },
    Failed(String),
}

pub struct Engine {
    pub meters: Arc<Meters>,
    cmd: Sender<Command>,
    events: Receiver<Event>,
    thread: Option<JoinHandle<()>>,
}

impl Engine {
    pub fn start(pid: u32) -> Self {
        let (cmd_tx, cmd_rx) = mpsc::channel();
        let (ev_tx, ev_rx) = mpsc::channel();
        let meters = Arc::new(Meters::default());
        let meters_thread = Arc::clone(&meters);

        let thread = std::thread::spawn(move || {
            engine_thread(pid, cmd_rx, ev_tx, meters_thread);
        });

        Self {
            meters,
            cmd: cmd_tx,
            events: ev_rx,
            thread: Some(thread),
        }
    }

    pub fn begin_recording(&self, path: PathBuf) {
        let _ = self.cmd.send(Command::Begin { path });
    }

    pub fn end_recording(&self) {
        let _ = self.cmd.send(Command::End);
    }

    pub fn poll_event(&self) -> Option<Event> {
        self.events.try_recv().ok()
    }

    pub fn shutdown(&mut self) {
        let _ = self.cmd.send(Command::Shutdown);
        if let Some(t) = self.thread.take() {
            let _ = t.join();
        }
    }
}

impl Drop for Engine {
    fn drop(&mut self) {
        self.shutdown();
    }
}

struct WriteJob {
    writer: OpusWriter,
    path: PathBuf,
    mixer: Mixer,
    started: Instant,
}

fn engine_thread(pid: u32, cmd: Receiver<Command>, ev: Sender<Event>, meters: Arc<Meters>) {
    let mut backend = capture::backend();
    let (tx, rx) = mpsc::channel();
    if let Err(e) = backend.start(pid, tx) {
        let _ = ev.send(Event::Failed(e.to_string()));
        return;
    }
    let fmt = backend.format();
    let _ = ev.send(Event::Previewing);

    let mut job: Option<WriteJob> = None;
    let mut running = true;

    while running {
        match rx.recv_timeout(Duration::from_millis(50)) {
            Ok(frame) => {
                let peak = frame_peak(&frame.samples);
                match frame.source {
                    Source::DiscordOutput => meters.set_discord(peak),
                    Source::Microphone => meters.set_mic(peak),
                }
                if let Some(j) = job.as_mut() {
                    if let Err(e) = feed_writer(j, &frame, fmt) {
                        let _ = ev.send(Event::Failed(e));
                        discard_job(job.take());
                    } else {
                        meters.set_elapsed_ms(j.started.elapsed().as_millis() as u64);
                    }
                }
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => break,
        }

        while let Ok(c) = cmd.try_recv() {
            match c {
                Command::Begin { path } => {
                    discard_job(job.take());
                    match OpusWriter::create(&path, fmt.channels) {
                        Ok(writer) => {
                            job = Some(WriteJob {
                                writer,
                                path,
                                mixer: Mixer::new(fmt.channels),
                                started: Instant::now(),
                            });
                            meters.set_elapsed_ms(0);
                            let _ = ev.send(Event::Recording);
                        }
                        Err(e) => {
                            let _ = ev.send(Event::Failed(format!("Could not create file: {e}")));
                        }
                    }
                }
                Command::End => {
                    if let Some(j) = job.take() {
                        let path = finish_job(j, &ev);
                        meters.set_elapsed_ms(0);
                        if let Some(path) = path {
                            let _ = ev.send(Event::Saved { path });
                        }
                    }
                }
                Command::Shutdown => {
                    running = false;
                    if let Some(j) = job.take() {
                        let _ = finish_job(j, &ev);
                    }
                }
            }
        }
    }

    let _ = backend.stop();
}

fn feed_writer(job: &mut WriteJob, frame: &Frame, fmt: StreamFormat) -> Result<(), String> {
    let converted = to_mix_format(&frame.samples, frame.channels, frame.sample_rate);
    match frame.source {
        Source::Microphone => {
            job.mixer.push_mic(&converted);
            Ok(())
        }
        Source::DiscordOutput => {
            let mixed = job.mixer.mix(&converted_or_pad(&converted, fmt));
            job.writer
                .write(&mixed)
                .map_err(|e| format!("Write failed: {e}"))
        }
    }
}

fn converted_or_pad(samples: &[f32], fmt: StreamFormat) -> Vec<f32> {
    if samples.is_empty() {
        vec![0.0; fmt.channels as usize]
    } else {
        samples.to_vec()
    }
}

fn finish_job(job: WriteJob, ev: &Sender<Event>) -> Option<PathBuf> {
    let path = job.path.clone();
    if let Err(e) = job.writer.finalize() {
        let _ = ev.send(Event::Failed(format!("Could not finalize: {e}")));
        let _ = std::fs::remove_file(&path);
        return None;
    }
    Some(path)
}

fn discard_job(job: Option<WriteJob>) {
    if let Some(job) = job {
        let path = job.path.clone();
        drop(job.writer);
        let _ = std::fs::remove_file(path);
    }
}

fn frame_peak(samples: &[f32]) -> f32 {
    samples.iter().fold(0.0f32, |m, s| m.max(s.abs()))
}

#[derive(Debug)]
pub struct MixReport {
    pub discord_peak: f32,
    pub mic_peak: f32,
    pub frames_out: u64,
    pub drift_ppm: f64,
    pub mic_underruns: u64,
    pub mic_overruns: u64,
    pub clamp_hits: u64,
    pub limited: u64,
}

/// Blocking mixed capture for the CLI harness and soak / crash tests.
pub fn record_mixed(
    pid: u32,
    duration: Duration,
    output: &Path,
    log: Option<&Path>,
) -> Result<MixReport, CaptureError> {
    let mut backend = capture::backend();
    let (tx, rx) = mpsc::channel();
    backend.start(pid, tx)?;
    let fmt = backend.format();

    let mut mixer = Mixer::new(fmt.channels);
    let mut writer = OpusWriter::create(output, fmt.channels).map_err(|e| {
        let _ = backend.stop();
        CaptureError::Platform(format!("create {output:?}: {e}"))
    })?;

    let mut telemetry = open_telemetry(log);
    let mut next_sample = Duration::from_secs(30);
    let mut discord_peak = 0.0f32;
    let mut mic_peak = 0.0f32;
    let started = Instant::now();
    let mut failed: Option<CaptureError> = None;

    while started.elapsed() < duration {
        if let Ok(frame) = rx.recv_timeout(Duration::from_millis(250)) {
            let peak = frame_peak(&frame.samples);
            let converted = to_mix_format(&frame.samples, frame.channels, frame.sample_rate);
            match frame.source {
                Source::Microphone => {
                    mic_peak = mic_peak.max(peak);
                    mixer.push_mic(&converted);
                }
                Source::DiscordOutput => {
                    discord_peak = discord_peak.max(peak);
                    let mixed = mixer.mix(&converted);
                    if let Err(e) = writer.write(&mixed) {
                        failed = Some(CaptureError::Platform(format!("write: {e}")));
                        break;
                    }
                }
            }

            if let Some(f) = telemetry.as_mut() {
                let elapsed = started.elapsed();
                if elapsed >= next_sample {
                    use std::io::Write;
                    let _ = writeln!(
                        f,
                        "{:.0},{:.1},{:.0},{},{:.2},{},{},{},{},{}",
                        elapsed.as_secs_f64(),
                        mixer.drift_ppm(),
                        mixer.smoothed_frames(),
                        mixer.buffered_frames(),
                        mixer.integral_frames(),
                        mixer.mic_underruns,
                        mixer.mic_overruns,
                        mixer.clamp_hits,
                        mixer.frames_out,
                        mixer.limited
                    );
                    let _ = f.flush();
                    next_sample += Duration::from_secs(30);
                }
            }
        }
    }

    let _ = backend.stop();

    if let Some(err) = failed {
        drop(writer);
        let _ = std::fs::remove_file(output);
        return Err(err);
    }

    writer
        .finalize()
        .map_err(|e| CaptureError::Platform(format!("finalize: {e}")))?;

    Ok(MixReport {
        discord_peak,
        mic_peak,
        frames_out: mixer.frames_out,
        drift_ppm: mixer.drift_ppm(),
        mic_underruns: mixer.mic_underruns,
        mic_overruns: mixer.mic_overruns,
        clamp_hits: mixer.clamp_hits,
        limited: mixer.limited,
    })
}

fn open_telemetry(log: Option<&Path>) -> Option<std::fs::File> {
    let path = log?;
    let mut f = std::fs::File::create(path).ok()?;
    use std::io::Write;
    let _ = writeln!(
        f,
        "elapsed_s,drift_ppm,smoothed_frames,raw_frames,integral,mic_underruns,mic_overruns,clamp_hits,frames_out,limited"
    );
    Some(f)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn silence_is_dead_air() {
        assert!(is_dead_air(0.0, 100));
        assert!(is_dead_air(0.5, 0));
        assert!(!is_dead_air(0.01, 3));
    }

    #[test]
    fn mono_upmixes_to_stereo() {
        let out = to_mix_format(&[0.25, 0.5], 1, 48_000);
        assert_eq!(out, vec![0.25, 0.25, 0.5, 0.5]);
    }

    #[test]
    fn stereo_passthrough() {
        let out = to_mix_format(&[0.1, 0.2, 0.3, 0.4], 2, 48_000);
        assert_eq!(out, vec![0.1, 0.2, 0.3, 0.4]);
    }

    #[test]
    fn extra_channels_keep_first_two() {
        let out = to_mix_format(&[1.0, 2.0, 9.0, 3.0, 4.0, 9.0], 3, 48_000);
        assert_eq!(out, vec![1.0, 2.0, 3.0, 4.0]);
    }
}
