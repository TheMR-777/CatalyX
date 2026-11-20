//! The upkeep: what must keep happening besides intents (recording a learned game program, the
//! auto-boost scan, sampling policy, live readings, debounced saving, closing, memory trims).
//!
//! It runs on the core's upkeep thread (`Core::start`), so it keeps working while the window is
//! minimised or busy. `State::upkeep` does one pass and says when it next needs to run; the thread
//! sleeps until then or until a kick (an intent, a hint, a job result, an engine status change, a
//! monitor sample).

use super::intent::{Event, Tone};
use super::state::State;
use crate::engine::{Cmd, Phase};
use crate::{ffi, gpu, procs};
use std::sync::Arc;
use std::time::{Duration, Instant};

/// How often live readings are taken while a skin shows them. This is also the fastest sampled
/// readings may wake the skin: the old window's 2 s idle / 0.6 s boosting cadence.
pub(super) const LIVE_EVERY: Duration = Duration::from_secs(2);
pub(super) const LIVE_EVERY_BOOST: Duration = Duration::from_millis(600);
const SERVICES_EVERY: Duration = Duration::from_secs(5);
/// A change is written this long after the last edit, so a burst of edits writes once.
const SAVE_AFTER: Duration = Duration::from_millis(500);
/// Hand memory the window is not using back to Windows; it comes back on demand.
const TRIM_EVERY: Duration = Duration::from_secs(30);
/// How long after the last frame of a minimised window it is trimmed.
pub(super) const TRIM_AFTER_MINIMISE: Duration = Duration::from_millis(1500);
/// A restore that has not finished by then is not worth holding the close for.
const CLOSE_CAP: Duration = Duration::from_secs(20);
const AUTO_SCAN_EVERY: Duration = Duration::from_secs(3);

impl State {
    /// One upkeep pass at `now`. Returns when it next wants to run (None: only a kick can make more
    /// work). Jobs go into `self.jobs`, for the thread to start with the lock released.
    pub(super) fn upkeep(&mut self, now: Instant) -> Option<Instant> {
        let mut next: Option<Instant> = None;
        self.sync();
        self.learn_game_program();

        // A game started outside CatalyX (a store, a shortcut) is the only way a boost can begin
        // while the window is minimised, so the scan keeps its cadence even then (decided).
        if !self.no_save && self.cfg.auto_boost_known_games && self.phase() == Phase::Idle {
            if now >= self.last_auto_scan + AUTO_SCAN_EVERY {
                self.auto_boost_scan(now);
            }
            soonest(&mut next, self.last_auto_scan + AUTO_SCAN_EVERY);
        }

        // Per-process scans only where they are shown (the apps list) or needed (a boost). The
        // monitor keeps its own cadence in between; these two values are what it reads.
        let phase = self.phase();
        procs::set_detail(self.interest.processes || phase.active() || self.check_run);
        procs::set_sample_interval(if self.minimised && !phase.active() {
            8000
        } else if phase.active() || self.interest.processes {
            1500
        } else {
            2500
        });

        // Live graphics card readings, only while they are on screen and the window draws.
        let live_every = self.live_every();
        if self.interest.gpu_live && !self.minimised {
            if now >= self.last_live + live_every {
                self.last_live = now;
                let live = gpu::live();
                // No card readings at all (most PCs): nothing changed, so the view is not rebuilt.
                if live.is_some() || self.hardware.live.is_some() {
                    Arc::make_mut(&mut self.hardware).live = live;
                    self.rev += 1;
                }
            }
            soonest(&mut next, self.last_live + live_every);
        }
        // This window's own footprint, only while it is on screen.
        if self.interest.own_usage && !self.minimised {
            if now >= self.last_own + live_every {
                self.last_own = now;
                let ws = ffi::own_working_set();
                if ws != self.system.own_working_set {
                    self.system.own_working_set = ws;
                    self.rev += 1;
                }
            }
            soonest(&mut next, self.last_own + live_every);
        }
        // Services change slowly: refresh every few seconds while they are shown. A refresh already
        // running is not asked again; its result kicks the next pass. A minimised window draws no
        // list, so it asks for none (the old per-frame upkeep never ran for it either).
        if !self.minimised && self.interest.services && !self.svc_busy {
            if now >= self.last_svc.map_or(now, |t| t + SERVICES_EVERY) {
                self.refresh_services();
            }
            if let Some(t) = self.last_svc {
                soonest(&mut next, t + SERVICES_EVERY);
            }
        }
        // Task Scheduler is asked once per visit (off this thread), not on every pass.
        if !self.minimised && self.interest.elevation && self.system.elevation.is_none() && !self.system.elevation_busy {
            self.refresh_elevation();
        }
        // Save a moment after the last change.
        if let Some(t) = self.dirty_since {
            if now >= t + SAVE_AFTER {
                self.save_now();
            } else {
                soonest(&mut next, t + SAVE_AFTER);
            }
        }
        // Closing during a boost: the engine puts everything back, then the window may go (the skin
        // keeps it open until ReadyToClose). The cap stops a stuck restore from holding it forever.
        if let Some(t) = self.closing {
            if self.phase() == Phase::Idle || now >= t + CLOSE_CAP {
                self.closing = None;
                self.save_now();
                self.event(Event::ReadyToClose);
                // The window must run once more to see this, even if it was minimised meanwhile.
                self.wake = true;
            } else {
                soonest(&mut next, t + CLOSE_CAP);
            }
        }
        // Footprint: hand back memory the window is not using right now (driver and font pages that
        // were only needed while drawing). They come back on demand, a few at a time.
        if !self.selftest {
            if now >= self.last_trim + TRIM_EVERY {
                self.last_trim = now;
                procs::trim(std::process::id());
            }
            soonest(&mut next, self.last_trim + TRIM_EVERY);
        }
        // A window that just stopped drawing is trimmed once from here: the 30 s timer is not what
        // matters at the moment the game goes fullscreen.
        if let Some(t) = self.trim_due {
            if now >= t {
                self.trim_due = None;
                procs::trim(std::process::id());
            } else {
                soonest(&mut next, t);
            }
        }
        next
    }

