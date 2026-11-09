//! The boost engine: applies a profile, starts the game, finds the real game program,
//! keeps listed apps and services off while it runs, and undoes everything afterwards.
//! Every change is written to a journal first, so a crash can still be undone.

mod apply;
mod boost;
mod desktop;
mod journal;
mod restore;
mod running;
mod types;

pub use desktop::{desktop_test, run_guard};
pub use journal::{clear_shell_journal, journal_has_shell};
pub use types::*;

use crate::config::{app_is_protected, service_is_protected, CpuLevel, GpuLevel, PowerChoice, ProcAction, Settings};
use crate::procs::{self, SharedSnapshot, Snapshot};
use crate::tweaks::{self, RegSet};
use crate::{gpu, launcher, power, services, shell};
use std::collections::{BTreeMap, HashSet};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use journal::{journal_path, persist_journal};

#[link(name = "kernel32")]
extern "system" {
    fn CreateMutexW(security: *const std::ffi::c_void, owned: i32, name: *const u16) -> isize;
    fn ReleaseMutex(handle: isize) -> i32;
    fn CloseHandle(handle: *mut std::ffi::c_void) -> i32;
}

struct Session {
    req: BoostRequest,
    undo: Vec<Undo>,
    started: u64,
    started_at: Instant,
    root_pid: Option<u32>,
    plan: launcher::LaunchPlan,
    game_pid: u32,
    game_lost_at: Option<Instant>,
    /// When the current game program became the active one, and whether it was already reported
    /// as the learned program (a launcher dialog must not overwrite a known real program).
    game_since: Instant,
    learned_sent: bool,
    paused: HashSet<u32>,
    stopped_services: HashSet<String>,
    gpu_applied: bool,
    /// The quiet desktop was tried for this session (it is tried once, when the game is found).
    quiet_tried: bool,
    gpu_cand: Option<(u32, u32)>,
    last_tick: Instant,
    last_temp: Instant,
}

pub struct Engine {
    status: SharedStatus,
    snap: SharedSnapshot,
    session: Option<Session>,
    // Held by the engine thread for the entire boost; Windows releases it after a crash.
    lock_handle: isize,
}

pub fn start(snap: SharedSnapshot) -> (Sender<Cmd>, SharedStatus) {
    let (tx, rx) = std::sync::mpsc::channel();
    let status: SharedStatus = Arc::new(Mutex::new(Status::default()));
    let st = status.clone();
    std::thread::spawn(move || {
        let mut e = Engine { status: st, snap, session: None, lock_handle: 0 };
        e.run(rx);
    });
    (tx, status)
}

impl Engine {
    fn acquire_lock(&mut self) -> bool {
        if self.lock_handle != 0 {
            return true;
        }
        let name: Vec<u16> = "Local\\CatalyX.ActiveBoost".encode_utf16().chain(Some(0)).collect();
        let handle = unsafe { CreateMutexW(std::ptr::null(), 0, name.as_ptr()) };
        if handle == 0 {
            self.log("Could not reserve a safe boost session");
            return false;
        }
        let result = unsafe { crate::ffi::WaitForSingleObject(handle as *mut std::ffi::c_void, 0) };
        if result != 0 && result != 0x80 {
            unsafe {
                CloseHandle(handle as *mut std::ffi::c_void);
            }
            self.log("Another CatalyX window is already boosting; restore it before starting a second session");
            return false;
        }
        self.lock_handle = handle;
        true
    }

    fn release_lock(&mut self) {
        if self.lock_handle != 0 {
            unsafe {
                ReleaseMutex(self.lock_handle);
                CloseHandle(self.lock_handle as *mut std::ffi::c_void);
            }
            self.lock_handle = 0;
        }
    }

    fn log(&self, s: impl Into<String>) {
        let s = s.into();
        if let Ok(mut st) = self.status.lock() {
            st.log.push(s);
            let n = st.log.len();
            if n > 200 {
                st.log.drain(0..n - 200);
            }
        }
    }

    fn set_phase(&self, p: Phase) {
        if let Ok(mut st) = self.status.lock() {
            st.phase = p;
        }
    }

    fn counts(&self, f: impl FnOnce(&mut Counts)) {
        if let Ok(mut st) = self.status.lock() {
            f(&mut st.counts);
        }
    }

    fn snapshot(&self) -> Snapshot {
        self.snap.lock().map(|s| s.clone()).unwrap_or_default()
    }

