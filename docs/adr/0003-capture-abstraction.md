# ADR-0003 — One capture interface, two backends

**Status: Accepted.** Date: 2026-09-01

## Context

Route A and Route B differ in almost every respect — transport, encryption,
per-user separation, platform, failure modes. What they have in common is that
both ultimately produce timestamped audio frames belonging to one or more
tracks.

Everything downstream of that — timeline reconstruction, gap filling, storage,
manifests, consent state, the control API, export — is identical between them.
Writing it twice would mean fixing [P1](../05-challenges.md#p1) twice, and
getting it wrong in one of them.

## Decision

A single `CaptureBackend` trait, defined in
[03-architecture.md](../03-architecture.md#the-capture-interface). Both routes
implement it. Nothing downstream knows which one is running, except:

- the session manifest, which records the route because the artifacts genuinely
  differ; and
- `set_opt_out`, which returns `Unsupported` on Route B.

`Frame` carries the source's own position — RTP timestamp or sample offset — and
deliberately has **no arrival-time field**, so [P1](../05-challenges.md#p1) is
awkward to write by accident.

## Why `set_opt_out` returns a result rather than being infallible

Mixed audio cannot have one participant removed ([P9](../05-challenges.md#p9)).
A trait method returning `()` would force Route B to either lie or panic. An
explicit `Unsupported` makes the limitation visible at the type level and forces
the UI to handle it, which is the only honest outcome.

## Consequences

- Route B can be built in Phase 4 and reuse all of Phases 2 and 3.
- A future Route C — should Discord ever document voice receive properly, or
  should a platform provide something new — plugs in the same way.
- The trait must stay narrow. Route-specific behaviour belongs behind it, not
  leaked through it as configuration flags.
