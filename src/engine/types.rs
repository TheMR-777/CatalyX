//! What goes in and out of the engine: requests, commands, status, and the recovery journal's records.

use crate::config::{Game, ProcAction, Settings};
use crate::tweaks::{Hive, RegValue};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::ops::{Deref, DerefMut};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, LockResult, Mutex, MutexGuard, OnceLock, PoisonError};

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

pub type SharedStatus = Arc<StatusCell>;

/// The engine's status plus a change counter and an optional listener, so a reader can learn
/// that the status changed without polling it.
pub struct StatusCell {
    status: Mutex<Status>,
    rev: AtomicU64,
    notify: OnceLock<Box<dyn Fn() + Send + Sync>>,
}

impl StatusCell {
    pub fn new(status: Status) -> Self {
        StatusCell { status: Mutex::new(status), rev: AtomicU64::new(0), notify: OnceLock::new() }
    }

    /// Sets the function called after each change. Only the first call counts (the watcher and
    /// the command line set none).
    pub fn set_notify(&self, f: impl Fn() + Send + Sync + 'static) {
        let _ = self.notify.set(Box::new(f));
    }

    /// How many changes were made so far: a reader compares it with the last value it saw.
    pub fn rev(&self) -> u64 {
        self.rev.load(Ordering::Acquire)
    }

    /// Locks the status, shaped like `Mutex::lock` so callers keep their `Result` handling. It
    /// never returns `Err`: a thread that panicked while holding the lock leaves the status
    /// readable, because a poisoned status must not stop a restore or hide what was changed.
    pub fn lock(&self) -> LockResult<StatusGuard<'_>> {
        let guard = self.status.lock().unwrap_or_else(PoisonError::into_inner);
        Ok(StatusGuard { guard: Some(guard), cell: self, touched: false })
    }
}

/// A locked status. Reading does nothing special; any mutable access counts as a change, which
/// is announced once when the guard is dropped.
pub struct StatusGuard<'a> {
    /// Always `Some` until `drop`, which takes it out to unlock before calling the listener.
    guard: Option<MutexGuard<'a, Status>>,
    cell: &'a StatusCell,
    touched: bool,
}

impl Deref for StatusGuard<'_> {
    type Target = Status;
    fn deref(&self) -> &Status {
        self.guard.as_deref().expect("status guard used after unlock")
    }
}

impl DerefMut for StatusGuard<'_> {
    fn deref_mut(&mut self) -> &mut Status {
        self.touched = true;
        self.guard.as_deref_mut().expect("status guard used after unlock")
    }
}

impl Drop for StatusGuard<'_> {
    fn drop(&mut self) {
        // The count moves while the lock is still held, so whoever sees the new count and then
        // locks also sees the new status.
        if self.touched {
            self.cell.rev.fetch_add(1, Ordering::AcqRel);
        }
        drop(self.guard.take());
        // Called after unlocking: the listener may lock the status itself.
        if self.touched {
            if let Some(f) = self.cell.notify.get() {
                f();
            }
        }
    }
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::AtomicUsize;

    fn counted_cell() -> (Arc<StatusCell>, Arc<AtomicUsize>) {
        let cell = Arc::new(StatusCell::new(Status::default()));
        let calls = Arc::new(AtomicUsize::new(0));
        let c = calls.clone();
        cell.set_notify(move || {
            c.fetch_add(1, Ordering::SeqCst);
        });
        (cell, calls)
    }

    #[test]
    fn status_read_does_not_bump_rev_or_notify() {
        let (cell, calls) = counted_cell();
        let st = cell.lock().map(|s| s.clone()).unwrap_or_default();
        assert_eq!(st.phase, Phase::Idle);
        assert_eq!(cell.lock().map(|s| s.phase).unwrap_or(Phase::Boosted), Phase::Idle);
        assert_eq!(cell.rev(), 0);
        assert_eq!(calls.load(Ordering::SeqCst), 0);
    }

    #[test]
    fn status_write_bumps_rev_and_notifies_once() {
        let (cell, calls) = counted_cell();
        if let Ok(mut st) = cell.lock() {
            st.phase = Phase::Boosting;
        }
        assert_eq!(cell.rev(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
        assert_eq!(cell.lock().map(|s| s.phase).unwrap_or(Phase::Idle), Phase::Boosting);
    }

    #[test]
    fn status_two_writes_in_one_guard_notify_once() {
        let (cell, calls) = counted_cell();
        if let Ok(mut st) = cell.lock() {
            st.phase = Phase::Running;
            st.log.push("one".into());
        }
        assert_eq!(cell.rev(), 1);
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn status_notifier_runs_after_unlocking() {
        let cell = Arc::new(StatusCell::new(Status::default()));
        let unlocked = Arc::new(AtomicUsize::new(0));
        let (weak, u) = (Arc::downgrade(&cell), unlocked.clone());
        // try_lock, not lock: a regression must fail the test, not hang it.
        cell.set_notify(move || {
            if weak.upgrade().is_some_and(|c| c.status.try_lock().is_ok()) {
                u.fetch_add(1, Ordering::SeqCst);
            }
        });
        if let Ok(mut st) = cell.lock() {
            st.game_pid = 7;
        }
        assert_eq!(unlocked.load(Ordering::SeqCst), 1);
    }

    #[test]
    fn status_poisoned_lock_stays_usable() {
        let cell = Arc::new(StatusCell::new(Status::default()));
        let c = cell.clone();
        let _ = std::thread::spawn(move || {
            let _guard = c.lock();
            panic!("poison the status lock");
        })
        .join();
        assert!(cell.status.is_poisoned());
        if let Ok(mut st) = cell.lock() {
            st.game_pid = 9;
        }
        assert_eq!(cell.lock().map(|s| s.game_pid).unwrap_or(0), 9);
    }
}
