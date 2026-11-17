//! The upkeep: what must keep happening besides intents (recording a learned game program, the
//! auto-boost scan, sampling policy, live readings, debounced saving, closing, memory trims).
//!
//! TRANSITIONAL (Phase 1a T2a): the window calls `tick` once per frame, with today's cadence. T2b moves
//! this onto its own thread driven by kicks and deadlines, so it also runs while the window is minimised.

use super::intent::{Event, FrameFacts, Tone, Upkeep};
use super::state::State;
use crate::engine::{Cmd, Phase};
use crate::{ffi, gpu, procs};
use std::sync::Arc;
use std::time::{Duration, Instant};

impl State {
    pub(super) fn tick(&mut self, f: &FrameFacts) -> Upkeep {
        self.learn_game_program();
        self.auto_boost_scan();

        let minimized = f.minimised;
        let phase = self.phase();
        // Per-process scans only where they are shown (the apps list) or needed (a boost).
        procs::set_detail(f.interest.processes || phase.active() || self.check_run);
        procs::set_sample_interval(if minimized && !phase.active() {
            8000
        } else if phase.active() || f.interest.processes {
            1500
        } else {
            2500
        });
        // Live graphics card readings, only while they are on screen.
        if f.interest.gpu_live && !minimized && self.last_live.elapsed() > Duration::from_millis(1500) {
            self.last_live = Instant::now();
            let live = gpu::live();
            // No card readings at all (most PCs): nothing changed, so the view is not rebuilt.
            if live.is_some() || self.hardware.live.is_some() {
                Arc::make_mut(&mut self.hardware).live = live;
                self.rev += 1;
            }
        }
        // This window's own footprint, only while it is on screen.
        if f.interest.own_usage && self.last_own.elapsed() > Duration::from_secs(1) {
            self.last_own = Instant::now();
            let ws = ffi::own_working_set();
            if ws != self.system.own_working_set {
                self.system.own_working_set = ws;
                self.rev += 1;
            }
        }
        // Services change slowly: refresh every few seconds while they are shown.
        if f.interest.services && self.last_svc.map(|t| t.elapsed() > Duration::from_secs(5)).unwrap_or(true) {
            self.refresh_services();
        }
        // Task Scheduler is asked once (off the UI thread), not on every frame.
        if f.interest.elevation && self.system.elevation.is_none() && !self.system.elevation_busy {
            self.refresh_elevation();
        }
        // Save a moment after the last change.
        if let Some(t) = self.dirty_since {
            if t.elapsed() > Duration::from_millis(500) {
                self.save_now();
            }
        }
        // Closing during a boost puts everything back first.
        let mut cancel_close = false;
        if f.close_requested && self.closing.is_none() {
            if phase.active() {
                cancel_close = true;
                self.closing = Some(Instant::now());
                let _ = self.tx.send(Cmd::Restore);
            } else {
                self.save_now();
            }
        }
        if let Some(t) = self.closing {
            if phase == Phase::Idle || t.elapsed() > Duration::from_secs(20) {
                self.save_now();
                self.event(Event::ReadyToClose);
            }
        }
        // Footprint: hand back memory the window is not using right now (driver and font pages
        // that were only needed while drawing). They come back on demand, a few at a time.
        if self.last_trim.elapsed() > Duration::from_secs(30) && !self.selftest {
            self.last_trim = Instant::now();
            procs::trim(std::process::id());
        }
        // A minimised window is not asked to draw again, so the 30 s timer above never runs for it
        // - and minimised is exactly where it stays during a game. Trim once, just after the last
        // frame, from a short-lived thread (nothing wakes this window again until it is restored).
        if minimized && !self.trimmed_minimized && !self.selftest {
            self.trimmed_minimized = true;
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(1500));
                procs::trim(std::process::id());
            });
        } else if !minimized {
            self.trimmed_minimized = false;
        }
        Upkeep { cancel_close, closing: self.closing.is_some() }
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
    fn auto_boost_scan(&mut self) {
        if self.no_save
            || !self.cfg.auto_boost_known_games
            || self.phase() != Phase::Idle
            || self.last_auto_scan.elapsed() < Duration::from_secs(3)
        {
            return;
        }
        self.last_auto_scan = Instant::now();
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
