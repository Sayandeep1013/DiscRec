# ADR-0002 — Rust daemon, no Electron

**Status: Proposed.** Date: 2026-09-01

## Context

R9 caps resident memory at 80 MB while recording. R10 requires the idle watcher
to be cheap enough to autostart at login. R11 wants one self-contained binary
with no runtime install.

An Electron shell is roughly 150 MB on disk and 200 MB resident before it
records a single sample. It fails R9 and R11 on its own, before any project code
exists. The capture work is trivial by comparison — the shell decides whether
the product is "lightweight", not the DSP.

## Decision

- **Daemon: Rust.** Native access to WASAPI, Core Audio and PipeWire; no GC
  pauses on the audio path; single static binary per platform. `davey` is
  published on crates.io, so Route A is reachable from Rust too.
- **UI: tray-first.** A tray icon and a menu cover the entire v1 surface —
  status, start, stop, open folder. If a settings window outgrows a menu, Tauri
  uses the system webview at roughly 10 MB rather than shipping a browser.
  Note that Tauri unifies the *shell* only — it does nothing for audio capture,
  which is the sole genuinely platform-specific part
  ([ADR-0007](0007-cross-platform-strategy.md)).
- **Not Electron.** Not for any part of the desktop product.

> **Note (2026-09-01):** the Node/Rust tension below concerned the bot, which is
> no longer being built ([ADR-0008](0008-manual-control.md)). The decision is now
> unambiguous: **Rust throughout**, one binary, no second runtime. Retained for
> the footprint reasoning, which is the product's whole premise.

## The open tension (resolved — bot dropped)

If [ADR-0001](0001-primary-capture-route.md) lands on bot-primary, the *proven*
Route A stack is Node — that is what Craig runs and what `@snazzah/davey` was
built for. Two honest options:

1. **Rust throughout**, using `davey` from crates.io. One language, best
   footprint, but the Rust crate has thinner documentation than the Node package
   and is less proven in this specific role.
2. **Node for the bot, Rust for the local daemon.** Each part uses its proven
   stack. Costs a second runtime and a second toolchain, and the bot process is
   then not covered by R9/R11 — arguably fine, since a self-hosted bot is a
   server process, not something running on the user's laptop.

**Recommendation: start with option 2 and converge later if it hurts.** Phase 1
is about proving DAVE receive works at all, and doing that on the exact stack
that is known to work removes a variable. Porting a working receiver to Rust
later is a contained task; debugging an unproven crate while also learning the
protocol is not.

R9 and R11 apply to the *desktop daemon*. A self-hosted bot has different
constraints and should not be forced into the same envelope.

## Consequences

- Phase 1 starts on Node with no new toolchain.
- Rust and MSVC build tools are needed before Phase 4
  ([08-toolchain-and-gaps.md](../08-toolchain-and-gaps.md)).
- Revisit this ADR at the end of Phase 2. If the Node bot is stable and small,
  the case for porting it weakens considerably.
