# Spec — The capture interface and repo layout

The seam between shared code and platform code. Everything else in the
application is identical on Windows and macOS.

## Repo layout

```
DiscRec/
├── Cargo.toml
├── src/
│   ├── main.rs              app entry, wiring
│   ├── discord.rs           find Discord's process
│   ├── capture/
│   │   ├── mod.rs           trait + compile-time backend selection
│   │   ├── windows.rs       #[cfg(windows)]        WASAPI
│   │   └── macos.rs         #[cfg(target_os="macos")]  Core Audio
│   ├── mixer.rs             drift compensation, sum, limiter
│   ├── writer.rs            Opus encode, Ogg pages, incremental commit
│   └── ui/                  window, meters, tray
└── docs/
```

**Platform conditionals appear only inside `src/capture/`.** If one is needed
anywhere else, that is a design smell worth raising rather than working around.

## The trait

```rust
/// Which stream a frame came from.
#[derive(Copy, Clone, PartialEq, Eq)]
pub enum Source {
    DiscordOutput,
    Microphone,
}

/// A block of audio, positioned by its own stream's clock.
///
/// Deliberately has no arrival-time field: position must come from the
/// device's sample counter, never from when the callback happened.
/// See docs/spec/mixing-and-timeline.md.
pub struct Frame {
    pub source: Source,
    pub sample_pos: u64,
    pub samples: Vec<f32>,   // interleaved
}

pub struct StreamFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

pub trait CaptureBackend: Send {
    /// Attach to Discord and the default input, and begin delivering frames.
    /// Must return an error rather than starting a stream that carries no audio.
    fn start(&mut self, discord_pid: u32, sink: FrameSink) -> Result<(), CaptureError>;

    fn stop(&mut self) -> Result<(), CaptureError>;

    /// Format of the Discord stream. The microphone is resampled to match.
    fn format(&self) -> StreamFormat;
}

#[derive(Debug)]
pub enum CaptureError {
    /// OS older than the minimum for per-process capture.
    UnsupportedOs { needs: &'static str },
    /// macOS TCC, or equivalent, refused.
    PermissionDenied,
    /// Stream opened but delivered digital silence. See P2.
    NoSignal,
    /// Discord not running, or no audio session.
    DiscordNotFound,
    Platform(String),
}
```

## Selection

```rust
pub fn backend() -> Box<dyn CaptureBackend> {
    #[cfg(windows)]
    return Box::new(windows::WasapiBackend::new());
    #[cfg(target_os = "macos")]
    return Box::new(macos::CoreAudioBackend::new());
}
```

No runtime detection, no plugin system, no dynamic loading. The binary contains
exactly one backend, chosen when it was compiled.

## Contract a backend must honor

1. **`sample_pos` comes from the device's own sample counter.** Not a wall
   clock, not a frame counter maintained by the backend. This is what
   [mixing-and-timeline.md](mixing-and-timeline.md) relies on to correct drift.
2. **Discord capture excludes other applications** (R2). Whole-system loopback
   does not satisfy this.
3. **Silence is an error at startup, not a valid stream** (R8,
   [P2](../05-challenges.md#p2)). Verify signal before reporting success.
4. **Errors are typed**, not strings, so the UI can respond specifically —
   particularly `PermissionDenied`, which needs a route to the settings pane.
5. **Frames are delivered off the audio callback thread.** Never block a
   real-time callback on encoding, allocation, or I/O.

## Why this makes one repository work

A contributor with a Mac implements `macos.rs` against this trait and gets the
whole application around it — process finding, mixing, encoding, writing, UI —
without touching any of it. There is no fork, no parallel build, and no merge.

→ [../CONTRIBUTING-macos.md](../CONTRIBUTING-macos.md)
