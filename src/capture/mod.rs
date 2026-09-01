//! Capture backends — the only platform-specific code in DiscRec.
//!
//! One trait, two implementations, selected at compile time. Everything
//! downstream (mixing, encoding, writing, UI) is shared.
//!
//! See `docs/spec/capture-interface.md` for the contract a backend must honor.

// Scaffolding: the trait and its types are defined ahead of the code that
// drives them. Remove this once Phase 1 wires capture into the recorder.
#![allow(dead_code)]

#[cfg(windows)]
pub mod windows;

#[cfg(target_os = "macos")]
pub mod macos;

#[cfg(not(any(windows, target_os = "macos")))]
compile_error!("DiscRec targets Windows and macOS only. See docs/02-constraints.md");

/// Which stream a frame came from.
#[derive(Copy, Clone, PartialEq, Eq, Debug)]
pub enum Source {
    DiscordOutput,
    Microphone,
}

/// A block of audio, positioned by its own stream's clock.
///
/// Deliberately carries no arrival timestamp. Position must come from the
/// device's sample counter, never from when the callback fired — the two
/// streams have independent hardware clocks and drift apart.
/// See `docs/spec/mixing-and-timeline.md`.
#[derive(Clone, Debug)]
pub struct Frame {
    pub source: Source,
    pub sample_pos: u64,
    pub samples: Vec<f32>,
}

#[derive(Copy, Clone, Debug)]
pub struct StreamFormat {
    pub sample_rate: u32,
    pub channels: u16,
}

#[derive(Debug)]
pub enum CaptureError {
    /// OS predates per-process audio capture.
    UnsupportedOs {
        needs: &'static str,
    },
    /// macOS TCC, or equivalent, refused.
    PermissionDenied,
    /// Stream opened but delivered digital silence. Never write this as a
    /// recording — see `docs/05-challenges.md`, P2.
    NoSignal,
    /// Discord is not running, or has no audio session.
    DiscordNotFound,
    Platform(String),
}

impl std::fmt::Display for CaptureError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CaptureError::UnsupportedOs { needs } => write!(f, "This needs {needs} or later."),
            CaptureError::PermissionDenied => {
                write!(f, "DiscRec needs permission to record audio.")
            }
            CaptureError::NoSignal => write!(f, "Started, but no audio is coming through."),
            CaptureError::DiscordNotFound => write!(f, "Start Discord first."),
            CaptureError::Platform(msg) => write!(f, "{msg}"),
        }
    }
}

impl std::error::Error for CaptureError {}

/// Where a backend delivers audio. Called off the real-time callback thread.
pub type FrameSink = Box<dyn FnMut(Frame) + Send>;

pub trait CaptureBackend: Send {
    /// Attach to Discord and the default input, and begin delivering frames.
    ///
    /// Must return `NoSignal` rather than starting a stream that carries
    /// nothing — a silent recording discovered a week later is the worst
    /// outcome this project has.
    fn start(&mut self, discord_pid: u32, sink: FrameSink) -> Result<(), CaptureError>;

    fn stop(&mut self) -> Result<(), CaptureError>;

    /// Format of the Discord stream. The microphone is resampled to match.
    fn format(&self) -> StreamFormat;
}

#[cfg(windows)]
pub fn backend() -> Box<dyn CaptureBackend> {
    Box::new(windows::WasapiBackend::new())
}

#[cfg(target_os = "macos")]
pub fn backend() -> Box<dyn CaptureBackend> {
    Box::new(macos::CoreAudioBackend::new())
}
