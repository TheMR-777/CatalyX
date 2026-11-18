# Skins: the contract between the core and the window

> **DRAFT, second round (2026-11-15).** The first round's four open questions are decided (see the end). Nothing
> here is built yet; once the maintainer agrees, this file is the reference for anyone writing a skin.

## What a skin is

CatalyX is a **headless core** plus **exactly one GUI skin**, chosen at compile time (the CLI is always there too).
The core owns everything that is not drawing: the engine, the config, every Windows operation, and the app logic
(boost requests, auto-boost, saving, sampling, restore-before-close). A skin draws what the core shows it, tells the
core what the person asked for, and **decides everything about its own look and how far that look can be
customised**.

```
app/Cargo.toml:   skin = { path = "../skins/classic", package = "catalyx-skin-classic" }
app/src/main.rs:  fn main() { catalyx_core::run(skin::SKIN) }
```

Another look is another crate in `skins/`; the build is pointed at it by that one line. There is no runtime
switching, no plugin loader and no `dyn` indirection: the exe carries only the skin it uses.

## The rules a skin lives by

1. **A skin never changes Windows or the core's config itself.** It cannot: the core's Windows modules are private
   to the core crate. Every change is an `Intent`.
2. **Safety is the core's job.** Journal first, a summary before a boost (when the person wants one), restore never
   asks, never Realtime priority: all enforced in the core. A skin may show the summary however it likes, but it
   cannot start a boost without the core's say-so.
3. **Draw only on change.** The core wakes the skin when its `View` changes; the skin redraws then (and for its own
   input and animations). A minimised skin draws nothing. The core keeps working without it (see Upkeep).
4. **Tell the core what is on screen** (`Hint`s), so it samples only what is shown.
5. **User-facing text from the core is plain English and final** (plan lines, notices, toasts, the activity log).
   A skin may restyle it, not reword its meaning.
6. **The skin owns its customisation** (next section): the core does not know what an accent, a theme or a boost
   control "style" is.

## A skin declares itself

```rust
// in the skin crate
pub static SKIN: catalyx_core::Skin = catalyx_core::Skin {
    name: "classic",            // namespaces its saved preferences
    prefs: &PREFS,              // what a person may customise in this skin (below)
    run,                        // fn(Core, SkinOptions) -> Result<(), String>
};
```

`Skin` is plain static data plus a function pointer, resolved at compile time.

### Skin preferences: the skin decides what is customisable

Everything about the look is the skin's: whether there are themes, accents, a light mode, where the boost control
sits and in how many styles, densities, fonts, motion... A skin lists what it lets people change as static data,
using the **same spec type as the core's settings catalogue**:

```rust
static PREFS: [Spec; 6] = [
    Spec::choice("look",    "Look",          "...", &[("focus", "Focus", "..."), ("slate", ..), ("mica", ..)], "focus"),
    Spec::choice("mode",    "Mode",          "...", &[("dark", ..), ("light", ..), ("system", ..)], "dark"),
    Spec::choice("accent",  "Accent",        "...", &[("default", ..), ("ember", ..), /* ... */], "default"),
    Spec::choice("tint",    "Surface tint",  "...", &[("tint-off", ..), ("tint-subtle", ..), ("tint-rich", ..)], "tint-subtle"),
    Spec::choice("control", "Boost control", "...", &[("bar", ..), ("gauge", ..), ("ring", ..)], "bar"),
    Spec::switch("motion",  "Animations",    "...", true),
];
```

Kinds: `switch`, `choice` (each value: key, label, hint), `number` (min, max, step), `color` (hex), and `text`
(free-form, not validated beyond length: room for anything a designer wants to keep, such as a layout).

What the core does with them, and nothing more:
- **Stores** them in the config under the skin's name (`skins: { "classic": { "accent": "ember", ... } }`), so a
  different skin never reads or clobbers another's choices.
- **Validates** a change against the spec (`Intent::SetPref { key, value }`); a stored value that the skin no longer
  accepts reads as the default.
