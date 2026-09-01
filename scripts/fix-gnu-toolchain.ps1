<#
.SYNOPSIS
    Restores the assembler that rustup's windows-gnu toolchain is missing.

.DESCRIPTION
    rustup's self-contained mingw directory ships dlltool.exe but no assembler.
    dlltool shells out to `as` to build import libraries, and it resolves `as`
    RELATIVE TO ITS OWN DIRECTORY -- not via PATH. So having MinGW on PATH does
    not help; the file has to sit next to dlltool.exe.

    Without it, building anything using the `windows` crate fails with:

        error: dlltool could not create import library ...
               dlltool.exe: CreateProcess

    A `rustup update` may remove as.exe again. Re-run this script if that error
    comes back.

    See docs/adr/0009-gnu-toolchain-no-visual-studio.md.
#>

$ErrorActionPreference = 'Stop'

$mingwAs = 'D:\mingw64\bin\as.exe'

if (-not $env:RUSTUP_HOME) { throw "RUSTUP_HOME is not set. Expected D:\rust\rustup." }
if (-not (Test-Path $mingwAs)) {
    throw "MinGW assembler not found at $mingwAs. Install MinGW-w64 to D:\mingw64 (see ADR-0009)."
}

$selfContained = Join-Path $env:RUSTUP_HOME `
    'toolchains\stable-x86_64-pc-windows-gnu\lib\rustlib\x86_64-pc-windows-gnu\bin\self-contained'

if (-not (Test-Path $selfContained)) { throw "Toolchain directory not found: $selfContained" }

Copy-Item $mingwAs (Join-Path $selfContained 'as.exe') -Force
Write-Output "Placed as.exe next to dlltool.exe in $selfContained"

# Verify by actually building an import library.
# Note: dlltool creates the .lib BEFORE invoking the assembler, so the file
# exists even on failure. The exit code is the only reliable signal.
$tmp = Join-Path $env:TEMP 'discrec-toolchain-check'
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
New-Item -ItemType Directory -Force -Path $tmp | Out-Null

$def = Join-Path $tmp 'check.def'
$lib = Join-Path $tmp 'check.lib'
Set-Content -Path $def -Value "LIBRARY kernel32.dll`nEXPORTS`nGetLastError" -Encoding ascii

& (Join-Path $selfContained 'dlltool.exe') `
    -d $def -D kernel32.dll -l $lib -m i386:x86-64 -f --64 --no-leading-underscore

if ($LASTEXITCODE -ne 0) {
    throw "dlltool still failing (exit $LASTEXITCODE). See ADR-0009."
}

Write-Output "Verified: dlltool builds import libraries (exit 0)."
Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue
