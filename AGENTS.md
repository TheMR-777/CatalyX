# AGENTS.md

Guidance for coding agents (and humans in a hurry) working in this repository. Read this first; the details
live in `docs/`.

## What this is

**CatalyX**: a Windows game booster in Rust. One binary is a GUI (egui/eframe on OpenGL), a CLI and a
background watcher. Its promise: *every change is journaled before it is made and undone afterwards*, with a
footprint of a few MB. It is Windows-only and uses hand-written Win32 FFI.

| Name | Where it appears |
|---|---|
| **CatalyX** (capital X) | display name, docs, window title (`FindWindow` looks the window up by this title!) |
| `catalyx` | crate, exe, CLI, `%APPDATA%\CatalyX`, task names `CatalyX` / `CatalyX Watcher` |
| `CATALYX_DATA` | env var: points config/journal/caches at a scratch folder (`PUREBOOST_DATA` still works) |

## Start here (context you will not otherwise have)

A new session knows nothing about how this project got here. Get the context from disk, in this order:

1. [docs/DECISIONS.md](docs/DECISIONS.md): the choices and *why* (read before changing any behaviour).
2. [docs/ROADMAP.md](docs/ROADMAP.md): what is **not verified**, known limits, ideas. Do not claim something works
   in a place it was never tested.
3. `docs/releases/` and `git log --oneline`: what changed and when.
4. If the maintainer's private notes exist next to this folder (`..\CatalyX-workbench\CONTEXT.md`), read them too:
   the maintainer's PC and setup, working preferences, open threads. They are private: never copy from them into
   tracked files.

How the maintainer likes to work: **show options or mock-ups before building a big UI change** and pick together;
**ask before publishing or releasing** (then use `scripts\release.ps1`); keep answers plain and honest about what
was and was not verified; look at screenshots yourself; run long jobs in the background instead of waiting; when a
chore repeats, make it a script.

## Commands you will use

```powershell
cargo build --release
cargo test --release
cargo clippy --release            # CI: -D warnings
cargo fmt                         # CI: cargo fmt --check
.\target\release\catalyx.exe --self-test shots\st.json --no-elevate     # end-to-end, ~10 s, scratch data
.\target\release\catalyx.exe --screenshot shots\x.png --tab boost --size 1180x760 --no-elevate [--demo-session]
.\target\release\catalyx.exe play "<game>" --dry-run                    # shows the plan, changes nothing
```

Before you say a change is done run **`.\scripts\check.ps1`** (format, clippy with warnings denied, tests, build,
subsystem check, real-window self-test and start-up check; `-Quick` skips the last two), and for UI changes look at
a screenshot yourself. Other scripts in `scripts/`: `release.ps1` (check, push, CI, tag, verify the published
files: use it, do not release by hand), `screenshots.ps1`, `click-test.ps1` (real mouse), `startup-profile.ps1`.
Details in `docs/DEVELOPING.md`.

## Map

`src/lib.rs` the program (CLI first, then window), `src/main.rs` the window front-end (no console), `src/bin/catalyx-cli.rs` the console front-end | `src/cli/` commands | `src/engine/` the boost engine (types,
journal, boost, apply, running, restore, desktop) | `src/config/` settings, presets, lists | `src/gui/` window
(`pages/`, `chrome.rs`, `theme.rs`, `widgets.rs`, `mark.rs`) | `src/shell.rs` quiet desktop | `src/procs.rs`
monitor | `src/services.rs` `power.rs` `tweaks.rs` `gpu.rs` Windows operations | `src/ffi.rs` Win32 bindings.
Full table with responsibilities: `docs/ARCHITECTURE.md`.

## Rules that must not be broken

1. **Journal first.** Any change to Windows state: `push_undo(Undo::X)` (it persists the journal; if that
   returns false, do not make the change), make the change, `forget_last_undo()` if it fails, and add the arm
   in `engine/restore.rs::undo_all`. Undo is identity-checked (pid *and* start time) for process changes.
2. **Opt-in and reversible.** Nothing is closed, stopped or tuned unless the person's profile says so. New
   defaults must not opt anyone into closing programs or stopping services. Never offer Realtime priority.
3. **Verify, don't claim.** Read the result back from Windows before reporting a step as done.
4. **No network, no telemetry, no updater, no tray icon, no background service.** Do not add them.
5. **Tests and checks must never hang** (every wait has a time limit) and **must not disturb a real PC**: use
   `CATALYX_DATA` scratch folders, stand-in games (a renamed copy of `ping.exe` outside the Windows folder),
   `--dry-run`. Never run a real game or a launcher with an anti-cheat driver.
