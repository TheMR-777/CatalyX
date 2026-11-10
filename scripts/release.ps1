<#
.SYNOPSIS
  Ships a release: checks, push, wait for CI, tag, wait for the release build, verify the published files.

.DESCRIPTION
  Everything the version in Cargo.toml needs, in order, stopping at the first thing that is wrong:

    0. preconditions   on main, nothing uncommitted, the tag is new, gh is logged in, no private files tracked
    1. checks          scripts\check.ps1 (format, clippy, tests, build, subsystems, self-test, start-up)
    2. push main       and wait for CI on that exact commit to pass
    3. tag vX.Y.Z      annotated, under your GitHub no-reply address (never your real e-mail), and push it
    4. release build   wait for the Release workflow
    5. verify          the release has all four files, each checksum matches, catalyx-cli.exe reports the
                       right version, catalyx.exe is a window program (subsystem 2), the CLI a console one (3)
    6. notes           docs\releases\<version>.md if it exists (otherwise GitHub's generated notes stay)

  Bump the version in Cargo.toml and commit FIRST (and write docs\releases\<version>.md). Run it from a normal
  PowerShell; it takes about 10 minutes, almost all of it waiting for GitHub. -DryRun does steps 0 and 1 only,
  smoke-tests every read-only GitHub call the later steps make, and says what it would do.
  -VerifyOnly vX.Y.Z runs just step 5 against a release that is already published.

.EXAMPLE
  .\scripts\release.ps1 -DryRun
  .\scripts\release.ps1
  .\scripts\release.ps1 -SkipChecks        # you just ran check.ps1 yourself
  .\scripts\release.ps1 -VerifyOnly v0.4.1 # check a published release again
#>
param([switch]$DryRun, [switch]$SkipChecks, [string]$VerifyOnly)
$ErrorActionPreference = 'Stop'
$root = Split-Path $PSScriptRoot
Set-Location $root

function Say([string]$m) { Write-Host "==> $m" -ForegroundColor Cyan }
function Ok([string]$m) { Write-Host "    ok  $m" -ForegroundColor Green }
function Fail([string]$m) { Write-Host "XX  $m" -ForegroundColor Red; exit 1 }
function Invoke-Gh { $o = & gh.exe @args 2>&1; if ($LASTEXITCODE -ne 0) { throw "gh $($args -join ' ') failed: $o" }; $o }

# ---- verification of a published release (step 5; also -VerifyOnly) ------------------------------------
function Test-Published([string]$tag, [string]$ver) {
  Say 'Verifying the published files'
  $tmp = Join-Path $env:TEMP "catalyx-release-$ver"; Remove-Item $tmp -Recurse -Force -ErrorAction SilentlyContinue; New-Item -ItemType Directory $tmp | Out-Null
  $names = (Invoke-Gh release view $tag --repo $repo --json assets --jq '.assets[].name') -split "`n" | ForEach-Object { $_.Trim() } | Where-Object { $_ }
  foreach ($want in 'catalyx.exe', 'catalyx.exe.sha256', 'catalyx-cli.exe', 'catalyx-cli.exe.sha256') { if ($names -notcontains $want) { Fail "the release is missing $want" } }
  Invoke-Gh release download $tag --repo $repo --dir $tmp --pattern 'catalyx*' | Out-Null
  foreach ($n in 'catalyx.exe', 'catalyx-cli.exe') {
    $want = ((Get-Content (Join-Path $tmp "$n.sha256")) -split '\s+')[0]
    $got = (Get-FileHash (Join-Path $tmp $n) -Algorithm SHA256).Hash.ToLower()
    if ($want -ne $got) { Fail "$n does not match its published checksum" }
    Ok "$n matches its checksum"
  }
  function Subsystem([string]$path) { $b = [IO.File]::ReadAllBytes($path); $pe = [BitConverter]::ToInt32($b, 0x3c); [BitConverter]::ToUInt16($b, $pe + 24 + 68) }
  if ((Subsystem (Join-Path $tmp 'catalyx.exe')) -ne 2) { Fail 'the published catalyx.exe is not a window program' }
  if ((Subsystem (Join-Path $tmp 'catalyx-cli.exe')) -ne 3) { Fail 'the published catalyx-cli.exe is not a console program' }
  Ok 'catalyx.exe is a window program, catalyx-cli.exe a console program'
  $reported = (& (Join-Path $tmp 'catalyx-cli.exe') version).Trim()
  if ($reported -ne "CatalyX $ver") { Fail "the published program says '$reported', expected 'CatalyX $ver'" }
  Ok "the published program reports $reported"
}

$repo0 = $null
if ($VerifyOnly) {
  Say "Verifying the published release $VerifyOnly"
  $repo = (Invoke-Gh repo view --json nameWithOwner --jq .nameWithOwner).Trim()
  Test-Published $VerifyOnly ($VerifyOnly.TrimStart('v'))
  exit 0
}

# ---- 0. preconditions -------------------------------------------------------------------------------------
Say 'Preconditions'
$ver = (Select-String -Path (Join-Path $root 'Cargo.toml') -Pattern '^version\s*=\s*"([^"]+)"' | Select-Object -First 1).Matches[0].Groups[1].Value
$tag = "v$ver"
Ok "version $ver, tag $tag"
& gh.exe auth status *> $null; if ($LASTEXITCODE -ne 0) { Fail 'gh is not logged in: run `gh auth login`' }
$repo = (Invoke-Gh repo view --json nameWithOwner --jq .nameWithOwner).Trim()
Ok "repository $repo"
if ((git branch --show-current) -ne 'main') { Fail 'not on main' }
if (git status --porcelain) { Fail 'uncommitted changes: commit them first (the version bump included)' }
if (git tag --list $tag) { Fail "tag $tag already exists: bump the version in Cargo.toml and commit" }
& gh.exe release view $tag --repo $repo *> $null; if ($LASTEXITCODE -eq 0) { Fail "a release $tag already exists on GitHub" }
$private = @(git ls-files | Select-String -Pattern '^(research|shots|target)/')
if ($private.Count -gt 0) { Fail "private or generated files are tracked: $($private -join ', ')" }
Ok 'clean tree, new tag, nothing private tracked'
$notes = Join-Path $root "docs\releases\$ver.md"
if (Test-Path $notes) { Ok "release notes: docs\releases\$ver.md" } else { Write-Host "    --  no docs\releases\$ver.md: GitHub's generated notes will stay" -ForegroundColor Yellow }

# ---- 1. checks --------------------------------------------------------------------------------------------
if ($SkipChecks) { Write-Host '    --  checks skipped' -ForegroundColor Yellow }
else { Say 'Checks'; & (Join-Path $PSScriptRoot 'check.ps1'); if ($LASTEXITCODE -ne 0) { Fail 'the checks failed; nothing was pushed' } }

if ($DryRun) {
  Say 'Smoke-testing the GitHub calls the later steps make (read-only)'
  $smoke = Invoke-Gh run list --repo $repo --workflow CI --limit 1 --json 'databaseId,headSha,headBranch,status' | ConvertFrom-Json
  Ok "workflow runs can be listed ($(@($smoke).Count) found)"
  $last = (Invoke-Gh release list --repo $repo --limit 1 --json tagName --jq '.[0].tagName').Trim()
  Ok "releases can be read (latest: $last)"
  $null = Invoke-Gh api user --jq .id
  Ok 'the signed-in GitHub user can be read (for the no-reply tag identity)'
  Say 'Dry run: would now push main, wait for CI, tag, push the tag, wait for the release build and verify it. Stopping here.'
  exit 0
}

# ---- 2. push main, wait for CI ---------------------------------------------------------------------------
Say 'Push main'
$sha = (git rev-parse HEAD).Trim()
git push origin main
if ($LASTEXITCODE -ne 0) { Fail 'git push failed' }

function Find-Run([string]$workflow, [string]$match, [string]$field) {
  for ($i = 0; $i -lt 60; $i++) {
    $runs = Invoke-Gh run list --repo $repo --workflow $workflow --limit 10 --json 'databaseId,headSha,headBranch' | ConvertFrom-Json
    $hit = $runs | Where-Object { $_.$field -eq $match } | Select-Object -First 1
    if ($hit) { return $hit.databaseId }
    Start-Sleep 3
  }
  Fail "no $workflow run appeared for $match"
}

Say 'Waiting for CI on this commit'
$ci = Find-Run 'CI' $sha 'headSha'
& gh.exe run watch $ci --repo $repo --exit-status *> $null
if ($LASTEXITCODE -ne 0) { Fail "CI failed: https://github.com/$repo/actions/runs/$ci (nothing was tagged)" }
Ok "CI passed (run $ci)"

# ---- 3. tag -----------------------------------------------------------------------------------------------
Say "Tag $tag"
$id = (Invoke-Gh api user --jq .id).Trim(); $login = (Invoke-Gh api user --jq .login).Trim()
git -c "user.name=$login" -c "user.email=$id+$login@users.noreply.github.com" tag -a $tag -m "CatalyX $ver"
git push origin $tag
if ($LASTEXITCODE -ne 0) { Fail 'pushing the tag failed' }

# ---- 4. the release build --------------------------------------------------------------------------------
Say 'Waiting for the release build'
$rel = Find-Run 'Release' $tag 'headBranch'
& gh.exe run watch $rel --repo $repo --exit-status *> $null
if ($LASTEXITCODE -ne 0) { Fail "the release build failed: https://github.com/$repo/actions/runs/$rel (the tag exists; fix, delete the tag and retry)" }
Ok "release build passed (run $rel)"

# ---- 5. verify what was published ------------------------------------------------------------------------
Test-Published $tag $ver

# ---- 6. notes ---------------------------------------------------------------------------------------------
if (Test-Path $notes) {
  Say 'Release notes'
  Invoke-Gh release edit $tag --repo $repo --title "CatalyX $ver" --notes-file $notes | Out-Null
  Ok "notes from docs\releases\$ver.md"
}
Write-Host ''
Write-Host "Released CatalyX $ver  ->  https://github.com/$repo/releases/tag/$tag" -ForegroundColor Green
