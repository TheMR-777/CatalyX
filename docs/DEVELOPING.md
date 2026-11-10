# Developing CatalyX

## Requirements

- Windows 10/11 (the program uses Win32 throughout; it does not build elsewhere).
- A recent stable Rust toolchain with the `x86_64-pc-windows-msvc` target (Visual Studio Build Tools).
- Optional: an NVIDIA card to exercise the graphics features; administrator rights for the features that need
  them.

## Two programs from one source

`src/lib.rs` is the program. `catalyx.exe` (`src/main.rs`) is a Windows *window* program (no console, so starting
it never opens a terminal; typed in a terminal with a command it joins that terminal and prints there).
`catalyx-cli.exe` (`src/bin/catalyx-cli.rs`) is the same code as a *console* program: use it in scripts, pipes and
CI, because a shell waits for it and exit codes, redirects and Ctrl+C behave. Both are built by `cargo build`.
Start-up timing: set `CATALYX_TRACE=1` and read `%TEMP%\catalyx-startup.txt`.

## Build and check

```powershell
cargo build --release            # target\release\catalyx.exe
cargo test --release             # unit tests (no Windows state is changed)
cargo clippy --release           # must be clean: CI runs it with -D warnings
cargo fmt --check                # rustfmt.toml: max_width 140, Unix newlines
```

Close CatalyX before building (Windows locks the running exe) and do not run two builds at once.

## The built-in checks (they drive the real window)

All of them use a scratch data folder and end by themselves under a 110-second watchdog. None touches your
real config, journal or games.

```powershell
# End-to-end: starts a stand-in "game" (a renamed copy of ping.exe), boosts it, checks its priority and the
# live HUD, restores, and writes a JSON report. Takes about 10 seconds.
.\target\release\catalyx.exe --self-test shots\selftest.json --no-elevate

# A screenshot of any page, with demo data
.\target\release\catalyx.exe --screenshot shots\boost.png --tab boost --size 1180x760 --no-elevate
.\target\release\catalyx.exe --screenshot shots\live.png  --tab boost --demo-session --accent ember --appearance dark --no-elevate

# Every page
.\target\release\catalyx.exe --tour shots\tour --no-elevate

# Does each boost feature really work on THIS PC? (changes and restores real settings, briefly)
.\target\release\catalyx.exe doctor --gpu
```

Pass a path without spaces to `Start-Process -ArgumentList` (or quote it), or the arguments split.
`--tab` takes `boost`, `games`, `apps`, `services`, `profiles`, `tuning`, `settings`.

## Testing against Windows safely

- **Never run a real game or a launcher with an anti-cheat driver** in a test. Use a stand-in: copy
  `%SystemRoot%\System32\PING.EXE` to a folder *outside* Windows and rename it (the program never takes
  programs from the Windows folder for a game).
- Point everything at a scratch folder: `$env:CATALYX_DATA = "$env:TEMP\cx-test"`. Remove it afterwards.
- `catalyx play <game> --dry-run` and `catalyx boost --dry-run` show the plan and change nothing.
- The quiet desktop switches your taskbar off for a few seconds: `catalyx desktop test 6`. If anything goes
  wrong, `catalyx desktop restore` (or Task Manager > Run new task > `explorer.exe`).
- Tests and checks must never hang: give every wait a time limit.

## Releases

Pushing a tag `vX.Y.Z` runs `.github/workflows/release.yml`, which builds `catalyx.exe` in release mode and
attaches it, with its SHA-256, to a GitHub release. Bump `version` in `Cargo.toml` first, and keep the README
in step.

## Regenerating assets

- **Icon:** edit the geometry in `src/gui/mark.rs`, build, then
  `.\target\release\catalyx.exe --write-icon assets\catalyx.ico` and build again (the build embeds the file).
- **Screenshots** in `docs/screenshots` are produced with `--screenshot` as above.
