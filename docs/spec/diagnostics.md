# Spec — Diagnostics and errors

Satisfies R8, R16. Addresses [P1](../05-challenges.md#p1),
[P2](../05-challenges.md#p2).

The project's two worst defects are silent. The log is the only thing that can
explain a bad recording afterwards, and a recording cannot be re-made.

## Principle

**Log enough to explain a bad file after the fact.** Not to reproduce it live —
by then the call is over.

## The sidecar

Each `.ogg` gets a `.json` beside it. Deleting it costs nothing but the log; the
audio remains fully playable without it
([storage-format.md](storage-format.md)).

```jsonc
{
  "version": 1,
  "started_at": "2026-09-01T19:30:14+05:30",
  "ended_at": "2026-09-01T20:14:52+05:30",   // null means the process died
  "app_version": "0.1.0",
  "os": "Windows 11 26200",
  "sources": {
    "discord": { "pid": 18244, "device": "Speakers (Realtek)", "rate": 48000 },
    "microphone": { "device": "Blue Yeti", "rate": 48000, "gain_db": 0.0 }
  },
  "drift": {                      // P1 — the important part
    "samples_corrected": 3441,
    "max_offset_ms": 1.8,
    "trend_ppm": 19.4             // a rising trend is the early warning
  },
  "gaps": [
    { "at_ms": 1204300, "duration_ms": 62, "reason": "device_change" }
  ],
  "limiter_engaged_ms": 840,
  "warnings": []
}
```

## The drift trace

Once a minute, record both streams' `sample_pos` and their difference. This is
what makes [P1](../05-challenges.md#p1) diagnosable from a finished recording
rather than only reproducible live.

A `trend_ppm` that grows across a session means drift compensation is not
working, even if `max_offset_ms` still looks acceptable.

## Never logged

Audio content. Nothing that leaves the machine, ever (R16).

## Errors

Stable codes, shared with the shell ([desktop-shell.md](desktop-shell.md)).

| Code | Meaning | Response |
|---|---|---|
| `discord_not_found` | Discord not running, or no audio session | Disable Record, retry automatically |
| `unsupported_os` | Below Win10 20348 / macOS 14.2 | Refuse; state the version found |
| `permission_denied` | TCC refused (macOS) | Blocked state, link to settings |
| `no_signal` | Stream open, digital silence for 3s | **Stop and report.** Never write it |
| `device_lost` | Capture device removed | Rebuild, pad the gap, continue |
| `disk_low` | Under 500 MB free | Refuse to start; warn under 2 GB |
| `encode_failed` | Opus encoder error | Finalize what exists, report |

`no_signal` is the one that matters. On both platforms the capture API can
return success while delivering nothing ([C2](../02-constraints.md)), and
treating that as a warning rather than an error is how someone ends up with an
hour of silence.

## Levels

`error` — recording stopped or audio lost. `warn` — recovered, or a known gap.
`info` — lifecycle. `debug` — per-buffer detail, off by default, and enabling it
must not affect audio timing.

## Reporting a problem

A **Copy diagnostics** action in the window collects the sidecar, app and OS
versions, and device names into the clipboard. It sends nothing (R16) — the user
pastes it wherever they choose, having seen it first.
