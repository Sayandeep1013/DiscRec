<#
.SYNOPSIS
    Four-hour drift soak (R6). Needs Discord open and producing audio.

.DESCRIPTION
    Runs the release CLI mixer for 14400 seconds and writes soak.csv every
    30 seconds. Pass is no monotonic trend in smoothed_frames, plus zero
    underruns and zero clamp hits. Analyse smoothed_frames, never raw_frames.

.EXAMPLE
    .\scripts\soak.ps1
#>
param(
    [int]$Seconds = 14400
)

$ErrorActionPreference = 'Stop'
$root = Resolve-Path (Join-Path $PSScriptRoot '..')
$exe = Join-Path $root 'target\release\discrec.exe'

if (-not (Test-Path $exe)) {
    Write-Output "Building release..."
    Push-Location $root
    cargo build --release
    Pop-Location
}

Write-Output "Soak: $Seconds s (~$([math]::Round($Seconds/3600, 2)) h) -> mixed.ogg + soak.csv"
Write-Output "Leave Discord in a call. Analyse smoothed_frames, not raw_frames."
Write-Output ""

Push-Location $root
& $exe $Seconds --mix --log
$code = $LASTEXITCODE
Pop-Location
exit $code
