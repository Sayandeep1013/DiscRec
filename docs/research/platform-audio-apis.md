# Research — Per-OS audio capture APIs

**Checked: 1 Sept 2026.** Windows and Linux from documentation; macOS from
documentation plus an open-source reference, **not verified on hardware**.

The common thread: every desktop OS gained per-process audio capture in the last
few years, so a recorder no longer needs a virtual audio driver. Mobile went the
other way and closed it off.

## Windows — WASAPI process loopback

**Available: Windows 10 build 20348+.**

`ActivateAudioInterfaceAsync` with the pseudo-device
`VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK` and `AUDIOCLIENT_ACTIVATION_PARAMS`:

- `ActivationType = AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK`
- `ProcessLoopbackParams.TargetProcessId = <pid>`
- Mode includes or excludes the target's **process tree** — Discord renders from
  child processes, so the tree flag matters.

Whole-device loopback (the older `IAudioClient` + `AUDCLNT_STREAMFLAGS_LOOPBACK`
on a render endpoint) still exists but captures everything, which cannot satisfy
R4.

- https://learn.microsoft.com/en-us/windows/win32/api/mmdeviceapi/nf-mmdeviceapi-activateaudiointerfaceasync
- https://learn.microsoft.com/en-us/windows/win32/coreaudio/loopback-recording
- Sample: https://learn.microsoft.com/en-us/samples/microsoft/windows-classic-samples/applicationloopbackaudio-sample/

## macOS — Core Audio process taps

**Available: macOS 14.2+ (14.4+ commonly cited for the working sample).**

`AudioHardwareCreateProcessTap` with a `CATapDescription` naming target
processes; the tap is then included in an aggregate device created via
`AudioHardwareCreateAggregateDevice`, and audio is read from that device. The
original audio keeps playing to the user. Gated behind a TCC permission prompt.

Before this, system audio capture meant a virtual audio device (BlackHole,
Soundflower) or a kernel extension — incompatible with R11.

Widely described as under-documented; the community reference is:

- https://github.com/insidegui/AudioCap
- https://gist.github.com/sudara/34f00efad69a7e8ceafa078ea0f76f6f

**Open concern:** an Apple developer forum thread discusses per-device
attenuation affecting tapped streams and how to obtain unattenuated raw app
audio. Verify signal levels on real hardware before trusting output gain.

## Linux — PipeWire

Capture a specific application by linking to its node's monitor ports rather
than the sink monitor, which keeps other applications out of the recording.
PulseAudio's `module-loopback` on a sink monitor is the older fallback and
captures the whole sink — R4 cannot hold there.

Complications: Discord ships as native, Flatpak and Snap, each identifying its
node differently and sandboxing differently.

## Android — blocked, and deliberately

`AudioPlaybackCapture` (API 29+) captures only players whose usage is
`USAGE_MEDIA`, `USAGE_GAME`, or `USAGE_UNKNOWN`. Voice/video call audio uses
`USAGE_VOICE_COMMUNICATION` and is excluded — the same protection preventing
apps from recording phone calls. Additionally an app may set its capture policy
to disallow capture entirely, and `MediaProjection` consent is required
regardless.

The exclusion is a privacy design decision, not an oversight, and there is no
flag that lifts it.

- https://developer.android.com/media/platform/av-capture
- https://android-developers.googleblog.com/2019/07/capturing-audio-in-android-q.html

## iOS — no mechanism

No system-audio capture API exists. ReplayKit provides only the broadcasting
app's own audio. Out of scope.

## Summary

| Platform | Per-process capture | Needs driver | Permission |
|---|---|---|---|
| Windows 10 20348+ | Yes | No | None |
| macOS 14.2+ | Yes | No | TCC prompt |
| Linux / PipeWire | Yes | No | None |
| Android | **No — voice excluded** | — | — |
| iOS | **No API** | — | — |
