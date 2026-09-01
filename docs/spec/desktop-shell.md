# Spec — The app window

Satisfies R13, R15. Addresses [P4](../05-challenges.md#p4),
[P5](../05-challenges.md#p5).

The whole product from the user's side. It must be usable within ten seconds of
first launch with no configuration (R13) — if it needs explaining, OBS was
already fine.

## Idle

```
┌────────────────────────────────┐
│  DiscRec                       │
│                                │
│  Discord    ▁▃▅▂▁▄▂            │   live meters before recording,
│  Mic        ▁▂▁▁▃▁▁            │   so a bad level is visible (P4)
│                                │
│        ┌──────────────┐        │
│        │  ●  Record   │        │
│        └──────────────┘        │
│                                │
│  Recordings ▸                  │
└────────────────────────────────┘
```

Meters run whenever the window is open, not only while recording. That is what
turns [P4](../05-challenges.md#p4) from a discovery into an observation.

## Recording

```
│  ●  Recording          14:32   │   elapsed, tabular figures
│  Discord    ▃▅▇▅▃▆▄            │
│  Mic        ▂▄▃▂▁▃▂            │
│        ┌──────────────┐        │
│        │  ■   Stop    │        │
│        └──────────────┘        │
```

On stop: the file is finalized and a brief confirmation names it, with **Show in
folder**. No dialog, no save prompt, no naming step.

## Blocked states

Every failure gets a plain sentence and, where possible, an action. Never a
disabled button with no reason.

| Condition | Shown |
|---|---|
| Discord not running | "Start Discord first." Record disabled, re-checks automatically |
| Discord running, no call | Record stays enabled — recording an idle Discord is allowed |
| Permission denied (macOS) | "DiscRec needs permission to record audio." → **Open Settings** |
| OS too old | "Needs Windows 10 build 20348 / macOS 14.2 or later." States the actual version found |
| No signal after start | "Started, but no audio is coming through." → **Retry** ([P2](../05-challenges.md#p2)) |
| Low disk | Warns under 2 GB; refuses to start under 500 MB |

The no-signal case matters most: it is the difference between an error and an
hour of silence discovered next week.

## Tray

A tray icon appears **only while recording**, so the app is visible when
minimised (R15). Menu: elapsed time, Stop, Show in folder.

The icon distinguishes state by **shape**, not colour alone — a filled circle
recording, nothing otherwise. Colour-only signals fail for a meaningful share of
users, and this is exactly the signal that must not be missed.

## First run

One panel, dismissed permanently:

> DiscRec records Discord's audio and your microphone into one file.
> **Everyone in the call is being recorded — tell them before you start.**

Not a consent checkbox. A checkbox trains people to click through; a single
honest line at the moment of use does not
([06-legal-and-consent.md](../06-legal-and-consent.md)).

## Footprint

The window is the main risk to R10 and R11
([P5](../05-challenges.md#p5)). Native Rust with a minimal windowing crate and a
tray crate. Meters redraw at ~15 fps while visible and stop entirely when the
window is hidden — a recording with a hidden window should cost almost nothing.

No web view, no Electron, no bundled runtime.

## Accessibility

Keyboard reachable throughout, visible focus, screen-reader labels on the record
button and both meters, and recording state announced on change — "Recording,
14 minutes" — never conveyed by icon alone.

## Not built

No settings window beyond [configuration.md](configuration.md)'s handful of
values, no waveform view, no built-in player, no editing, no history browser.
The recordings folder opens in the OS file manager.
