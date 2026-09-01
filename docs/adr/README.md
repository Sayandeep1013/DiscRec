# Architecture decision records

One decision per file. Status is `Proposed`, `Accepted`, or
`Superseded by ADR-nnnn`. Do not build against a `Proposed` ADR without saying
so in the code review.

**Start with [0008](0008-manual-control.md)** — it defines the current product
and supersedes 0001. ADR-0007 governs the capture layer, which is now most of
the codebase.

| ADR | Decision | Status |
|---|---|---|
| [0001](0001-primary-capture-route.md) | Which capture route is primary | ~~Accepted~~ **Superseded by 0007/0008** |
| [0002](0002-language-and-runtime.md) | Rust daemon, no Electron | Proposed |
| [0003](0003-capture-abstraction.md) | One capture interface, two backends | Accepted |
| [0004](0004-storage-opus-passthrough.md) | Store Opus, never decode on the hot path | Accepted |
| [0005](0005-timeline-dual-clock.md) | Position from the stream clock; wall clock for diagnostics only | Accepted |
| [0006](0006-mobile-out-of-scope.md) | Mobile is out of scope entirely | Accepted |
| [0007](0007-cross-platform-strategy.md) | What is shared vs duplicated across Windows/macOS | **Accepted** — native, resolved in Phase 1 |
| [0008](0008-manual-control.md) | Manual control, no auto-start | **Accepted** — supersedes 0001 |
| [0009](0009-gnu-toolchain-no-visual-studio.md) | GNU toolchain on Windows, no Visual Studio | Accepted |
