# ADR-0001 — Which capture route is primary

**Status: Superseded by [ADR-0008](0008-manual-control.md).** Date: 2026-09-01

> **This decision no longer holds.** Auto-start was removed and mobile was
> dropped, voiding three of the four reasons below. The bot route is not being
> built; its specs are in [deferred/](../deferred/README.md). Retained for the
> reasoning and the DAVE findings, which remain accurate.

**Original decision: Route A (bot) primary, Route B (local capture) as a
required fallback.** The calls that matter happen in servers the user owns or
administers, so a bot can always be invited. Recorded 2026-09-01.

## Context

Two capture routes are viable ([03-architecture.md](../03-architecture.md)):

- **Route A — bot.** Joins the call as an MLS member and receives per-user Opus.
- **Route B — local capture.** Taps the OS audio mixer after decryption.

They differ in ways that cannot be reconciled by engineering:

| | Route A | Route B |
|---|---|---|
| Per-speaker tracks | Yes | No — one mixed stream |
| Works in servers you don't control | **No** — needs invite permission | Yes |
| Works in DMs and group calls | **No** | Yes |
| Auto-start from any device | Yes — gateway event | Desktop only |
| Makes Android useful | Yes — phone as remote | No |
| Can honor opt-out (R13) | Yes | **No** ([P9](../05-challenges.md#p9)) |
| Depends on undocumented Discord behaviour | Yes ([C6](../02-constraints.md)) | No |
| Needs hosting | Yes | No |

## The decision this rested on

**Where do the calls that matter actually happen?** This was never an
engineering judgement — it is a fact about the user, and it has now been
established: **mostly in servers owned or administered by the user.** A bot can
always be invited, so Route A's one disqualifying limitation does not apply.

## Decision

**Route A primary, Route B as a required fallback.**

Reasoning:

1. ~~It is the only route that satisfies the original "Windows, Mac, Android"
   request~~ — void, see Revision below.
2. Per-user tracks are strictly more useful and cannot be recovered later from
   mixed audio. Recording the wrong format is not reversible.
3. It is the only route that can honor an opt-out, which makes R13 achievable
   rather than a stated limitation.
4. The blocker that previously ruled it out is gone — see the reversal below.

Route B is not optional under this recommendation. It covers DMs, group calls,
and guest servers, and it is the fallback that no Discord platform change can
take away ([P8](../05-challenges.md#p8)).

## Reversal of the earlier assessment

An initial assessment concluded Route A was blocked because no library shipped
DAVE receive — `@discordjs/voice` had it broken in the open and py-cord 2.8.0
shipped sending only.

**That conclusion was wrong.** It was true of the mainstream libraries and false
of the ecosystem. `@snazzah/davey` is an MIT-licensed, OpenMLS-based DAVE
implementation published for Rust, Node and Python, and Craig uses it in
production with active commits as of Sept 2026. Route A is viable today.

The cost that remains is operational rather than blocking: MLS epoch transitions
fail often enough that Craig carries explicit recovery logic
([P3](../05-challenges.md#p3)).

→ [research/prior-art-craig.md](../research/prior-art-craig.md)

## Consequences

- Phase 1 becomes bot receive, and can start immediately on installed Node.
- A bot application, a test server, and somewhere to run the bot are needed.
- Route B moves to Phase 4 but is not dropped.
- [ADR-0002](0002-language-and-runtime.md) needs revisiting: if the bot is the
  primary recorder and the Node stack is the proven path, the case for a Rust
  daemon weakens for Route A specifically — though `davey` ships on crates.io,
  so Rust remains open.

## Revision — 2026-09-01, mobile dropped

Reason 1 below ("the only route that satisfies Windows, Mac, Android") **no
longer applies.** Mobile was removed from scope entirely
([ADR-0006](0006-mobile-out-of-scope.md)), so the phone argument is void.

This decision still stands, on the remaining reasons:

- Per-user tracks cannot be recovered from mixed audio later. Recording in the
  wrong format is not reversible.
- Route A is the only route where opt-out (R13) is implementable at all.
- The bot records regardless of which desktop the user is at, and survives them
  switching machines mid-call.

The margin is narrower than it was. If Route A's operational cost turns out to
be high in Phase 1 — failing DAVE transitions, hosting friction — this decision
is worth revisiting rather than defending, because Route B alone is now a
coherent product in a way it was not when a phone client depended on the bot.

## What was given up

Route A cannot record DMs or group calls, and cannot record servers where the
user is only a guest. Route B covers exactly those cases, which is why it is a
**required** fallback rather than an optional extra — the product is incomplete
without it, and it is scheduled at Phase 4 rather than dropped.

Had the answer gone the other way, Route B would have become the whole product:
Phase 4 first, R5 and R13 dropped from v1 with the opt-out limitation surfaced
in the UI ([P9](../05-challenges.md#p9)), and the Android app reduced to a
remote for a desktop that has to be running anyway.
