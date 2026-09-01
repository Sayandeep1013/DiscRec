# ADR-0009 — GNU toolchain on Windows, no Visual Studio

**Status: Accepted.** Date: 2026-09-01

## Context

Rust on Windows has two host targets:

- `x86_64-pc-windows-msvc` — the conventional default. Requires Visual Studio
  Build Tools and the Windows SDK.
- `x86_64-pc-windows-gnu` — MinGW-w64 based. rustup ships it self-contained.

The development machine has a hard constraint: **no tooling on the C: drive.**
Everything reusable lives directly under `D:\`, project-specific work stays in
the repository.

MSVC cannot satisfy that. The Visual Studio installer honors `--installPath` for
its own payload, but the Windows SDK installs to
`C:\Program Files (x86)\Windows Kits` regardless, and the installer plus its
package cache land on C:. Several GB, with no supported relocation.

## Decision

**Target `x86_64-pc-windows-gnu`.** No Visual Studio, no Windows SDK.

| Path | Contents |
|---|---|
| `D:\rust\cargo` | `CARGO_HOME` — binaries, registry cache |
| `D:\rust\rustup` | `RUSTUP_HOME` — toolchains |
| `D:\mingw64` | MinGW-w64 16.2.0 posix-seh-msvcrt — see below |
| `D:\Projects\DiscRec\target` | Build artifacts, project-local |

Nothing on C:. Verified: no `.cargo` or `.rustup` in the user profile.

## Why this works for WASAPI

The obvious objection is that Windows audio work needs the Windows SDK headers —
`mmdeviceapi.h`, `audioclient.h`, and the process-loopback definitions
(`AUDIOCLIENT_ACTIVATION_PARAMS`, `VIRTUAL_AUDIO_DEVICE_PROCESS_LOOPBACK`)
added in build 20348.

It does not, because we use the **`windows` crate**, which generates bindings
from Windows *metadata* rather than C headers. It ships its own definitions,
supports both `-msvc` and `-gnu` targets, and covers the process-loopback APIs.
MinGW-w64's own headers, which do lag, are never consulted.

Verified: `windows` 0.62.2 compiles cleanly on this toolchain.

## The one thing that broke, and why

The predicted risk materialised on the first real dependency. Worth recording
precisely, because the obvious diagnosis is wrong.

**Symptom.** `cargo build` with the `windows` crate:

```
error: error calling dlltool 'dlltool.exe': program not found
```

and once found on PATH:

```
error: dlltool could not create import library ...
       dlltool.exe: CreateProcess
```

**Cause.** rustup's self-contained directory ships only four files —
`dlltool.exe`, `ld.exe`, `libwinpthread-1.dll`, `x86_64-w64-mingw32-gcc.exe` —
and **no assembler.** `dlltool` shells out to `as` to build import libraries.

The non-obvious part: **`dlltool` resolves `as` relative to its own executable
directory, not via PATH.** Installing MinGW-w64 and putting `D:\mingw64\bin` on
PATH did *not* fix it. That misleads you into concluding rustup's `dlltool` is
broken. It is not — an earlier revision of this ADR claimed exactly that, and
was wrong.

**Fix — one file:**

```
copy D:\mingw64\bin\as.exe  ->  <toolchain>\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained\
```

rustup's own `dlltool.exe` then works, producing import libraries byte-identical
to MinGW's. No binary substitution, nothing patched.

`scripts\fix-gnu-toolchain.ps1` does this and verifies it.

**A `rustup update` may remove `as.exe` again.** The symptom is the
`CreateProcess` error above; re-run the script.

**Verification trap:** `dlltool` creates the output `.lib` file *before*
invoking the assembler, so the file exists even when the run fails. Check the
exit code, never the file's existence.

## Why MinGW-w64 is installed at all

Only `as.exe` is strictly required, but the full toolchain is kept because:

- Phase 2's Opus encoder is a binding to a C library and needs a C compiler.
  That was a separate listed risk; it is now pre-solved.
- `D:\mingw64\bin` is appended to PATH **last**, so rustup's own `ld` and `gcc`
  are never shadowed.

789 MB. The flavour (posix-seh-msvcrt) matches rustup's bundled gcc, which
reports itself as `x86_64-posix-seh-rev2, Built by MinGW-Builds project 14.2.0`.

## Remaining risks

This is the less-travelled path on Windows; most documentation and CI examples
assume MSVC.

| Risk | Status |
|---|---|
| A dependency assumes MSVC or ships MSVC-only prebuilt artifacts | **Occurred once**, resolved above. Expect it again with C-dependent crates |
| Linker differences produce obscure errors | Not yet seen. Errors will be at link time and specific |
| Weaker debugger tooling than MSVC | Accepted. `gdb` works and the codebase is small |

**Fallback if GNU ever blocks progress:** `xwin` fetches MSVC headers and
libraries into a D: directory without the Visual Studio installer, keeping the
C: constraint intact. Not needed so far — do not pre-emptively switch.

## Effect on macOS

None. macOS builds with clang from the Xcode Command Line Tools. A contributor
there follows [CONTRIBUTING-macos.md](../CONTRIBUTING-macos.md) unchanged; only
`src/capture/windows.rs` is compiled under this toolchain.
