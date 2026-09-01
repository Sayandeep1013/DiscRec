//! WASAPI process loopback. See `docs/spec/capture-windows.md`.
//!
//! Phase 1: `record_to_wav` proves the mechanism — per-process capture of
//! Discord only, with everything else excluded (R2). The `CaptureBackend`
//! trait implementation lands in Phase 2 once mixing exists.

use super::{CaptureBackend, CaptureError, FrameSink, StreamFormat};

use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use windows::core::{implement, Interface, Ref, Result as WinResult, PCWSTR};
use windows::Win32::Foundation::S_OK;
use windows::Win32::Media::Audio::{
    ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK, WAVEFORMATEX,
};
use windows::Win32::System::Com::{CoInitializeEx, COINIT_MULTITHREADED};

/// `VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK` — the pseudo-device that process
/// loopback activates against. Not exposed as a constant by the crate.
const VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK: &str = "VAD\\Process_Loopback";

const VT_BLOB: u16 = 65;
/// We read the capture buffer as `f32`, so the stream must be declared as
/// IEEE float, not integer PCM.
const WAVE_FORMAT_IEEE_FLOAT: u16 = 0x0003;
const AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK: u32 = 1;
const PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE: u32 = 0;

/// Mirrors `AUDIOCLIENT_PROCESS_LOOPBACK_PARAMS`.
#[repr(C)]
#[derive(Clone, Copy)]
struct ProcessLoopbackParams {
    target_process_id: u32,
    process_loopback_mode: u32,
}

/// Mirrors `AUDIOCLIENT_ACTIVATION_PARAMS`.
///
/// A 4-byte enum followed directly by the union — 12 bytes, 4-byte aligned.
/// There is no padding between them; inserting any makes the driver read the
/// process id from the wrong offset and activation fails.
#[repr(C)]
#[derive(Clone, Copy)]
struct ActivationParams {
    activation_type: u32,
    loopback: ProcessLoopbackParams,
}

/// A `PROPVARIANT` holding a `VT_BLOB`. Laid out by hand because the generated
/// union is awkward to construct and we only ever need this one shape.
#[repr(C)]
struct PropVariantBlob {
    vt: u16,
    reserved1: u16,
    reserved2: u16,
    reserved3: u16,
    cb_size: u32,
    _pad: u32,
    blob_data: *mut u8,
}

/// Set once `ActivateAudioInterfaceAsync` has completed.
type Signal = Arc<(Mutex<bool>, Condvar)>;

/// Signals when `ActivateAudioInterfaceAsync` has finished. The callback
/// arrives on a COM pool thread, so this uses a plain condvar rather than a
/// Win32 event.
#[implement(IActivateAudioInterfaceCompletionHandler)]
struct CompletionHandler {
    signal: Signal,
}

impl IActivateAudioInterfaceCompletionHandler_Impl for CompletionHandler_Impl {
    fn ActivateCompleted(
        &self,
        _operation: Ref<'_, IActivateAudioInterfaceAsyncOperation>,
    ) -> WinResult<()> {
        let (done, cv) = &*self.signal;
        *done.lock().unwrap() = true;
        cv.notify_all();
        Ok(())
    }
}

/// What a capture run observed. Used to distinguish real audio from the
/// silent-success failure mode (P2).
#[derive(Debug, Default)]
pub struct CaptureStats {
    pub frames: u64,
    pub silent_packets: u64,
    pub peak: f32,
    pub duration: Duration,
}

