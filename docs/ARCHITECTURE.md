# Architecture

CatalyX is one Rust binary (`catalyx.exe`) that is a window, a command-line tool and a background watcher
depending on how it is started. There is no service, no driver and no second program.

```
src/
  main.rs            entry: CLI first, then the window; check runs (--screenshot / --self-test / --tour)
  cli/               every command (mod = parsing + dispatch; one file per area)
  engine/            the boost engine (own thread): the heart of the program
  config/            settings, presets, the app/service lists, the saved config file
  gui/               the egui window: pages/, chrome (title bar + sidebar), control (the boost control), theme, widgets, the mark
  shell.rs           the quiet desktop: stop/start explorer.exe, the exit shortcut, the guard
  procs.rs           the process monitor (own thread) and per-process operations
  services.rs        Windows services: list, stop (with dependents), start
  power.rs           power plans through powercfg
  tweaks.rs          registry and scheduled-task changes, each remembering the old value
  gpu.rs             NVIDIA clock offsets (NvAPI) and readings (NVML)
  launcher.rs        starting a game and finding the real program behind a launcher
  library.rs         finding installed games
  catalog.rs         known apps and services with categories and suggestions
  elevate.rs         one-click administrator start and watcher tasks (Task Scheduler)
  watch.rs           `catalyx watch`: the window-less auto-boost loop
  doctor.rs          `catalyx doctor`: proves each feature against real Windows state
  selftest.rs        the window's built-in end-to-end check
  ffi.rs             hand-written Win32 bindings used across the program
build.rs             embeds assets/catalyx.ico as the program icon (no SDK resource compiler needed)
```

## Threads and processes

| Who | Role |
|---|---|
| UI thread | draws the window; sends commands; reads status |
| **Engine thread** | owns the boost session: applies changes, journals them, watches the game, restores |
| Monitor thread | samples processes, memory, CPU, GPU on an adaptive interval (2.5 s normal, 8 s minimised, 1.5 s while boosting or on the Apps page) |
| Hotkey thread | exists only while the desktop is off; listens for Ctrl+Alt+End |
| Guard process | `catalyx shell-guard <pid>`; exists only while the desktop is off |
| Watcher process | `catalyx watch`; optional; its own engine, no window |

The UI and the engine talk through a channel (`Cmd`: `Boost`, `Restore`, `ExitQuietDesktop`) and a shared
`Status` (phase, counts, activity log, pending-journal flag). The window never changes Windows itself.

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

`%APPDATA%\CatalyX\config.json`, versioned (`CONFIG_VERSION`) with idempotent migrations in
`config::Config::migrate`. Presets are `Settings` bundles; a game either follows a preset or carries its own
`custom` settings plus per-game app/service overrides. `CATALYX_DATA` points everything (config, journal,
caches) at a scratch folder; the built-in checks and the tests always use it. A first start copies an older
`%APPDATA%\PureBoost` folder over once.

## The window

- **egui/eframe 0.35 on OpenGL (glow).** A frameless window: `gui/chrome.rs` draws the title bar and sidebar,
  `widgets::window_edges` provides resizing, and `ffi::style_borderless_window` gives Windows 11's rounded
  corners and shadow.
- **Repaint policy** (this is why the footprint is small): the window draws only when something changes, with
  a slow tick for live meters (2 s idle, 0.6 s while boosting) and *no* repaints while minimised. Because a
  minimised window never runs its frame logic again, `gui/mod.rs` trims the process once, 1.5 s after the
  minimise, from a short-lived thread.
- **Theme**: `theme::palette(look, accent, tint, dark)` blends a look (Focus/Slate/Mica), an accent colour and a
  tint strength. Layout uses an 8-point grid; see AGENTS.md for the design rules.
- **The mark** (`gui/mark.rs`) is one geometry used for the title-bar tile, the window icon and the `.ico`.

## Check runs

`--screenshot <png> [--tab X --size WxH --accent A --appearance dark|light --demo-session]`, `--tour <dir>`
and `--self-test <json>` run the real window with demo data and a scratch data folder, under a 110-second
watchdog so a stuck check can never hang its caller. `--write-icon <path>` regenerates `assets/catalyx.ico`.
