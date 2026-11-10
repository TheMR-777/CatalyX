<#
.SYNOPSIS
  Where does start-up time go? Launches the window several times with CATALYX_TRACE=1 and prints the
  median time of every milestone (milliseconds since the program started).

.EXAMPLE
  .\scripts\startup-profile.ps1            # 7 launches
  .\scripts\startup-profile.ps1 -Runs 15
#>
param([int]$Runs = 7)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
$exe = Join-Path $root 'target\release\catalyx.exe'
if (-not (Test-Path $exe)) { throw 'Build first: cargo build --release' }
$env:CATALYX_DATA = Join-Path $env:TEMP 'catalyx-profile-data'; New-Item -ItemType Directory -Force $env:CATALYX_DATA | Out-Null
$env:CATALYX_TRACE = '1'
$trace = Join-Path $env:TEMP 'catalyx-startup.txt'
$samples = @{}; $order = New-Object System.Collections.Generic.List[string]
for ($i = 0; $i -lt $Runs + 1; $i++) {
  Remove-Item $trace -ErrorAction SilentlyContinue
  $p = Start-Process $exe -ArgumentList '--no-elevate' -PassThru
  for ($w = 0; $w -lt 60; $w++) { Start-Sleep -Milliseconds 100; if ((Test-Path $trace) -and (Select-String -Path $trace -Pattern 'frame 3' -Quiet)) { break } }
  Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue; Start-Sleep -Milliseconds 400
  if ($i -eq 0) { continue }   # the first launch is cold (disk cache, antivirus scan): not representative
  foreach ($line in Get-Content $trace) {
    if ($line -match '^\s*([\d.]+) ms\s+(.+)$') {
      $k = $Matches[2].Trim(); if (-not $samples.ContainsKey($k)) { $samples[$k] = New-Object System.Collections.Generic.List[double]; $order.Add($k) }
      $samples[$k].Add([double]$Matches[1])
    }
  }
}
Remove-Item Env:CATALYX_TRACE, Env:CATALYX_DATA -ErrorAction SilentlyContinue
Write-Host "Median of $Runs warm launches (the cold first launch is skipped):`n"
foreach ($k in $order) { $v = @($samples[$k] | Sort-Object); '{0,8:N1} ms   {1}   (min {2:N0}, max {3:N0})' -f $v[[int]($v.Count / 2)], $k, $v[0], $v[-1] }