6. **Minimal footprint.** The window draws only on change and not at all when minimised; no polling loops;
   think twice before adding a dependency (the dependency list is short on purpose).
7. **Do not commit** `research/`, `shots/`, `target/` (ignored; `research/` is a private working-notes folder that may
   exist on a maintainer's machine). Do not copy text, numbers or code from other products. Derive facts from
   Microsoft's documentation or from a measurement on a real PC, and cite that.

## Adding a setting

1. `src/config/settings.rs`: field on `Settings` (+ doc comment) and its value in `impl Default for Settings`.
2. `src/config/presets.rs`: its value in Gentle and Max Squeeze (Balanced uses the default).
3. Saved files: new fields need `#[serde(default)]` (the struct has it). If existing built-in presets must change,
   bump `CONFIG_VERSION` and add an idempotent migration in `Config::migrate` (and a test, see
   `max_squeeze_gets_the_quiet_desktop_once...`). Never overwrite a person's own presets or per-game settings.
4. `src/selftest.rs`: it builds a `Settings` literal; add the field.
5. `src/cli/profiles.rs`: a key in `describe()` and `apply_setting()`; add it to the `KEYS` text in
   `src/cli/help.rs`.
6. `src/gui/pages/profiles.rs`: a `toggle(...)`; `src/gui/pages/boost.rs`: a line in "This boost will".
7. `src/engine/`: apply it (rule 1) and undo it.
8. Docs: `docs/HOW-IT-WORKS.md` (and `docs/CLI.md` if the help text changed).

## The boost control

A boost is started from exactly one place, `gui/control.rs`, in one of three looks (bar, gauge, ring) that share a Ctl state and the Act actions. Do not start a boost from anywhere else in the window: go through `request_boost` (it shows the summary dialog unless the person turned it off) and `restore` (never confirms). A new look is a new function that takes &Ctl and returns Option<Act>; add its key to `Style`, the Settings row and the CLI `theme` tokens. Boosted colours come from the palette accent, never a fixed hue.

## Design rules for the window

8-point grid; card padding 20, gap 16, controls 32 px high; four text sizes (12/14/16/22); borders only on
cards, flat hairline rows inside lists; pages centred with a max width (`widgets::page`). Reduce visual noise:
when everything is accented, nothing is, so accents tint the main elements mildly. Selectable labels are
disabled globally so the title bar can be dragged from anywhere. Look: `theme::palette(look, accent, tint,
dark)`. Check every visual change in dark and light and at the minimum size (760 x 560).

## Pitfalls we already hit

- **A minimised egui window gets no more frames.** Anything that must happen while minimised cannot live in
  `logic()`; see the one-shot trim thread in `gui/mod.rs`.
- `GetActiveWindow` is per-thread; from another thread use `ffi::our_window()` (finds the window by title).
- **UAC may be off** (every process elevated, no second token). Code that wants "the ordinary token" must
  fall back to the token the shell already has (`shell.rs`).
- Two extern declarations of the same Win32 function with different signatures warn (and are wrong): declare
  it once in `ffi.rs` and `pub` it.
- Config files and the journal are user data. Never delete or rewrite them in tests; use the scratch folder.
- Windows PowerShell: there is no `grep`; backticks inside double-quoted strings are escapes (`` `r `` is a
  carriage return); pass paths without spaces (or quote them) to `Start-Process -ArgumentList`.
- **`catalyx.exe` has no console** (it is a window program, so starting it never opens a terminal). In scripts, tests and agents call `catalyx-cli.exe` for commands: a shell does not wait for `catalyx.exe`, and its output only appears if it can join a terminal. Never make `catalyx.exe` a console program again (CI checks its PE subsystem is 2). Every helper process must be started through `ffi::run_timeout` or with `CREATE_NO_WINDOW`, and scheduled tasks must start `catalyx.exe` (`elevate::gui_exe()`), never the CLI.
- The window starts hidden and is shown after two frames (`reveal_when_ready`); do not put slow work before that, and do not make check runs (`--screenshot`, `--self-test`, `--tour`) start hidden.
- The repository is LF (`.gitattributes`, `rustfmt.toml`). Do not write CRLF.

## Style

Rust 2021, `cargo fmt` as configured, clippy clean. Comments say *why*, not what. Every OS call can fail:
handle it, log it, and fail closed (skip the change) rather than half-apply. Keep `unsafe` inside thin
wrappers in `ffi.rs`, `shell.rs`, `procs.rs`, `gpu.rs`. User-facing text is plain and friendly, no jargon
without a hint line.
