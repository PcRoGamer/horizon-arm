<#
.SYNOPSIS
Launch the native Windows ARM64 Horizon debug build with the Android
touch controls visibly rendered by Horizon's own GPU/HUD pipeline.

.DESCRIPTION
The controls are a visual debug mirror of Android's Java Canvas overlay.
They do not translate mouse events into virtual touch on Windows. No release
build includes this preview (Rust debug_assertions only).
#>
[CmdletBinding()]
param(
    [ValidateRange(0, 3)][int]$Scheme = 1,
    [ValidateRange(80, 120)][int]$Scale = 100,
    [ValidateRange(35, 100)][int]$Opacity = 72,
    [switch]$Title,
    [switch]$Capture,
    [string]$World = 'ARM64DebugTest'
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$choices = @(
    (Join-Path $repo 'target\aarch64-pc-windows-msvc\debug\horizon-client.exe'),
    (Join-Path $repo 'target-windows-arm64-debug\aarch64-pc-windows-gnullvm\debug\horizon-client.exe')
)
$exe = $choices | Where-Object { Test-Path $_ } | Select-Object -First 1
if (-not $exe) {
    throw 'No Windows ARM64 Horizon debug executable found. Compile on Windows ARM64 or extract the CI artifact into target/aarch64-pc-windows-msvc/debug.'
}
$bytes = [IO.File]::ReadAllBytes($exe)
$peOffset = [BitConverter]::ToInt32($bytes, 0x3c)
$machine = [BitConverter]::ToUInt16($bytes, $peOffset + 4)
if ($machine -ne 0xAA64) {
    throw 'The chosen executable is not native Windows ARM64 (expected PE machine 0xAA64).'
}
$resources = Join-Path $repo '.local\horizon-client\26.3'
if (-not (Test-Path (Join-Path $resources 'horizon-resources.json'))) {
    throw "Missing game resources: $resources. Run python tools/windows_arm_debug_assets.py after obtaining Horizon-generated engine tables."
}
$game = Join-Path $repo 'artifacts\windows-arm64-debug\game'
New-Item -ItemType Directory -Force $game | Out-Null
$env:HORIZON_TOUCH_PREVIEW = '1'
$env:HORIZON_TOUCH_PREVIEW_SCHEME = [string]$Scheme
$env:HORIZON_TOUCH_PREVIEW_SCALE = [string]$Scale
$env:HORIZON_TOUCH_PREVIEW_OPACITY = [string]$Opacity
$env:HORIZON_TOUCH_PREVIEW_TITLE = if ($Title) { '1' } else { '0' }
$env:HORIZON_HOME = $game

$flags = @('--resources', $resources, '--game-dir', $game, '--width', '1600', '--height', '900')
if ($Title) {
    if ($Capture) {
        $flags += @('--frames', '180', '--screenshot-after', '80', '--timeout-seconds', '120')
    }
} else {
    $flags += @('--world', $World, '--creative', '--render-distance', '6')
    if ($Capture) {
        $flags += @('--world-frames', '240', '--screenshot-after', '90', '--timeout-seconds', '180')
    }
}
Write-Host "Horizon Windows ARM64 touch preview: scheme=$Scheme, size=$Scale%, opacity=$Opacity%" -ForegroundColor Cyan
Write-Host "Executable: $exe"
Write-Host "Capture mode: $([bool]$Capture); title mode: $([bool]$Title)"
& $exe @flags
if ($LASTEXITCODE -ne 0) { throw "Horizon exited with code $LASTEXITCODE" }
if ($Capture) {
    $path = Join-Path $game ('screenshots\horizon-arm64-debug-' + $(if ($Title) {'title'} else {'world'}) + '.png')
    if (-not (Test-Path $path)) { throw "No GPU screenshot produced at $path" }
    Write-Host "Actual in-engine screenshot: $path" -ForegroundColor Green
}