- **Shows** them in the View (`view.prefs.choice("accent")`, `.switch("motion")`, `.number(..)`, ...).
- **Exposes them on the command line** with no skin-specific code: `catalyx skin` lists them, `catalyx skin set
  accent ember` sets one, and `catalyx theme ember ring light` (the existing command) matches each word against the
  choice values of the skin's prefs. That is why value keys should be unique across a skin's choices.

How those choices are presented (a Settings page, a quick toggle in the title bar, a hidden key combination) and what
they do is entirely the skin's. Example: the classic skin's three boost-control styles become one of *its* prefs; a
new skin may have one control, or five, or let people drag it anywhere.

## The API

```rust
pub struct Core { /* cheap to clone: a handle to the core's state and threads */ }
impl Core {
    /// Called by the skin once its window exists. `wake` must be cheap and thread-safe (egui: request_repaint).
    pub fn connect(&self, wake: impl Fn() + Send + Sync + 'static);
    /// The current state. Cloning a View is a handful of Arc clones; sections are rebuilt only when they change.
    pub fn view(&self) -> View;
    /// The only way to change anything. Cheap intents (config edits) are applied before this returns, so the next
    /// view() already shows them; slow ones (Windows operations) run on core threads and report back via events.
    pub fn send(&self, intent: Intent);
    /// Things the core needs the skin to show (drained; at most 32 are kept while nobody drains them).
    pub fn take_events(&self) -> Vec<Event>;
    /// What is on screen, minimised, closing.
    pub fn hint(&self, hint: Hint);
}
```

The waker is stored once (behind an `Arc`); it is called only when the view changes, never per frame.

### View (read-only)

| Section | Contents | Changes when |
|---|---|---|
| `rev: u64` | bumps on any change; a skin may skip work when it has not moved | any change |
| `config: Arc<Config>` | the saved config, read-only (games, presets, rules, options). Helper queries: `settings_for(game)`, `apps_for`, `services_for`, `app_action(scope, exe)`, `service_stopped(scope, name)`, `profile_label(game)` | an intent edits it |
| `prefs: Arc<Prefs>` | this skin's preferences, validated against its spec | `SetPref` |
| `session: Arc<Session>` | phase, game name/id/pid/exe, counts (before/after memory and clock, apps closed/paused/lowered, services stopped...), the activity log (`LogLine`s, below), `pending_journal`, `quiet_desktop` | the engine reports |
| `boost: Arc<BoostView>` | the selected game's name and profile label, whether there is a game, the stages and whether each is done, a 0..1 progress estimate, the plan ("This boost will" lines + how many options are off), and the notices to show (interrupted boost, not administrator, nothing chosen, no game). The core supplies the facts and the plan's final text; the headline wording around a control is the skin's | config, session or service-list changes |
| `meters: Meters` | CPU %, memory used/total, GPU %, CPU name/cores/clock/base clock, `ready` | each monitor sample |
| `apps: Arc<Vec<AppRow>>` | running programs grouped by exe (name, category, count, CPU, memory, GPU, protected) plus rule-only rows | each detailed sample, only while `Interest::Processes` |
| `services: Arc<Vec<ServiceRow>>` | name, display name, running, can_stop, **can_start (cached, not asked per frame)**, catalogue entry (why, impact, recommended), protected | refreshed while `Interest::Services` |
| `hardware: Arc<Hardware>` | graphics card (name, vendor, supported, clock ranges, note), planned offsets per level, power plans and the active one, live card readings (`Interest::GpuLive`), CatalyX's own memory (`Interest::OwnUsage`) | probes finish / live ticks |
| `system` | `admin`, elevation state `(one-click task, watcher)` (`Interest::Elevation`), version | rarely |
| `library` | installed-games scan result, real-program candidates per game id, and the busy flags (`scanning`, `probing`...) | jobs finish |

