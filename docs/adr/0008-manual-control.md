# ADR-0008 — Manual control, no auto-start

**Status: Accepted.** Date: 2026-09-01
**Supersedes [ADR-0001](0001-primary-capture-route.md).**

## Context

The original request was for a recorder that starts by itself when you join a
voice channel. That single requirement was responsible for most of the system's
complexity:

- A join-detection subsystem with three mechanisms behind an interface, none
  reliable on its own.
- Discord RPC, which needs a registered Discord application and is capped at 50
  users until Discord approves it — meaning the "zero configuration" promise was
  never actually deliverable.
- A heuristic fallback that misfires on soundboards, voice messages and Go Live.
- A background daemon idling at login, with its own footprint budget, autostart
  registration, and lifecycle.
- Consent machinery, because a recorder that starts on its own captures people
  without an operator present.

Separately, the bot route was chosen as primary partly because auto-start was
free on it. Removing auto-start removes that advantage.

## Decision

**The user opens the app and presses record.** No auto-start, no join detection,
no RPC, no background service.

"Detect Discord" now means locating the process to attach to — a lookup, not
state inference.

## Consequences

**Deleted outright:** `spec/join-detection.md`, the RPC integration and its
approval dependency, the heuristic detector, the background watcher, autostart
registration, and the idle-cost requirement that existed to support it.

**Simplified:** consent handling. The operator is present and chose to record,
so the elaborate announce-and-audit machinery reduces to an honest reminder
([06-legal-and-consent.md](../06-legal-and-consent.md)).

**Collapsed:** the daemon/shell split. It is one process, so the local control
API went too.

**Lost:** recordings do not start unless you remember to start them. That is a
real cost — forgetting to press record is the main way recordings fail to
exist, and it was the strongest argument for the whole auto-start apparatus. It
is accepted deliberately in exchange for a product that is small, predictable,
and has nothing to configure.

## Why this supersedes ADR-0001

ADR-0001 chose the bot route as primary. Three of its four supporting reasons
are now void:

| Reason | Status |
|---|---|
| Only route reaching mobile | Void — mobile dropped ([ADR-0006](0006-mobile-out-of-scope.md)) |
| Auto-start free via gateway events | Void — no auto-start |
| Only route that can honor opt-out | Still true, but opt-out is no longer a requirement |
| Per-person tracks | **Still true** — and now the only remaining argument |

Per-person tracks alone do not justify a hosted bot, a Discord application, and
server admin rights, for a product whose entire premise is that OBS is too much
setup. The bot work is preserved in [deferred/](../deferred/README.md).

## Revisit if

Forgetting to press record turns out to be a frequent, real annoyance in
practice. A narrowly-scoped auto-start — heuristic only, no RPC, opt-in — could
return without bringing back the rest. Do not reintroduce the full subsystem.
