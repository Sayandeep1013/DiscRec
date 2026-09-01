//! WASAPI capture. See `docs/spec/capture-windows.md`.
//!
//! Two independent streams, each on its own thread with its own device clock:
//! Discord's process loopback, and the default microphone. They are *not*
//! aligned here — that is the mixer's job, and doing it in the wrong place is
//! how P1 happens. Each frame carries the position its own device reported.

use super::{CaptureBackend, CaptureError, Frame, FrameSink, Source, StreamFormat};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::channel;
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use windows::core::{implement, Interface, Ref, Result as WinResult, PCWSTR};
use windows::Win32::Foundation::S_OK;
use windows::Win32::Media::Audio::{
    eCapture, eCommunications, ActivateAudioInterfaceAsync, IActivateAudioInterfaceAsyncOperation,
    IActivateAudioInterfaceCompletionHandler, IActivateAudioInterfaceCompletionHandler_Impl,
    IAudioCaptureClient, IAudioClient, IMMDeviceEnumerator, MMDeviceEnumerator,
    AUDCLNT_BUFFERFLAGS_SILENT, AUDCLNT_SHAREMODE_SHARED, AUDCLNT_STREAMFLAGS_LOOPBACK,
    WAVEFORMATEX,
};
use windows::Win32::System::Com::{
    CoCreateInstance, CoInitializeEx, CoTaskMemFree, CLSCTX_ALL, COINIT_MULTITHREADED,
};

/// The pseudo-device process loopback activates against. Not exposed as a
/// constant by the `windows` crate.
const VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK: &str = "VAD\\Process_Loopback";

const VT_BLOB: u16 = 65;
/// We read capture buffers as `f32`, so streams must be declared IEEE float.
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
/// A 4-byte enum followed directly by an 8-byte union — 12 bytes, 4-byte
/// aligned, no padding between them. Inserting any makes the driver read the
/// process id from the wrong offset, and activation fails with a
/// plausible-looking HRESULT rather than an obvious error.
#[repr(C)]
#[derive(Clone, Copy)]
struct ActivationParams {
    activation_type: u32,
    loopback: ProcessLoopbackParams,
}

/// A `PROPVARIANT` holding a `VT_BLOB`, laid out by hand because the generated
/// union is awkward to construct and this is the only shape we need.
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

type Signal = Arc<(Mutex<bool>, Condvar)>;

/// Signals completion of `ActivateAudioInterfaceAsync`. The callback arrives on
/// a COM pool thread, so this uses a condvar rather than a Win32 event.
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

fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// An opened capture stream, ready to pump.
struct Stream {
    client: IAudioClient,
    capture: IAudioCaptureClient,
    channels: u16,
    sample_rate: u32,
    /// True when samples arrive as `f32`; otherwise 16-bit ints.
    float_samples: bool,
}

