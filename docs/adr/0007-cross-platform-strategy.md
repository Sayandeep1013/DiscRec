# ADR-0007 — Cross-platform strategy: what is actually shared

**Status: Proposed.** Date: 2026-09-01

## Context

Question raised: can this be one codebase for Windows and macOS rather than two,
"Tauri-type"?

Worth answering precisely, because the assumption behind the question — that
most of the work is platform-specific — is wrong, and because Tauri addresses a
different layer than the one that is duplicated.

## What is already shared

With [ADR-0001](0001-primary-capture-route.md) landing on bot-primary, most of
the product has no platform-specific code at all:

| Component | Platform-specific? |
|---|---|
| **Route A — the bot** | **No.** It is a network client. Runs identically on Windows, macOS, Linux, a Pi, or a VPS |
| Timeline, gap fill, ordering | No |
| Storage, Ogg writing, manifest | No |
| Consent, opt-out, config, export, control API, diagnostics | No |
| Join detection — gateway | No |
| Join detection — RPC | Nearly. One named-pipe vs unix-socket path difference |
| Tray icon | Thin, via a cross-platform tray crate |
| **Route B — capture backend** | **Yes. This is the only genuinely duplicated part.** |

So the duplicated surface is one module — the fallback capture path — behind the
trait that [ADR-0003](0003-capture-abstraction.md) already defines for exactly
this purpose.

## On Tauri specifically

Tauri unifies the **UI shell**. It does nothing for audio capture, which is the
only part that is actually platform-specific here.

It remains a reasonable answer for the settings window — system webview, roughly
10 MB, well within R9/R11 where Electron is not. But the shell was specced
tray-first ([spec/desktop-shell.md](../spec/desktop-shell.md)), and a tray icon
needs a small crate, not a webview framework. Tauri earns its place only if the
settings window outgrows a menu.

**Tauri is not the answer to the cross-platform question**, because the hard
part is below it.

## Options for the capture layer

Three real ones, verified against released code on 2026-09-01.

### Option A — `cpal` 0.18.2

Mature, widely used, and it does have loopback on both platforms. **But it is
system-wide, not per-process**, on both:

- macOS (`src/host/coreaudio/macos/loopback.rs`, present in the v0.18.2 tag)
  calls `AudioHardwareCreateProcessTap` with an *empty* process list and
  `setExclusive(true)` — "exclude nothing", i.e. record everything.
- Windows sets `AUDCLNT_STREAMFLAGS_LOOPBACK` on a render endpoint. There is no
  `AUDIOCLIENT_ACTIVATION_TYPE_PROCESS_LOOPBACK` usage in the crate.

**One codepath, mature dependency, but R4 is lost** — your music, notifications
and everything else land in the recording.

### Option B — `flexaudio` 0.2.0

Rust core with Python and Node bindings, MIT. Its stated capability matrix is
per-process capture on Windows (WASAPI process loopback), macOS (Core Audio
process taps, 14.4+) and Linux (PipeWire), selected by a `target_pid` in the
stream config. It uses cpal for microphone input and documents the macOS TCC
gate including `NSAudioCaptureUsageDescription` and a `PermissionDenied` error.

That is, almost exactly, the `CaptureBackend` trait this project defined
independently — already implemented for both target platforms.

**Risk: it is very young.** One release on crates.io (0.2.0, 4 July 2026), ~448
downloads, 5 stars, effectively one maintainer; a 0.3.0 exists in the repository
but is unpublished. This would be a load-bearing dependency on the capture path.

Mitigations: it is MIT and small, so it can be vendored or forked; and
[ADR-0003](0003-capture-abstraction.md)'s trait means replacing it touches one
module.

### Option C — native per-OS, as currently specced

Two implementations behind the trait
([capture-windows.md](../spec/capture-windows.md),
[capture-macos.md](../spec/capture-macos.md)). Most control, no dependency risk,
most work — and the macOS half cannot be verified on the current hardware
either way.

## Decision

**Try Option B in Phase 4, keep Option C as the documented fallback.**

Reasoning:

1. It is the only option that preserves R4 *and* gives one codepath.
2. The dependency risk is contained by the trait that already exists. If
   flexaudio proves unreliable, the replacement is one module, not a rewrite.
3. It is MIT and small enough to vendor if the upstream goes quiet — which, at
   one maintainer and one release, is a realistic outcome to plan for.
4. Phase 4 is far enough out that the package will have another six months of
   history to judge by before anything depends on it.

**Do not adopt it without an evaluation spike**: verify per-process capture
actually works on Windows against Discord specifically, before writing anything
against its API.

## The question this leaves open

**Is R4 worth it?** Option A is one mature codepath today if system-wide capture
is acceptable — the cost being that music, game audio and notifications end up
in the recording alongside the call.

R4 is currently `SHOULD`, not `MUST`. If per-process capture turns out to be the
thing that blocks macOS, dropping R4 and taking cpal is a legitimate trade, and
a much smaller loss than not shipping macOS at all.

## Consequences

- Phase 4 gains an evaluation spike before implementation.
- [ADR-0002](0002-language-and-runtime.md) is reinforced: Rust for the daemon
  makes both Option A and Option B directly available.
- The Windows and macOS capture specs stay valid — they describe the mechanism a
  library would wrap, and remain the fallback if no library is used.
