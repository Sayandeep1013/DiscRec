# Toolchain and gaps

Checked on the primary development machine (Windows 11), 1 Sept 2026.

## Present

| Tool | Version |
|---|---|
| git | 2.51.0 |
| gh | 2.97.0 |
| Node | 22.19.0 |
| Python | 3.12.1 |

Node and Python are not needed by the product — they were used for research and
tooling. The shipped binary has no runtime dependency (R12).

## Needed for Phase 0

| Tool | Why | How |
|---|---|---|
| **Rust + cargo** | The entire application | `winget install Rustlang.Rustup` |
| **MSVC build tools + Windows SDK** | Rust's MSVC target, and the WASAPI headers | Visual Studio Build Tools, "Desktop development with C++" |

Neither is currently installed. Together they are the only thing between here
and Phase 1.

## Needed later

| Tool | Phase | Why |
|---|---|---|
| ffmpeg | Testing | Generating sync-tone fixtures and verifying output |
| Audacity or similar | Phase 2 | Visual confirmation of drift in the soak test |

## Cannot be done on this machine

**macOS (Phase 4) is unbuildable and unverifiable here** — and it is half the
product. Core Audio process taps, TCC prompts, signing and notarization all
require Mac hardware, plus a paid Apple Developer account (~$99/yr) for
distribution.

This is the largest known risk in the plan. Mitigation: a contributor with a Mac
clones the repository and implements one file against an existing trait
([CONTRIBUTING-macos.md](CONTRIBUTING-macos.md)). The rest of the application
compiles and runs for them unchanged.

Consequences to accept:

- [spec/capture-macos.md](spec/capture-macos.md) is written from Apple's
  documentation and the AudioCap reference. It is **unverified**; treat it as a
  starting point and expect revision.
- Its open questions — particularly whether tapped streams arrive attenuated —
  can only be answered on hardware.
- Notarization cannot be tested without the developer account.

## MCP servers

None are needed. The connected servers — chrome-devtools, playwright, mint,
supabase, vercel — are browser, 3D-asset and web-hosting oriented, and this is
native audio work with no browser or backend anywhere in it.

The gaps are toolchain and hardware: Rust, MSVC, and access to a Mac. No tooling
integration changes that.
