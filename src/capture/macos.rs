//! Core Audio process taps. See `docs/spec/capture-macos.md`.
//!
//! Not implemented yet. This is the file a macOS contributor writes —
//! see `docs/CONTRIBUTING-macos.md`.
//!
//! Note: the macOS spec is UNVERIFIED. It was written from Apple's
//! documentation without access to hardware. Where it disagrees with
//! reality, reality wins and the spec gets corrected.

use super::{CaptureBackend, CaptureError, FrameSink, StreamFormat};

pub struct CoreAudioBackend {
    format: StreamFormat,
}

impl CoreAudioBackend {
    pub fn new() -> Self {
        Self {
            format: StreamFormat {
                sample_rate: 48_000,
                channels: 2,
            },
        }
    }
}

impl Default for CoreAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for CoreAudioBackend {
    fn start(&mut self, _discord_pid: u32, _sink: FrameSink) -> Result<(), CaptureError> {
        // TODO(phase-4): CATapDescription naming Discord's AudioObjectID,
        // then AudioHardwareCreateProcessTap, then an aggregate device
        // holding the tap.
        //
        // Pass Discord explicitly. An empty process list with
        // setExclusive(true) records everything — that is system-wide
        // capture and fails R2. It is also what cpal's built-in loopback
        // does, which is why cpal cannot be used as-is here.
        Err(CaptureError::Platform(
            "macOS capture backend not implemented yet — see docs/CONTRIBUTING-macos.md".into(),
        ))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        Ok(())
    }

    fn format(&self) -> StreamFormat {
        self.format
    }
}
