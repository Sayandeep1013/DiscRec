# ADR-0004 — Store Opus

**Status: Accepted, amended 2026-09-01.** Date: 2026-09-01

> **Amendment:** the passthrough half of this decision applied to the bot route,
> which received Opus already encoded. Local capture yields PCM, so DiscRec
> encodes once, at capture time. The conclusion — store Opus, never WAV — is
> unchanged, and the reasoning below still holds for why.

## Context

Discord transmits Opus at 48 kHz, typically 64 kbps. A recorder can either
decode to PCM and re-encode, or write the received Opus packets through
untouched.

Decoding and re-encoding costs CPU proportional to speaker count, adds
generation loss, and buys nothing — the source is already a good lossy codec at
a sane bitrate.

## Decision

- **Route A writes received Opus packets directly into Ogg pages.** No decode,
  no re-encode. CPU cost is approximately zero regardless of how many people are
  talking, which is what makes R9 comfortable rather than tight.
- **Route B encodes once**, from captured PCM to Opus, because it has no choice —
  the OS hands over PCM.
- **Decoding happens only on export** (R17), never during capture.
- **Never store WAV as the primary artifact.** Roughly 10× the size for no
  benefit; offered as an export target only.

## Storage consequences

~29 MB per stream-hour at 64 kbps, continuous. Silence suppression on Route A
reduces this substantially in practice, since Discord sends nothing while a user
is quiet — but capacity planning should assume the continuous figure
([P6](../05-challenges.md#p6)).

A weekly 4-hour call with five participants, per-track: ~580 MB.

## Consequences

- Retention policy is required at v1, not later. A recorder that silently fills
  a disk is a bug.
- Export is a separate operation with its own cost, and needs ffmpeg
  ([08-toolchain-and-gaps.md](../08-toolchain-and-gaps.md)).
- Route A recordings are bit-identical to what was transmitted, which is the
  best possible archival outcome.