`BoostView` describes the boost, not a control: a skin may draw a dial, a launch bar, a list of stages, or
show only the plan. `busy` and `boosted` are not separate fields; they follow from `session.phase`.

Games and presets are referred to by **id** everywhere in the API, never by list index (indexes go stale when
something is removed).

### The activity log: tagged lines

```rust
pub struct LogLine { pub text: String, pub tags: Tags }   // Tags: a u32 bit set, so a line can carry several
```

The engine tags each line when it writes it, instead of skins guessing from the words in it. Starting set:
- **tone** (exactly one): `OK`, `WARN`, `ERROR`, `INFO`
- **area** (zero or more): `POWER`, `WINDOWS`, `APPS`, `SERVICES`, `MEMORY`, `GPU`, `GAME`, `DESKTOP`, `RESTORE`
- **qualities** (zero or more): `VERIFIED` (read back from Windows), `NEEDS_ADMIN`, `SKIPPED`

New tags can be added without breaking skins (unknown bits are ignored). A skin can colour by tone, group by area
(for example a step list), or mark verified lines.

### Intent (every change a person can ask for)

| Group | Intents |
|---|---|
| Boost | `Boost { game: Option<GameId> }` (None = gaming mode without a game), `ConfirmBoost { dont_ask_again: bool }`, `CancelBoost`, `Restore`, `ExitQuietDesktop` |
| Games | `SelectGame(id)`, `AddGame { path, name, args }`, `AddInstalled(Vec<Installed>)`, `RemoveGame(id)`, `EditGame { id, name, launch, args }`, `SetRealProgram { id, exe: Option<String> }`, `SetGameProfile { id, preset }`, `GiveGameOwnSettings(id)`, `FollowPreset(id)`, `ScanLibrary`, `FindCandidates(id)` |
| Profiles | `NewPreset { from: Target }`, `RenamePreset { id, name }`, `DeletePreset(id)`, `ResetPreset(id)`, `SetDefaultPreset(id)`, `SetSetting { target: Target, key, value }` (keys from the catalogue below) |
| Apps | `SetAppRule { scope: Option<GameId>, exe, action }`, `UseSuggestedApps`, `ClearAppRules`, `EndAppNow { exe }` |
| Services | `SetServiceRule { scope, name, stop }`, `UseRecommendedServices`, `ClearServiceRules`, `RefreshServices` |
| Tuning | `RefreshHardware`, `UsePowerPlan(guid)`, `BuildCatalyxPlan`, `SetHeatLimit(u32)`, `TestGraphicsCard` |
| Options | `SetOption { key, value }` (restore when the game closes, keep apps off, reopen closed apps, ask before boosting, auto-boost, open as administrator) |
| System | `FinishWelcome { use_suggestions }`, `SetupOneClickStart`, `RemoveOneClickStart`, `SetWatcher(bool)`, `RelaunchAsAdmin`, `ExportConfig(path)`, `ImportConfig(path)`, `ResetConfig` |
| Skin | `SetPref { key, value }` |

Confirmation dialogs for ordinary destructive actions (remove a game, reset settings, end an app, clear rules, the
graphics test) are the skin's UX: it asks, then sends the intent. The one confirmation that is *policy* (the summary
before a boost) belongs to the core.

### Event (the core tells the skin)

| Event | Meaning |
|---|---|
| `ConfirmBoost(Plan)` | the person asked to boost and wants a summary first: show it, then send `ConfirmBoost` or `CancelBoost` |
| `Toast { tone, text }` | a short message (added, saved, failed, learned the real game program...) |
| `ReadyToClose` | after `Hint::CloseRequested`: everything is restored and saved; close the window now |
| `Exit` | the program is restarting (as administrator): close at once |

### Hint (the skin tells the core)

