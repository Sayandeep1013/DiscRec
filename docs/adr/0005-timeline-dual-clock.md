# ADR-0005 — Position from the stream clock, wall clock for diagnostics only

**Status: Accepted.** Date: 2026-09-01

## Context

[P1](../05-challenges.md#p1) is the defect that quietly ruins Discord
recordings: because Discord transmits nothing during silence, appending packets
in arrival order compresses each speaker's timeline by the length of their
pauses. Tracks drift apart, nothing errors, and the files play — incorrectly.

Two clocks are available:

- The **stream clock** — the RTP timestamp (Route A), a 48 kHz counter carried
  in every packet, or the device sample offset (Route B).
- The **wall clock** — when the process received the data.

## Decision

**Frame position always comes from the stream clock.** Gaps are filled by
padding exactly the missing sample count, computed from the timestamp
difference. `Frame` carries no arrival-time field, so the wrong clock is not
reachable from the write path.

**The wall clock is recorded alongside, for diagnostics only.** Craig does this
— an `hrtime` reading kept next to the RTP timestamp — and it is worth copying:
divergence between the two clocks is how you detect that the stream clock is
lying, a socket has stalled, or a DAVE transition dropped audio
([P3](../05-challenges.md#p3)).

Out-of-order packets are buffered briefly and sorted by timestamp before being
written, rather than being written in arrival order or dropped.

## Verification

This decision is only real if it is tested. R6 requires a 4-hour soak with a
periodic sync tone across at least three speakers, asserting alignment within
50ms. That test exists in Phase 2 and gates it — before any recordings worth
keeping are made, because recordings made under a broken timeline cannot be
repaired afterwards.

## Consequences

- Phase 2 cannot be declared done on a short manual check. The soak is the exit
  criterion.
- The manifest records both clocks per track, so drift is diagnosable after the
  fact rather than only reproducible live.
