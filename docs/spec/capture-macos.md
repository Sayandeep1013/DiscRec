# Spec — Capture backend: macOS

Implements [capture-interface.md](capture-interface.md). Satisfies R1, R2, R3,
R8. Target: macOS 14.2+, 14.4+ preferred.

Implementation: `src/capture/macos.rs`. How to build and run:
[CONTRIBUTING-macos.md](../CONTRIBUTING-macos.md). Mac agent jobs:
[AGENT-macos-e2e.md](../AGENT-macos-e2e.md).

> Written from Apple's documentation, AudioCap, cpal's aggregate-device
> dictionary, and `objc2-core-audio` 0.3. First hardware proof is the Mac
> contributor's job; the **design is decided**. If a crate symbol moved, rename
> it. Do not change the behaviour below.

## Discord capture — process taps

```
Discord root PID
    descendant_pids (helpers included — audio is rendered by a child)
        ▼
kAudioHardwarePropertyTranslatePIDToProcessObject  (retry ~2 s)
        ▼
CATapDescription::initStereoMixdownOfProcesses(ids)
   .isPrivate              = true
   .muteBehavior           = Unmuted
   .processRestoreEnabled  = true
        ▼
AudioHardwareCreateProcessTap
        ▼
private aggregate (tap UID in TapList, TapAutoStart, IsPrivate)
        ▼
wait kAudioDevicePropertyDeviceIsAlive
        ▼
IOProc  →  Frame { source: DiscordOutput, sample_pos from mSampleTime, … }
```

**Pass Discord's process objects explicitly.** A tap created with an empty
process list and `setExclusive(true)` records *everything* — that fails R2.
That is what `cpal`'s loopback does; do not copy it.

If Core Audio has no process object after 2 seconds, return a platform error
telling the user to join a voice channel. Do not fall back to a global tap.

## Microphone

Default input device, **second** IOProc, `Source::Microphone`. The shared mixer
corrects drift. Do not block shipping on putting mic + tap in one aggregate
with `kAudioSubTapDriftCompensationKey`. Measuring residual drift is fine;
changing the architecture to wait on it is not.

## Permissions ([P3](../05-challenges.md#p3))

- `NSAudioCaptureUsageDescription` and `NSMicrophoneUsageDescription` in
  `macos/Info.plist`.
- Bundle id `com.discrec.app`. TCC follows the **.app**, not `cargo run`.
- Denial: `PermissionDenied` when the HAL returns paramErr / equivalent; the
  shell shows **Open Settings**. There is no reliable query API for
  system-audio TCC. A denied prompt looks like silence — reset with
  `tccutil reset AudioCapture com.discrec.app`.
- Quiet calls are valid. Do not treat "nobody is speaking" as `NoSignal`.

## Signing

Ad-hoc (`codesign --sign -`) is required for local TCC. Notarization is only
for giving the app to someone else. The app **must not** be sandboxed.

## Device changes (R5)

SHOULD, not MUST for the first Mac cut. If the default output changes,
rebuilding the tap is a follow-up. Do not hold Record for it.

## Hardware notes to fill in after first success

These are observations, not blockers. Write what you measured into this file:

1. Are tapped streams quieter than Discord's own output? If yes, by how much?
2. Does `processRestoreEnabled` survive a Discord relaunch mid-session?
3. Sample rate the aggregate actually ran at (expect 48 kHz or 44.1 kHz; the
   session already resamples).