| Hint | Effect in the core |
|---|---|
| `Interest(set)` with `Processes`, `GpuLive`, `Services`, `Elevation`, `OwnUsage` | samples only what is shown: per-process detail and 1.5 s sampling, live card readings every 1.5 s, the service list every 5 s, a one-off task check, own memory |
| `Shown` | the window is on screen: start the hardware probes (power plans, graphics card, services), so they never compete with the first frames |
| `Minimised(bool)` | 8 s sampling, nothing live, the trim-once-after-minimise |
| `CloseRequested` | restore first if a boost is on, save, then `Event::ReadyToClose` |

Interests are about *data*, not page names, so a skin may arrange its pages however it wants.

### Upkeep: the core keeps working when the window does not draw

A minimised window gets no frames, so nothing the program must keep doing may depend on the skin drawing. The
engine already runs on its own thread (it notices a closed game and restores, whatever the window does). The rest
of the upkeep runs on a core thread driven by deadlines and kicks (an intent, a hint, a job result, an engine
status change, a monitor sample): the debounced save, recording a learned game program, the auto-boost scan for
games started outside CatalyX (every 3 s, also while minimised, and only once a game has a known program),
live readings only while a skin says it shows them and the window draws, the close-before-restore dance, and
the memory trims. That thread wakes the skin only when the view changed and the window is not minimised.
Sampled readings share one draw cadence (2 s idle, 0.6 s while boosting), so two sources do not draw twice as
often; a job result or an event is shown at once.

### The settings catalogue

One `&'static` table in the core describes every core setting once with the same `Spec` type skins use: key (as the
CLI types it), label, hint line, group, whether it needs administrator rights, and its kind. The CLI's
`describe`/`set` and its help text, the plan lines and any skin's settings form all read it, so a new setting
appears everywhere at once (today a setting is described in up to five places). Contents: the 18 `Settings` fields
(game priority, power plan, CPU performance, graphics card tuning, disk and memory priority, app rules, services,
lower everything else, free memory, flush cached memory, Game Mode, background recording, notifications, efficiency
mode, maintenance, transparency, multimedia scheduler, quiet desktop) and the app options listed under Options.

### Window helpers

A small public module in the core, `catalyx_core::window`, so a skin needs no `unsafe` of its own: the native file
dialog, borderless-window styling (rounded corners and shadow on Windows 11), the title-bar colours, "does Windows
prefer dark", and opening a folder in Explorer. They find the window by its title, so a skin's window must be titled
`CatalyX`. They change nothing outside the window.

### What data exists (design against this, nothing else)

Measured and available: CPU load/name/cores/clock (current, base, peak during a boost), memory used/total/available
(before and during a boost), GPU load per process and total, per-process CPU/memory, services and their state, power
plans, NVIDIA card temperature/clocks/load/power/video memory (NVML), session counts, the activity log, CatalyX's own
memory.

**Not available**: CPU temperature (no reliable source without a kernel driver), frame rate or input latency (not
measured), live readings for AMD/Intel cards (detected only), history (no time series is kept yet; a short ring of
samples is a planned addition for the next skin).

## Check runs

`--screenshot`, `--tour` and `--self-test` stay. The argument parsing is the core's; `SkinOptions` carries what a
skin needs (page, size, dialog, demo flags) and `--pref key=value` sets any skin preference for the run (replacing
today's `--theme`, `--accent`, `--appearance`, `--control`, which the classic skin keeps as aliases). The self-test
splits: the boost/restore part (a stand-in game, priority read back from Windows) runs in the core with no window;
the skin keeps the visual part (what was drawn). In check runs the core's demo mode supplies steady readings (today's
live graphics-card line is not steady, which makes those screenshots differ between runs).

## Decided (2026-11-15)

1. **Upkeep** runs on a core thread and keeps working while the window is minimised (see Upkeep).
2. **Skin preferences** are declared by the skin and stored per skin; the core knows no theme, accent or control
   style. This changes the config file (the classic skin's five look fields move under `skins.classic`, with a
   one-time migration) and is part of Phase 1.
3. **Window helpers** live in `catalyx_core::window`.
4. **Log lines carry tags**, several per line, in a bit set that can grow.
