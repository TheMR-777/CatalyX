//! While a game runs: finding the real game program behind a launcher, boosting it, keeping listed
//! apps and services away, and the graphics card's thermal guard.

use super::*;

impl Engine {
    // ------------------------------------------------------------------ while running

    pub(super) fn tick(&mut self) {
        let snap = self.snapshot();
        if snap.serial > 0 {
            let (free, mhz) = (snap.mem_free(), snap.cpu_mhz);
            self.counts(|c| {
                c.free_now = free;
                c.mhz_now = mhz;
                c.mhz_peak = c.mhz_peak.max(mhz);
            });
        }
        let Some(sess) = &mut self.session else { return };
        sess.last_tick = Instant::now();
        let has_game = sess.req.game.is_some() || sess.req.attach_pid.is_some();
        let real = sess.req.game.as_ref().map(|g| g.real_exe.clone()).unwrap_or_default();
        let since = sess.started;
        let elapsed = sess.started_at.elapsed();

        if has_game {
            let alive = sess.game_pid != 0 && snap.find(sess.game_pid).is_some();
            // Keep looking for a better match for the first 90 seconds (launchers hand over).
            let cand = if let Some(pid) = sess.req.attach_pid.filter(|p| snap.find(*p).is_some()) {
                snap.find(pid).cloned().map(|p| (p, 100))
            } else {
                let by_name = launcher::best(&snap, &sess.plan, sess.root_pid, &real, since);
                if by_name.is_none() && !alive && elapsed > Duration::from_secs(20) {
                    // Nothing matched by name or folder: watch for a program that has been
                    // working the graphics card hard for a few checks in a row.
                    match launcher::best_by_gpu(&snap, since) {
                        Some(p) => {
                            let n = match sess.gpu_cand {
                                Some((pid, n)) if pid == p.pid => n + 1,
                                _ => 1,
                            };
                            sess.gpu_cand = Some((p.pid, n));
                            if n >= 3 {
                                Some((p, 50))
                            } else {
                                None
                            }
                        }
                        None => {
                            sess.gpu_cand = None;
                            None
                        }
                    }
                } else {
                    by_name
                }
            };
            let switch = match (&cand, alive) {
                (Some((p, _)), false) => Some(p.clone()),
                (Some((p, sc)), true) if elapsed < Duration::from_secs(90) && p.pid != sess.game_pid => {
                    let cur =
                        snap.find(sess.game_pid).map(|g| launcher::score(g, &snap, &sess.plan, sess.root_pid, &real, since)).unwrap_or(0);
                    if *sc > cur + 15 {
                        Some(p.clone())
                    } else {
                        None
                    }
                }
                _ => None,
            };
            if let Some(p) = switch {
                sess.game_pid = p.pid;
                sess.game_since = Instant::now();
                sess.learned_sent = false;
                sess.game_lost_at = None;
                let settings = sess.req.settings.clone();
                let dry = sess.req.dry_run;
                let gid = sess.req.game.as_ref().map(|g| g.id.clone()).unwrap_or_default();
                // Learn at once only when nothing is known. When a real program is already known (typed in,
                // predicted from the launcher, or learned before), a different program - usually the
                // launcher's own dialog - must first stay the active game for a while (see below).
                let learned_changed = real.is_empty();
                if let Ok(mut st) = self.status.lock() {
                    st.phase = Phase::Running;
                    st.game_pid = p.pid;
                    st.game_exe = p.name.clone();
                    if learned_changed && !gid.is_empty() {
                        st.learned = Some((gid, p.name.clone()));
                    }
                }
                self.log(format!("Found the game: {} (pid {})", p.name, p.pid));
                self.boost_game_process(p.pid, &settings, dry);
                return;
            }
            let Some(sess) = &mut self.session else { return };
            if !alive && sess.game_pid != 0 {
                let lost = *sess.game_lost_at.get_or_insert_with(Instant::now);
                if lost.elapsed() > Duration::from_secs(8) {
                    let restore = sess.req.restore_when_game_closes;
                    sess.game_pid = 0;
                    self.log("The game has closed");
                    if restore {
                        self.restore();
                        return;
                    } else {
                        self.exit_quiet_desktop("the game closed");
                        self.set_phase(Phase::Boosted);
                    }
                }
            } else if alive && !sess.learned_sent && !real.is_empty() && sess.game_since.elapsed() > Duration::from_secs(25) {
                // A program other than the known one has stayed the active game for 25 s: it really is
                // the game (the known name may have changed with an update), so remember it.
                sess.learned_sent = true;
                let cur = snap.find(sess.game_pid).map(|p| p.name.clone()).unwrap_or_default();
                let gid = sess.req.game.as_ref().map(|g| g.id.clone()).unwrap_or_default();
                let launched = sess.req.game.as_ref().map(|g| crate::config::file_name_lower(&g.launch)).unwrap_or_default();
                // The file the person started is the launcher, not a replacement for the known program.
                if !cur.is_empty() && !cur.eq_ignore_ascii_case(&real) && !cur.eq_ignore_ascii_case(&launched) && !gid.is_empty() {
                    if let Ok(mut st) = self.status.lock() {
                        st.learned = Some((gid, cur));
                    }
                }
            } else if sess.game_pid == 0 && elapsed > Duration::from_secs(240) {
                let restore = sess.req.restore_when_game_closes;
                if restore {
                    self.log("The game was not found within 4 minutes - everything is being put back. Set its program in Games if it keeps happening.");
                    self.restore();
                    return;
                }
                self.log("The game was not found within 4 minutes; staying in gaming mode");
                self.set_phase(Phase::Boosted);
                if let Some(s) = &mut self.session {
                    s.req.game = None;
                    s.req.attach_pid = None;
                }
            }
        }

        self.maybe_quiet_desktop();

        // Keep the listed apps and services off.
        let Some(sess) = &self.session else { return };
        let (keep, apps_on, svcs_on, apps, svcs, dry) = (
            sess.req.keep_apps_off,
            sess.req.settings.apps,
            sess.req.settings.services,
            sess.req.apps.clone(),
            sess.stopped_services.clone(),
            sess.req.dry_run,
        );
        if keep && apps_on && !dry {
            self.apply_apps(&apps, true);
        }
        if keep && svcs_on && !dry && !svcs.is_empty() {
            let list = services::list();
            for s in list.iter().filter(|s| s.running && svcs.contains(&s.name)) {
                if !services::failed_today(&s.name) && services::stop_with_timeout(&s.name).is_ok() {
                    self.log(format!("Stopped service {} again", s.display));
                }
            }
        }
        // Thermal guard for the graphics card.
        let Some(sess) = &mut self.session else { return };
        if sess.gpu_applied && sess.last_temp.elapsed() > Duration::from_secs(10) {
            sess.last_temp = Instant::now();
            let limit = sess.req.gpu_temp_limit;
            let temp = gpu::temperature();
            if temp.is_none_or(|t| t >= limit) {
                let undo = sess.undo.iter().rev().find_map(|u| match u {
                    Undo::Gpu { core, mem } => Some((*core, *mem)),
                    _ => None,
                });
                if let Some((core, mem)) = undo {
                    let reason = temp.map(|t| format!("reached {t} °C")).unwrap_or_else(|| "lost its temperature reading".into());
                    if gpu::restore(core, mem).is_ok() {
                        sess.gpu_applied = false;
                        self.log(format!("Graphics card {reason}: clocks put back to their previous values"));
                        self.counts(|c| {
                            c.gpu_core_mhz = 0;
                            c.gpu_mem_mhz = 0;
                        });
                    } else {
                        self.log(format!("Graphics card {reason}: could not restore clocks; retrying"));
                    }
                }
            }
        }
    }

