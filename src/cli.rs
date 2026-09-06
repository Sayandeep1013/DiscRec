//! Development / diagnostic harness. The window is the product; this remains
//! so soaks, crash tests, and capture spikes keep a stable command line.
//!
//!   discrec [secs]              Discord only, WAV
//!   discrec [secs] --pid N      arbitrary process
//!   discrec [secs] --both       both streams, stats only
//!   discrec [secs] --mix        mixed Ogg/Opus (the real path)
//!   discrec [secs] --mix --log  plus soak.csv every 30s
//!   discrec --devices           list render endpoints

use crate::capture::{self, CaptureError, Source};
use crate::discord;
use crate::session;
use std::collections::HashMap;
use std::path::Path;
use std::sync::mpsc::channel;
use std::time::{Duration, Instant};

pub fn run(args: Vec<String>) {
    if args.iter().any(|a| a == "--help" || a == "-h") {
        print_help();
        return;
    }

    println!("DiscRec {}\n", env!("CARGO_PKG_VERSION"));

    let seconds: u64 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(12);

    let pid_override = args
        .iter()
        .position(|a| a == "--pid")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<u32>().ok());

    if args.iter().any(|a| a == "--both") {
        run_both_streams(seconds, pid_override);
        return;
    }

    if args.iter().any(|a| a == "--mix") {
        run_mixed(seconds, pid_override, args.iter().any(|a| a == "--log"));
        return;
    }

    #[cfg(windows)]
    if args.iter().any(|a| a == "--devices") {
        match capture::windows::list_render_devices() {
            Ok(devs) => {
                println!("Active output endpoints:\n");
                for d in &devs {
                    println!(
                        "  [{}] {}{}",
                        d.index,
                        d.name,
                        if d.is_default {
                            "   (WINDOWS DEFAULT)"
                        } else {
                            ""
                        }
                    );
                }
                println!("\nCapture one with:  discrec <secs> --device <n>");
            }
            Err(e) => eprintln!("Could not list endpoints: {e}"),
        }
        return;
    }

    #[cfg(windows)]
    if let Some(idx) = args
        .iter()
        .position(|a| a == "--device")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok())
    {
        let name = capture::windows::list_render_devices()
            .ok()
            .and_then(|d| d.into_iter().find(|d| d.index == idx).map(|d| d.name))
            .unwrap_or_else(|| format!("endpoint {idx}"));
        println!("Target       endpoint [{idx}] {name}");
        println!("Recording    {seconds}s -> capture.wav\n");
        match capture::windows::record_device_to_wav(
            idx,
            Duration::from_secs(seconds),
            "capture.wav",
        ) {
            Ok(st) => {
                println!("Frames       {}", st.frames);
                println!("Peak         {:.4}\n", st.peak);
                println!(
                    "{}",
                    if st.is_silent() {
                        "SILENT — nothing is playing to this endpoint."
                    } else {
                        "SIGNAL CAPTURED -> capture.wav"
                    }
                );
            }
            Err(e) => eprintln!("Capture failed: {e}"),
        }
        return;
    }

    #[cfg(windows)]
    if args.iter().any(|a| a == "--system") {
        println!("Target       WHOLE SYSTEM (default render endpoint)");
        println!("Recording    {seconds}s -> capture.wav\n");
        match capture::windows::record_system_to_wav(Duration::from_secs(seconds), "capture.wav") {
            Ok(st) => {
                println!("Frames       {}", st.frames);
                println!("Peak         {:.4}\n", st.peak);
                println!(
                    "{}",
                    if st.is_silent() {
                        "SILENT — nothing playing on this machine at all."
                    } else {
                        "SIGNAL CAPTURED -> capture.wav"
                    }
                );
            }
            Err(e) => eprintln!("Capture failed: {e}"),
        }
        return;
    }

    let pid = match resolve_pid(pid_override) {
        Some(p) => p,
        None => {
            eprintln!("Discord isn't running. Start it and try again.");
            std::process::exit(1);
        }
    };

    let out = "capture.wav";
    println!("Recording    {seconds}s -> {out}\n");

    #[cfg(not(windows))]
    {
        let _ = pid;
        eprintln!("WAV capture is Windows-only in this build.");
        std::process::exit(1);
    }

    #[cfg(windows)]
    match capture::windows::record_to_wav(pid, Duration::from_secs(seconds), out) {
        Ok(stats) => {
            println!("Frames       {}", stats.frames);
            println!("Audio        {:.1}s", stats.frames as f32 / 48_000.0);
            println!("Silent pkts  {}", stats.silent_packets);
            println!("Peak         {:.4}\n", stats.peak);

            if stats.frames == 0 {
                println!("NO DATA — stream opened but delivered nothing.");
                std::process::exit(2);
            } else if stats.is_silent() {
                println!("SILENT — frames arrived, every sample zero.");
                println!("Either the source made no sound, or capture is attached");
                println!("to the wrong thing. See docs/05-challenges.md#p2.");
                std::process::exit(3);
            }
            println!("SIGNAL CAPTURED -> {out}");
        }
        Err(e) => {
            eprintln!("Capture failed: {e}");
            std::process::exit(1);
        }
    }
}