    /// Records the real game program the engine found behind a launcher.
    fn learn_game_program(&mut self) {
        // Only take it when there is one: a mutable access counts as a status change.
        let learned = match self.status.lock() {
            Ok(mut s) if s.learned.is_some() => s.learned.take(),
            _ => None,
        };
        if let Some((gid, exe)) = learned {
            if let Some(i) = self.cfg.games.iter().position(|g| g.id == gid) {
                if self.cfg.games[i].real_exe != exe {
                    self.cfg_mut().games[i].real_exe = exe.clone();
                    self.toast(Tone::Ok, format!("Learned the real game program: {exe}"));
                    self.mark_dirty();
                }
            }
        }
    }

    /// Boost a known game that was started outside CatalyX (a store launcher, a shortcut).
    /// Uses the cheap process list, not the full monitor.
    fn auto_boost_scan(&mut self, now: Instant) {
        self.last_auto_scan = now;
        // With no known program there is nothing a process walk could match; skip it entirely.
        if !self.cfg.games.iter().any(|g| !g.real_exe.is_empty()) {
            return;
        }
        let list = ffi::quick_list();
        let mut hit: Option<(usize, u32)> = None;
        for (i, g) in self.cfg.games.iter().enumerate() {
            let real = g.real_exe.to_lowercase();
            if real.is_empty() {
                continue;
            }
            if let Some((pid, _, _)) = list.iter().find(|(pid, _, name)| *name == real && !self.auto_ignore.contains(pid)) {
                hit = Some((i, *pid));
                break;
            }
        }
        if let Some((i, pid)) = hit {
            self.auto_ignore.insert(pid);
            procs::set_detail(true);
            let g = self.cfg.games[i].clone();
            let req = self.request_for(Some(&g), Some(pid));
            let _ = self.tx.send(Cmd::Boost(Box::new(req)));
            self.toast(Tone::Info, format!("{} started - boosting automatically", g.name));
        }
    }
}

/// Keeps the earliest of two deadlines.
fn soonest(next: &mut Option<Instant>, t: Instant) {
    let at = next.map_or(t, |n| n.min(t));
    *next = Some(at);
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app::Interest;
    use crate::config::Config;
    use crate::engine::{Status, StatusCell};
    use crate::procs::Snapshot;
    use std::sync::mpsc::channel;
    use std::sync::Mutex;

    /// A State that saves nothing (so nothing can be written to a real config file).
    fn state() -> State {
        let (tx, _rx) = channel();
        let status = Arc::new(StatusCell::new(Status::default()));
        let snap = Arc::new(Mutex::new(Arc::new(Snapshot::default())));
        State::new(Config::default(), true, false, false, tx, status, snap, false)
    }

    #[test]
    fn a_change_is_saved_once_it_has_settled() {
        let mut st = state();
        st.mark_dirty();
        let now = Instant::now();
        let due = st.upkeep(now).expect("a save is due");
        assert!(due <= now + Duration::from_millis(500), "the save waits for the burst of edits to end");
        assert!(st.dirty_since.is_some());
        st.upkeep(now + Duration::from_secs(1));
        assert!(st.dirty_since.is_none(), "and then runs");
    }

    #[test]
    fn nothing_live_is_read_while_the_window_draws_nothing() {
        let mut st = state();
        st.interest = Interest { gpu_live: true, own_usage: true, services: true, ..Default::default() };
        st.minimised = true;
        let (live, own) = (st.last_live, st.last_own);
        let now = Instant::now();
        let next = st.upkeep(now).expect("the trim still falls due");
        assert_eq!(st.last_live, live, "no card readings while nothing is drawn");
        assert_eq!(st.last_own, own, "no memory readings either");
        assert!(st.last_svc.is_none(), "and no service list");
        assert!(st.jobs.is_empty(), "nothing is started");
        assert!(next > now, "the next pass waits for the trim");
    }
}
