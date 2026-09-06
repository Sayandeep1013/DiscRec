//! Core Audio process taps. See `docs/spec/capture-macos.md`.
//!
//! Discord is tapped with `CATapDescription::initStereoMixdownOfProcesses` —
//! never an empty process list with `exclusive`, which is system-wide and
//! fails R2. The tap sits on a private aggregate device; the microphone is a
//! second stream on the default input. Drift stays the mixer's job.

use super::{CaptureBackend, CaptureError, Frame, FrameSink, Source, StreamFormat};

use std::ffi::{c_void, CStr};
use std::ptr::NonNull;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::thread;
use std::time::{Duration, Instant};

use libc::pid_t;
use objc2::rc::{autoreleasepool, Retained};
use objc2::AnyThread;
use objc2_core_audio::{
    AudioDeviceCreateIOProcID, AudioDeviceDestroyIOProcID, AudioDeviceIOProcID, AudioDeviceStart,
    AudioDeviceStop, AudioHardwareCreateAggregateDevice, AudioHardwareCreateProcessTap,
    AudioHardwareDestroyAggregateDevice, AudioHardwareDestroyProcessTap, AudioObjectGetPropertyData,
    AudioObjectID, AudioObjectPropertyAddress, AudioObjectPropertyScope, CATapDescription,
    CATapMuteBehavior, kAudioAggregateDeviceNameKey, kAudioAggregateDeviceTapAutoStartKey,
    kAudioAggregateDeviceTapListKey, kAudioAggregateDeviceUIDKey, kAudioDevicePropertyDeviceIsAlive,
    kAudioDevicePropertyNominalSampleRate, kAudioDevicePropertyStreamFormat,
    kAudioEndPointDeviceIsPrivateKey, kAudioHardwarePropertyDefaultInputDevice,
    kAudioHardwarePropertyTranslatePIDToProcessObject, kAudioObjectPropertyElementMain,
    kAudioObjectPropertyScopeGlobal, kAudioObjectPropertyScopeInput, kAudioObjectSystemObject,
    kAudioSubTapDriftCompensationKey, kAudioSubTapUIDKey,
};
use objc2_core_audio_types::{
    AudioBuffer, AudioBufferList, AudioStreamBasicDescription, AudioTimeStamp,
};
use objc2_core_foundation::{
    CFArray, CFDictionary, CFMutableDictionary, CFRetained, CFString, kCFAllocatorDefault,
    kCFTypeArrayCallBacks, kCFTypeDictionaryKeyCallBacks, kCFTypeDictionaryValueCallBacks,
};
use objc2_foundation::{NSArray, NSNumber, NSString};

const NO_ERR: i32 = 0;
const MIX_RATE: u32 = 48_000;

struct IoCtx {
    sink: Sender<Frame>,
    source: Source,
    channels: u16,
    sample_rate: u32,
    fallback_pos: AtomicU64,
}

struct Live {
    tap_id: AudioObjectID,
    agg_id: AudioObjectID,
    agg_proc: AudioDeviceIOProcID,
    mic_id: AudioObjectID,
    mic_proc: AudioDeviceIOProcID,
    tap_ctx: *mut IoCtx,
    mic_ctx: *mut IoCtx,
}

// IOProc client pointers never leave this process; the session thread owns the backend.
unsafe impl Send for CoreAudioBackend {}

pub struct CoreAudioBackend {
    format: StreamFormat,
    live: Option<Live>,
}

impl CoreAudioBackend {
    pub fn new() -> Self {
        Self {
            format: StreamFormat {
                sample_rate: MIX_RATE,
                channels: 2,
            },
            live: None,
        }
    }
}

impl Default for CoreAudioBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl CaptureBackend for CoreAudioBackend {
    fn start(&mut self, discord_pid: u32, sink: FrameSink) -> Result<(), CaptureError> {
        let _ = self.stop();
        if !macos_at_least(14, 2) {
            return Err(CaptureError::UnsupportedOs {
                needs: "macOS 14.2",
            });
        }

        autoreleasepool(|_| unsafe { start_inner(self, discord_pid, sink) })
    }

