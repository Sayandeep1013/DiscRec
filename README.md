# DiscRec

A small native app that records your Discord calls. Open it, press record, it
finds Discord and captures the call. Press stop, you get one file.

Not a background service. Not a bot. Not a scene collection.

**Windows is ready.** Download the exe from
[Releases](https://github.com/Sayandeep1013/DiscRec/releases). macOS is next.

---

## Why this exists

OBS can already do this — Application Audio Capture pointed at Discord. It is
free and excellent.

It is also ~200 MB, needs a scene and source configured before it records
anything, and is a video production suite you are using for one small job.

DiscRec is that one job, as one binary you press a button in. The Windows
release is under 1 MB.

The honest comparison, including when you should just use OBS:
[docs/09-alternatives.md](docs/09-alternatives.md).

## What it does

- Captures **Discord's audio only** — music, games and notifications stay out
- Mixes in **your microphone**, so the recording is the whole conversation
- Captures **screenshare and Go Live audio**, because it records what you hear
- Writes one Ogg/Opus file per session (`Downloads/DiscRec/` by default)
- Survives a crash with a playable file

## What it deliberately does not do

Auto-start, per-person tracks, video, transcription, cloud anything, mobile.
Each was considered and cut; the reasoning is in
[docs/04-requirements.md](docs/04-requirements.md) and `docs/adr/`.

## Download (Windows)

1. Get `discrec.exe` from the latest
   [Release](https://github.com/Sayandeep1013/DiscRec/releases).
2. Start Discord, join a call, open DiscRec, press Record.
3. Stop when you are done. The file lands in `Downloads\DiscRec\` unless you
   pick another folder in the app.

First launch reminds you that everyone in the call is being recorded. Discord's
Terms require you to tell them; the app cannot say it for you.
→ [docs/06-legal-and-consent.md](docs/06-legal-and-consent.md)

Windows 10 (build 20348+) or Windows 11. No installer, no extra runtime.

## Building

```bash
cargo build --release      # builds for whatever OS you are on
cargo run --release        # the app (Windows)
cargo test
```

Pass any argument and you get the development harness instead of the window
(`discrec --help`). That path is how soaks and crash tests run.

Windows contributors on this repo's unusual GNU toolchain: start at
[docs/HANDOFF.md](docs/HANDOFF.md). macOS contributors: start at
[docs/CONTRIBUTING-macos.md](docs/CONTRIBUTING-macos.md) — capture is a stub
until that work lands, and there is no Mac window yet.

## How it's built

Rust. One repo. Capture is the only platform-specific part.

```
src/discord.rs     find Discord's root process
src/capture/       WASAPI process loopback (Windows) · Core Audio taps (macOS, stub)
src/session.rs     preview meters; mix + write on Record
src/mixer.rs       Discord is the timeline; the mic is resampled to match
src/writer.rs      Opus in Ogg, pages committed as they are made
src/ui.rs          Win32 window + tray (Windows only for now)
```

Picking this up cold? [docs/HANDOFF.md](docs/HANDOFF.md) is the operational
entry, then [docs/PROJECT-LOG.md](docs/PROJECT-LOG.md) for why the decisions
are what they are.

| Doc | For |
|---|---|
| [docs/README.md](docs/README.md) | The full document tree |
| [docs/03-architecture.md](docs/03-architecture.md) | How it works |
| [docs/07-roadmap.md](docs/07-roadmap.md) | What to build, in order |
| [docs/CONTRIBUTING-macos.md](docs/CONTRIBUTING-macos.md) | Setting up the Mac side |

## Status

| | |
|---|---|
| Windows app | Works. Open, press Record. |
| macOS | Not built. Needs a Mac. |
| 4-hour drift soak (R6) | Outstanding measurement, not a missing feature |
| Release CPU vs 3% (R11) | Same |

## Licence

MIT
