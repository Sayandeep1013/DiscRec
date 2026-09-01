# Spec — Capture backend: Windows

Implements [capture-interface.md](capture-interface.md). Satisfies R1, R2, R3,
R8. Target: Windows 10 build 20348+ / Windows 11.

> Before writing this by hand, see
> [ADR-0007](../adr/0007-cross-platform-strategy.md) — a crate may cover this
> and macOS in one codepath. This spec describes the mechanism either way.

## Discord capture — process loopback

WASAPI process loopback captures what a specific process renders, without a
virtual device and without picking up the rest of the system. That isolation is
requirement R2, and it is the reason your music stays out of the recording.

```
AUDIOCLIENT_ACTIVATION_PARAMS {
  ActivationType = AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK,
  ProcessLoopbackParams = {
    TargetProcessId     = <Discord PID>,
    ProcessLoopbackMode = INCLUDE_TARGET_PROCESS_TREE
  }
}
        ▼
ActivateAudioInterfaceAsync(
    VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK, IID_IAudioClient, params, …)
        ▼
IAudioClient::Initialize(AUDCLNT_SHAREMODE_SHARED,
    AUDCLNT_STREAMFLAGS_LOOPBACK | AUDCLNT_STREAMFLAGS_EVENTCALLBACK, …)
        ▼
IAudioCaptureClient::GetBuffer  ──▶  Frame { source: DiscordOutput, … }
```

`INCLUDE_TARGET_PROCESS_TREE` is not optional — Discord renders audio from child
processes, and targeting the parent PID alone can capture nothing at all. This
is the most likely cause of a silent recording that otherwise looks healthy.

Microsoft's ApplicationLoopback sample is the working reference.
→ [../research/platform-audio-apis.md](../research/platform-audio-apis.md)

## Microphone

A second, independent `IAudioClient` in ordinary capture mode on the default
communications input device.

**Its clock is unrelated to the loopback stream's.** Report `sample_pos` from
this device's own counter and let the mixer reconcile them — do not attempt to
align the two streams here
([mixing-and-timeline.md](mixing-and-timeline.md), [P1](../05-challenges.md#p1)).

## Finding Discord

Never match on window title. Enumerate processes for `Discord.exe`,
`DiscordCanary.exe`, `DiscordPTB.exe`; prefer the instance with an active audio
session. Handle Discord not running at all as a normal state, not an error
(`DiscordNotFound`).

## Format

Discord renders at 48 kHz. Request that to avoid a resample. If the endpoint mix
format differs, resample once at capture and record the fact — never emit a
stream whose actual rate disagrees with its declared rate.

## Device changes (R5)

Register an `IMMNotificationClient`. On default-device change or removal, tear
down and rebuild the affected client, then resume into the **same recording**,
letting the gap flow through the normal padding path. Unplugging headphones must
not end a recording.

## Silence

Loopback delivers buffers flagged `AUDCLNT_BUFFERFLAGS_SILENT` when the target
renders nothing. Honor the flag rather than pushing zeros through the encoder,
but still advance the position — the silence is real and its duration matters.

Distinguish this from [P2](../05-challenges.md#p2): a stream that is *always*
silent from the first seconds is an error; a stream that goes quiet between
sentences is normal.

## Failure modes

| Symptom | Cause | Handling |
|---|---|---|
| Activation fails `E_INVALIDARG` | OS older than build 20348 | `UnsupportedOs`; state the version found |
| Opens fine, output silent | Wrong PID, or tree flag omitted | Assert RMS over first 3s → `NoSignal` |
| Buffer overruns under load | Capture thread starved | Dedicated thread, raised priority; count glitches |

The second row is the dangerous one — it looks like success. Verify signal, not
status codes.

## Verification

- R2: play music throughout; assert absent from output.
- R3: assert both sources present in the mix.
- R5: unplug headphones mid-recording; assert one continuous file.
- R8: force a wrong PID; assert `NoSignal` rather than a silent recording.
