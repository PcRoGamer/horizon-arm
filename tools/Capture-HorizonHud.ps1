param(
    [ValidateSet(0,1,2,3)][int]$Scheme = 1,
    [ValidateRange(80,120)][int]$Scale = 100,
    [ValidateRange(35,100)][int]$Opacity = 72,
    [int]$Width = 1280,
    [int]$Height = 800,
    [string]$Output = 'horizon-hud.png'
)
$ErrorActionPreference = 'Stop'
if (-not (Get-Command adb -ErrorAction SilentlyContinue)) { throw 'adb not on PATH' }
& adb shell wm size "$($Width)x$($Height)"
if ($LASTEXITCODE -ne 0) { throw 'Failed to set emulator viewport' }
& adb shell am start -n 'dev.horizon.hudpreview/.HudPreviewActivity' --ei scheme $Scheme --ei scale $Scale --ei opacity $Opacity
if ($LASTEXITCODE -ne 0) { throw 'Failed to launch HUD-only renderer' }
Start-Sleep -Milliseconds 450
$target = [IO.Path]::GetFullPath($Output)
$p = [Diagnostics.Process]::new()
$p.StartInfo.FileName = 'adb'
$p.StartInfo.Arguments = 'exec-out screencap -p'
$p.StartInfo.UseShellExecute = $false
$p.StartInfo.RedirectStandardOutput = $true
$p.StartInfo.RedirectStandardError = $true
if (-not $p.Start()) { throw 'Failed to start adb screencap' }
$stream = [IO.File]::Create($target)
try { $p.StandardOutput.BaseStream.CopyTo($stream) } finally { $stream.Dispose() }
$p.WaitForExit()
if ($p.ExitCode -ne 0) { throw "adb screencap error: $($p.StandardError.ReadToEnd())" }
$bytes = [IO.File]::ReadAllBytes($target)
if ($bytes.Length -lt 8 -or $bytes[0] -ne 137 -or $bytes[1] -ne 80 -or $bytes[2] -ne 78 -or $bytes[3] -ne 71) { throw 'Output is not PNG' }
Write-Output "Native Android HUD screenshot: $target ($($bytes.Length) bytes)"
