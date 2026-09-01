# DiscRec

A small app that records your Discord calls. Open it, press record, it finds
Discord and captures the call. Press stop, you get a file.

Not a background service. Not a bot. Not a scene collection.

**Status: specification. Implementation starting.**
**Targets: Windows and macOS.**

---

## Why this exists

OBS can already do this — Application Audio Capture pointed at Discord, on both
platforms. It is free and excellent.

It is also ~200 MB, needs a scene and source configured before it records
anything, and is a video production suite you are using for one small job.

DiscRec is that one job, as one binary you press a button in.

→ [docs/09-alternatives.md](docs/09-alternatives.md) for the honest comparison,
including when you should just use OBS instead.

## What it does

- Captures **Discord's audio only** — your music, game and notifications stay out
- Mixes in **your microphone**, so the recording is a whole conversation
- Captures **screenshare and Go Live audio**, because it records what you hear
- Writes one Ogg/Opus file per session
- Survives a crash with a playable file

## What it deliberately does not do

Auto-start, per-person tracks, video, transcription, cloud anything, mobile.
Each was considered and cut; the reasoning is in
[docs/04-requirements.md](docs/04-requirements.md) and `docs/adr/`.

## Start here

**Picking this up cold?** Start with
[docs/HANDOFF.md](docs/HANDOFF.md) — current state, how to run it, what to do
next. Then [docs/PROJECT-LOG.md](docs/PROJECT-LOG.md) for why the decisions are
what they are.

| Doc | For |
|---|---|
| [docs/README.md](docs/README.md) | The full document tree |
| [docs/03-architecture.md](docs/03-architecture.md) | How it works, in four parts |
| [docs/07-roadmap.md](docs/07-roadmap.md) | What to build, in order |
| [docs/CONTRIBUTING-macos.md](docs/CONTRIBUTING-macos.md) | **Setting up the Mac side** |

## Building

One repository, one codebase. The platform-specific part is the capture backend
and nothing else — two files behind one trait, selected at compile time.

```bash
cargo build --release      # builds for whatever OS you are on
```

Windows needs the MSVC toolchain; macOS needs Xcode Command Line Tools and
macOS 14.2+. Full setup for macOS contributors is in
[docs/CONTRIBUTING-macos.md](docs/CONTRIBUTING-macos.md).

## Recording other people

This records everyone in the call. Discord's Terms require you to tell them, and
many places require their consent. The app cannot do that for you — it has no
way to speak in the channel. Say it out loud before you press record.

→ [docs/06-legal-and-consent.md](docs/06-legal-and-consent.md)
