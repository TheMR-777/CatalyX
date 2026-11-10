<#
.SYNOPSIS
  Drives the REAL window with the real mouse: click the boost control, confirm the summary, check the game
  starts, press Restore, check everything is put back.

.DESCRIPTION
  Uses scratch data and a harmless stand-in game (a renamed copy of ping.exe) with every boost step switched
  off, so nothing on this PC is changed. The window is brought to the front and the mouse really moves, so
  leave the machine alone while it runs (about 20 seconds per look). Screenshots of each stage go to a temp
  folder. The click positions assume the default 1180x760 window and the layouts in src/gui/control.rs; if you
  move those, update the coordinates below.

  Exit code 0 = passed for every look tested, 1 = something failed.

.EXAMPLE
  .\scripts\click-test.ps1                 # the launch bar
  .\scripts\click-test.ps1 -Control all    # bar, gauge and ring
#>
param([ValidateSet('bar', 'gauge', 'ring', 'all')] [string]$Control = 'bar')
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$win = Join-Path $root 'target\release\catalyx.exe'
$cli = Join-Path $root 'target\release\catalyx-cli.exe'
if (-not (Test-Path $win)) { throw 'Build first: cargo build --release' }

$src = @'
using System; using System.Runtime.InteropServices; using System.Threading;
public class CT {
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x,int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f,int dx,int dy,uint d,UIntPtr e);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h,IntPtr after,int x,int y,int w,int hh,uint f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  public static void Move(int x,int y){ SetCursorPos(x,y); Thread.Sleep(80); }
  public static void Hover(int x,int y){ Move(x-4,y); Move(x-2,y); Move(x,y); Thread.Sleep(250); }
  public static void Click(int x,int y){ Hover(x,y); mouse_event(2,0,0,0,UIntPtr.Zero); Thread.Sleep(120); mouse_event(4,0,0,0,UIntPtr.Zero); Thread.Sleep(900); }
  public static void Top(IntPtr h){ SetWindowPos(h,(IntPtr)(-1),0,0,0,0,3); SetForegroundWindow(h); }
}
'@
if (-not ('CT' -as [type])) { Add-Type -TypeDefinition $src }
Add-Type -AssemblyName System.Drawing

function Test-One([string]$style) {
  $shots = Join-Path $env:TEMP "catalyx-click-$style"; Remove-Item $shots -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory $shots | Out-Null
  $env:CATALYX_DATA = Join-Path $env:TEMP 'catalyx-click-data'; Remove-Item $env:CATALYX_DATA -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory $env:CATALYX_DATA | Out-Null
  $dir = Join-Path $env:TEMP 'catalyx-standin'; New-Item -ItemType Directory -Force $dir | Out-Null
  Copy-Item "$env:SystemRoot\System32\PING.EXE" "$dir\standin-game.exe" -Force
  & $cli add "$dir\standin-game.exe" --name Standin --args '-n 40 127.0.0.1' | Out-Null
  foreach ($k in 'apps', 'services', 'clean-ram', 'game-mode', 'dvr-off', 'notifications-off', 'transparency-off', 'maintenance-off', 'mmcss', 'no-throttling', 'lower-others', 'io-boost', 'purge-standby', 'quiet-desktop') { & $cli set Standin $k off | Out-Null }
  & $cli set Standin power keep | Out-Null; & $cli set Standin cpu off | Out-Null; & $cli set Standin gpu off | Out-Null; & $cli set Standin priority normal | Out-Null
  & $cli theme $style ember | Out-Null
  $cfgPath = Join-Path $env:CATALYX_DATA 'config.json'; $cfg = Get-Content $cfgPath -Raw | ConvertFrom-Json; $cfg.onboarded = $true; ($cfg | ConvertTo-Json -Depth 20) | Set-Content $cfgPath -Encoding utf8

  $p = Start-Process $win -ArgumentList '--no-elevate', '--size', '1180x760' -PassThru
  $started = $false; $restored = $false; $problems = @()
  try {
    Start-Sleep 5; $p.Refresh(); $h = $p.MainWindowHandle
    if ($h -eq 0) { throw 'the window never appeared' }
    [CT]::Top($h); Start-Sleep 1
    function Rc { $o = New-Object CT+RECT; [void][CT]::GetWindowRect($h, [ref]$o); $o }
    function Shot($name) { $r = Rc; $b = New-Object System.Drawing.Bitmap(($r.R - $r.L), ($r.B - $r.T)); $g = [System.Drawing.Graphics]::FromImage($b); $g.CopyFromScreen($r.L, $r.T, 0, 0, $b.Size); $b.Save("$shots\$name.png"); $g.Dispose(); $b.Dispose() }
    $r = Rc; Shot '1-idle'
    # where the control is, per look (offsets from the window's corners)
    switch ($style) {
      'bar'   { $cx = $r.R - 24 - 63; $cy = $r.B - 36 }
      'gauge' { $cx = $r.L + 713 - 26; $cy = $r.T + 205 }
      'ring'  { $cx = $r.L + 670; $cy = $r.T + 205 }
    }
    [CT]::Click($cx, $cy); Shot '2-summary'
    [CT]::Click($r.L + 423, $r.T + 491)            # the summary's "Boost & play" button
    Start-Sleep 5
    $started = [bool](Get-Process standin-game -ErrorAction SilentlyContinue); Shot '3-boosted'
    $r = Rc
    if ($style -eq 'bar') { [CT]::Click($r.R - 24 - 55, $r.B - 36) } else { [CT]::Click($cx, $cy) }   # Restore
    Start-Sleep 8; Shot '4-restored'
    $restored = -not (Test-Path (Join-Path $env:CATALYX_DATA 'journal.json'))
  } catch { $problems += $_.Exception.Message }
  finally {
    Get-Process standin-game -ErrorAction SilentlyContinue | Stop-Process -Force
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Remove-Item Env:CATALYX_DATA -ErrorAction SilentlyContinue
  }
  if (-not $started) { $problems += 'the stand-in game did not start after confirming' }
  if (-not $restored) { $problems += 'a recovery journal was left behind after Restore' }
  $ok = $problems.Count -eq 0
  Write-Host ("{0,-6} {1}   screenshots: {2}" -f $style, $(if ($ok) { 'PASS' } else { 'FAIL: ' + ($problems -join '; ') }), $shots) -ForegroundColor $(if ($ok) { 'Green' } else { 'Red' })
  $ok
}

$styles = if ($Control -eq 'all') { 'bar', 'gauge', 'ring' } else { @($Control) }
$allOk = $true
foreach ($s in $styles) { if (-not (Test-One $s)) { $allOk = $false } }
if ($allOk) { exit 0 } else { exit 1 }
