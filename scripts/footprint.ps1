<#
.SYNOPSIS
  Measures a running program's memory and CPU, the way docs/FOOTPRINT.md describes.

.EXAMPLE
  .\scripts\footprint.ps1 -Exe .\target\release\catalyx.exe -Arguments '--no-elevate' -Seconds 75
  .\scripts\footprint.ps1 -Exe .\target\release\catalyx.exe -Arguments '--no-elevate' -Seconds 60 -MinimizeAt 10
  .\scripts\footprint.ps1 -Exe .\target\release\catalyx.exe -Arguments 'watch --quiet' -Seconds 60 -NoWindow
#>
param(
  [Parameter(Mandatory)] [string] $Exe,
  [string] $Arguments = '',
  [int] $Seconds = 60,
  [int] $Step = 4,
  [int] $MinimizeAt = 0,
  [switch] $NoWindow
)

$src = @'
using System; using System.Runtime.InteropServices;
public class Fp { [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int c); }
'@
if (-not ('Fp' -as [type])) { Add-Type -TypeDefinition $src }

# A scratch data folder: never measure (or disturb) the real config.
$env:CATALYX_DATA = Join-Path $env:TEMP 'catalyx-footprint'
New-Item -ItemType Directory -Force $env:CATALYX_DATA | Out-Null

$exe = (Resolve-Path $Exe).Path
$p = if ($Arguments) { Start-Process $exe -ArgumentList $Arguments -PassThru -WindowStyle $(if ($NoWindow) { 'Hidden' } else { 'Normal' }) }
     else { Start-Process $exe -PassThru }
$t0 = Get-Date; $minimized = $false
$c0 = $null; $tc0 = $null
"{0,5}  {1,10}  {2,12}  {3,10}" -f 'sec', 'WS (MB)', 'private (MB)', 'commit (MB)'
while (((Get-Date) - $t0).TotalSeconds -lt $Seconds -and -not $p.HasExited) {
  $p.Refresh()
  if ($MinimizeAt -gt 0 -and -not $minimized -and ((Get-Date) - $t0).TotalSeconds -ge $MinimizeAt -and $p.MainWindowHandle -ne 0) {
    [void][Fp]::ShowWindow($p.MainWindowHandle, 6); $minimized = $true; '  -- minimised --'
  }
  $perf = Get-CimInstance Win32_PerfFormattedData_PerfProc_Process -Filter "IDProcess=$($p.Id)" -ErrorAction SilentlyContinue
  "{0,5:N0}  {1,10:N1}  {2,12:N1}  {3,10:N1}" -f ((Get-Date) - $t0).TotalSeconds, ($p.WorkingSet64 / 1MB), ($perf.WorkingSetPrivate / 1MB), ($p.PrivateMemorySize64 / 1MB)
  Start-Sleep $Step
}
if (-not $p.HasExited) {
  # CPU over a fresh 20-second window at the end
  $p.Refresh(); $c0 = $p.TotalProcessorTime.TotalMilliseconds; $tc0 = Get-Date; Start-Sleep 20; $p.Refresh()
  $cpu = (($p.TotalProcessorTime.TotalMilliseconds - $c0) / ((Get-Date) - $tc0).TotalMilliseconds) * 100
  "CPU over the last 20 s: {0:N3} % of one core;  threads {1}, handles {2}, modules {3}" -f $cpu, $p.Threads.Count, $p.HandleCount, $p.Modules.Count
  Stop-Process -Id $p.Id -Force
}
Remove-Item Env:CATALYX_DATA
