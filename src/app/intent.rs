//! What a skin can ask of the core (`Intent`), what the core asks the skin to show (`Event`), and what
//! the skin tells the core about each frame (`FrameFacts`, the forerunner of the `Hint`s in docs/SKINS.md).

use crate::config::Config;

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
    RefreshServices,
    /// Apply the graphics card's Safe step briefly, read it back, undo it.
    TestGraphicsCard,
    /// Run a CatalyX command as administrator (one prompt) and report back with `done` on success.
    RunElevated {
        args: &'static str,
        done: &'static str,
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
}

/// TRANSITIONAL (Phase 1a T2a, replaced by hints in T2b): what the window knows about this frame.
#[derive(Clone, Copy, Default, Debug)]
pub struct FrameFacts {
    pub interest: Interest,
    pub minimised: bool,
    /// The person (or Windows) asked the window to close in this frame.
    pub close_requested: bool,
}

/// What a tick asks of the window right now.
#[derive(Clone, Copy, Default, Debug)]
pub struct Upkeep {
    /// Keep the window open: a boost is being put back first (`Event::ReadyToClose` follows).
    pub cancel_close: bool,
    /// A close is waiting for the restore to finish.
    pub closing: bool,
}
