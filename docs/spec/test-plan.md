# Spec — Test plan

"Manual" is acceptable for a single-user tool. "Unverified" is not.

The defining property of this project is that **its worst failures are silent.**
Drift, silent capture, and a clipped mix all produce files that exist and play.
Tests must assert on *audio content*, never on exit codes.

## Fixtures

- **Sync tone** — a 1 kHz burst every 15 minutes, injected into both the
  microphone and the Discord side. The basis of every alignment test.
- **Reference call** — two people, overlapping speech, long silences, music
  playing on the recording machine throughout.
- **Chaos** — unplug headphones, switch default device, `SIGKILL` the app, fill
  the disk.

## Automated

| Test | Asserts | Req |
|---|---|---|
| Mixer unit tests | Rate correction converges; gaps pad exactly; no sample drops | R6 |
| Limiter | Two full-scale sources sum without clipping | R9 |
| Wraparound | 64-bit position handling across a `u32` device counter wrap | R6 |
| Crash recovery | `SIGKILL` at 100 random offsets; every file decodes | R7 |
| Silence detection | Forced wrong PID → `no_signal`, not a written file | R8 |
| No egress | Full session under packet capture; zero outbound connections | R16 |
| Sidecar | Parses after truncation at any byte | — |

## The soak — gates Phase 2

**The most important test in the project.** Nothing else catches
[P1](../05-challenges.md#p1), and a recording made under broken drift
compensation cannot be repaired — the information needed to fix it was never
written down.

1. Four hours, sync tone into both sources every 15 minutes.
2. Include a device change and a forced stall.
3. Cross-correlate the tone pairs at each sync point.

**Pass: offset under 50 ms at every point, and no monotonic trend.**

Both conditions. Steady growth that stays under threshold still means the
mechanism is wrong; it has only had four hours to be wrong in.

## Manual, per platform

| Test | Windows | macOS |
|---|---|---|
| Discord-only (R2) | Play music; assert absent | Same |
| Both sides present (R3) | Assert mic and remote audible | Same |
| Device change (R5) | Unplug headphones mid-recording | Same |
| Permission denied | n/a | Dismiss TCC prompt → blocked state, **not** a silent file |
| Level correctness | Assert not clipped | **Assert not attenuated** — open question in the spec |
| Stream audio (R4) | Record with a Go Live running | Same |
| Clean run (R12) | Fresh VM, no installer | Fresh VM, signed build |

## Footprint — R10, R11, R13

Sampled during the soak: under 40 MB resident, under 3% of one core. Measured
separately with the window hidden, which should cost close to nothing.

R13 needs a person: someone who has not seen the app records a call within 10
seconds of first launch, unassisted and without asking a question. If they
hesitate, the UI is wrong, not the tester.

## Currently unexecutable

Everything macOS. There is no Mac on the primary development machine, so
[capture-macos.md](capture-macos.md) is unverified and its tests unrun. Tracked
in [../08-toolchain-and-gaps.md](../08-toolchain-and-gaps.md) rather than hidden
here. → [../CONTRIBUTING-macos.md](../CONTRIBUTING-macos.md)
