//! What a skin can ask of the core (`Intent`), what the core asks the skin to show (`Event`), and what
//! the skin tells the core about itself (`Hint`). See docs/SKINS.md for the contract.

use crate::config::{Config, CpuLevel};

/// Every change a person can ask for. Games are still referred to by list index here; ids come with T5.
pub enum Intent {
    /// Boost, starting this game (None = gaming mode without a game). The core decides whether the
    /// summary comes first (`Event::ConfirmBoost`).
    Boost {
        game: Option<usize>,
    },
    /// The person read the summary and wants the boost.
    ConfirmBoost {
        game: Option<usize>,
        dont_ask_again: bool,
    },
    /// The person closed the summary without boosting.
    CancelBoost,
    /// Put everything back. Never asks.
    Restore,
    ExitQuietDesktop,
    /// Add a game from a file or a store link (form, drag-and-drop, installed list). `real_exe` is the
    /// program a store already told us about.
    AddGame {
        path: String,
        name: Option<String>,
        real_exe: Option<String>,
    },
    /// Find what Steam, Epic and GOG installed (off the UI thread).
    ScanLibrary,
    /// Forget the installed-games list (the person hid it).
    HideInstalled,
    /// Read the programs in a game's folder once, as candidates for its real program.
    FindCandidates {
        id: String,
        folder: String,
    },
    RefreshHardware,
    /// Ask for the service list now rather than at the next regular refresh.
    #[allow(dead_code)] // part of the skin contract; the classic skin refreshes through Hint::Interest
    RefreshServices,
    /// Apply the graphics card's Safe step briefly, read it back, undo it.
    TestGraphicsCard,
    /// Run a CatalyX command as administrator (one prompt) and report back with `done` on success.
    RunElevated {
        args: &'static str,
        done: &'static str,
    },
    /// End every running copy of this program (the person confirmed it). Runs off the UI thread and
    /// checks each pid's start time before ending it.
    EndApp {
        exe: String,
        name: String,
    },
    /// Save, put everything back, then open CatalyX again as administrator and close this copy.
    RelaunchAdmin,
    /// Switch Windows to this power plan now (an explicit choice, like Windows' own setting).
    SetPowerPlan {
        guid: String,
    },
    /// Build or refresh CatalyX's own power plan for this CPU level (Off builds the Boost level).
    CreateBoostPlan {
        level: CpuLevel,
    },
    ExportConfig(String),
    ImportConfig(String),
    /// TRANSITIONAL (Phase 1a T2a, removed in T5): a page edits the config directly. Applied at once,
    /// marks the config for saving.
    Edit(Box<dyn FnOnce(&mut Config) + Send>),
}

/// How a toast reads; the skin picks the colours.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tone {
    Ok,
    Info,
    Error,
}

/// Things the core needs the skin to show.
#[derive(Debug)]
pub enum Event {
    /// The person asked to boost and wants a summary first: show it, then send `ConfirmBoost` or
    /// `CancelBoost`.
    ConfirmBoost {
        game: Option<usize>,
    },
    Toast {
        tone: Tone,
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
