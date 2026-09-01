# Architecture

Four parts. Everything except one of them is platform-neutral.

```
   [● Record]
        │
        ▼
  ┌──────────────┐   PID    ┌─────────────────────────┐
  │ Process      │─────────▶│ Capture backend         │  ← only platform-
  │ finder       │          │  · Discord loopback     │    specific code
  └──────────────┘          │  · default microphone   │
                            └────────────┬────────────┘
                                         │ two PCM streams,
                                         │ each with its own clock
                            ┌────────────▼────────────┐
                            │ Mixer + timeline        │
                            │  drift compensation,    │
                            │  sum, limiter           │
                            └────────────┬────────────┘
                            ┌────────────▼────────────┐
                            │ Writer                  │
                            │  Opus encode, Ogg pages,│
                            │  incremental commit     │
                            └────────────┬────────────┘
                                         ▼
                              Documents/DiscRec/*.ogg
```

## 1. Process finder

Locates Discord's process so capture can target it rather than the whole system
(R3). Handles the stable client plus Canary and PTB, and prefers the instance
with an active audio session.

Not "detect that a call started" — just "which PID do I attach to". State
inference was removed with auto-start ([ADR-0008](adr/0008-manual-control.md)).

If nothing is found, Record is disabled with a plain reason.

## 2. Capture backend — the only platform-specific part

One trait, two implementations selected at compile time:

```rust
pub struct Frame {
    pub source: Source,     // DiscordOutput | Microphone
    pub sample_pos: u64,    // this stream's own clock — never arrival time
    pub samples: Vec<f32>,  // interleaved
}

pub trait CaptureBackend: Send {
    fn start(&mut self, discord_pid: u32, sink: FrameSink) -> Result<()>;
    fn stop(&mut self) -> Result<()>;
    fn format(&self) -> StreamFormat;
}
```

`src/capture/windows.rs` and `src/capture/macos.rs`, behind `#[cfg]`. Nothing
else in the codebase carries a platform conditional — if it needs one, that is a
design smell.

→ [spec/capture-interface.md](spec/capture-interface.md),
[capture-windows.md](spec/capture-windows.md),
[capture-macos.md](spec/capture-macos.md)

## 3. Mixer + timeline

Takes two independently-clocked streams and produces one aligned stream.

This is where the project's hardest problem lives ([C3](02-constraints.md)). The
streams drift; drift compensation happens **here**, before summing, using each
stream's own sample position rather than wall-clock arrival.

Also applies a limiter, because summing two sources clips.

→ [spec/mixing-and-timeline.md](spec/mixing-and-timeline.md)

## 4. Writer

Encodes the mixed stream to Opus and writes Ogg pages incrementally so a crash
leaves a playable file (R7).

→ [spec/storage-format.md](spec/storage-format.md)

## Shell

A window with a record button, elapsed time, and level meters; a tray indicator
while recording so it is visible when minimised. No daemon, no IPC, no local
API — it is one process.

→ [spec/desktop-shell.md](spec/desktop-shell.md)

## What is shared across Windows and macOS

Everything except part 2. Process finding differs only in how the executable is
named; mixing, encoding, writing, and the entire UI are identical.

That is why one repository works: the Mac contributor writes one file against an
existing trait, and gets the rest of the application for free.

→ [ADR-0007](adr/0007-cross-platform-strategy.md),
[CONTRIBUTING-macos.md](CONTRIBUTING-macos.md)
