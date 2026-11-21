//! What a skin can ask of the core (`Intent`), what the core asks the skin to show (`Event`), and what
//! the skin tells the core about itself (`Hint`). See docs/SKINS.md for the contract.
//!
//! Games and presets are named by id, never by list position: an index goes stale as soon as
//! something is removed, and a skin holding one would silently change the wrong thing.

use super::boost::Plan;
use super::spec::Target;
use crate::config::{CpuLevel, ProcAction};
use crate::engine::Tags;

/// A game, by the id the config gives it.
pub type GameId = String;
/// A preset, by the id the config gives it (the built-ins keep their fixed ids).
pub type PresetId = String;

/// Every change a person can ask for.
pub enum Intent {
    /// Boost, starting this game (None = gaming mode without a game). The core decides whether the
    /// summary comes first (`Event::ConfirmBoost`).
    Boost {
        game: Option<GameId>,
    },
    /// The person read the summary and wants the boost.
    ConfirmBoost {
        game: Option<GameId>,
        dont_ask_again: bool,
    },
    /// The person closed the summary without boosting.
    CancelBoost,
    /// Put everything back. Never asks.
    Restore,
    ExitQuietDesktop,

    // ---------------------------------------------------------------- games
    /// Make this game the one the boost starts.
    SelectGame {
        id: GameId,
    },
    /// Add a game from a file or a store link (form, drag-and-drop, installed list). `args` are the
    /// optional launch arguments; `real_exe` is the program a store already told us about.
    AddGame {
        path: String,
        name: Option<String>,
        args: String,
        real_exe: Option<String>,
    },
    /// Forget a game in the library (the person confirmed it). The game itself is not touched.
    RemoveGame {
        id: GameId,
    },
    /// Change a game's own entry: its name, what it starts, and its arguments.
    EditGame {
        id: GameId,
        name: String,
        launch: String,
        args: String,
    },
    /// The program the boost should find the game by. None = find it automatically on launch.
    SetRealProgram {
        id: GameId,
        exe: Option<String>,
    },
    /// Follow this preset again (its own settings, if any, are dropped).
    SetGameProfile {
        id: GameId,
        preset: PresetId,
    },
    /// Give this game a copy of its current settings to change on its own.
    GiveGameOwnSettings {
        id: GameId,
    },
    /// Drop a game's own settings and follow its preset again.
    FollowPreset {
        id: GameId,
    },
    /// Find what Steam, Epic and GOG installed (off the UI thread).
    ScanLibrary,
    /// Forget the installed-games list (the person hid it).
    HideInstalled,
    /// Read the programs in a game's folder once, as candidates for its real program.
    FindCandidates {
        id: GameId,
        folder: String,
    },

    // ---------------------------------------------------------------- profiles
    /// A new preset holding a copy of this target's settings. The core appends it.
    NewPreset {
        from: Target,
    },
    /// Rename a person's own preset (a built-in keeps its name).
    RenamePreset {
        id: PresetId,
        name: String,
    },
    /// Delete a person's own preset; its games and the default move to a built-in one.
    DeletePreset {
        id: PresetId,
    },
    /// Put a built-in preset's settings back.
    ResetPreset {
        id: PresetId,
    },
    /// Use this preset for new games and for Boost only.
    SetDefaultPreset {
        id: PresetId,
    },
    /// Change one profile setting by its catalogue key and CLI value (`app/spec.rs`). A preset, or a
    /// game that already has its own settings; the core ignores a target that is gone.
    SetSetting {
        target: Target,
        key: String,
        value: String,
    },

    // ---------------------------------------------------------------- apps and services
    /// Rule for a program while playing: a game's own rule, or the global one (scope None).
    SetAppRule {
        scope: Option<GameId>,
        exe: String,
        action: ProcAction,
    },
    /// Fill the suggestions into this scope; an answer the person already gave is kept.
    UseSuggestedApps {
        scope: Option<GameId>,
    },
    /// Set every app back to Keep, for all games.
    ClearAppRules,
    /// End every running copy of this program (the person confirmed it). Runs off the UI thread and
    /// checks each pid's start time before ending it.
    EndApp {
        exe: String,
        name: String,
    },
    /// Rule for a Windows service: a game's own rule, or the global one (scope None).
    SetServiceRule {
        scope: Option<GameId>,
        name: String,
        stop: bool,
    },
    /// Fill the recommended services into this scope; a choice the person made is kept.
    UseRecommendedServices {
        scope: Option<GameId>,
    },
    /// Keep every service running, for all games.
    ClearServiceRules,
    /// What the welcome and the Boost notice offer: the app and service suggestions together.
    UseSuggestions {
        scope: Option<GameId>,
    },

