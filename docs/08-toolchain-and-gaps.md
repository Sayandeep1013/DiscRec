# Toolchain and gaps

Checked on the primary development machine (Windows 11), 1 Sept 2026.

## Installed — Phase 0 is complete

Everything lives on `D:`. Nothing was installed to `C:`.

| Tool | Version | Location |
|---|---|---|
| **Rust** | 1.98.0 `x86_64-pc-windows-gnu` | `D:ustustup` |
| **cargo** | 1.98.0 | `D:ust\cargo` |
| clippy / rustfmt | 0.1.98 / 1.9.0 | ” |
| ffmpeg | 7.1.1 | `D:fmpeg\...in` (pre-existing, added to PATH) |
| git | 2.51.0 | pre-existing |
| gh | 2.97.0 | pre-existing |

Environment (user scope): `CARGO_HOME=D:ust\cargo`,
`RUSTUP_HOME=D:ustustup`; `D:ust\cargoin` and the ffmpeg `bin` on
PATH. Build artifacts go to the project-local `target/`. Total Rust footprint
0.88 GB.

**No Visual Studio, no Windows SDK.** The GNU toolchain is self-contained and
the `windows` crate generates its bindings from Windows metadata rather than SDK
headers. Reasoning and risks: [ADR-0009](adr/0009-gnu-toolchain-no-visual-studio.md).

Verified working:

```
cargo build              clean
cargo clippy -D warnings clean
cargo fmt --check        clean
cargo run                runs
```

Node and Python are present but are not product dependencies — they were used
for research. The shipped binary has no runtime dependency (R12).

## Still useful later

| Tool | Phase | Why |
|---|---|---|
| Audacity | Phase 2 | Visual confirmation of drift in the soak test. Already at `D:udacity` |

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
