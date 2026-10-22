<#
.SYNOPSIS
    Installs discrec.exe and adds DiscRec to the Start menu, so searching
    "DiscRec" in Windows finds and opens it.

.DESCRIPTION
    Copies the exe to a per-user folder (no admin rights) and creates a Start
    menu shortcut that uses the exe's embedded logo. Re-run it to update to a
    newer build; pass -Uninstall to remove both.

.EXAMPLE
    .\scripts\install-windows.ps1                              # installs target\release\discrec.exe
    .\scripts\install-windows.ps1 -Exe .\discrec.exe           # a downloaded release exe
    .\scripts\install-windows.ps1 -InstallDir D:\Apps\DiscRec  # somewhere other than C:
    .\scripts\install-windows.ps1 -Uninstall
#>
param(
    [string]$Exe = (Join-Path $PSScriptRoot '..\target\release\discrec.exe'),
    [string]$InstallDir = (Join-Path $env:LOCALAPPDATA 'Programs\DiscRec'),
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
$shortcut = Join-Path ([Environment]::GetFolderPath('Programs')) 'DiscRec.lnk'
$target = Join-Path $InstallDir 'discrec.exe'

if ($Uninstall) {
    Remove-Item $shortcut -ErrorAction SilentlyContinue
    Remove-Item $target -ErrorAction SilentlyContinue
    if ((Test-Path $InstallDir) -and -not (Get-ChildItem $InstallDir)) { Remove-Item $InstallDir }
    Write-Output "Removed DiscRec from the Start menu and $InstallDir"
    return
}

if (-not (Test-Path $Exe)) { throw "No exe at $Exe. Run 'cargo build --release' or pass -Exe <path>." }
if (Get-Process discrec -ErrorAction SilentlyContinue) { throw "DiscRec is running. Close it, then re-run." }

New-Item -ItemType Directory -Force -Path $InstallDir | Out-Null
Copy-Item $Exe $target -Force

$lnk = (New-Object -ComObject WScript.Shell).CreateShortcut($shortcut)
$lnk.TargetPath = $target
$lnk.WorkingDirectory = $InstallDir
$lnk.IconLocation = "$target,0"
$lnk.Description = 'Record your Discord calls'
$lnk.Save()

Write-Output "Installed $target"
Write-Output "Start menu: $shortcut  (search 'DiscRec')"
