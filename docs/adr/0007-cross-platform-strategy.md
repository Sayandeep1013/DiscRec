# ADR-0007 — Cross-platform strategy for the capture layer

**Status: Accepted (native).** Date: 2026-09-01, resolved by the Phase 1 spike.

## Context

The question raised was: can this be one codebase for Windows and macOS rather
than two — "Tauri-type"?

Yes, and more of it is shared than the question assumes. But **Tauri addresses a
different layer than the one that is actually duplicated.**

## What is shared

| Component | Platform-specific? |
|---|---|
| Process finder | Only the executable names |
| Mixer, drift compensation, limiter | No |
| Opus encoding, Ogg writing | No |
| Window, meters, tray | Thin, via cross-platform crates |
| **Capture backend** | **Yes — the only genuinely duplicated part** |

One module, behind the trait [ADR-0003](0003-capture-abstraction.md) defines for
exactly this purpose. A macOS contributor writes `src/capture/macos.rs` and gets
the rest of the application unchanged.

## On Tauri specifically

Tauri unifies the **UI shell**. It does nothing for audio capture, which is the
only part that is platform-specific here.

It remains reasonable if the settings window ever outgrows a menu — system
webview, roughly 10 MB, well within R10/R12 where Electron is not. But the shell
is specced tray-first ([spec/desktop-shell.md](../spec/desktop-shell.md)), and a
tray icon needs a small crate, not a webview framework.

**Tauri is not the answer to the cross-platform question**, because the hard
part is below it.

## Options considered

Verified against released code on 2026-09-01.

### Option A — `cpal` 0.18.2

Mature and widely used, with loopback on both platforms. **But system-wide, not
per-process**, on both — verified by reading the source:

- macOS (`src/host/coreaudio/macos/loopback.rs`, present in the v0.18.2 tag)
  calls `AudioHardwareCreateProcessTap` with an *empty* process list and
  `setExclusive(true)` — "exclude nothing", i.e. record everything.
- Windows sets `AUDCLNT_STREAMFLAGS_LOOPBACK` on a render endpoint. There is no
  `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK` usage anywhere in the crate.

One codepath, mature — but **R2 is lost**: music, games and notifications all
land in the recording.

### Option B — `flexaudio` 0.2.0

Rust core, MIT, claiming per-process capture on Windows, macOS and Linux via a
`target_pid`. Almost exactly the `CaptureBackend` trait this project defined
independently, already implemented for both targets.

**Too young to be load-bearing.** One crates.io release (0.2.0, 4 July 2026),
~448 downloads, 5 stars, effectively one maintainer, with a 0.3.0 sitting
unpublished in the repository.

### Option C — native per-OS

Two implementations behind the trait. Most control, no dependency risk, more
code.

## Decision

**Option C — native, using the `windows` crate on Windows.**

Reasoning:

1. **Maturity gap is decisive.** `windows` 0.62.2 has ~310M downloads and
   explicit GNU-target support; `flexaudio` has 448 and is almost certainly
   untested on the GNU toolchain this project uses
   ([ADR-0009](0009-gnu-toolchain-no-visual-studio.md)). The capture path is the
   one place a fragile dependency is least acceptable.
2. **R2 is non-negotiable and Option A cannot meet it.** Per-process isolation
   is the difference between this and a system recorder.
3. **macOS will be written natively regardless**, since Core Audio process taps
   need direct control over the `CATapDescription` process list. A shared crate
   would only have paid off if it worked well on *both*; carrying its risk for
   one platform buys nothing.

## Outcome — verified in Phase 1

Per-process capture works, and isolation is proven rather than assumed. Two
`ffplay` instances were run simultaneously at 440 Hz and 1000 Hz, and each was
captured in turn while the other played:

| Captured | 440 Hz band | 1000 Hz band |
|---|---|---|
| the 440 Hz process | **-21.1 dB** | -41.6 dB |
| the 1000 Hz process | -40.6 dB | **-21.1 dB** |

Symmetric, ~20 dB separation. The excluded process is not in the recording.

## Consequences

- The Windows backend is `src/capture/windows.rs`, written against the `windows`
  crate. No third-party audio dependency on the capture path.
- macOS follows the same pattern
  ([spec/capture-macos.md](../spec/capture-macos.md),
  [CONTRIBUTING-macos.md](../CONTRIBUTING-macos.md)).
- Revisit only if `flexaudio` matures substantially *and* something makes
  maintaining two native backends painful. Neither is true today.

## The question this leaves open

**Is R2 worth it?** If per-process capture ever turns out to be what blocks
macOS, falling back to system-wide capture there is a legitimate trade — a much
smaller loss than shipping no Mac build. That would be a deliberate decision,
not a drift, and it would need a new ADR.
