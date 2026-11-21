# Architecture

CatalyX is one Rust workspace of three packages: the core (no window in it), the classic skin (the window), and
the program that joins them. The core is also the console program. There is no service, no driver and no
background daemon.

```
Cargo.toml             workspace: the version, the dependency versions, one build profile
crates/catalyx-core/   everything that is not drawing (serde, serde_json, sysinfo, winreg; no egui)
  lib.rs               the public surface (app, config, catalog, format, start, trace); every other module is private
  start.rs             what either front-end does first (trace, console, hooks) and the check-run watchdog
  bin/catalyx-cli.rs   front-end 1: catalyx-cli.exe, the console program (scripts, pipes, agents)
  app/                 the core behind the window: state, intents, options, view, settings catalogue, boost description, jobs, upkeep, selftest
  cli/                 every command (mod = parsing + dispatch; one file per area)
  engine/              the boost engine (own thread): the heart of the program
  config/              settings, presets, the app/service lists, the saved config file
  trace.rs             optional start-up timing (CATALYX_TRACE=1)
  shell.rs             the quiet desktop: stop/start explorer.exe, the exit shortcut, the guard
  procs/               the process monitor (own thread), the shared sample; act.rs holds the per-process actions
  services.rs          Windows services: list, stop (with dependents), start
  power.rs             power plans through powercfg
  tweaks.rs            registry and scheduled-task changes, each remembering the old value
  gpu.rs               NVIDIA clock offsets (NvAPI) and readings (NVML)
  launcher.rs          starting a game and finding the real program behind a launcher
  library.rs           finding installed games
  catalog.rs           known apps and services with categories and suggestions
  elevate.rs           one-click administrator start and watcher tasks (Task Scheduler)
  watch.rs             `catalyx watch`: the window-less auto-boost loop
  doctor.rs            `catalyx doctor`: proves each feature against real Windows state
  ffi.rs               hand-written Win32 bindings used across the program
skins/classic/         the window: the only package that links egui/eframe
  lib.rs               the skin's entry: reads its own flags from the core's parsed options
  gui/                 the egui window: pages/, chrome (title bar + sidebar), control (the boost control), theme, widgets, the mark
app/                   the window program
  main.rs              front-end 2: catalyx.exe (no console, never a terminal), and the one line that picks the skin
  assets/catalyx.ico   the program's icon, beside the package that embeds it
  build.rs             embeds it as the program icon (no SDK resource compiler needed)
```

## Threads and processes

| Who | Role |
|---|---|
| UI thread | draws the window; sends commands; reads status |
| **Engine thread** | owns the boost session: applies changes, journals them, watches the game, restores |
| Monitor thread | samples processes, memory, CPU, GPU on an adaptive interval (2.5 s normal, 8 s minimised, 1.5 s while boosting or on the Apps page), and publishes each sample as an `Arc<Snapshot>` |
| Hotkey thread | exists only while the desktop is off; listens for Ctrl+Alt+End |
| Guard process | `catalyx shell-guard <pid>`; exists only while the desktop is off |
| Watcher process | `catalyx watch`; optional; its own engine, no window |

The UI and the engine talk through a channel (`Cmd`: `Boost`, `Restore`, `ExitQuietDesktop`) and a shared
`Status` (phase, counts, activity log, pending-journal flag). The window never changes Windows itself.

A monitor sample is shared as an `Arc<Snapshot>` (`procs::latest` / `procs::publish`): a reader takes one
Arc clone under a short lock, so reading never copies the process list and never keeps the sampler waiting
while a few hundred rows are duplicated. A sample a reader holds cannot change under it; the next one is a
new allocation. That is what makes it safe for the engine to hold a sample across a long Windows call while
the window keeps drawing, and it is asserted in `procs/mod.rs`'s tests.

## The engine

`engine::Engine` is a state machine around an optional `Session`. Files by responsibility:

| File | Responsibility |
|---|---|
| `types.rs` | `BoostRequest`, `Cmd`, `Phase`, `Counts`, `Status`, and `Undo` (one record per reversible change) |
| `journal.rs` | atomic journal writes; reading it; the quiet-desktop record helpers |
| `mod.rs` | the `Engine`/`Session` structs, the lock, the journal-push helpers, the command loop |
| `boost.rs` | validation, the order of changes, starting or attaching to the game |
| `apply.rs` | each change: power, tweaks, services, apps, lower-others, memory, GPU |
| `running.rs` | per tick: find the real game, boost it, keep apps away, thermal guard |
| `restore.rs` | undo from the session list or from a journal left by a crash |
| `desktop.rs` | quiet desktop on/off, the guard loop, `desktop test` |

**Rule for any new change:** `push_undo(Undo::X)` first (it persists the journal; if that fails, skip the
change), then make the change, and `forget_last_undo()` if it fails. Add the matching arm in
`restore.rs::undo_all`. See [AGENTS.md](../AGENTS.md#adding-a-setting).

## Configuration

`%APPDATA%\CatalyX\config.json`, carrying `CONFIG_VERSION`. There are no migrations: a file with another version
is moved to `config.json.old` and defaults are used (`config::Config::load`), so a reader never half-understands
an older shape. Presets are `Settings` bundles; a game either follows a preset or carries its own `custom`
settings plus per-game app/service overrides. Each skin's look choices live in `skins.<skin>` (a plain map the
core stores and never interprets, `docs/SKINS.md`); the classic skin's five fields moved there at the same
version, so a file without them simply loads with that skin's defaults. `CATALYX_DATA` points everything (config, journal, caches) at a
scratch folder; the built-in checks and the tests always use it.

## The window

- **egui/eframe 0.35 on OpenGL (glow).** A frameless window: `skins/classic/src/gui/chrome.rs` draws the title bar and sidebar,
  `widgets::window_edges` provides resizing, and `ffi::style_borderless_window` gives Windows 11's rounded
  corners and shadow.
- **Repaint policy** (this is why the footprint is small): the window draws only when something changes. The
  core's upkeep thread (`crates/catalyx-core/src/app/mod.rs`, `app/upkeep.rs`) wakes it when the view moved (a job result, fresh
  readings, the engine's progress) and never while it is minimised, which draws nothing. Sampled readings share
  one draw cadence, 2 s idle and 0.6 s while boosting, the same as the old frame tick; a job result or an event
  is shown at once. The same thread does the work that must not depend on frames: the debounced save, the
  auto-boost scan, live and service readings on their own deadlines, the 1.5 s trim after minimising, the 30 s
  trim, and putting a boost back before the window closes.
- **Theme**: `skins/classic/src/gui/theme.rs`'s `palette(look, accent, tint, dark)` blends a look (Focus/Slate/Mica), an accent colour and a
  tint strength. Layout uses an 8-point grid; see AGENTS.md for the design rules.
- **The mark** (`skins/classic/src/gui/mark.rs`) is one geometry used for the title-bar tile, the window icon and
  the `.ico`; `app/build.rs` embeds that `.ico` as the program's own icon.

## Check runs

`--screenshot <png> [--tab X --size WxH --accent A --appearance dark|light --demo-session]`, `--tour <dir>`
and `--self-test <json>` run the real window with demo data and a scratch data folder, under a 110-second
watchdog so a stuck check can never hang its caller. `--write-icon <path>` regenerates `app/assets/catalyx.ico`.