    // ---------------------------------------------------------------- hardware
    RefreshHardware,
    /// Apply the graphics card's Safe step briefly, read it back, undo it.
    TestGraphicsCard,
    /// Switch Windows to this power plan now (an explicit choice, like Windows' own setting).
    UsePowerPlan {
        guid: String,
    },
    /// Build or refresh CatalyX's own power plan for this CPU level (Off builds the Boost level).
    CreateBoostPlan {
        level: CpuLevel,
    },
    /// Clocks go back to normal above this temperature.
    SetHeatLimit {
        c: u32,
    },

    // ---------------------------------------------------------------- system
    /// The welcome is over (or dismissed): remember it, with the suggestions if the person asked.
    FinishWelcome {
        use_suggestions: bool,
    },
    /// Set up the elevated one-click start (one Windows prompt).
    SetupOneClickStart,
    /// Remove the elevated one-click start.
    RemoveOneClickStart,
    /// Turn the small background watcher on or off.
    SetWatcher {
        on: bool,
    },
    /// Save, put everything back, then open CatalyX again as administrator and close this copy.
    RelaunchAsAdmin,
    ExportConfig(String),
    ImportConfig(String),
    /// Forget all games, rules and presets, starting again with the suggestions.
    ResetConfig,
    /// Change one app option by its catalogue key and on/off value.
    SetOption {
        key: String,
        value: String,
    },
    /// One of the skin's own preferences, saved under the skin's name (`CoreOptions::skin`, see
    /// `docs/SKINS.md`). The core stores the value without ever looking at what it means; the name
    /// is established once at start, so a skin cannot write another skin's choices.
    SetPref {
        key: &'static str,
        value: String,
    },
}

/// Things the core needs the skin to show.
#[derive(Debug)]
pub enum Event {
    /// The person asked to boost and wants a summary first: show this plan, then send `ConfirmBoost`
    /// or `CancelBoost`. The plan is a snapshot for exactly that game, made when it was asked for.
    ConfirmBoost { game: Option<GameId>, plan: Plan },
    Toast {
        /// The same tone vocabulary as the activity log: `OK`, `WARN`, `ERROR`, or no tone bit for a
        /// plain message. The skin picks the colours.
        tone: Tags,
        text: String,
    },
    /// After a close request: everything is restored and saved; close the window now.
    ReadyToClose,
    /// The elevated copy is starting: this one saved its changes, put Windows back, and may close.
    ReadyToQuit,
}

/// What is on screen, so the core samples only what is shown. About data, not page names.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Interest {
    /// Per-process figures (the apps list).
    pub processes: bool,
    /// Live graphics card readings.
    pub gpu_live: bool,
    /// The service list.
    pub services: bool,
    /// Whether the one-click task and the watcher exist.
    pub elevation: bool,
    /// This window's own memory use (Settings > About).
    pub own_usage: bool,
}

/// What a skin tells the core about itself: what it is showing, whether it draws at all, and when it
/// was first seen or is asked to close. Sent only when it changes; the core acts before `hint` returns.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Hint {
    /// What is on screen, so the core samples only what is shown. About data, not page names.
    Interest(Interest),
    /// The window is on screen for the first time: start the probes that must not compete with the
    /// first frames. Later hints of this kind are ignored.
    Shown,
    /// The window is minimised: it draws nothing, so live readings pause and the working set is
    /// trimmed once shortly after the last frame. False means it draws again.
    Minimised(bool),
    /// The person (or Windows) asked the window to close: put a running boost back first, save, then
    /// answer with `Event::ReadyToClose`. The skin keeps the window open until that arrives.
    CloseRequested,
}