    fn stop(&mut self) -> Result<(), CaptureError> {
        let Some(live) = self.live.take() else {
            return Ok(());
        };
        unsafe {
            let _ = AudioDeviceStop(live.agg_id, live.agg_proc);
            if live.mic_id != 0 {
                let _ = AudioDeviceStop(live.mic_id, live.mic_proc);
            }
            let _ = AudioDeviceDestroyIOProcID(live.agg_id, live.agg_proc);
            if live.mic_id != 0 {
                let _ = AudioDeviceDestroyIOProcID(live.mic_id, live.mic_proc);
            }
            let _ = AudioHardwareDestroyAggregateDevice(live.agg_id);
            let _ = AudioHardwareDestroyProcessTap(live.tap_id);
            if !live.tap_ctx.is_null() {
                drop(Box::from_raw(live.tap_ctx));
            }
            if !live.mic_ctx.is_null() {
                drop(Box::from_raw(live.mic_ctx));
            }
        }
        Ok(())
    }

    fn format(&self) -> StreamFormat {
        self.format
    }
}

impl Drop for CoreAudioBackend {
    fn drop(&mut self) {
        let _ = self.stop();
    }
}

unsafe fn start_inner(
    backend: &mut CoreAudioBackend,
    discord_pid: u32,
    sink: FrameSink,
) -> Result<(), CaptureError> {
    let proc_ids = wait_for_process_objects(discord_pid)?;
    let numbers: Vec<Retained<NSNumber>> = proc_ids
        .iter()
        .map(|&id| NSNumber::new_u32(id))
        .collect();
    let ns_ids = NSArray::from_retained_slice(&numbers);

    let tap_desc = CATapDescription::initStereoMixdownOfProcesses(
        CATapDescription::alloc(),
        &ns_ids,
    );
    tap_desc.setPrivate(true);
    tap_desc.setMuteBehavior(CATapMuteBehavior::Unmuted);
    tap_desc.setProcessRestoreEnabled(true);
    tap_desc.setName(&NSString::from_str("DiscRec Discord tap"));

    let mut tap_id: AudioObjectID = 0;
    let status = AudioHardwareCreateProcessTap(Some(&tap_desc), &mut tap_id);
    check_status("AudioHardwareCreateProcessTap", status)?;

    let tap_uid = tap_desc.UUID().UUIDString();
    let agg_uid = format!("com.discrec.tap.{}", std::process::id());
    let props = aggregate_properties(&tap_uid, &agg_uid, "DiscRec Discord Aggregate");

    let mut agg_id: AudioObjectID = 0;
    let status = AudioHardwareCreateAggregateDevice(props.as_ref(), NonNull::from(&mut agg_id));
    if status != NO_ERR {
        let _ = AudioHardwareDestroyProcessTap(tap_id);
        return Err(status_err("AudioHardwareCreateAggregateDevice", status));
    }

    if let Err(e) = wait_until_alive(agg_id) {
        let _ = AudioHardwareDestroyAggregateDevice(agg_id);
        let _ = AudioHardwareDestroyProcessTap(tap_id);
        return Err(e);
    }

    let (agg_rate, agg_ch) = stream_format(agg_id, kAudioObjectPropertyScopeInput)
        .unwrap_or((MIX_RATE, 2));
    backend.format = StreamFormat {
        sample_rate: agg_rate,
        channels: agg_ch,
    };

    let tap_ctx = Box::into_raw(Box::new(IoCtx {
        sink: sink.clone(),
        source: Source::DiscordOutput,
        channels: agg_ch,
        sample_rate: agg_rate,
        fallback_pos: AtomicU64::new(0),
    }));

    let mut agg_proc: AudioDeviceIOProcID = None;
    let status = AudioDeviceCreateIOProcID(
        agg_id,
        Some(io_proc),
        tap_ctx.cast(),
        NonNull::from(&mut agg_proc),
    );
    if status != NO_ERR {
        drop(Box::from_raw(tap_ctx));
        let _ = AudioHardwareDestroyAggregateDevice(agg_id);
        let _ = AudioHardwareDestroyProcessTap(tap_id);
        return Err(status_err("AudioDeviceCreateIOProcID (tap)", status));
    }

    let mic_id = default_input_device().unwrap_or(0);
    let (mic_rate, mic_ch) = if mic_id != 0 {
        stream_format(mic_id, kAudioObjectPropertyScopeInput).unwrap_or((MIX_RATE, 1))
    } else {
        (MIX_RATE, 1)
    };

    let mic_ctx = Box::into_raw(Box::new(IoCtx {
        sink,
        source: Source::Microphone,
        channels: mic_ch,
        sample_rate: mic_rate,
        fallback_pos: AtomicU64::new(0),
    }));

    let mut mic_proc: AudioDeviceIOProcID = None;
    if mic_id != 0 {
        let status = AudioDeviceCreateIOProcID(
            mic_id,
            Some(io_proc),
            mic_ctx.cast(),
            NonNull::from(&mut mic_proc),
        );
        if status != NO_ERR {
            drop(Box::from_raw(mic_ctx));
            let _ = AudioDeviceDestroyIOProcID(agg_id, agg_proc);
            drop(Box::from_raw(tap_ctx));
            let _ = AudioHardwareDestroyAggregateDevice(agg_id);
            let _ = AudioHardwareDestroyProcessTap(tap_id);
            return Err(status_err("AudioDeviceCreateIOProcID (mic)", status));
        }
        let _ = AudioDeviceStart(mic_id, mic_proc);
    }

    let status = AudioDeviceStart(agg_id, agg_proc);
    if status != NO_ERR {
        if mic_id != 0 {
            let _ = AudioDeviceStop(mic_id, mic_proc);
            let _ = AudioDeviceDestroyIOProcID(mic_id, mic_proc);
        }
        drop(Box::from_raw(mic_ctx));
        let _ = AudioDeviceDestroyIOProcID(agg_id, agg_proc);
        drop(Box::from_raw(tap_ctx));
        let _ = AudioHardwareDestroyAggregateDevice(agg_id);
        let _ = AudioHardwareDestroyProcessTap(tap_id);
        return Err(status_err("AudioDeviceStart (tap)", status));
    }

    backend.live = Some(Live {
        tap_id,
        agg_id,
        agg_proc,
        mic_id,
        mic_proc,
        tap_ctx,
        mic_ctx,
    });
    Ok(())
}

