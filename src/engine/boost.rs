//! Starting a boost: validation, the order the changes are made in, and starting the game.

use super::*;

impl Engine {
    // ------------------------------------------------------------------ boost

    pub(super) fn boost(&mut self, req: BoostRequest) {
        let game_name = req.game.as_ref().map(|g| g.name.clone()).unwrap_or_default();
        let game_id = req.game.as_ref().map(|g| g.id.clone()).unwrap_or_default();
        if let Ok(mut st) = self.status.lock() {
            st.phase = Phase::Boosting;
            st.game_name = game_name.clone();
            st.game_id = game_id;
            st.game_pid = 0;
            st.game_exe.clear();
            st.counts = Counts::default();
            st.learned = None;
            st.dry_run = req.dry_run;
            st.log.clear();
        }
        let dry = req.dry_run;
        // Validate before touching power, services or other programs. A typo in a game path
        // must not leave the machine boosted while the user waits for a game that cannot start.
        if req.attach_pid.is_none() {
            if let Some(g) = &req.game {
                if !g.launch.contains("://") && !std::path::Path::new(&g.launch).is_file() && !dry {
                    self.log(format!("Could not start the game: file not found: {}", g.launch));
                    self.set_phase(Phase::Idle);
                    return;
                }
            }
        }
        self.log(if game_name.is_empty() { "Switching to gaming mode".to_string() } else { format!("Boosting for {game_name}") });
        if dry {
            self.log("Test run: nothing on this PC is changed");
        }
        let started = procs::now_secs();
        self.session = Some(Session {
            req: req.clone(),
            undo: Vec::new(),
            started,
            started_at: Instant::now(),
            root_pid: None,
            plan: launcher::LaunchPlan::default(),
            game_pid: 0,
            game_lost_at: None,
            game_since: Instant::now(),
            learned_sent: false,
            paused: HashSet::new(),
            stopped_services: HashSet::new(),
            gpu_applied: false,
            quiet_tried: false,
            gpu_cand: None,
            last_tick: Instant::now(),
            last_temp: Instant::now(),
        });
        if !dry && crate::ffi::is_admin() {
            // Held by every administrator token but switched off until asked for: lets CatalyX
            // reach elevated games and give them high disk priority.
            crate::ffi::enable_privilege("SeDebugPrivilege");
            crate::ffi::enable_privilege("SeIncreaseBasePriorityPrivilege");
        }
        // Everything below acts on process ids, so it needs a snapshot scanned just now (the window's
        // light mode carries an old list over). Ask for one and wait for it.
        let base = self.fresh_snapshot(Duration::from_secs(8));
        self.counts(|c| {
            c.free_before = base.mem_free();
            c.free_now = base.mem_free();
            c.mhz_before = base.cpu_mhz;
            c.mhz_now = base.cpu_mhz;
            c.mhz_peak = base.cpu_mhz;
        });
        let s = req.settings.clone();
        self.apply_power(&s);
        self.apply_tweaks(&s);
        if s.services {
            self.apply_services(&req.services);
        }
        if s.apps {
            self.apply_apps(&req.apps, false);
        }
        if s.lower_others {
            self.lower_others(&req.apps);
        }
        if s.clean_ram {
            self.clean_ram();
        }
        if s.purge_standby {
            if self.dry() {
                self.log("Would empty cached standby pages (Max Squeeze only)");
                self.counts(|c| c.planned += 1);
            } else if procs::purge_standby() {
                self.log("Emptied the Windows standby cache; Windows will rebuild it as needed");
                self.counts(|c| c.standby_purged = true);
            } else {
                self.log("Standby cache untouched: Windows refused or administrator rights are needed");
            }
        }
        if s.gpu != GpuLevel::Off {
            self.apply_gpu(s.gpu);
        }
        if s.quiet_desktop && dry && req.game.is_some() && req.attach_pid.is_none() {
            self.log(format!("Would switch Windows' desktop off once the game is found ({} brings it back)", shell::EXIT_KEYS));
            self.counts(|c| c.planned += 1);
        }

        // Start (or attach to) the game.
        if let Some(pid) = req.attach_pid {
            if let Some(sess) = &mut self.session {
                sess.root_pid = Some(pid);
            }
            self.set_phase(Phase::Finding);
        } else if let Some(g) = &req.game {
            if self.dry() && g.id != "selftest" {
                // A test run must never start the person's real game.
                self.log(format!("Would start {} and look for its real program", g.name));
                self.counts(|c| c.planned += 1);
                self.set_phase(Phase::Boosted);
                self.log("Boost applied");
                return;
            }
            self.log(format!("Starting {}", g.name));
            match launcher::launch(&g.launch, &g.args, self.dry() || g.id == "selftest") {
                Ok((pid, plan)) => {
                    if !plan.hints.is_empty() {
                        self.log(format!("Launcher mentions: {}", plan.hints.join(", ")));
                    }
                    if let Some(sess) = &mut self.session {
                        sess.root_pid = pid;
                        sess.plan = plan;
                        // Anything that was already running before the start can't be the game,
                        // unless it is the program learned earlier.
                        sess.started = started;
                    }
                    self.set_phase(Phase::Finding);
                }
                Err(e) => {
                    self.log(format!("Could not start the game: {e}"));
                    self.restore();
                    return;
                }
            }
        } else {
            self.set_phase(Phase::Boosted);
        }
        self.log("Boost applied");
    }
}
