# Spec — Capture backend: macOS

Implements [capture-interface.md](capture-interface.md). Satisfies R1, R2, R3,
R8. Target: macOS 14.2+, 14.4+ preferred.

> **Status: unverified.** Written from Apple's documentation and the AudioCap
> open-source reference, without access to Mac hardware. Nothing here has been
> compiled or run. Treat it as a starting point and expect to revise it.
> → [../CONTRIBUTING-macos.md](../CONTRIBUTING-macos.md)

## Discord capture — process taps

A Core Audio process tap copies the audio a chosen process renders while it
continues playing normally. Before macOS 14.2 this required a virtual audio
driver or kernel extension, neither compatible with R12.

```
Discord PID ──▶ AudioObjectID
        ▼
CATapDescription(processes: [discordObjectID])
   .isPrivate    = true      // keep it out of other apps' device lists
   .muteBehavior = .unmuted  // audio keeps reaching the user
        ▼
AudioHardwareCreateProcessTap(desc, &tapID)
        ▼
aggregate device dictionary including the tap UUID in its tap list
        ▼
AudioHardwareCreateAggregateDevice(dict, &aggregateID)
        ▼
IO proc  ──▶  Frame { source: DiscordOutput, … }
```

**Pass Discord's process explicitly.** A tap created with an empty process list
and `setExclusive(true)` records *everything* — that is system-wide capture and
fails R2. This is exactly what `cpal`'s built-in loopback does, and why it
cannot be used as-is ([ADR-0007](../adr/0007-cross-platform-strategy.md)).

The aggregate-device step is under-documented and is where implementations
usually stall; AudioCap exists precisely because of that.
→ [../research/platform-audio-apis.md](../research/platform-audio-apis.md)

## Microphone

Captured separately via the default input device. Worth attempting to include
both the tap and the input in **one aggregate device with drift compensation
enabled** (`kAudioSubTapDriftCompensationKey`) — if that works, Core Audio
handles much of [P1](../05-challenges.md#p1) for free, which Windows cannot.

Measure and log residual drift regardless. Do not assume it is zero.

## Permissions ([P3](../05-challenges.md#p3))

Gated by TCC under `kTCCServiceAudioCapture`.

- `NSAudioCaptureUsageDescription` in `Info.plist`, and a microphone usage
  string for the input.
- Trigger the prompt at first launch and **verify real signal then**, not at
  first recording.
- Denial returns `PermissionDenied`, which the shell surfaces with a link to the
  settings pane — never a silent recording
  ([desktop-shell.md](desktop-shell.md)).

## Signing and notarization

A work item, not a build step: Apple Developer account, hardened runtime,
signing, notarization, stapling. An unsigned binary requesting audio capture
will not run for a normal user. Budget most of Phase 4 for this.

## Device changes (R5)

Observe `kAudioHardwarePropertyDefaultOutputDevice` and device lifecycle
notifications. Rebuild the tap and aggregate device, resume into the same
recording.

## Known unknowns

Each needs real hardware. **Answering these is the first job of the macOS
contributor**, ahead of writing much code:

1. **Do tapped streams arrive attenuated?** There is an open Apple developer
   thread on per-device attenuation and obtaining unattenuated app audio. If
   levels are wrong, everything downstream is wrong.
2. Does a tap survive Discord restarting, or must it be rebuilt?
3. Behaviour when Discord is not running at tap-creation time.
4. Does an aggregate device containing both tap and input actually give usable
   drift compensation?
5. Sample-rate negotiation when the aggregate disagrees with 48 kHz.
