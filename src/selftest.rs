//! `--self-test <file.json>`: runs the core loop inside the real window, in three steps, and writes
//! the result as JSON:
//!   move - the pages can be switched and the Apps page lists what is running,
//!   act  - a real boost of a `.bat` game: the program behind the batch file is found, its
//!          priority really becomes High (read back from Windows), and is put back afterwards,
//!   hud  - the live meters, the boost control and the session card are drawn.
//!
//! It can never hang: every stage has its own time limit, and `finish` is also reached by an
//! independent watchdog thread (see `main.rs`) that ends the process whatever the window does.
//! Only the self-test's own stand-in game is touched; nothing else on the PC changes.

use crate::engine::{Cmd, Phase};
use crate::gui::{CatalyxApp, Tab};
use crate::{ffi, procs};
use serde_json::json;
use std::time::{Duration, Instant};

pub struct SelfTest {
    out: String,
    stage: u32,
    stage_frames: u32,
    stage_started: Instant,
    started: Instant,
    steps: Vec<serde_json::Value>,
    bat: Option<std::path::PathBuf>,
    launched_at: u64,
    found: String,
    helper: Option<(u32, u64)>,
    priority_seen: Option<u32>,
    restore_sent: bool,
    apps_ok: bool,
    apps_rows: usize,
}

impl SelfTest {
    pub fn new(out: String) -> Self {
        SelfTest {
            out,
            stage: 0,
            stage_frames: 0,
            stage_started: Instant::now(),
            started: Instant::now(),
            steps: Vec::new(),
            bat: None,
            launched_at: 0,
            found: String::new(),
            helper: None,
            priority_seen: None,
            restore_sent: false,
            apps_ok: false,
            apps_rows: 0,
        }
    }

    fn next(&mut self) {
        self.stage += 1;
        self.stage_frames = 0;
        self.stage_started = Instant::now();
    }

    fn step(&mut self, name: &str, passed: bool, detail: String) {
        self.steps.push(json!({"step": name, "passed": passed, "detail": detail}));
    }