fn print_help() {
    println!(
        "DiscRec {} — records Discord's audio.\n\n\
         No arguments          open the app\n\
         <secs>                capture Discord to capture.wav\n\
         <secs> --mix          mix Discord + mic to mixed.ogg\n\
         <secs> --mix --log    also write soak.csv every 30s\n\
         <secs> --both         per-stream clock stats, no file\n\
         <secs> --pid N        target an arbitrary process\n\
         --devices             list output endpoints\n\
         --help                this text\n",
        env!("CARGO_PKG_VERSION")
    );
}

fn resolve_pid(pid_override: Option<u32>) -> Option<u32> {
    match pid_override {
        Some(p) => {
            println!("Target       pid {p} (override)");
            Some(p)
        }
        None => discord::find().map(|f| {
            println!(
                "Discord      {} (root pid {})",
                discord::variant_name(&f),
                f.pid
            );
            f.pid
        }),
    }
}

fn run_mixed(seconds: u64, pid_override: Option<u32>, log: bool) {
    let pid = match resolve_pid(pid_override) {
        Some(p) => p,
        None => {
            eprintln!("Discord isn't running. Start it and try again.");
            std::process::exit(1);
        }
    };

    let out = Path::new("mixed.ogg");
    println!("Recording    {seconds}s -> {}", out.display());
    println!("             Discord + your microphone, Ogg/Opus");
    if log {
        println!("Telemetry    soak.csv, sampled every 30s");
    }
    println!();

    let log_path = log.then_some(Path::new("soak.csv"));
    match session::record_mixed(pid, Duration::from_secs(seconds), out, log_path) {
        Ok(r) => {
            println!("\nDiscord peak    {:.4}", r.discord_peak);
            println!("Microphone peak {:.4}", r.mic_peak);
            println!("Frames written  {}", r.frames_out);
            println!("Live drift      {:+.0} ppm", r.drift_ppm);
            println!("Mic underruns   {}", r.mic_underruns);
            println!("Mic overruns    {}", r.mic_overruns);
            println!("Clamp hits      {}", r.clamp_hits);
            println!("Limiter acted   {} samples", r.limited);

            if r.discord_peak < 1.0e-6 {
                println!("\nNo Discord audio — nobody was talking, or the wrong process.");
            } else if r.mic_peak < 1.0e-6 {
                println!("\nNo microphone audio — check the default input device.");
            } else {
                println!("\nBOTH SIDES CAPTURED -> {}", out.display());
            }
        }
        Err(CaptureError::NoSignal) => {
            eprintln!("Started, but no audio is coming through.");
            std::process::exit(3);
        }
        Err(e) => {
            eprintln!("Capture failed: {e}");
            std::process::exit(1);
        }
    }
}

