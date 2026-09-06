# ADR-0010 — Native Win32 shell, CLI retained

**Status: Accepted.** Date: 2026-09-06

## Context

Phase 2 built a working recorder behind a command-line harness. Phase 3 is the
product: a window, a record button, meters, a tray icon while recording, and a
first-run notice ([spec/desktop-shell.md](../spec/desktop-shell.md)). macOS is
explicitly out of this cut.

Footprint is the product (R10 < 40 MB resident, R11 < 3% of one core, R12 one
binary). The engine already sits at ~24 MB. The shell cannot spend the rest on
a GPU stack.

The GNU toolchain (ADR-0009) has already failed twice on C-dependent crates.
A UI crate that pulls wgpu, CMake, or an MSVC-only prebuilt is a session-killer,
not a maybe.

The 4-hour soak (R6) still gates *claiming* Phase 2 is done. It does not gate
being able to press Record. The soak needs Discord open for hours; it cannot be
the thing that prevents a usable app existing.

## Options considered

### A — egui / eframe (or iced)

Fastest to sketch. Default path is GPU (wgpu or glow). Published measurements
put similar tools at ~100–135 MB resident, which fails R10 on the shell alone.
A third-party software backend exists and is unproven on `windows-gnu`.

Rejected.

### B — Slint, software renderer

The honest low-RAM retained-mode option (~30 MB reported with the GPU off).
Cross-platform, so a Mac contributor would inherit the window. Slint's own
Windows binaries are documented as `x86_64-pc-windows-msvc`. FemtoVG/Skia
prebuilts and extra crates are the exact class of dependency ADR-0009 warned
about. Wrong risk for a same-session ship.

Deferred until a Mac contributor exists and can share the cost.

### C — Win32 GDI + tray via the `windows` crate we already compile

No new UI toolkit. No GPU. No extra C toolchain. The window is Windows-only,
which this cut already is. The Mac shell will be written against the same
[desktop-shell spec](../spec/desktop-shell.md) later; the session engine
underneath is shared.

**Chosen.**

## Decision

1. **Shell:** a small Win32 window drawn with GDI, plus `Shell_NotifyIcon`
   while recording. Looks are secondary to "one button, one file, small".
2. **Engine:** capture / mix / write live in `src/session.rs`, used by both
   the window and the CLI. Platform-specific code still lives in
   `src/capture/` and, for this cut only, `src/ui.rs`. The `#[cfg]` in `ui.rs`
   is a known smell and is the Mac shell's first cleanup.
3. **CLI kept.** No arguments opens the app. `discrec 45 --mix` and the crash
   / soak scripts keep working. The 4-hour soak is a *run*, not a rewrite.
4. **R8 is enforced on write.** Digital silence on Discord for ~3 s after
   Record starts deletes the file and shows the blocked state. Preview meters
   (app open, not recording) do not trip it — idle Discord is allowed.
5. **Settings window is not in this cut.** Defaults from
   [configuration.md](../spec/configuration.md) apply with no file on disk.
6. **Idle meters run whenever Discord is found**, by capturing without a
   writer. Audio reaches disk only after Record.

## Consequences

- A person can open the binary and record a call without reading a flag list
  (R13).
- Resident memory should stay close to the current ~24 MB plus a GDI window,
  which is the only way R10 still has headroom.
- The 4-hour soak and release CPU sample remain outstanding measurements.
  They are not an excuse to withhold the button.
- macOS still needs a windowing backend. Do not treat this ADR as "Win32 is
  the cross-platform UI".