/// Open Discord's process loopback.
///
/// Must be called on the thread that will pump it — the COM interfaces are
/// apartment-bound and not `Send`.
unsafe fn open_loopback(pid: u32) -> Result<Stream, CaptureError> {
    const SAMPLE_RATE: u32 = 48_000;
    const CHANNELS: u16 = 2;

    let params = ActivationParams {
        activation_type: AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
        loopback: ProcessLoopbackParams {
            target_process_id: pid,
            // Discord renders audio from child processes; targeting the root
            // alone captures nothing while still reporting success.
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

    // Process loopback exposes no mix format to query — we state one.
    let block_align = CHANNELS * 32 / 8;
    let format = WAVEFORMATEX {
        wFormatTag: WAVE_FORMAT_IEEE_FLOAT,
        nChannels: CHANNELS,
        nSamplesPerSec: SAMPLE_RATE,
        nAvgBytesPerSec: SAMPLE_RATE * block_align as u32,
        nBlockAlign: block_align,
        wBitsPerSample: 32,
        cbSize: 0,
    };

    client
        .Initialize(
            AUDCLNT_SHAREMODE_SHARED,
            AUDCLNT_STREAMFLAGS_LOOPBACK,
            200 * 10_000, // 200 ms, in 100 ns units
            0,
            &format,
            None,
        )
        .map_err(|e| CaptureError::Platform(format!("loopback Initialize: {e}")))?;

    let capture: IAudioCaptureClient = client
        .GetService()
        .map_err(|e| CaptureError::Platform(format!("loopback GetService: {e}")))?;

    Ok(Stream {
        client,
        capture,
        channels: CHANNELS,
        sample_rate: SAMPLE_RATE,
        float_samples: true,
    })
}

/// Open the default communications microphone.
///
/// Its clock is unrelated to the loopback stream's. Whatever rate the device
/// reports is passed through unchanged; reconciling the two is the mixer's job.
unsafe fn open_microphone() -> Result<Stream, CaptureError> {
    let enumerator: IMMDeviceEnumerator =
        CoCreateInstance(&MMDeviceEnumerator, None, CLSCTX_ALL)
            .map_err(|e| CaptureError::Platform(format!("MMDeviceEnumerator: {e}")))?;

    let device = enumerator
        .GetDefaultAudioEndpoint(eCapture, eCommunications)
        .map_err(|_| CaptureError::Platform("no default microphone".into()))?;

    let client: IAudioClient = device
        .Activate(CLSCTX_ALL, None)
        .map_err(|e| CaptureError::Platform(format!("mic Activate: {e}")))?;

    let mix = client
        .GetMixFormat()
        .map_err(|e| CaptureError::Platform(format!("GetMixFormat: {e}")))?;

    let channels = (*mix).nChannels;
    let sample_rate = (*mix).nSamplesPerSec;
    let bits = (*mix).wBitsPerSample;

    let result = client.Initialize(AUDCLNT_SHAREMODE_SHARED, 0, 200 * 10_000, 0, mix, None);

    CoTaskMemFree(Some(mix as *const _));
    result.map_err(|e| CaptureError::Platform(format!("mic Initialize: {e}")))?;

    let capture: IAudioCaptureClient = client
        .GetService()
        .map_err(|e| CaptureError::Platform(format!("mic GetService: {e}")))?;

    Ok(Stream {
        client,
        capture,
        channels,
        sample_rate,
        float_samples: bits == 32,
    })
}

/// Read from a stream until stopped, emitting frames positioned by the
/// device's own sample counter.
///
/// `sample_pos` comes from `GetBuffer`'s device-position out-param — never a
/// counter we maintain, never the wall clock
/// (`docs/adr/0005-timeline-dual-clock.md`).
unsafe fn pump(stream: Stream, source: Source, sink: FrameSink, stop: Arc<AtomicBool>) {
    if stream.client.Start().is_err() {
        return;
    }

    let channels = stream.channels as usize;
    let mut fallback_pos: u64 = 0;

    while !stop.load(Ordering::Relaxed) {
        let available = stream.capture.GetNextPacketSize().unwrap_or(0);
        if available == 0 {
            std::thread::sleep(Duration::from_millis(4));
            continue;
        }

        let mut data: *mut u8 = std::ptr::null_mut();
        let mut frames: u32 = 0;
        let mut flags: u32 = 0;
        let mut device_pos: u64 = 0;

        if stream
            .capture
            .GetBuffer(
                &mut data,
                &mut frames,
                &mut flags,
                Some(&mut device_pos),
                None,
            )
            .is_err()
        {
            break;
        }

        let count = frames as usize * channels;
        let silent = (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0;

        // Some devices report a device position of zero. Fall back to a
        // running count rather than collapsing every frame onto position 0.
        let sample_pos = if device_pos == 0 {
            fallback_pos
        } else {
            device_pos
        };
        fallback_pos = sample_pos + frames as u64;

        let samples: Vec<f32> = if silent || data.is_null() {
            vec![0.0; count]
        } else if stream.float_samples {
            std::slice::from_raw_parts(data as *const f32, count).to_vec()
        } else {
            std::slice::from_raw_parts(data as *const i16, count)
                .iter()
                .map(|&s| s as f32 / 32768.0)
                .collect()
        };

        let _ = stream.capture.ReleaseBuffer(frames);

        if sink
            .send(Frame {
                source,
                sample_pos,
                samples,
            })
            .is_err()
        {
            break; // receiver dropped
        }
    }

    let _ = stream.client.Stop();
}

pub struct WasapiBackend {
    format: StreamFormat,
    stop: Arc<AtomicBool>,
    threads: Vec<JoinHandle<()>>,
}

impl WasapiBackend {
    pub fn new() -> Self {
        Self {
            format: StreamFormat {
                sample_rate: 48_000,
                channels: 2,
            },
            stop: Arc::new(AtomicBool::new(false)),
            threads: Vec::new(),
        }
    }
}

impl Default for WasapiBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for WasapiBackend {
    fn start(&mut self, discord_pid: u32, sink: FrameSink) -> Result<(), CaptureError> {
        self.stop.store(false, Ordering::SeqCst);

        // Each thread reports whether it opened, so start() fails loudly
        // instead of returning a backend that silently records nothing.
        let (ready_tx, ready_rx) = channel::<Result<String, CaptureError>>();

        for (source, label) in [
            (Source::DiscordOutput, "discord"),
            (Source::Microphone, "microphone"),
        ] {
            let stop = Arc::clone(&self.stop);
            let sink = sink.clone();
            let ready = ready_tx.clone();

            self.threads.push(std::thread::spawn(move || unsafe {
                let _ = CoInitializeEx(None, COINIT_MULTITHREADED);

                let opened = match source {
                    Source::DiscordOutput => open_loopback(discord_pid),
                    Source::Microphone => open_microphone(),
                };

                match opened {
                    Ok(stream) => {
                        let _ = ready.send(Ok(format!(
                            "{label}: {} Hz, {} ch",
                            stream.sample_rate, stream.channels
                        )));
                        pump(stream, source, sink, stop);
                    }
                    Err(e) => {
                        let _ = ready.send(Err(e));
                    }
                }
            }));
        }
        drop(ready_tx);

        let mut opened = Vec::new();
        for _ in 0..2 {
            match ready_rx.recv() {
                Ok(Ok(desc)) => opened.push(desc),
                Ok(Err(e)) => return Err(e),
                Err(_) => {
                    return Err(CaptureError::Platform(
                        "a capture thread exited before reporting".into(),
                    ))
                }
            }
        }

        for line in opened {
            eprintln!("  opened {line}");
        }

        Ok(())
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        self.stop.store(true, Ordering::SeqCst);
        for t in self.threads.drain(..) {
            let _ = t.join();
        }
        Ok(())
    }

    fn format(&self) -> StreamFormat {
        self.format
    }
}

/// What a capture run observed. Distinguishes real audio from the
/// silent-success failure mode (P2).
#[derive(Debug, Default)]
pub struct CaptureStats {
    pub frames: u64,
    pub silent_packets: u64,
    pub peak: f32,
    pub duration: Duration,
}

impl CaptureStats {
    pub fn is_silent(&self) -> bool {
        self.peak < 1.0e-6
    }
}

/// Phase 1 spike, retained: capture one process to WAV with no mixing.
/// Still the quickest way to answer "does this process make sound at all".
pub fn record_to_wav(
    pid: u32,
    duration: Duration,
    path: &str,
) -> Result<CaptureStats, CaptureError> {
    unsafe {
        let _ = CoInitializeEx(None, COINIT_MULTITHREADED);
        let stream = open_loopback(pid)?;

        let spec = hound::WavSpec {
            channels: stream.channels,
            sample_rate: stream.sample_rate,
            bits_per_sample: 32,
            sample_format: hound::SampleFormat::Float,
        };
        let mut writer = hound::WavWriter::create(path, spec)
            .map_err(|e| CaptureError::Platform(format!("WavWriter::create: {e}")))?;

        stream
            .client
            .Start()
            .map_err(|e| CaptureError::Platform(format!("Start: {e}")))?;

        let mut stats = CaptureStats::default();
        let started = Instant::now();
        let channels = stream.channels as usize;

        while started.elapsed() < duration {
            let available = stream.capture.GetNextPacketSize().unwrap_or(0);
            if available == 0 {
                std::thread::sleep(Duration::from_millis(5));
                continue;
            }

            let mut data: *mut u8 = std::ptr::null_mut();
            let mut frames: u32 = 0;
            let mut flags: u32 = 0;

            if stream
                .capture
                .GetBuffer(&mut data, &mut frames, &mut flags, None, None)
                .is_err()
            {
                break;
            }

            let count = frames as usize * channels;
            if (flags & AUDCLNT_BUFFERFLAGS_SILENT.0 as u32) != 0 || data.is_null() {
                stats.silent_packets += 1;
                for _ in 0..count {
                    let _ = writer.write_sample(0.0f32);
                }
            } else {
                for &s in std::slice::from_raw_parts(data as *const f32, count) {
                    if s.abs() > stats.peak {
                        stats.peak = s.abs();
                    }
                    let _ = writer.write_sample(s);
                }
            }

            stats.frames += frames as u64;
            let _ = stream.capture.ReleaseBuffer(frames);
        }

        let _ = stream.client.Stop();
        stats.duration = started.elapsed();

        writer
            .finalize()
            .map_err(|e| CaptureError::Platform(format!("finalize: {e}")))?;

        Ok(stats)
    }
}
