//! WASAPI process loopback. See `docs/spec/capture-windows.md`.
//!
//! Not implemented yet — Phase 1.

use super::{CaptureBackend, CaptureError, FrameSink, StreamFormat};

pub struct WasapiBackend {
    format: StreamFormat,
}

impl WasapiBackend {
    pub fn new() -> Self {
        Self {
            format: StreamFormat {
                sample_rate: 48_000,
                channels: 2,
            },
        }
    }
}

impl Default for WasapiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for WasapiBackend {
    fn start(&mut self, _discord_pid: u32, _sink: FrameSink) -> Result<(), CaptureError> {
        // TODO(phase-1): ActivateAudioInterfaceAsync with
        // VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK and
        // AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK.
        //
        // INCLUDE_TARGET_PROCESS_TREE is required: Discord renders audio from
        // child processes, and targeting the parent PID alone captures
        // nothing while still reporting success.
        Err(CaptureError::Platform(
            "Windows capture backend not implemented yet".into(),
        ))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        Ok(())
    }

    fn format(&self) -> StreamFormat {
        self.format
    }
}
