# ADR-0006 — Mobile is out of scope

**Status: Accepted.** Date: 2026-09-01

## Context

The project was originally scoped as "Windows, Mac, Android". Two findings
changed that, in two steps.

**Step 1 — Android cannot record.** Android's `AudioPlaybackCapture` API (API
29+) admits only players whose usage is `USAGE_MEDIA`, `USAGE_GAME`, or
`USAGE_UNKNOWN`. Discord voice plays as `USAGE_VOICE_COMMUNICATION` — the same
category as phone calls, excluded deliberately for privacy. No permission,
entitlement, or `MediaProjection` configuration lifts this. iOS has no
system-audio capture API at all.

This ADR originally concluded that Android should therefore ship as a *control
surface* — a remote for a recorder running elsewhere.

**Step 2 — that was dropped too.** Targets are now **Windows and macOS only**.
A companion app that cannot record, for a recorder the user is already sitting
in front of, was not worth its own build toolchain, store presence, and
maintenance.

## Decision

No mobile client of any kind. No Android app, no iOS app, no phone control
surface.

The control API was later removed entirely along with the daemon/shell split —
DiscRec is a single process ([ADR-0008](0008-manual-control.md)).

## Consequences

- `spec/android-companion.md` is deleted.
- R18 and R19 are dropped from the requirements.
- No JDK, Android SDK, or Gradle needed
  ([08-toolchain-and-gaps.md](../08-toolchain-and-gaps.md)).
- No control API, no IPC, no auth model — subsequently collapsed into one
  process by [ADR-0008](0008-manual-control.md).

## Effect on ADR-0001

[ADR-0001](0001-primary-capture-route.md) chose the bot route partly *because*
it was the only route that made a phone useful. Removing mobile voided that
reason and narrowed the margin; [ADR-0008](0008-manual-control.md) later voided
two more and superseded it entirely.

## If mobile ever returns

Only as a control surface for a server-side recorder — a phone can start and
manage a recording elsewhere, but can never capture audio itself. Since the bot
route was also dropped, there is nothing for it to control. The capture
constraint is a platform decision by Google and Apple and is not expected to
change.
