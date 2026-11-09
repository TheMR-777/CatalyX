//! Running a boost: `play`, `boost`, `attach`, `doctor`, `restore` and `status`.

use super::*;

impl Cli {
    pub(super) fn play(&self) -> Result<(), String> {
        let mut cfg = Config::load();
        let i = find_game(&cfg, &self.arg(1, "game")?)?;
        if let Some(p) = self.value("--preset") {
            let p = find_preset(&cfg, &p)?;
            cfg.games[i].preset = cfg.presets[p].id.clone();
            cfg.games[i].custom = None;
        }
        cfg.games[i].plays += 1;
        cfg.selected_game = i;
        cfg.save();
        let game = cfg.games[i].clone();
        println!("Boosting {} with {}", game.name, profile_name(&cfg, &game));
        self.run_session(&mut cfg, Some(game), None)
    }

    pub(super) fn boost(&self) -> Result<(), String> {
        let mut cfg = Config::load();
        if let Some(p) = self.value("--preset") {
            let p = find_preset(&cfg, &p)?;
            cfg.default_preset = cfg.presets[p].id.clone();
        }
        println!("Gaming mode with {} (Enter or Ctrl+C to restore)", cfg.preset(&cfg.default_preset).name);
        self.run_session(&mut cfg, None, None)
    }

    /// `attach <program|pid>`: boost a game that is already running (started by you, a store
    /// launcher or a shortcut). Uses the matching game profile when there is one.
    pub(super) fn attach(&self) -> Result<(), String> {
        let q = self.arg(1, "program name or process id")?;
        let mut cfg = Config::load();
        if let Some(p) = self.value("--preset") {
            let p = find_preset(&cfg, &p)?;
            cfg.default_preset = cfg.presets[p].id.clone();
        }
        let ql = q.to_lowercase().trim_end_matches(".exe").to_string();
        let game = cfg
            .games
            .iter()
            .find(|g| {
                let real = g.real_exe.to_lowercase();
                (!real.is_empty() && real.trim_end_matches(".exe") == ql)
                    || config::file_name_lower(&g.launch).trim_end_matches(".exe") == ql
                    || g.name.to_lowercase() == ql
            })
            .cloned();
        println!(
            "Boosting the running game '{q}'{}",
            game.as_ref().map(|g| format!(" with {} ({})", profile_name(&cfg, g), g.name)).unwrap_or_default()
        );
        self.run_session(&mut cfg, game, Some(q))
    }

    /// `doctor`: proves on this PC, step by step, that each boost feature really works - and
    /// undoes it again. Everything it changes is put back; `--gpu` also tries the graphics
    /// card offsets for a few seconds.
    pub(super) fn doctor(&self) -> Result<(), String> {
        crate::doctor::run(self.flag("--gpu"), self.json)
    }

    /// Runs one boost in this process and prints what happens, until the game closes,
    /// Enter (gaming mode) or Ctrl+C. Always restores before returning.
    pub(super) fn run_session(&self, cfg: &mut Config, game: Option<Game>, attach: Option<String>) -> Result<(), String> {
        let limit = self
            .value("--for")
            .map(|v| {
                v.parse::<u64>()
                    .ok()
                    .filter(|n| (1..=86_400).contains(n))
                    .ok_or_else(|| "--for needs a number of seconds between 1 and 86400".to_string())
            })
            .transpose()?;
        if self.flag("--for") && limit.is_none() {
            return Err("--for needs a number of seconds between 1 and 86400".into());
        }
        ffi::catch_ctrl_c();
        let snap = procs::start_monitor();
        let (tx, status) = engine::start(snap.clone());
        let dry = self.flag("--dry-run");
        // Resolve a running game by process id or program name (the biggest one by memory).
        let attach_pid = match &attach {
            None => None,
            Some(q) => {
                let t = Instant::now();
                loop {
                    let s = snap.lock().map(|s| s.clone()).unwrap_or_default();
                    if s.serial >= 1 || t.elapsed() > Duration::from_secs(8) {
                        let ql = q.to_lowercase();
                        let ql = ql.trim_end_matches(".exe").to_string();
                        let hit = match q.parse::<u32>() {
                            Ok(pid) => s.procs.iter().find(|p| p.pid == pid),
                            Err(_) => s.procs.iter().filter(|p| p.name_lower.trim_end_matches(".exe") == ql).max_by_key(|p| p.mem),
                        };
                        match hit {
                            Some(p) => break Some(p.pid),
                            None => return Err(format!("no running program matches '{q}'")),
                        }
                    }
                    std::thread::sleep(Duration::from_millis(100));
                }
            }
        };
        let req = BoostRequest {
            settings: cfg.settings_for(game.as_ref()),
            apps: cfg.apps_for(game.as_ref()),
            services: cfg.services_for(game.as_ref()),
            keep_apps_off: cfg.keep_apps_off,
            restore_when_game_closes: true,
            reopen_closed_apps: cfg.reopen_closed_apps,
            gpu_temp_limit: cfg.gpu_temp_limit_c,
            dry_run: dry,
            attach_pid,
            game: game.clone(),
        };
        if !ffi::is_admin() {
            println!("  (not administrator: services and some Windows settings will be skipped)");
        }
        let _ = tx.send(Cmd::Boost(Box::new(req)));
        // Gaming mode waits for Enter on another thread.
        let enter = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        if game.is_none() && attach.is_none() {
            let e = enter.clone();
            std::thread::spawn(move || {
                let mut s = String::new();
                let _ = std::io::stdin().read_line(&mut s);
                e.store(true, std::sync::atomic::Ordering::SeqCst);
            });
        }
        let mut printed = 0usize;
        let mut was_active = false;
        let mut restore_sent = false;
        let started = Instant::now();
        loop {
            std::thread::sleep(Duration::from_millis(200));
            let st = status.lock().map(|s| s.clone()).unwrap_or_default();
            for line in st.log.iter().skip(printed) {
                println!("  {line}");
            }
            printed = st.log.len();
            if st.phase.active() {
                was_active = true;
            }
            if let Some(g) = &game {
                if !st.learned.is_none() {
                    if let Some((gid, exe)) = &st.learned {
                        if *gid == g.id {
                            if let Some(gm) = cfg.games.iter_mut().find(|x| x.id == *gid) {
                                if gm.real_exe != *exe {
                                    gm.real_exe = exe.clone();
                                    cfg.save();
                                    println!("  Learned the real game program: {exe}");
                                }
                            }
                        }
                    }
                    if let Ok(mut s) = status.lock() {
                        s.learned = None;
                    }
                }
            }
            let time_up = limit.map(|s| started.elapsed() > Duration::from_secs(s)).unwrap_or(false);
            // A test run starts no game, so there is nothing to wait for once the plan is shown.
            let dry_done = dry && st.phase == Phase::Boosted;
            let stop = ffi::CTRL_C.load(std::sync::atomic::Ordering::SeqCst)
                || enter.load(std::sync::atomic::Ordering::SeqCst)
                || time_up
                || dry_done;
            if stop && !restore_sent {
                println!("  Restoring...");
                let _ = tx.send(Cmd::Restore);
                restore_sent = true;
            }
            let done = (was_active || started.elapsed() > Duration::from_secs(5)) && st.phase == Phase::Idle;
            if done {
                // Print anything logged during the restore.
                std::thread::sleep(Duration::from_millis(150));
                let st = status.lock().map(|s| s.clone()).unwrap_or_default();
                for line in st.log.iter().skip(printed) {
                    println!("  {line}");
                }
                if let Some(message) = st.log.iter().find(|line| {
                    line.starts_with("Could not start the game:")
                        || line.starts_with("Another CatalyX window")
                        || line.starts_with("Could not reserve a safe boost")
                }) {
                    return Err(message.clone());
                }
                if st.pending_journal {
                    return Err("some changes could not be restored; run `catalyx restore`".into());
                }
                let k = &st.counts;
                println!(
                    "Done. {} apps closed, {} paused, {} lowered, {} services stopped, {} freed{}",
                    k.apps_closed,
                    k.apps_paused,
                    k.apps_lowered,
                    k.services_stopped,
                    procs::fmt_mem(k.ram_freed),
                    if dry { " (test run, nothing changed)" } else { "" }
                );
                return Ok(());
            }
        }
    }

