//! What goes in and out of the engine: requests, commands, status, and the recovery journal's records.

use crate::config::{Game, ProcAction, Settings};
use crate::tweaks::{Hive, RegValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};

use super::journal::journal_path;

#[derive(Clone, Debug)]
pub struct BoostRequest {
    pub game: Option<Game>,
    pub settings: Settings,
    pub apps: BTreeMap<String, ProcAction>,
    pub services: BTreeMap<String, bool>,
    pub keep_apps_off: bool,
    pub restore_when_game_closes: bool,
    /// Start closed apps again when restoring (off by default).
    pub reopen_closed_apps: bool,
    pub gpu_temp_limit: u32,
    pub dry_run: bool,
    /// Boost a game that is already running (auto boost) instead of starting it.
    pub attach_pid: Option<u32>,
}

pub enum Cmd {
    Boost(Box<BoostRequest>),
    Restore,
    /// Bring Windows' desktop back now, but keep the rest of the boost.
    ExitQuietDesktop,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    Idle,
    Boosting,
    Finding,
    Running,
    Boosted,
    Restoring,
}

impl Phase {
    pub fn label(self) -> &'static str {
        match self {
            Phase::Idle => "Normal mode",
            Phase::Boosting => "Boosting...",
            Phase::Finding => "Looking for the game...",
            Phase::Running => "Game running - boosted",
            Phase::Boosted => "Gaming mode on",
            Phase::Restoring => "Restoring...",
        }
    }
    pub fn active(self) -> bool {
        !matches!(self, Phase::Idle)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Counts {
    pub apps_closed: u32,
    pub apps_paused: u32,
    pub apps_lowered: u32,
    pub services_stopped: u32,
    pub tweaks: u32,
    pub ram_freed: u64,
    pub gpu_core_mhz: i32,
    pub gpu_mem_mhz: i32,
    pub power_plan: String,
    pub planned: u32,
    /// The game's own priority was raised (set when the real change succeeded).
    pub game_priority: bool,
    /// What the CPU level did: label such as "Boost" or "Max" (empty = untouched).
    pub cpu_level: String,
    /// Available memory (bytes) and processor clock (MHz) when the boost began, and the latest
    /// readings while it runs: the proof that a boost did something.
    pub free_before: u64,
    pub free_now: u64,
    pub mhz_before: u32,
    pub mhz_now: u32,
    pub mhz_peak: u32,
    /// Game disk and memory priority were raised.
    pub io_boost: bool,
    /// Standby memory flushed.
    pub standby_purged: bool,
    /// Windows' desktop was switched off for the game.
    pub quiet_desktop: bool,
}

#[derive(Clone, Debug)]
pub struct Status {
    pub phase: Phase,
    pub game_name: String,
    pub game_id: String,
    pub game_pid: u32,
    pub game_exe: String,
    pub log: Vec<String>,
    pub counts: Counts,
    /// (game id, real program name) found behind a launcher; the UI saves it to the profile.
    pub learned: Option<(String, String)>,
    pub dry_run: bool,
    pub restored_runs: u32,
    pub pending_journal: bool,
    /// Windows' desktop is switched off right now (the quiet desktop).
    pub quiet_desktop: bool,
}

impl Default for Status {
    fn default() -> Self {
        Status {
            phase: Phase::Idle,
            game_name: String::new(),
            game_id: String::new(),
            game_pid: 0,
            game_exe: String::new(),
            log: Vec::new(),
            counts: Counts::default(),
            learned: None,
            dry_run: false,
            restored_runs: 0,
            pending_journal: journal_path().exists(),
            quiet_desktop: false,
        }
    }
}

pub type SharedStatus = Arc<Mutex<Status>>;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum Undo {
    Power {
        prev: String,
    },
    Reg {
        hive: Hive,
        path: String,
        name: String,
        prev: Option<RegValue>,
    },
    Task {
        name: String,
        was_enabled: bool,
    },
    Service {
        name: String,
    },
    Resume {
        pid: u32,
        name: String,
        #[serde(default)]
        start: u64,
    },
    Priority {
        pid: u32,
        class: u32,
        #[serde(default)]
        start: u64,
    },
    IoPriority {
        pid: u32,
        level: u32,
        start: u64,
    },
    Throttling {
        pid: u32,
        start: u64,
        control: u32,
        state: u32,
    },
    Gpu {
        core: i32,
        mem: i32,
    },
    /// Windows' desktop (explorer.exe) was switched off for the game: start it again, as the user.
    Shell,
    /// Start a closed app again from its program file, in its own folder.
    Reopen {
        exe: String,
        #[serde(default)]
        pid: u32,
        #[serde(default)]
        start: u64,
    },
}
