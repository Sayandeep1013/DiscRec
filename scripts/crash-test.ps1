<#
.SYNOPSIS
    Verifies R7: killing the recorder must leave a playable file.

.DESCRIPTION
    Starts a recording, kills it at a random offset with no chance to clean up
    (Stop-Process -Force, the Windows equivalent of SIGKILL), then checks the
    resulting file actually decodes.

    "The file exists" is not the test. A truncated Ogg page, a missing header,
    or a back-patched length field all produce files that exist and fail to
    play. Only a real decode proves it.

.PARAMETER Runs
    How many kill cycles. The spec calls for 100; fewer is fine for a smoke
    check.

.EXAMPLE
    .\scripts\crash-test.ps1 -Runs 25
#>
param(
    [int]$Runs = 25,
    [double]$MinSeconds = 1.5,
    [double]$MaxSeconds = 6.0
)

$ErrorActionPreference = 'Stop'

$exe = Join-Path $PSScriptRoot '..\target\debug\discrec.exe' | Resolve-Path
$work = Join-Path $env:TEMP 'discrec-crash-test'
New-Item -ItemType Directory -Force -Path $work | Out-Null

$ffmpeg = Get-Command ffmpeg -ErrorAction SilentlyContinue
if (-not $ffmpeg) { throw "ffmpeg not on PATH; needed to verify the files decode." }

$results = [ordered]@{
    playable  = 0
    unplayable = 0
    empty     = 0
}
$failures = @()

Write-Output "Crash test: $Runs kill cycles, killing between $MinSeconds and $MaxSeconds s"
Write-Output ""

for ($i = 1; $i -le $Runs; $i++) {
    Push-Location $work
    # Ask for a long recording so the kill always lands mid-stream.
    $p = Start-Process -FilePath $exe -ArgumentList '60', '--mix' `
        -PassThru -WindowStyle Hidden -RedirectStandardOutput "$work\out.txt" `
        -RedirectStandardError "$work\err.txt"
    Pop-Location

    $delay = Get-Random -Minimum $MinSeconds -Maximum $MaxSeconds
    Start-Sleep -Seconds $delay

    if (-not $p.HasExited) { Stop-Process -Id $p.Id -Force }
    Start-Sleep -Milliseconds 300

    $file = Join-Path $work 'mixed.ogg'
    $size = if (Test-Path $file) { (Get-Item $file).Length } else { 0 }

    if ($size -eq 0) {
        $results.empty++
        $failures += "run $i (killed at ${delay}s): no file or zero bytes"
    } else {
        # Decode the whole thing. Any error means the file is not playable.
        & ffmpeg -hide_banner -v error -i $file -f null NUL 2> "$work\dec.txt"
        $errText = Get-Content "$work\dec.txt" -Raw -ErrorAction SilentlyContinue
        if ([string]::IsNullOrWhiteSpace($errText)) {
            $results.playable++
            Write-Output ("  run {0,3}  killed {1,5:N1}s  {2,8} bytes  OK" -f $i, $delay, $size)
        } else {
            $results.unplayable++
            $first = ($errText -split "`n")[0].Trim()
            $failures += "run $i (killed at ${delay}s, $size bytes): $first"
            Write-Output ("  run {0,3}  killed {1,5:N1}s  {2,8} bytes  FAILED" -f $i, $delay, $size)
        }
    }

    Remove-Item $file -Force -ErrorAction SilentlyContinue
}

Write-Output ""
Write-Output "playable   : $($results.playable) / $Runs"
Write-Output "unplayable : $($results.unplayable)"
Write-Output "empty      : $($results.empty)"

if ($failures.Count -gt 0) {
    Write-Output ""
    Write-Output "Failures:"
    $failures | ForEach-Object { Write-Output "  $_" }
    exit 1
}

Write-Output ""
Write-Output "R7 satisfied: every killed recording decoded."
