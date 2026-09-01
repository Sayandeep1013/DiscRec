//! DiscRec — records Discord's audio. Press record.
//!
//! Start with `docs/README.md`. The architecture is four parts:
//! process finder, capture backend, mixer, writer.
//!
//! Phase 1 spike: this binary finds Discord, captures its process audio for a
//! few seconds, and writes a WAV. Proving per-process isolation (R2) is the
//! whole point — play music while it runs and the output must not contain it.

mod capture;
mod discord;

use std::time::Duration;

fn main() {
    println!(
        "DiscRec {} — Phase 1 capture spike\n",
        env!("CARGO_PKG_VERSION")
    );

    // Test hook: `discrec <secs> --pid N` captures an arbitrary process, so
    // the pipeline can be validated against something known to be making
    // noise. Distinguishes "my code is wrong" from "Discord is silent".
    let args: Vec<String> = std::env::args().collect();
    let pid_override = args
        .iter()
        .position(|a| a == "--pid")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<u32>().ok());

    if let Some(pid) = pid_override {
        let seconds: u64 = args.get(1).and_then(|a| a.parse().ok()).unwrap_or(8);
        println!("Target       pid {pid} (override)");
        println!(
            "Recording    {seconds}s -> probe.wav
"
        );
        #[cfg(windows)]
        match capture::windows::record_to_wav(pid, Duration::from_secs(seconds), "probe.wav") {
            Ok(st) => {
                println!(
                    "Frames {}  peak {:.4}  silent_pkts {}",
                    st.frames, st.peak, st.silent_packets
                );
                println!(
                    "{}",
                    if st.is_silent() {
                        "SILENT"
                    } else {
                        "SIGNAL CAPTURED"
                    }
                );
            }
            Err(e) => println!("failed: {e}"),
        }
        return;
    }

    let found = match discord::find() {
        Some(f) => {
            println!(
                "Discord     {} (root pid {})",
                discord::variant_name(&f),
                f.pid
            );
            f
        }
        None => {
            eprintln!("Discord isn't running. Start it and try again.");
            std::process::exit(1);
        }
    };

    let seconds: u64 = std::env::args()
        .nth(1)
        .and_then(|a| a.parse().ok())
        .unwrap_or(12);

    let out = "discord-capture.wav";
    println!("Recording    {seconds}s -> {out}");
    println!("             (play music now — it must NOT appear in the file)\n");

    #[cfg(windows)]
    let result = capture::windows::record_to_wav(found.pid, Duration::from_secs(seconds), out);

    #[cfg(not(windows))]
    let result: Result<capture::CaptureStats, capture::CaptureError> = {
        let _ = found;
        Err(capture::CaptureError::Platform(
            "the Phase 1 spike is Windows-only so far".into(),
        ))
    };

    match result {
        Ok(stats) => {
            let secs = stats.duration.as_secs_f32();
            println!("Frames       {}", stats.frames);
            println!(
                "Duration     {:.1}s of audio",
                stats.frames as f32 / 48_000.0
            );
            println!("Silent pkts  {}", stats.silent_packets);
            println!("Peak         {:.4}", stats.peak);
            println!("Elapsed      {secs:.1}s\n");

            if stats.frames == 0 {
                println!("NO DATA. The stream opened but delivered nothing.");
                println!("Likely the wrong PID, or the process tree flag was ignored.");
                std::process::exit(2);
            } else if stats.is_silent() {
                println!("SILENT. Frames arrived but every sample was zero.");
                println!("This is the P2 failure mode — a healthy-looking stream carrying");
                println!("nothing. If Discord genuinely made no sound, that's expected;");
                println!("otherwise the capture is attached to the wrong thing.");
                std::process::exit(3);
            } else {
                println!("Signal captured. Now verify isolation:");
                println!("  1. Open {out} and confirm you hear Discord.");
                println!("  2. Confirm you do NOT hear the music that was playing.");
                println!("That second check is requirement R2.");
            }
        }
        Err(e) => {
            eprintln!("Capture failed: {e}");
            std::process::exit(1);
        }
    }
}
