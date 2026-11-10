<#
.SYNOPSIS
  Re-shoots every screenshot in docs/screenshots from the current build, the same way every time.

.DESCRIPTION
  Uses the window's own --screenshot mode (demo data, scratch settings, steady demo meters), so the images
  never show your games, your real numbers or whatever your PC was doing. Renders into a temp folder and copies
  into docs/screenshots (OneDrive and spaces in paths trip up the capture otherwise).

.EXAMPLE
  .\scripts\screenshots.ps1                    # all of them
  .\scripts\screenshots.ps1 -Only gauge,ring   # just the ones whose name matches
#>
param([string[]]$Only)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$exe = Join-Path $root 'target\release\catalyx.exe'
if (-not (Test-Path $exe)) { throw 'Build first: cargo build --release' }
$out = Join-Path $env:TEMP 'catalyx-screens'; Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory $out | Out-Null

# name, tab, size, accent, appearance, boost control, extra flags
$shots = @(
  @('boost-live',    'boost',    '1180x760',  'ember', 'dark',  'bar',   @('--demo-session')),
  @('boost',         'boost',    '1180x760',  'ember', 'dark',  'bar',   @()),
  @('control-gauge', 'boost',    '1180x760',  'ember', 'dark',  'gauge', @('--demo-session')),
  @('control-ring',  'boost',    '1180x760',  'ember', 'dark',  'ring',  @('--demo-session')),
  @('dialog',        'boost',    '1180x760',  'ember', 'dark',  'bar',   @('--dialog', 'boost')),
  @('games',         'games',    '1180x760',  'ember', 'dark',  'bar',   @()),
  @('profiles',      'profiles', '1180x1000', 'ember', 'dark',  'bar',   @()),
  @('tuning',        'tuning',   '1180x760',  'ember', 'dark',  'bar',   @()),
  @('settings',      'settings', '1180x1000', 'ember', 'dark',  'bar',   @()),
  @('light-sage',    'boost',    '1180x760',  'sage',  'light', 'bar',   @('--demo-session'))
)
if ($Only) { $shots = @($shots | Where-Object { $n = $_[0]; $Only | Where-Object { $n -like "*$_*" } }) }
if ($shots.Count -eq 0) { throw 'No screenshot matches -Only' }

foreach ($s in $shots) {
  $png = Join-Path $out "$($s[0]).png"
  $args2 = @('--screenshot', $png, '--tab', $s[1], '--size', $s[2], '--accent', $s[3], '--appearance', $s[4], '--control', $s[5], '--no-elevate') + $s[6]
  $p = Start-Process $exe -ArgumentList $args2 -PassThru
  if (-not $p.WaitForExit(90000)) { $p.Kill(); throw "$($s[0]): the capture did not finish" }
  Start-Sleep -Milliseconds 600
  if (-not (Test-Path $png) -or (Get-Item $png).Length -lt 50000) { throw "$($s[0]): no usable image was written" }
  Copy-Item $png (Join-Path $root "docs\screenshots\$($s[0]).png") -Force
  Write-Host ("  {0,-14} {1,4} KB" -f $s[0], [int]((Get-Item $png).Length / 1KB))
}
Write-Host "Updated $($shots.Count) screenshot(s) in docs/screenshots. Look at them before committing." -ForegroundColor Green