    pub fn finish(&self) {
        let v = json!({"version": 1, "steps": self.steps});
        if let Some(dir) = std::path::Path::new(&self.out).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.out, serde_json::to_string_pretty(&v).unwrap_or_default());
    }

    /// Called once per frame. Returns true when the test is complete (the caller exits).
    pub fn step_frame(&mut self, app: &mut CatalyxApp) -> bool {
        self.stage_frames += 1;
        let waited = self.stage_started.elapsed();
        match self.stage {
            // Wait for the first live readings.
            0 => {
                let ready = app.snap.lock().map(|s| s.serial >= 2 && !s.procs.is_empty()).unwrap_or(false);
                if ready || waited > Duration::from_secs(15) {
                    app.tab = Tab::Apps;
                    self.next();
                }
            }
            // move: Apps (with a real list), then Games.
            1 => {
                if self.stage_frames == 6 {
                    let rows = app.snap.lock().map(|s| s.procs.len()).unwrap_or(0);
                    let drawn = app.drawn.contains("apps");
                    self.apps_ok = drawn && rows > 0 && app.tab == Tab::Apps;
                    self.apps_rows = rows;
                    app.tab = Tab::Games;
                }
                if self.stage_frames == 12 {
                    let ok = app.tab == Tab::Games && app.drawn.contains("games");
                    self.step(
                        "move",
                        ok && self.apps_ok,
                        format!(
                            "switched to Apps (it listed {} running programs) and then to Games (now on {:?})",
                            self.apps_rows, app.tab
                        ),
                    );
                    self.start_act(app);
                    self.next();
                }
            }
            // act: boost a .bat game for real (priority only) and watch it through.
            2 => {
                let st = app.status.lock().map(|s| s.clone()).unwrap_or_default();
                if !st.game_exe.is_empty() && self.found.is_empty() {
                    self.found = st.game_exe.clone();
                    let snap = app.snap.lock().map(|s| s.clone()).unwrap_or_default();
                    self.helper = snap
                        .find(st.game_pid)
                        .filter(|p| p.name_lower == "catalyx-testgame.exe" && p.start + 3 >= self.launched_at)
                        .map(|p| (p.pid, p.start));
                }
                if self.priority_seen.is_none() && st.counts.game_priority && st.game_pid != 0 {
                    self.priority_seen = procs::get_priority(st.game_pid);
                }
                if !self.restore_sent && (self.priority_seen.is_some() || (!self.found.is_empty() && waited > Duration::from_secs(14))) {
                    let _ = app.tx.send(Cmd::Restore);
                    self.restore_sent = true;
                }
                let done = (self.restore_sent && st.phase == Phase::Idle && st.restored_runs > 0) || waited > Duration::from_secs(30);
                if done {
                    let after = self.helper.and_then(|(pid, _)| procs::get_priority(pid));
                    let high = ffi::HIGH_PRIORITY_CLASS;
                    let back = after.map(|c| c == ffi::NORMAL_PRIORITY_CLASS).unwrap_or(true);
                    let ok = self.found.eq_ignore_ascii_case("catalyx-testgame.exe")
                        && self.helper.is_some()
                        && self.priority_seen == Some(high)
                        && back
                        && st.phase == Phase::Idle;
                    let detail = if self.found.is_empty() {
                        format!(
                            "boosted the test game but the real program behind the batch file was not found in {:.0}s",
                            waited.as_secs_f32()
                        )
                    } else {
                        format!(
                            "real boost of a .bat game: found the program {} behind it, its priority became {} (read back from Windows), then {} after restoring",
                            self.found,
                            match self.priority_seen { Some(c) if c == high => "High".to_string(), Some(c) => format!("0x{c:x} (expected High)"), None => "unchanged (not raised)".to_string() },
                            match after { Some(c) if c == ffi::NORMAL_PRIORITY_CLASS => "Normal again".to_string(), Some(c) => format!("0x{c:x} (not put back)"), None => "the program had ended".to_string() },
                        )
                    };
                    self.step("act", ok, detail);
                    self.cleanup(app);
                    app.tab = Tab::Boost;
                    self.next();
                }
            }
            // hud: the Boost page with its meters, button and session card.
            3 if self.stage_frames >= 10 => {
                let snap = app.snap.lock().map(|s| s.clone()).unwrap_or_default();
                let d = &app.drawn;
                let ok = d.contains("stats") && d.contains("session") && d.contains("boost_control") && snap.serial >= 2;
                self.step(
                    "hud",
                    ok,
                    format!(
                        "CPU {:.0}%, RAM {}, GPU {:.0}% meters, the boost control and the session card are drawn ({} apps listed)",
                        snap.cpu_total,
                        procs::fmt_mem(snap.mem_used),
                        snap.gpu_total,
                        snap.procs.len()
                    ),
                );
                self.finish();
                return true;
            }
            _ => {}
        }
        if self.started.elapsed() > Duration::from_secs(90) {
            self.step("timeout", false, "self-test took longer than 90 seconds".into());
            self.cleanup(app);
            self.finish();
            return true;
        }
        false
    }

    fn start_act(&mut self, app: &mut CatalyxApp) {
        let dir = std::env::temp_dir().join("catalyx-selftest");
        let _ = std::fs::create_dir_all(&dir);
        let bat = dir.join("start_game.bat");
        // Windows programs are never taken for a game, so the stand-in is a renamed copy of ping.
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        let _ = std::fs::copy(format!("{sys}\\System32\\PING.EXE"), dir.join("catalyx-testgame.exe"));
        let _ = std::fs::write(&bat, "@echo off\r\nrem test launcher: the real program is started by this batch file\r\n\"%~dp0catalyx-testgame.exe\" -n 16 127.0.0.1 >nul\r\n");
        // A real boost, but only of the one thing that dies with the test: the stand-in's own priority.
        let only_priority = crate::config::Settings {
            priority: crate::config::Priority::High,
            power: crate::config::PowerChoice::Keep,
            cpu: crate::config::CpuLevel::Off,
            apps: false,
            services: false,
            clean_ram: false,
            purge_standby: false,
            io_boost: false,
            windows_game_mode: false,
            game_dvr_off: false,
            notifications_off: false,
            transparency_off: false,
            pause_maintenance: false,
            mmcss_games: false,
            no_power_throttling: false,
            gpu: crate::config::GpuLevel::Off,
            lower_others: false,
            quiet_desktop: false,
        };
        app.cfg.games.push(crate::config::Game {
            id: "selftest".into(),
            name: "Self-test game".into(),
            launch: bat.to_string_lossy().to_string(),
            preset: crate::config::PRESET_MAX.into(),
            custom: Some(only_priority),
            ..Default::default()
        });
        app.cfg.selected_game = app.cfg.games.len() - 1;
        self.bat = Some(bat);
        let g = app.cfg.games.last().cloned();
        let req = app.request_for(g.as_ref(), None);
        self.launched_at = procs::now_secs();
        let _ = app.tx.send(Cmd::Boost(Box::new(req)));
        app.tab = Tab::Boost;
    }

    /// Stop only the stand-in we started (and the batch shell that started it), then delete its files.
    fn cleanup(&mut self, app: &mut CatalyxApp) {
        if let Some((pid, start)) = self.helper.take() {
            if procs::same_process(pid, start) {
                let snap = app.snap.lock().map(|s| s.clone()).unwrap_or_default();
                if let Some(p) = snap.find(pid) {
                    if let Some(cmd) =
                        snap.procs.iter().find(|c| c.pid == p.parent && c.name_lower == "cmd.exe" && c.start + 3 >= self.launched_at)
                    {
                        if procs::same_process(cmd.pid, cmd.start) {
                            procs::terminate(cmd.pid);
                        }
                    }
                }
                procs::terminate(pid);
            }
        }
        if let Some(b) = self.bat.take() {
            std::thread::sleep(Duration::from_millis(300));
            let _ = std::fs::remove_file(b.with_file_name("catalyx-testgame.exe"));
            let _ = std::fs::remove_file(b);
        }
    }
}
