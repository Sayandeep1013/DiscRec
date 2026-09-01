# Requirements

`MUST` = does not ship without it. `SHOULD` = ship without it only with a
recorded reason.

Footprint and simplicity are `MUST` here, not aspirations — they are the reason
the project exists rather than a recommendation to use OBS.

## Capture

| ID | Requirement | Pri | Verified by |
|---|---|---|---|
| **R1** | Pressing Record begins capturing within 500ms, or fails with a stated reason. | MUST | Timed, 20 runs |
| **R2** | Discord's audio is captured in isolation — other applications are absent from the recording. | MUST | Play music throughout; assert absent from output |
| **R3** | The default microphone is captured and mixed into the recording. | MUST | Assert both sides present |
| **R4** | Screenshare and Go Live audio are captured. | SHOULD | Manual, with a stream running |
| **R5** | Recording continues across an audio device change. | SHOULD | Unplug headphones mid-session |

## Integrity — the silent failures

| ID | Requirement | Pri | Verified by |
|---|---|---|---|
| **R6** | No drift. Microphone and Discord audio stay aligned within 50ms across 4 hours, with no monotonic trend. | MUST | Soak with sync tone — gates the build |
| **R7** | A crash leaves a playable file. Audio is committed incrementally. | MUST | `SIGKILL` at 100 random offsets; every file plays |
| **R8** | Capture that produces no signal is reported as an error, never written as a silent recording. | MUST | Force wrong PID and denied permission; assert error |
| **R9** | The mix does not clip when both sources are loud. | SHOULD | Sum two hot sources; assert no samples at full scale |

## Footprint — the reason to exist

| ID | Requirement | Pri | Verified by |
|---|---|---|---|
| **R10** | Under 40 MB resident while recording. | MUST | Process sampling during soak |
| **R11** | Under 3% of one core while recording. | MUST | Same |
| **R12** | A single self-contained binary. No installer, driver, kernel extension, or runtime dependency. | MUST | Clean-VM run |
| **R13** | Usable within 10 seconds of first launch, with no configuration. | MUST | Fresh user, timed, unassisted |

## Output

| ID | Requirement | Pri | Verified by |
|---|---|---|---|
| **R14** | One Ogg/Opus file per session, openable in any common player and editor. | MUST | Opens in VLC, Audacity, ffmpeg |
| **R15** | Recording state is visible at a glance while running. | SHOULD | Manual, incl. colour-blind simulation |
| **R16** | Nothing leaves the machine. No telemetry, no update check transmitting usage, no cloud. | MUST | Packet capture across a full session |

## Explicitly out of scope

Auto-start and join detection ([ADR-0008](adr/0008-manual-control.md));
per-person tracks ([deferred/](deferred/README.md)); video; transcription; cloud
storage; mobile ([ADR-0006](adr/0006-mobile-out-of-scope.md)); Linux.

## Traceability

| Requirement | Addressed in |
|---|---|
| R1, R2, R8 | [spec/capture-windows.md](spec/capture-windows.md), [capture-macos.md](spec/capture-macos.md) |
| R3, R6, R9 | [spec/mixing-and-timeline.md](spec/mixing-and-timeline.md) |
| R7, R14 | [spec/storage-format.md](spec/storage-format.md) |
| R10–R13, R15 | [spec/desktop-shell.md](spec/desktop-shell.md), [ADR-0002](adr/0002-language-and-runtime.md) |
| R8, R16 | [spec/diagnostics.md](spec/diagnostics.md) |
| all | [spec/test-plan.md](spec/test-plan.md) |
