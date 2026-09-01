# DiscRec documentation

Written Sept 2026. Platform API and library claims have a shelf life; see
`research/` for what was checked and when.

## Understand it

| Doc | Answers |
|---|---|
| [01-overview.md](01-overview.md) | What it is, what it deliberately isn't |
| [02-constraints.md](02-constraints.md) | What the platforms allow. Read before designing anything |
| [03-architecture.md](03-architecture.md) | The four parts and how audio moves between them |
| [09-alternatives.md](09-alternatives.md) | Why not OBS — and when you should just use OBS |

## Build it

| Doc | Answers |
|---|---|
| [04-requirements.md](04-requirements.md) | R1–R14, the acceptance surface |
| [05-challenges.md](05-challenges.md) | P1–P6, the defects that cost weeks, with fixes |
| [06-legal-and-consent.md](06-legal-and-consent.md) | Recording other people |
| [07-roadmap.md](07-roadmap.md) | Phases with exit criteria |
| [08-toolchain-and-gaps.md](08-toolchain-and-gaps.md) | What's installed, missing, and unverifiable |
| [CONTRIBUTING-macos.md](CONTRIBUTING-macos.md) | **Onboarding for the macOS contributor** |

## Decisions — `adr/`

Numbered, dated, with status. [Index](adr/README.md).

## Specifications — `spec/`

Implementation detail, one per component. [Index](spec/README.md).

## Research — `research/`

Evidence with sources and check dates. Specs cite these rather than restating
platform facts.

## Deferred — `deferred/`

Specced, researched, not being built. [Index](deferred/README.md). The bot and
DAVE material lives here.