unsafe extern "C-unwind" fn io_proc(
    _device: AudioObjectID,
    _now: NonNull<AudioTimeStamp>,
    input: NonNull<AudioBufferList>,
    input_time: NonNull<AudioTimeStamp>,
    _output: NonNull<AudioBufferList>,
    _output_time: NonNull<AudioTimeStamp>,
    client: *mut c_void,
) -> i32 {
    if client.is_null() {
        return NO_ERR;
    }
    let ctx = &*(client as *const IoCtx);
    let samples = copy_buffers(input.as_ptr(), ctx.channels);
    if samples.is_empty() {
        return NO_ERR;
    }
    let sample_pos = sample_pos_from(
        input_time.as_ptr(),
        &ctx.fallback_pos,
        samples.len(),
        ctx.channels,
    );
    let _ = ctx.sink.send(Frame {
        source: ctx.source,
        sample_pos,
        channels: ctx.channels,
        sample_rate: ctx.sample_rate,
        samples,
    });
    NO_ERR
}

unsafe fn copy_buffers(list: *const AudioBufferList, fallback_ch: u16) -> Vec<f32> {
    let nbuf = (*list).mNumberBuffers as usize;
    if nbuf == 0 {
        return Vec::new();
    }
    let first = (*list).mBuffers.as_ptr();
    if nbuf == 1 {
        return copy_one(&*first);
    }
    // Non-interleaved: one buffer per channel. Interleave to match the mixer.
    let ch = nbuf.max(fallback_ch as usize);
    let frames = (*first).mDataByteSize as usize / std::mem::size_of::<f32>();
    let mut out = vec![0.0f32; frames * ch];
    for c in 0..nbuf {
        let buf = &*first.add(c);
        let src = copy_one(buf);
        for (i, s) in src.into_iter().enumerate() {
            if i < frames {
                out[i * ch + c] = s;
            }
        }
    }
    out
}

unsafe fn copy_one(buf: &AudioBuffer) -> Vec<f32> {
    if buf.mData.is_null() || buf.mDataByteSize == 0 {
        return Vec::new();
    }
    let bytes = buf.mDataByteSize as usize;
    let n = bytes / std::mem::size_of::<f32>();
    let ptr = buf.mData as *const f32;
    std::slice::from_raw_parts(ptr, n).to_vec()
}

fn sample_pos_from(
    ts: *const AudioTimeStamp,
    fallback: &AtomicU64,
    sample_count: usize,
    channels: u16,
) -> u64 {
    let frames = (sample_count / channels.max(1) as usize) as u64;
    if !ts.is_null() {
        let flags = unsafe { (*ts).mFlags };
        if flags.bits() & 1 != 0 {
            let t = unsafe { (*ts).mSampleTime };
            if t.is_finite() && t >= 0.0 {
                fallback.store(t as u64 + frames, Ordering::Relaxed);
                return t as u64;
            }
        }
    }
    fallback.fetch_add(frames, Ordering::Relaxed)
}