    pub(super) fn boost_game_process(&mut self, pid: u32, s: &Settings, dry: bool) {
        if dry {
            self.log(format!("Would set game priority to {}", s.priority.label()));
            self.counts(|c| c.planned += 1);
            return;
        }
        let start = self.snapshot().find(pid).map(|p| p.start).unwrap_or(0);
        if let Some(prev) = procs::get_priority(pid) {
            let class = procs::priority_class(s.priority);
            if prev == class {
                self.log(format!("Game priority already {}", s.priority.label()));
            } else if start == 0 {
                self.log("Game priority unchanged: could not identify the game process safely");
            } else if self.push_undo(Undo::Priority { pid, class: prev, start }) {
                if procs::set_priority(pid, class) {
                    // Old profiles can still contain Realtime; never apply that dangerous class.
                    if s.priority == crate::config::Priority::Realtime {
                        self.log("Game priority: High (old Realtime choice capped for safety)");
                    } else {
                        self.log(format!("Game priority: {}", s.priority.label()));
                    }
                    self.counts(|c| c.game_priority = true);
                } else {
                    self.forget_last_undo();
                    if !crate::ffi::is_admin() {
                        self.log("Windows blocked the priority change: this game runs as administrator, so CatalyX must too (turn on Settings > Ask for administrator rights at start)");
                    } else {
                        self.log("Windows refused to change the game's priority");
                    }
                }
            }
        } else {
            self.log("Could not read the game's priority (try running as administrator)");
        }
        if s.io_boost && start != 0 {
            if let Some(previous) = procs::io_priority(pid) {
                if previous != 3 && self.push_undo(Undo::IoPriority { pid, level: previous, start }) {
                    if !procs::set_io_priority(pid, 3) {
                        self.log("Game disk I/O priority unavailable; leaving it unchanged");
                        self.forget_last_undo();
                    } else if procs::io_priority(pid) == Some(3) {
                        self.log("Game disk I/O priority: High (checked with Windows)");
                        self.counts(|c| c.io_boost = true);
                        // Memory page priority dies with the process, so it needs no recovery record.
                        if procs::set_memory_priority(pid, 5) {
                            self.log("Game memory pages protected from trimming");
                        }
                    } else {
                        self.log("Game disk I/O priority could not be verified; recovery is still recorded");
                    }
                }
            } else {
                self.log("Game disk I/O priority unchanged: Windows would not report its previous setting");
            }
        }
        if s.no_power_throttling && start != 0 {
            if let Some((control, state)) = procs::power_throttling(pid) {
                if self.push_undo(Undo::Throttling { pid, start, control, state }) {
                    if procs::set_power_throttling(pid, 0x1 | 0x4, 0) {
                        self.log("Windows efficiency mode kept off for the game");
                    } else {
                        self.forget_last_undo();
                    }
                }
            } else {
                self.log("Windows efficiency mode unchanged (could not read its previous setting)");
            }
        }
    }
}