    /// Turns the per-process scan on and waits (up to limit) for a sample that really scanned.
    fn fresh_snapshot(&self, limit: Duration) -> Snapshot {
        procs::set_detail(true);
        let serial0 = self.snapshot().serial;
        let t = Instant::now();
        loop {
            let s = self.snapshot();
            if (s.detailed && s.serial > serial0) || t.elapsed() > limit {
                return s;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }

    /// Return false when a change cannot be recovered; the caller must skip that change.
    fn push_undo(&mut self, u: Undo) -> bool {
        let Some(s) = &self.session else { return false };
        if !s.req.dry_run {
            let mut next = s.undo.clone();
            next.push(u.clone());
            if let Err(e) = persist_journal(&next) {
                self.log(format!("Change skipped: recovery journal could not be saved ({e})"));
                return false;
            }
        }
        if let Some(s) = &mut self.session {
            s.undo.push(u);
        }
        true
    }

    /// Record a whole group of changes with ONE journal write (instead of one per change), before
    /// any of them is made. Returns false when the journal cannot be saved; then nothing may be done.
    fn push_undos(&mut self, items: Vec<Undo>) -> bool {
        let Some(s) = &self.session else { return false };
        if items.is_empty() {
            return true;
        }
        if !s.req.dry_run {
            let mut next = s.undo.clone();
            next.extend(items.iter().cloned());
            if let Err(e) = persist_journal(&next) {
                self.log(format!("Changes skipped: recovery journal could not be saved ({e})"));
                return false;
            }
        }
        if let Some(s) = &mut self.session {
            s.undo.extend(items);
        }
        true
    }

    /// Drop recorded changes that turned out not to happen (one journal write for the lot).
    fn forget_undos(&mut self, gone: impl Fn(&Undo) -> bool) {
        let Some(s) = &mut self.session else { return };
        let before = s.undo.len();
        s.undo.retain(|u| !gone(u));
        if s.undo.len() != before && !s.req.dry_run {
            let list = s.undo.clone();
            if list.is_empty() {
                let _ = std::fs::remove_file(journal_path());
            } else {
                let _ = persist_journal(&list);
            }
        }
    }

    /// A change was recorded for recovery but then failed (usually "access denied"): take the
    /// record back out, so a failed change can never leave a boost that "needs restoring".
    fn forget_last_undo(&mut self) {
        let Some(s) = &mut self.session else { return };
        if s.undo.pop().is_some() && !s.req.dry_run {
            let list = s.undo.clone();
            if list.is_empty() {
                let _ = std::fs::remove_file(journal_path());
            } else {
                let _ = persist_journal(&list);
            }
        }
    }

    fn dry(&self) -> bool {
        self.session.as_ref().map(|s| s.req.dry_run).unwrap_or(false)
    }

    fn run(&mut self, rx: Receiver<Cmd>) {
        // A session that ended badly (crash, power cut) may have left Windows' desktop switched off.
        // That one thing is put back at once, without asking: nobody wants to find no taskbar.
        if journal_has_shell() {
            if self.acquire_lock() {
                self.log("A previous session left the Windows desktop switched off: bringing it back");
                self.restore_journal();
                self.release_lock();
            } else if let Ok(mut st) = self.status.lock() {
                st.log.clear(); // another CatalyX owns that session; not our business
            }
        }
        loop {
            match rx.recv_timeout(Duration::from_millis(500)) {
                Ok(Cmd::Boost(req)) => {
                    if self.session.is_some() {
                        self.restore();
                    }
                    // Reserve the recovery journal before checking it: two GUI/CLI processes
                    // must never race to change Windows or overwrite one another's undo list.
                    if !req.dry_run && !self.acquire_lock() {
                        continue;
                    }
                    // A previous interrupted boost must be restored before beginning another.
                    // In particular, do not replace its journal with a fresh session's first entry.
                    if !req.dry_run && journal_path().exists() {
                        self.log("A previous boost needs restoring first; choose Restore to normal mode");
                        if let Ok(mut st) = self.status.lock() {
                            st.phase = Phase::Idle;
                            st.pending_journal = true;
                        }
                        self.release_lock();
                        continue;
                    }
                    self.boost(*req);
                    if self.session.is_none() {
                        self.release_lock();
                    }
                }
                Ok(Cmd::Restore) => {
                    if self.session.is_some() {
                        self.restore();
                    } else if self.acquire_lock() {
                        self.restore_journal();
                        self.release_lock();
                    }
                }
                Ok(Cmd::ExitQuietDesktop) => self.exit_quiet_desktop("asked for"),
                Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
                Err(_) => {
                    if self.session.is_some() {
                        self.restore();
                    }
                    return;
                }
            }
            if self.session.is_some() && shell::take_exit_request() {
                self.exit_quiet_desktop("shortcut pressed");
            }
            let due = self.session.as_ref().map(|s| s.last_tick.elapsed() >= Duration::from_millis(1500)).unwrap_or(false);
            if due {
                self.tick();
            }
        }
    }
}
