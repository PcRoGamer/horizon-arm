<#
.SYNOPSIS
Run Horizon ARM64 with release-level optimizations, without visual-debug probes.
.DESCRIPTION
Builds --profile fast on first use and runs the native ARM64 executable.
Uses an isolated game directory to avoid modifying personal worlds.
The Android touch visualization is intentionally debug-only.
#>
[CmdletBinding()]
param(
    [switch]$Build,
    [ValidateSet('dx12', 'vulkan', 'auto')][string]$Backend = 'dx12',
    [string]$World = '',
    [ValidateRange(2, 128)][int]$RenderDistance = 12,
    [string]$Resources = '',
    [string]$GameDir = ''
)
$ErrorActionPreference = 'Stop'
$repo = (Resolve-Path (Join-Path $PSScriptRoot '..')).Path
$target = 'aarch64-pc-windows-msvc'
$targetRoot = if ($env:CARGO_TARGET_DIR) { $env:CARGO_TARGET_DIR } else { Join-Path $repo 'target' }
$exe = Join-Path $targetRoot "$target\fast\horizon-client.exe"
if ($Build -or -not (Test-Path $exe)) {
    Write-Host 'Compiling optimized native Windows ARM64 Horizon (profile=fast)...' -ForegroundColor Cyan
    $vcvars = Join-Path $env:ProgramFiles 'Microsoft Visual Studio\18\Community\VC\Auxiliary\Build\vcvarsarm64.bat'
    if (-not (Test-Path $vcvars)) {
        $vswhere = Join-Path ${env:ProgramFiles(x86)} 'Microsoft Visual Studio\Installer\vswhere.exe'
        if (Test-Path $vswhere) {
            $installation = & $vswhere -latest -products * -property installationPath
            if ($installation) { $vcvars = Join-Path $installation 'VC\Auxiliary\Build\vcvarsarm64.bat' }
        }
    }
    if (-not (Test-Path $vcvars)) {
        throw 'Missing native ARM64 MSVC build tools (vcvarsarm64.bat).'
    }
    # Use native ARM64 Rust, not the machine's x86-64 default compiler.
    $nativeToolchain = 'stable-aarch64-pc-windows-msvc'
    $installedToolchains = & rustup toolchain list
    if (-not ($installedToolchains | Select-String -SimpleMatch $nativeToolchain)) {
        throw "Native Rust toolchain missing: rustup toolchain install $nativeToolchain --profile minimal"
    }
    # ring's ARM64 Windows build requires Clang. Find our verified portable LLVM
    # (not the Android NDK's clang, which lacks the Windows SDK headers here).
    $llvmRoot = Join-Path $repo 'artifacts\toolchains'
    $llvmBin = Get-ChildItem -Path $llvmRoot -Directory -Filter 'llvm-*-win-arm64' -ErrorAction SilentlyContinue |
        Sort-Object Name -Descending |
        ForEach-Object { Get-ChildItem -Path $_.FullName -Directory -Filter 'clang+llvm-*' -ErrorAction SilentlyContinue } |
        ForEach-Object { Join-Path $_.FullName 'bin' } |
        Where-Object { Test-Path (Join-Path $_ 'clang.exe') } |
        Select-Object -First 1
    if (-not $llvmBin) {
        throw 'Native Windows ARM64 LLVM/Clang not found under artifacts\toolchains. See docs/windows-arm-benchmark-isolation.md.'
    }
    $jobs = if ($env:CARGO_BUILD_JOBS) { $env:CARGO_BUILD_JOBS } else { '4' }
    Push-Location $repo
    try {
        $buildCommand = 'call "' + $vcvars + '" >nul' +
            ' && set "PATH=' + $llvmBin + ';%PATH%"' +
            ' && set "CC_aarch64_pc_windows_msvc=' + (Join-Path $llvmBin 'clang.exe') + '"' +
            ' && set "RUSTUP_TOOLCHAIN=' + $nativeToolchain + '"' +
            ' && set "CARGO_BUILD_JOBS=' + $jobs + '"' +
            ' && cargo build --profile fast -p horizon-client --target ' + $target
        & cmd.exe /d /s /c $buildCommand
        if ($LASTEXITCODE -ne 0) { throw "Optimized build failed: cargo exit $LASTEXITCODE" }
    } finally {
        Pop-Location
    }
}
if (-not (Test-Path $exe)) { throw "Optimized executable not found: $exe" }
# Validate the PE header without loading the whole executable into memory.
$stream = [System.IO.File]::OpenRead($exe)
try {
    $reader = [System.IO.BinaryReader]::new($stream)
    $stream.Position = 0x3c
    $peOffset = $reader.ReadInt32()
    $stream.Position = $peOffset + 4
    $machine = $reader.ReadUInt16()
    if ($machine -ne 0xAA64) { throw "Executable is not native ARM64: $exe" }
} finally { $stream.Dispose() }
if (-not $Resources) {
    $candidates = @(
        (Join-Path $repo '.local\horizon-client\26.3'),
        (Join-Path (Split-Path $repo -Parent) 'horizon-android-polish\.local\horizon-client\26.3')
    )
    $Resources = $candidates | Where-Object { Test-Path (Join-Path $_ 'horizon-resources.json') } | Select-Object -First 1
}
if (-not $Resources -or -not (Test-Path (Join-Path $Resources 'horizon-resources.json'))) {
    throw 'Prepared Horizon resources not found. Specify -Resources with a valid 26.3 resource directory.'
}
if (-not $GameDir) { $GameDir = Join-Path $repo 'artifacts\windows-arm64-fast\game' }
New-Item -ItemType Directory -Force -Path $GameDir | Out-Null
$env:HORIZON_HOME = $GameDir
# Debug-only HUD probes should never leak into a normal play session.
Get-ChildItem Env:HORIZON_TOUCH_PREVIEW* | Remove-Item -ErrorAction SilentlyContinue
if ($Backend -eq 'auto') {
    Remove-Item Env:WGPU_BACKEND -ErrorAction SilentlyContinue
} else {
    $env:WGPU_BACKEND = $Backend
}
$flags = @('--resources', $Resources, '--game-dir', $GameDir, '--render-distance', [string]$RenderDistance)
if ($World) { $flags += @('--world', $World) }
Write-Host "Horizon ARM64 optimized: $exe" -ForegroundColor Green
Write-Host "Backend: $Backend; profile: fast; game dir: $GameDir"
Push-Location $GameDir
try {
    & $exe @flags
    if ($LASTEXITCODE -ne 0) { throw "Horizon exited with code $LASTEXITCODE" }
} finally {
    Pop-Location
}
