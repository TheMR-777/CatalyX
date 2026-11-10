<#
.SYNOPSIS
  Every check, in one command: what CI runs, plus the real-window checks CI cannot.

.DESCRIPTION
  1. cargo fmt --check            2. clippy with warnings denied    3. unit tests
  4. release build                5. the window program really is a window program (PE subsystem 2),
                                     the console program a console program (3)
  6. the version in Cargo.toml is the version the program reports
  7. (full run) the window's end-to-end self-test: starts a stand-in game, boosts, restores
  8. (full run) the window starts, draws and is shown, within a time limit

  Exits 0 when everything passed, 1 otherwise, with a table of results. Never touches your real settings
  (checks use scratch data). Needs a Windows desktop session for steps 7 and 8.

.EXAMPLE
  .\scripts\check.ps1            # everything (about 3 minutes cold, less when cached)
  .\scripts\check.ps1 -Quick     # skip the two real-window steps
#>
param([switch]$Quick)
$ErrorActionPreference = 'Continue'
$root = Split-Path $PSScriptRoot
Set-Location $root
$rows = New-Object System.Collections.Generic.List[object]

function Step([string]$name, [scriptblock]$body) {
  Write-Host ("--> {0}" -f $name) -ForegroundColor Cyan
  $sw = [Diagnostics.Stopwatch]::StartNew()
  $global:LASTEXITCODE = 0
  $ok = $true; $detail = ''
  try {
    $out = (& $body 2>&1 | Out-String).Trim()
    if ($LASTEXITCODE -ne 0) { $ok = $false }
    $detail = $out
  } catch { $ok = $false; $detail = $_.Exception.Message }
  $sw.Stop()
  $rows.Add([pscustomobject]@{ Step = $name; Ok = $ok; Seconds = [math]::Round($sw.Elapsed.TotalSeconds, 1); Detail = $detail })
  if (-not $ok) { Write-Host $detail -ForegroundColor Red }
}

function Subsystem([string]$path) {
  $b = [IO.File]::ReadAllBytes($path); $pe = [BitConverter]::ToInt32($b, 0x3c)
  [BitConverter]::ToUInt16($b, $pe + 24 + 68)
}

$win = Join-Path $root 'target\release\catalyx.exe'
$cli = Join-Path $root 'target\release\catalyx-cli.exe'

Step 'format'  { cargo fmt --check }
Step 'clippy'  { cargo clippy --release -- -D warnings }
Step 'tests' {
  $o = cargo test --release 2>&1 | Out-String
  if ($LASTEXITCODE -ne 0) { $o } else { ($o -split "`n" | Select-String 'test result' | Select-Object -First 1).Line }
}
Step 'build'   { cargo build --release }

Step 'subsystems' {
  $w = Subsystem $win; $c = Subsystem $cli
  if ($w -ne 2) { throw "catalyx.exe has subsystem $w, expected 2 (window): it would open a terminal when started" }
  if ($c -ne 3) { throw "catalyx-cli.exe has subsystem $c, expected 3 (console)" }
  "catalyx.exe = window (2), catalyx-cli.exe = console (3)"
}

Step 'version' {
  $want = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
  $got = (& $cli version).Trim()
  if ($got -ne "CatalyX $want") { throw "Cargo.toml says $want but the program says '$got'" }
  $got
}

if (-not $Quick) {
  Step 'self-test (real window)' {
    $json = Join-Path $env:TEMP 'catalyx-check-selftest.json'; Remove-Item $json -ErrorAction SilentlyContinue
    $p = Start-Process $win -ArgumentList '--self-test', $json, '--no-elevate' -PassThru
    if (-not $p.WaitForExit(150000)) { $p.Kill(); throw 'the self-test did not finish in 150 s' }
    if (-not (Test-Path $json)) { throw 'the self-test wrote no report' }
    $steps = (Get-Content $json -Raw | ConvertFrom-Json).steps
    $bad = @($steps | Where-Object { -not $_.passed })
    if ($bad.Count -gt 0) { throw ('failed: ' + (($bad | ForEach-Object { "$($_.step): $($_.detail)" }) -join '; ')) }
    ($steps | ForEach-Object { $_.step }) -join ', '
  }

  Step 'start-up (window shown)' {
    $env:CATALYX_DATA = Join-Path $env:TEMP 'catalyx-check-startup'; New-Item -ItemType Directory -Force $env:CATALYX_DATA | Out-Null
    $env:CATALYX_TRACE = '1'; $trace = Join-Path $env:TEMP 'catalyx-startup.txt'; Remove-Item $trace -ErrorAction SilentlyContinue
    $p = Start-Process $win -ArgumentList '--no-elevate' -PassThru
    $shown = $null
    for ($i = 0; $i -lt 100 -and -not $shown; $i++) {
      Start-Sleep -Milliseconds 100
      if (Test-Path $trace) { $m = Select-String -Path $trace -Pattern '^\s*([\d.]+) ms\s+window shown' | Select-Object -First 1; if ($m) { $shown = [double]$m.Matches[0].Groups[1].Value } }
    }
    Stop-Process -Id $p.Id -Force -ErrorAction SilentlyContinue
    Remove-Item Env:CATALYX_TRACE, Env:CATALYX_DATA -ErrorAction SilentlyContinue
    if (-not $shown) { throw 'the window was never shown' }
    if ($shown -gt 2500) { throw "the window took $shown ms to be shown (limit 2500)" }
    "window shown after $shown ms"
  }
}

Write-Host ''
$rows | Format-Table @{n = 'Step'; e = { $_.Step } }, @{n = 'Result'; e = { if ($_.Ok) { 'PASS' } else { 'FAIL' } } }, @{n = 'Sec'; e = { $_.Seconds } }, @{n = 'Detail'; e = { ($_.Detail -split "`n")[-1] } } -AutoSize | Out-String -Width 200 | Write-Host
if (@($rows | Where-Object { -not $_.Ok }).Count -gt 0) { Write-Host 'CHECKS FAILED' -ForegroundColor Red; exit 1 }
Write-Host 'ALL CHECKS PASSED' -ForegroundColor Green
exit 0