impl CaptureStats {
    /// True when the stream carried no audible signal at all.
    pub fn is_silent(&self) -> bool {
        self.peak < 1.0e-6
    }
}

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Capture `pid`'s audio (and its child processes') for `duration`, writing
/// 32-bit float WAV to `path`.
///
/// This is the Phase 1 spike. It proves per-process isolation: play music
/// while this runs and the output must contain only Discord.
pub fn record_to_wav(
    pid: u32,
    duration: Duration,
    path: &str,
) -> Result<CaptureStats, CaptureError> {
    const SAMPLE_RATE: u32 = 48_000;
    const CHANNELS: u16 = 2;

    unsafe {
        // MTA: the completion handler is invoked on a pool thread.
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

        let params = ActivationParams {
            activation_type: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
            loopback: ProcessLoopbackParams {
                target_process_id: pid,
                // Discord renders audio from child processes; targeting the
                // root alone captures nothing. See docs/spec/capture-windows.md.
                process_loopback_mode: PROCESS_LOOPBACK_MODE_INCLUDE_TARGET_PROCESS_TREE,
            },
        };

        let mut pv = PropVariantBlob {
            vt: VT_BLOB,
            reserved1: 0,
            reserved2: 0,
            reserved3: 0,
            cb_size: std::mem::size_of::<ActivationParams>() as u32,
            _pad: 0,
            blob_data: &params as *const ActivationParams as *mut u8,
        };

        let signal: Signal = Arc::new((Mutex::new(false), Condvar::new()));
        let handler: IActivateAudioInterfaceCompletionHandler = CompletionHandler {
            signal: Arc::clone(&signal),
        }
        .into();

        let device = wide(VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK);

        let operation: IActivateAudioInterfaceAsyncOperation = ActivateAudioInterfaceAsync(
            PCWSTR(device.as_ptr()),
            &<IAudioClient as Interface>::IID,
            Some(&mut pv as *mut PropVariantBlob as *const _),
            &handler,
        )
        .map_err(|e| CaptureError::Platform(format!("ActivateAudioInterfaceAsync: {e}")))?;

        {
            let (done, cv) = &*signal;
            let mut ready = done.lock().unwrap();
            while !*ready {
                ready = cv.wait(ready).unwrap();
            }
        }

        let mut hr = S_OK;
        let mut unknown = None;
        operation
            .GetActivateResult(&mut hr, &mut unknown)
            .map_err(|e| CaptureError::Platform(format!("GetActivateResult: {e}")))?;

        if hr.is_err() {
            // E_NOTIMPL / AUDCLNT_E_UNSUPPORTED_FORMAT here usually means the
            // OS predates process loopback. Anything else is a real bug, so
            // surface the code rather than guessing.
            const E_NOTIMPL: i32 = -2147467263; // 0x80004001
            if hr.0 == E_NOTIMPL {
                return Err(CaptureError::UnsupportedOs {
                    needs: "Windows 10 build 20348",
                });
            }
            return Err(CaptureError::Platform(format!(
                "activation failed: HRESULT 0x{:08X}",
                hr.0 as u32
            )));
        }

        let client: IAudioClient = unknown
            .ok_or_else(|| CaptureError::Platform("activation returned no interface".into()))?
            .cast::<IAudioClient>()
            .map_err(|e| CaptureError::Platform(format!("cast to IAudioClient: {e}")))?;

        // Process loopback has no mix format to query — we state the format.
        let bits = 32u16;
        let block_align = CHANNELS * bits / 8;
        let format = WAVEFORMATEX {
            wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
            nChannels: CHANNELS,
            nSamplesPerSec: SAMPLE_RATE,
            nAvgBytesPerSec: SAMPLE_RATE * block_align as u32,
            nBlockAlign: block_align,
            wBitsPerSample: bits,
            cbSize: 0,
        };

        // 200 ms buffer, in 100 ns units.
        let buffer_duration: i64 = 200 * 10_000;

        client
            .Initialize(
                AUDCLNT_SHAREMODE_SHARED,
                AUDCLNT_STREAMFLAGS_LOOPBACK,
                buffer_duration,
                0,
                &format,
                None,
            )
            .map_err(|e| CaptureError::Platform(format!("IAudioClient::Initialize: {e}")))?;

        let capture: IAudioCaptureClient = client
            .GetService()
            .map_err(|e| CaptureError::Platform(format!("GetService: {e}")))?;

        let spec = hound::WavSpec {
            channels: CHANNELS,
            sample_rate: SAMPLE_RATE,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|e| CaptureError::Platform(format!("WavWriter::create: {e}")))?;

        client
            .Start()
            .map_err(|e| CaptureError::Platform(format!("IAudioClient::Start: {e}")))?;

        let mut stats = CaptureStats::default();
        let started = Instant::now();

        while started.elapsed() < duration {
            let available = capture.GetNextPacketSize().unwrap_or(0);
            if available == 0 {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }

            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames: u32 = 0;
            let mut flags: u32 = 0;

            if capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                .is_err()
            {
                break;
            }

            let silent = (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;
            if silent {
                stats.silent_packets += 1;
                for _ in 0..frames * CHANNELS as u32 {
                    let _ = writer.write_sample(0.0f32);
                }
            } else {
                let samples = std::slice::from_raw_parts(
                    data as *const f32,
                    (frames * CHANNELS as u32) as usize,
                );
                for &s in samples {
                    if s.abs() > stats.peak {
                        stats.peak = s.abs();
                    }
                    let _ = writer.write_sample(s);
                }
            }

            stats.frames += frames as u64;
            let _ = capture.ReleaseBuffer(frames);
        }

        let _ = client.Stop();
        stats.duration = started.elapsed();

        writer
            .finalize()
            .map_err(|e| CaptureError::Platform(format!("WavWriter::finalize: {e}")))?;

        // `params` lives on the stack and the PROPVARIANT only borrowed it,
        // so there is nothing to free here.
        Ok(stats)
    }
}

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
        // Phase 2: drive `record_to_wav`'s inner loop from a thread and push
        // Frames into the sink instead of a WAV writer.
        Err(CaptureError::Platform(
            "streaming capture lands in Phase 2 — use record_to_wav for now".into(),
        ))
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        Ok(())
    }

    fn format(&self) -> StreamFormat {
        self.format
    }
}