#[derive(Default)]
struct Track {
    packets: u64,
    samples: u64,
    first_pos: Option<u64>,
    last_pos: u64,
    peak: f32,
    first_seen: Option<Instant>,
    last_seen: Option<Instant>,
}

impl Track {
    fn advance(&self) -> f64 {
        self.last_pos.saturating_sub(self.first_pos.unwrap_or(0)) as f64
    }

    fn span_secs(&self) -> f64 {
        match (self.first_seen, self.last_seen) {
            (Some(a), Some(b)) => b.duration_since(a).as_secs_f64(),
            _ => 0.0,
        }
    }
}

fn run_both_streams(seconds: u64, pid_override: Option<u32>) {
    let pid = match resolve_pid(pid_override) {
        Some(p) => p,
        None => {
            eprintln!("Discord isn't running. Start it and try again.");
            std::process::exit(1);
        }
    };

    println!("Capturing    {seconds}s from both streams\n");

    let mut backend = capture::backend();
    let (tx, rx) = channel();

    if let Err(e) = backend.start(pid, tx) {
        eprintln!("\nCapture failed to start: {e}");
        std::process::exit(1);
    }

    let mut tracks: HashMap<&'static str, Track> = HashMap::new();
    let started = Instant::now();
    let warmup = Duration::from_secs(3);

    while started.elapsed() < Duration::from_secs(seconds) {
        if let Ok(frame) = rx.recv_timeout(Duration::from_millis(250)) {
            if started.elapsed() < warmup {
                continue;
            }
            let name = match frame.source {
                Source::DiscordOutput => "discord",
                Source::Microphone => "microphone",
            };
            let t = tracks.entry(name).or_default();
            t.packets += 1;
            t.samples += frame.samples.len() as u64;
            t.first_pos.get_or_insert(frame.sample_pos);
            t.last_pos = frame.sample_pos;
            t.first_seen.get_or_insert_with(Instant::now);
            t.last_seen = Some(Instant::now());
            for s in &frame.samples {
                if s.abs() > t.peak {
                    t.peak = s.abs();
                }
            }
        }
    }

    let _ = backend.stop();
    let elapsed = started.elapsed().as_secs_f64();

    println!(
        "\n{:<12} {:>8} {:>12} {:>12} {:>8}",
        "stream", "packets", "samples", "clock adv", "peak"
    );
    for name in ["discord", "microphone"] {
        match tracks.get(name) {
            Some(t) => println!(
                "{:<12} {:>8} {:>12} {:>12} {:>8.4}",
                name,
                t.packets,
                t.samples,
                t.advance() as u64,
                t.peak
            ),
            None => println!("{name:<12}     NONE"),
        }
    }

    println!("\nRun wall clock  {elapsed:.2}s");

    let mut rates: Vec<(&str, f64)> = Vec::new();
    for name in ["discord", "microphone"] {
        if let Some(t) = tracks.get(name) {
            let (advance, span) = (t.advance(), t.span_secs());
            if advance > 0.0 && span > 0.0 {
                let rate = advance / span;
                rates.push((name, rate));
                println!(
                    "{name:<15} {rate:9.1} Hz over its own {span:.2}s  ({:+.0} ppm vs 48000)",
                    (rate / 48_000.0 - 1.0) * 1.0e6
                );
            }
        }
    }

    if rates.len() == 2 {
        let rel = (rates[0].1 / rates[1].1 - 1.0) * 1.0e6;
        println!("\nRelative drift  {rel:+.0} ppm between the two clocks");
        println!(
            "                ~{:.0} ms per hour if uncorrected",
            rel.abs() * 3.6
        );
        println!("\nShort runs are noisy: an order of magnitude, not a calibration.");
        println!("The mixer measures continuously rather than trusting one reading.");
    }

    if tracks.len() < 2 {
        println!("\nOnly one stream produced frames — the other is not working.");
        std::process::exit(2);
    }
    println!("\nBoth streams delivered frames.");
}
