<#
.SYNOPSIS
  Builds fulltalk dev and installs it for the current user (no admin needed).

.DESCRIPTION
  Copies the app to %LOCALAPPDATA%\Programs\fulltalk dev and adds a Start Menu
  shortcut, so it shows up in Windows Search like any other application.
  Running it again updates the installed copy.

.PARAMETER Desktop
  Also create a shortcut on the desktop.

.PARAMETER Uninstall
  Remove the app and its shortcuts. Settings, history and the model stay.

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\install-windows.ps1

.EXAMPLE
  powershell -ExecutionPolicy Bypass -File scripts\install-windows.ps1 -Desktop
#>
[CmdletBinding()]
param(
    [switch]$Desktop,
    [switch]$Uninstall
)

$ErrorActionPreference = 'Stop'
Set-Location (Split-Path -Parent $PSScriptRoot)

$name = 'fulltalk dev'
$installDir = Join-Path $env:LOCALAPPDATA "Programs\$name"
$exe = Join-Path $installDir 'fulltalk.exe'
$ico = Join-Path $installDir 'fulltalk.ico'
$startMenuLink = Join-Path ([Environment]::GetFolderPath('Programs')) "$name.lnk"
$desktopLink = Join-Path ([Environment]::GetFolderPath('DesktopDirectory')) "$name.lnk"
$runKey = 'HKCU:\Software\Microsoft\Windows\CurrentVersion\Run'

# Windows cannot replace or delete an .exe that is running.
function Stop-InstalledApp {
    $running = Get-Process -Name fulltalk -ErrorAction SilentlyContinue |
        Where-Object { $_.Path -eq $exe }
    if ($running) {
        Write-Host "Closing the running $name..."
        $running | Stop-Process -Force
        $running | Wait-Process -Timeout 10 -ErrorAction SilentlyContinue
    }
}

function New-AppShortcut([string]$Path) {
    $shell = New-Object -ComObject WScript.Shell
    $link = $shell.CreateShortcut($Path)
    $link.TargetPath = $exe
    $link.WorkingDirectory = $installDir
    $link.IconLocation = "$ico,0"
    $link.Description = 'Local push-to-talk dictation. Code is cheap, show me the talk.'
    $link.Save()
}

if ($Uninstall) {
    Stop-InstalledApp
    Remove-Item $startMenuLink, $desktopLink -Force -ErrorAction SilentlyContinue
    Remove-ItemProperty -Path $runKey -Name $name -ErrorAction SilentlyContinue
    if (Test-Path $installDir) { Remove-Item $installDir -Recurse -Force }
    Write-Host "Removed $name. Settings, history and the model were left in place."
    return
}

cargo build --release -p fulltalk
if ($LASTEXITCODE -ne 0) { throw "cargo build failed (exit code $LASTEXITCODE)" }

Stop-InstalledApp
New-Item -ItemType Directory -Path $installDir -Force | Out-Null
Copy-Item 'target\release\fulltalk.exe' $exe -Force
Copy-Item 'packaging\windows\fulltalk.ico' $ico -Force

New-AppShortcut $startMenuLink
if ($Desktop) { New-AppShortcut $desktopLink }

Write-Host "Installed to $installDir"
Write-Host "Press the Windows key and type ""fulltalk"" to open it."
