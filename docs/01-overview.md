# Overview

## What it is

A small app that records Discord calls. Open it, press record, it finds Discord
and captures the call. Press stop, you get one audio file.

## Why it exists

OBS already does this well, on both target platforms, for free. The gap is not
capability — it is that OBS is a ~200 MB video production suite that needs a
scene and a source configured before it records anything.

DiscRec is that one job as one binary. **The product is the ergonomics and the
footprint**, not the audio capture, which is the same OS API underneath.

If mixed audio in a configured OBS scene is acceptable to you, use OBS. That is
a real recommendation, not modesty. → [09-alternatives.md](09-alternatives.md)

## Goals

1. **Zero setup.** Install, open, press record. No accounts, no scenes, no
   device routing, no virtual cables.
2. **Small.** Under 40 MB idle, under 3% of one core recording. If it is not
   dramatically lighter than OBS it has no reason to exist.
3. **Discord only.** Music, games, and notifications stay out of the recording.
4. **Complete conversation.** Discord's output plus your microphone, mixed, so
   the recording has both sides.
5. **Hard to lose.** A crash leaves a playable file.

## What it records

| Source | In the recording |
|---|---|
| Other people in the call | Yes |
| Your microphone | Yes, mixed in |
| Screenshare / Go Live audio | Yes — it is part of what Discord plays you |
| Your music, games, notifications | No — per-process capture excludes them |

Capturing stream audio is a genuine advantage over the bot-based recorders, none
of which can receive it ([02-constraints.md](02-constraints.md)).

## Non-goals

Each was considered and cut, with the reasoning recorded:

- **Auto-start on joining a call.** Removed the entire join-detection subsystem —
  three imperfect mechanisms, a Discord app registration, and an approval cap.
  → [ADR-0008](adr/0008-manual-control.md)
- **Per-person tracks.** Requires being inside the call as a bot, with hosting
  and server admin rights. → [deferred/](deferred/README.md)
- **Video, transcription, cloud storage, mobile, Linux.**
- **Configuration surface.** A handful of settings, not a preferences system.

## Who it is for

One person recording calls they are in, for their own reference. That keeps the
scope honest: no multi-user concerns, no distribution infrastructure, no
accounts.

## The two things that can go wrong quietly

Both produce files that exist and play, and are wrong. They drive more of the
design than anything else:

1. **Clock drift** between the Discord and microphone streams, baked permanently
   into a mix written at capture time.
2. **Silent capture** — a healthy-looking stream carrying nothing.

→ [05-challenges.md](05-challenges.md)