fn wait_for_process_objects(root_pid: u32) -> Result<Vec<AudioObjectID>, CaptureError> {
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let pids = crate::discord::descendant_pids(root_pid);
        let mut objects = Vec::new();
        for pid in pids {
            if let Some(id) = translate_pid(pid) {
                objects.push(id);
            }
        }
        if !objects.is_empty() {
            objects.sort_unstable();
            objects.dedup();
            return Ok(objects);
        }
        if Instant::now() >= deadline {
            return Err(CaptureError::Platform(
                "Discord is running but Core Audio has no process object yet. Join a voice \
                 channel, then press Record again."
                    .into(),
            ));
        }
        thread::sleep(Duration::from_millis(100));
    }
}

fn translate_pid(pid: u32) -> Option<AudioObjectID> {
    let mut qualifier: pid_t = pid as pid_t;
    let mut id: AudioObjectID = 0;
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyTranslatePIDToProcessObject,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut size = std::mem::size_of::<AudioObjectID>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject as AudioObjectID,
            NonNull::from(&address),
            std::mem::size_of::<pid_t>() as u32,
            (&mut qualifier as *mut pid_t).cast(),
            NonNull::from(&mut size),
            NonNull::from(&mut id).cast(),
        )
    };
    if status == NO_ERR && id != 0 {
        Some(id)
    } else {
        None
    }
}

fn default_input_device() -> Option<AudioObjectID> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioHardwarePropertyDefaultInputDevice,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut id: AudioObjectID = 0;
    let mut size = std::mem::size_of::<AudioObjectID>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            kAudioObjectSystemObject as AudioObjectID,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut id).cast(),
        )
    };
    if status == NO_ERR && id != 0 {
        Some(id)
    } else {
        None
    }
}

fn stream_format(device: AudioObjectID, scope: AudioObjectPropertyScope) -> Option<(u32, u16)> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyStreamFormat,
        mScope: scope,
        mElement: kAudioObjectPropertyElementMain,
    };
    let mut asbd = unsafe { std::mem::zeroed::<AudioStreamBasicDescription>() };
    let mut size = std::mem::size_of::<AudioStreamBasicDescription>() as u32;
    let status = unsafe {
        AudioObjectGetPropertyData(
            device,
            NonNull::from(&address),
            0,
            std::ptr::null(),
            NonNull::from(&mut size),
            NonNull::from(&mut asbd).cast(),
        )
    };
    if status != NO_ERR || asbd.mSampleRate == 0.0 {
        let mut rate = 0.0f64;
        let rate_addr = AudioObjectPropertyAddress {
            mSelector: kAudioDevicePropertyNominalSampleRate,
            mScope: kAudioObjectPropertyScopeGlobal,
            mElement: kAudioObjectPropertyElementMain,
        };
        let mut rate_size = std::mem::size_of::<f64>() as u32;
        let st = unsafe {
            AudioObjectGetPropertyData(
                device,
                NonNull::from(&rate_addr),
                0,
                std::ptr::null(),
                NonNull::from(&mut rate_size),
                NonNull::from(&mut rate).cast(),
            )
        };
        if st != NO_ERR || rate == 0.0 {
            return None;
        }
        return Some((rate as u32, 2));
    }
    Some((asbd.mSampleRate as u32, asbd.mChannelsPerFrame.max(1) as u16))
}

fn wait_until_alive(device: AudioObjectID) -> Result<(), CaptureError> {
    let address = AudioObjectPropertyAddress {
        mSelector: kAudioDevicePropertyDeviceIsAlive,
        mScope: kAudioObjectPropertyScopeGlobal,
        mElement: kAudioObjectPropertyElementMain,
    };
    let deadline = Instant::now() + Duration::from_secs(2);
    loop {
        let mut alive: u32 = 0;
        let mut size = std::mem::size_of::<u32>() as u32;
        let status = unsafe {
            AudioObjectGetPropertyData(
                device,
                NonNull::from(&address),
                0,
                std::ptr::null(),
                NonNull::from(&mut size),
                NonNull::from(&mut alive).cast(),
            )
        };
        if status == NO_ERR && alive != 0 {
            return Ok(());
        }
        if Instant::now() >= deadline {
            return Err(CaptureError::Platform(
                "Core Audio aggregate device never became alive (silent tap).".into(),
            ));
        }
        thread::sleep(Duration::from_millis(20));
    }
}

