# ADR-0009 — GNU toolchain on Windows, no Visual Studio

**Status: Accepted.** Date: 2026-09-01

## Context

Rust on Windows has two host targets:

- `x86_64-pc-windows-msvc` — the conventional default. Requires Visual Studio
  Build Tools and the Windows SDK.
- `x86_64-pc-windows-gnu` — MinGW-w64 based. rustup ships it self-contained;
  nothing else needs installing.

The development machine has a hard constraint: **no tooling on the C: drive.**
Everything reusable lives directly under `D:\`, project-specific work stays in
the repository.

MSVC cannot satisfy that. The Visual Studio installer honors `--installPath`
for its own payload, but the Windows SDK installs to
`C:\Program Files (x86)\Windows Kits` regardless, and the installer itself plus
its package cache land on C:. That is several GB with no supported relocation.

## Decision

**Target `x86_64-pc-windows-gnu`.** No Visual Studio, no Windows SDK.

Installed layout:

| Path | Contents |
|---|---|
| `D:\rust\cargo` | `CARGO_HOME` — binaries, registry cache |
| `D:\rust\rustup` | `RUSTUP_HOME` — toolchains |
| `D:\Projects\DiscRec\target` | Build artifacts, project-local |

Total footprint 0.88 GB, entirely on D:. Verified: no `.cargo` or `.rustup` in
the user profile, and no SDK or Build Tools installed by this setup.

## Why this is viable for WASAPI

The obvious objection is that Windows audio work needs the Windows SDK headers —
`mmdeviceapi.h`, `audioclient.h`, and the newer process-loopback definitions
(`AUDIOCLIENT_ACTIVATION_PARAMS`, `VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK`)
introduced with build 20348.

It does not, because we use the **`windows` crate**, which generates bindings
from Windows *metadata* rather than from C headers. It ships its own definitions,
supports both `-msvc` and `-gnu` targets, and covers the process-loopback APIs.
MinGW-w64's own headers, which do lag behind, are never consulted.

## Verified

```
rustc 1.98.0 (x86_64-pc-windows-gnu)
cargo build            clean
cargo clippy -D warnings   clean
cargo fmt --check      clean
cargo run              runs
```

## Risks

**This is the less-travelled path on Windows.** Most Rust-on-Windows
documentation, CI examples and Stack Overflow answers assume MSVC.

| Risk | Likelihood | Response |
|---|---|---|
| A dependency assumes MSVC or ships only MSVC-compatible prebuilt artifacts | Moderate — audio crates with C dependencies are the likely candidates | Prefer pure-Rust crates. `windows` and `opus` bindings both build under GNU |
| Linker differences produce obscure errors | Low, but painful when it happens | The error will be at link time and specific; check the crate's GNU support first |
| Debugger tooling is weaker than MSVC's | Certain | Acceptable — `gdb` works, and this codebase is small |

**Fallback if GNU blocks Phase 1:** install Build Tools and accept the C: usage,
or use `xwin` to fetch the MSVC headers and libraries into a D: directory
without the Visual Studio installer. Neither is needed unless something actually
breaks — do not pre-emptively switch.

## Effect on macOS

None. macOS uses clang from the Xcode Command Line Tools and is unaffected by
this decision. A contributor there follows
[CONTRIBUTING-macos.md](../CONTRIBUTING-macos.md) unchanged.

The shared code is target-agnostic; only `src/capture/windows.rs` is compiled
under this toolchain.