    pub(super) fn restore(&self) -> Result<(), String> {
        let snap = procs::start_monitor();
        let (tx, status) = engine::start(snap);
        let pending = status.lock().map(|s| s.pending_journal).unwrap_or(false);
        if !pending {
            println!("Nothing to restore: no boost is left over.");
            return Ok(());
        }
        let _ = tx.send(Cmd::Restore);
        let t = Instant::now();
        while t.elapsed() < Duration::from_secs(30) {
            std::thread::sleep(Duration::from_millis(150));
            let st = status.lock().map(|s| s.clone()).unwrap_or_default();
            if st.restored_runs > 0 {
                for l in &st.log {
                    println!("{l}");
                }
                return Ok(());
            }
        }
        Err("restore took too long".into())
    }

    pub(super) fn status(&self) -> Result<(), String> {
        let snap = procs::start_monitor();
        let t = Instant::now();
        let s = loop {
            let s = snap.lock().map(|s| s.clone()).unwrap_or_default();
            if s.serial >= 2 || t.elapsed() > Duration::from_secs(5) {
                break s;
            }
            std::thread::sleep(Duration::from_millis(100));
        };
        let mut top = s.procs.iter().filter(|p| p.pid > 4).cloned().collect::<Vec<_>>();
        top.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal).then(b.mem.cmp(&a.mem)));
        top.truncate(12);
        let pending = std::fs::metadata(config::data_dir().join("journal.json")).is_ok();
        if self.json {
            let v = json!({
                "cpu": s.cpu_name, "threads": s.cores, "cpu_percent": s.cpu_total,
                "mem_used": s.mem_used, "mem_total": s.mem_total, "gpu_percent": s.gpu_total,
                "admin": ffi::is_admin(), "boost_left_over": pending,
                "top": top.iter().map(|p| json!({"name": p.name, "pid": p.pid, "cpu": p.cpu, "mem": p.mem, "gpu": p.gpu})).collect::<Vec<_>>(),
            });
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            return Ok(());
        }
        println!("{}  ({} threads)", s.cpu_name.trim(), s.cores);
        println!(
            "CPU {:>5.1}%   RAM {} / {}   GPU {:>3.0}%",
            s.cpu_total,
            procs::fmt_mem(s.mem_used),
            procs::fmt_mem(s.mem_total),
            s.gpu_total
        );
        println!(
            "{}{}",
            if ffi::is_admin() { "Administrator" } else { "Not administrator" },
            if pending { "   - a boost is left over: run `catalyx restore`" } else { "" }
        );
        println!();
        println!("{:<34} {:>7} {:>7} {:>10} {:>5}", "Busiest apps", "pid", "CPU", "Memory", "GPU");
        for p in &top {
            println!("{:<34} {:>7} {:>6.1}% {:>10} {:>4.0}%", trunc(&p.name, 34), p.pid, p.cpu, procs::fmt_mem(p.mem), p.gpu);
        }
        Ok(())
    }
}
