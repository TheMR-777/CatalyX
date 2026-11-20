<#
.SYNOPSIS
  Golden screenshots: capture a fixed set of window images, or compare two sets pixel by pixel.

.DESCRIPTION
  For refactors that must not change what the window looks like. Capture before (the "golden" set), change
  the code, capture again, compare. Uses the window's own --screenshot mode (demo data, scratch settings,
  steady demo meters). Never writes into docs/ (that is screenshots.ps1's job).

  Some pages show live data from this PC (the Apps page lists running programs, the Services page lists
  services), so they can differ between two captures with no code change at all; they are reported as
  "live" and do not fail the comparison unless -Strict is given.

.EXAMPLE
  .\scripts\golden.ps1 -Capture ..\CatalyX-workbench\baseline\shots
  .\scripts\golden.ps1 -Compare ..\CatalyX-workbench\baseline\shots, $env:TEMP\catalyx-golden-now
#>
param(
  [string] $Capture,
  [string[]] $Compare,
  [switch] $Strict
)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot

# name, tab, size, accent, appearance, boost control, extra flags
$shots = @(
  @('boost-bar',          'boost',    '1180x760', 'ember', 'dark',  'bar',   @()),
  @('boost-bar-live',     'boost',    '1180x760', 'ember', 'dark',  'bar',   @('--demo-session')),
  @('boost-gauge-live',   'boost',    '1180x760', 'ember', 'dark',  'gauge', @('--demo-session')),
  @('boost-ring',         'boost',    '1180x760', 'ember', 'dark',  'ring',  @()),
  @('boost-ring-live',    'boost',    '1180x760', 'ember', 'dark',  'ring',  @('--demo-session')),
  @('boost-dialog',       'boost',    '1180x760', 'ember', 'dark',  'bar',   @('--dialog', 'boost')),
  @('boost-min',          'boost',    '760x560',  'ember', 'dark',  'bar',   @()),
  @('boost-light-live',   'boost',    '1180x760', 'sage',  'light', 'bar',   @('--demo-session')),
  @('games',              'games',    '1180x760', 'ember', 'dark',  'bar',   @()),
  @('games-min',          'games',    '760x560',  'ember', 'dark',  'bar',   @()),
  @('apps',               'apps',     '1180x760', 'ember', 'dark',  'bar',   @()),
  @('services',           'services', '1180x760', 'ember', 'dark',  'bar',   @()),
  @('profiles',           'profiles', '1180x1000','ember', 'dark',  'bar',   @()),
  @('profiles-light',     'profiles', '1180x1000','default','light','bar',   @()),
  @('tuning',             'tuning',   '1180x760', 'ember', 'dark',  'bar',   @()),
  @('settings',           'settings', '1180x1000','ember', 'dark',  'bar',   @()),
  @('settings-min',       'settings', '760x560',  'ember', 'dark',  'bar',   @())
)
# Images whose content comes from this PC's live state (see .DESCRIPTION): the Apps, Services and Tuning
# pages, and the demo session's "Graphics card now" line (real NVML readings; screenshot mode fixes the
# title-bar meters but not this line yet).
$live = @('apps', 'services', 'tuning', 'boost-bar-live', 'boost-gauge-live', 'boost-ring-live', 'boost-light-live')

function Invoke-Capture([string]$dir) {
  $exe = Join-Path $root 'target\release\catalyx.exe'
  if (-not (Test-Path $exe)) { throw 'Build first: cargo build --release' }
  # Render into a temp folder without spaces (Start-Process -ArgumentList splits on them), then copy.
  $tmp = Join-Path $env:TEMP 'catalyx-golden-tmp'
  Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory $tmp | Out-Null
  New-Item -ItemType Directory -Force $dir | Out-Null
  # The pointer is part of the picture: whichever button it rests on is lit, and a lit button made the
  # same build photograph differently from one run to the next. Park it on the screen edge for the
  # whole run and put it back afterwards, so two captures of one build are the same picture.
  Add-Type -AssemblyName System.Windows.Forms
  $pointer = [System.Windows.Forms.Cursor]::Position
  try {
    [System.Windows.Forms.Cursor]::Position = [System.Drawing.Point]::new(0, [System.Windows.Forms.Screen]::PrimaryScreen.Bounds.Height - 1)
    foreach ($s in $shots) {
      $png = Join-Path $tmp "$($s[0]).png"
      $a = @('--screenshot', $png, '--tab', $s[1], '--size', $s[2], '--accent', $s[3], '--appearance', $s[4], '--control', $s[5], '--no-elevate') + $s[6]
      $p = Start-Process $exe -ArgumentList $a -PassThru
      if (-not $p.WaitForExit(90000)) { $p.Kill(); throw "$($s[0]): the capture did not finish" }
      Start-Sleep -Milliseconds 300
      if (-not (Test-Path $png)) { throw "$($s[0]): no image was written" }
      Copy-Item $png (Join-Path $dir "$($s[0]).png") -Force
      Write-Host ("  captured {0}" -f $s[0])
    }
  } finally {
    [System.Windows.Forms.Cursor]::Position = $pointer
  }
  Write-Host "Captured $($shots.Count) image(s) into $dir" -ForegroundColor Green
}

$cs = @'
using System; using System.Drawing; using System.Drawing.Imaging; using System.Runtime.InteropServices;
public static class GoldenDiff {
  // Returns the number of differing pixels (any channel differs by more than tol); writes a diff image
  // (differences in magenta over a dimmed copy of the first image) when diffPath is not null. -1 = sizes differ.
  public static long Compare(string a, string b, int tol, string diffPath) {
    using (var A = new Bitmap(a)) using (var B = new Bitmap(b)) {
      if (A.Width != B.Width || A.Height != B.Height) return -1;
      var r = new Rectangle(0, 0, A.Width, A.Height);
      var da = A.LockBits(r, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
      var db = B.LockBits(r, ImageLockMode.ReadOnly, PixelFormat.Format32bppArgb);
      int n = Math.Abs(da.Stride) * A.Height;
      var pa = new byte[n]; var pb = new byte[n];
      Marshal.Copy(da.Scan0, pa, 0, n); Marshal.Copy(db.Scan0, pb, 0, n);
      A.UnlockBits(da); B.UnlockBits(db);
      long diff = 0; var outp = diffPath != null ? new byte[n] : null;
      for (int i = 0; i < n; i += 4) {
        bool d = Math.Abs(pa[i] - pb[i]) > tol || Math.Abs(pa[i+1] - pb[i+1]) > tol || Math.Abs(pa[i+2] - pb[i+2]) > tol;
        if (d) diff++;
        if (outp != null) {
          if (d) { outp[i] = 255; outp[i+1] = 0; outp[i+2] = 255; }
          else { outp[i] = (byte)(pa[i] / 3); outp[i+1] = (byte)(pa[i+1] / 3); outp[i+2] = (byte)(pa[i+2] / 3); }
          outp[i+3] = 255;
        }
      }
      if (outp != null && diff > 0) {
        using (var D = new Bitmap(A.Width, A.Height, PixelFormat.Format32bppArgb)) {
          var dd = D.LockBits(r, ImageLockMode.WriteOnly, PixelFormat.Format32bppArgb);
          Marshal.Copy(outp, 0, dd.Scan0, n); D.UnlockBits(dd); D.Save(diffPath, ImageFormat.Png);
        }
      }
      return diff;
    }
  }
}
'@

function Invoke-Compare([string]$a, [string]$b) {
  if (-not ('GoldenDiff' -as [type])) {
    # PowerShell 7 (.NET) keeps Bitmap in System.Drawing.Common; Windows PowerShell 5.1 in System.Drawing.
    # Newer .NET splits it further (System.Private.Windows.*), so reference the loaded files by path.
    $refs = if ($PSVersionTable.PSEdition -eq 'Core') {
      Add-Type -AssemblyName System.Drawing.Common
      [void][System.Drawing.Bitmap]
      [AppDomain]::CurrentDomain.GetAssemblies() | Where-Object {
        -not $_.IsDynamic -and $_.Location -and $_.GetName().Name -match '^(System\.Drawing|System\.Private\.Windows|System\.Runtime|System\.Collections|System\.Memory|netstandard|System\.ComponentModel)'
      } | ForEach-Object { $_.Location } | Sort-Object -Unique
    } else { @('System.Drawing') }
    Add-Type -TypeDefinition $cs -ReferencedAssemblies $refs
  }
  $a = (Resolve-Path $a).Path; $b = (Resolve-Path $b).Path
  $diffDir = Join-Path $b '_diff'; Remove-Item $diffDir -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory $diffDir | Out-Null
  $failed = 0
  $rows = foreach ($s in $shots) {
    $name = $s[0]; $fa = Join-Path $a "$name.png"; $fb = Join-Path $b "$name.png"
    if (-not (Test-Path $fa) -or -not (Test-Path $fb)) { $failed++; [pscustomobject]@{ Image = $name; Result = 'MISSING'; Pixels = '' }; continue }
    $n = [GoldenDiff]::Compare($fa, $fb, 2, (Join-Path $diffDir "$name.png"))
    $isLive = ($live -contains $s[1]) -or ($live -contains $name)
    $res = if ($n -eq 0) { 'same' } elseif ($n -lt 0) { 'SIZE DIFFERS' } elseif ($isLive -and -not $Strict) { 'differs (live page)' } else { 'DIFFERS' }
    if ($res -eq 'DIFFERS' -or $res -eq 'SIZE DIFFERS') { $failed++ }
    [pscustomobject]@{ Image = $name; Result = $res; Pixels = $n }
  }
  $rows | Format-Table -AutoSize | Out-String -Width 160 | Write-Host
  if ($failed -gt 0) { Write-Host "$failed image(s) differ; magenta diffs in $diffDir" -ForegroundColor Red; exit 1 }
  Write-Host 'No differences outside live pages.' -ForegroundColor Green
}

if ($Capture) { Invoke-Capture $Capture }
elseif ($Compare -and $Compare.Count -eq 2) { Invoke-Compare $Compare[0] $Compare[1] }
else { throw 'Use -Capture <dir> or -Compare <golden dir>, <new dir>' }
