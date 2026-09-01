# Specifications

Implementation detail. Each names the requirements it satisfies and the
challenges it addresses, and cites `../research/` rather than restating platform
facts.

## Capture — the only platform-specific layer

| Spec | Covers | Status |
|---|---|---|
| [capture-interface.md](capture-interface.md) | The trait both backends implement, and the repo layout | Current |
| [capture-windows.md](capture-windows.md) | WASAPI process loopback | Researched — documented API + vendor sample |
| [capture-macos.md](capture-macos.md) | Core Audio process taps | **Unverified** — written without Mac hardware |

## Core — shared across both platforms

| Spec | Covers |
|---|---|
| [mixing-and-timeline.md](mixing-and-timeline.md) | Drift compensation, summing, limiting. R3, R6, R9 — **the hardest part** |
| [storage-format.md](storage-format.md) | Ogg/Opus, incremental writes, crash safety. R7, R14 |
| [desktop-shell.md](desktop-shell.md) | Window, record button, meters, tray. R13, R15 |
| [configuration.md](configuration.md) | The few settings that exist |
| [diagnostics.md](diagnostics.md) | Logging and the error taxonomy. R8, R16 |
| [test-plan.md](test-plan.md) | How every requirement is actually verified |

## Reading order for a new contributor

[capture-interface.md](capture-interface.md) first — it defines the seam. Then
the backend for your platform. Then
[mixing-and-timeline.md](mixing-and-timeline.md), because it explains what the
backend's output has to be correct about.