fn to_cfstring(cstr: &'static CStr) -> CFRetained<CFString> {
    unsafe {
        CFString::with_c_string(
            kCFAllocatorDefault,
            cstr.as_ptr(),
            0x0800_0100, /* kCFStringEncodingUTF8 */
        )
    }
    .expect("cfstring from static key")
}

fn yes_number() -> Retained<NSNumber> {
    NSNumber::initWithBool(NSNumber::alloc(), true)
}

/// Same aggregate-device dictionary shape as cpal's loopback helper, but the
/// tap itself is a process mixdown — never exclusive+empty (that is system-wide).
fn aggregate_properties(
    tap_uid: &NSString,
    agg_uid: &str,
    agg_name: &str,
) -> CFRetained<CFDictionary> {
    unsafe {
        let tap_inner = CFMutableDictionary::new(
            kCFAllocatorDefault,
            2,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
        .expect("tap dict");
        let yes = yes_number();
        CFMutableDictionary::set_value(
            Some(tap_inner.as_ref()),
            &*to_cfstring(kAudioSubTapUIDKey) as *const _ as *const c_void,
            tap_uid as *const NSString as *const c_void,
        );
        CFMutableDictionary::set_value(
            Some(tap_inner.as_ref()),
            &*to_cfstring(kAudioSubTapDriftCompensationKey) as *const _ as *const c_void,
            &*yes as *const NSNumber as *const c_void,
        );

        let taps_list = [tap_inner];
        let taps = CFArray::new(
            kCFAllocatorDefault,
            taps_list.as_ptr() as *mut *const c_void,
            taps_list.len() as isize,
            &kCFTypeArrayCallBacks,
        )
        .expect("tap list");

        let dict = CFMutableDictionary::new(
            kCFAllocatorDefault,
            5,
            &kCFTypeDictionaryKeyCallBacks,
            &kCFTypeDictionaryValueCallBacks,
        )
        .expect("aggregate dict");
        let name = CFString::from_str(agg_name);
        let uid = CFString::from_str(agg_uid);
        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioAggregateDeviceNameKey) as *const _ as *const c_void,
            &*name as *const CFString as *const c_void,
        );
        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioAggregateDeviceUIDKey) as *const _ as *const c_void,
            &*uid as *const CFString as *const c_void,
        );
        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioAggregateDeviceTapListKey) as *const _ as *const c_void,
            &*taps as *const CFArray as *const c_void,
        );
        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioAggregateDeviceTapAutoStartKey) as *const _ as *const c_void,
            &*yes as *const NSNumber as *const c_void,
        );
        CFMutableDictionary::set_value(
            Some(dict.as_ref()),
            &*to_cfstring(kAudioEndPointDeviceIsPrivateKey) as *const _ as *const c_void,
            &*yes as *const NSNumber as *const c_void,
        );
        CFRetained::cast_unchecked(dict)
    }
}

fn check_status(op: &str, status: i32) -> Result<(), CaptureError> {
    if status == NO_ERR {
        Ok(())
    } else {
        Err(status_err(op, status))
    }
}

fn status_err(op: &str, status: i32) -> CaptureError {
    // TCC denial often surfaces as paramErr (-50) or a HAL error rather than a
    // dedicated code. Point the UI at Settings instead of a silent file.
    if status == -50 || status == 560_947_818 {
        return CaptureError::PermissionDenied;
    }
    CaptureError::Platform(format!("{op} failed: OSStatus {status}"))
}

fn macos_at_least(major: u32, minor: u32) -> bool {
    let mut buf = [0u8; 32];
    let mut size = buf.len();
    let name = c"kern.osproductversion";
    let err = unsafe {
        libc::sysctlbyname(
            name.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if err != 0 {
        return true; // if we cannot read, try anyway and let the API fail
    }
    let text = String::from_utf8_lossy(&buf[..size.saturating_sub(1)]);
    let mut parts = text.split('.');
    let maj: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    let min: u32 = parts.next().and_then(|s| s.parse().ok()).unwrap_or(0);
    maj > major || (maj == major && min >= minor)
}
