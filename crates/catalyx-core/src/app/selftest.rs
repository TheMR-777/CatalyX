//! `--self-test <file.json>`: the boost/restore half of the check run, in the core, so every skin
//! gets it and none has to reach for Windows. The skin only shows frames and reports which pages it
//! drew; the steps, the stand-in game, the priority read-back and the report all live here.
//!
//!   move - the window showed Apps (with a real list) and then Games,
//!   act  - a real boost of a `.bat` game: the program behind the batch file is found, its
//!          priority really becomes High (read back from Windows), and is put back afterwards,
//!   hud  - the Boost page was drawn with live readings.
//!
//! It can never hang: every stage has its own limit, and `catalyx_core::start::begin_check_run`
//! starts an independent watchdog that ends the process whatever the window does.
//! Only the self-test's own stand-in game is touched; nothing else on the PC changes.

use super::intent::Intent;
use super::state::State;
use crate::engine::Phase;
use crate::{ffi, format, procs};
use serde_json::json;
use std::time::{Duration, Instant};

/// What a check run asks the window to show. About a run, not a product: a skin without these
/// pages can ignore it, and the report will say what it saw.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum CheckPage {
    Apps,
    Games,
    Boost,
}

/// Which pages a skin has drawn since it started, for a check run. Every skin can answer this; it
/// says nothing about how the page is built.
#[derive(Clone, Copy, Default, PartialEq, Eq, Debug)]
pub struct Drawn {
    pub apps: bool,
    pub games: bool,
    pub boost: bool,
}

pub struct SelfTest {
    out: String,
    page: Option<CheckPage>,
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
            page: None,
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

    /// The page the run needs on screen next, if it has moved on from the one it asked for.
    pub fn page(&self) -> Option<CheckPage> {
        self.page
    }

    fn next(&mut self) {
        self.stage += 1;
        self.stage_frames = 0;
        self.stage_started = Instant::now();
    }

    fn record(&mut self, name: &str, passed: bool, detail: String) {
        self.steps.push(json!({"step": name, "passed": passed, "detail": detail}));
    }

    fn finish(&self) {
        let v = json!({"version": 1, "steps": self.steps});
        if let Some(dir) = std::path::Path::new(&self.out).parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = std::fs::write(&self.out, serde_json::to_string_pretty(&v).unwrap_or_default());
    }

    /// One window frame. Returns true when the report is written and the caller should exit.
    pub(super) fn step(&mut self, state: &mut State, drawn: Drawn) -> bool {
        self.stage_frames += 1;
        let waited = self.stage_started.elapsed();
        let snap = procs::latest(&state.snap);
        match self.stage {
            // Wait for the first live readings.
            0 => {
                let ready = snap.serial >= 2 && !snap.procs.is_empty();
                if ready || waited > Duration::from_secs(15) {
                    self.page = Some(CheckPage::Apps);
                    self.next();
                }
            }
            // move: Apps (with a real list), then Games.
            1 => {
                if self.stage_frames == 6 {
                    self.apps_rows = snap.procs.len();
                    self.apps_ok = drawn.apps && self.apps_rows > 0;
                    self.page = Some(CheckPage::Games);
                }
                if self.stage_frames == 12 {
                    let ok = drawn.games && self.apps_ok;
                    self.record("move", ok, format!("showed Apps (it listed {} running programs) and then Games", self.apps_rows));
                    self.start_act(state);
                    self.next();
                }
            }
            // act: boost a .bat game for real (priority only) and watch it through.
            2 => {
                // Read the handful of facts this needs and drop the lock: no clone of the whole
                // status (the activity log grows while a boost runs).
                let (game_exe, game_pid, priority_done, phase, restored) = match state.status.lock() {
                    Ok(s) => (s.game_exe.clone(), s.game_pid, s.counts.game_priority, s.phase, s.restored_runs),
                    Err(_) => return false,
                };
                if !game_exe.is_empty() && self.found.is_empty() {
                    self.found = game_exe;
                    self.helper = snap
                        .find(game_pid)
                        .filter(|p| p.name_lower == "catalyx-testgame.exe" && p.start + 3 >= self.launched_at)
                        .map(|p| (p.pid, p.start));
                }
                if self.priority_seen.is_none() && priority_done && game_pid != 0 {
                    self.priority_seen = procs::get_priority(game_pid);
                }
                if !self.restore_sent && (self.priority_seen.is_some() || (!self.found.is_empty() && waited > Duration::from_secs(14))) {
                    state.apply(Intent::Restore);
                    self.restore_sent = true;
                }
                let done = (self.restore_sent && phase == Phase::Idle && restored > 0) || waited > Duration::from_secs(30);
                if done {
                    let after = self.helper.and_then(|(pid, _)| procs::get_priority(pid));
                    let high = ffi::HIGH_PRIORITY_CLASS;
                    let back = after.map(|c| c == ffi::NORMAL_PRIORITY_CLASS).unwrap_or(true);
                    let ok = self.found.eq_ignore_ascii_case("catalyx-testgame.exe")
                        && self.helper.is_some()
                        && self.priority_seen == Some(high)
                        && back
                        && phase == Phase::Idle;
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
                    self.record("act", ok, detail);
                    self.cleanup(state);
                    self.page = Some(CheckPage::Boost);
                    self.next();
                }
            }
            // hud: the Boost page with its live readings.
            3 if self.stage_frames >= 10 => {
                let ok = drawn.boost && snap.serial >= 2;
                self.record(
                    "hud",
                    ok,
                    format!(
                        "the Boost page is drawn: CPU {:.0}%, RAM {}, GPU {:.0}% meters ({} programs listed)",
                        snap.cpu_total,
                        format::mem(snap.mem_used),
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
            self.record("timeout", false, "self-test took longer than 90 seconds".into());
            self.cleanup(state);
            self.finish();
            return true;
        }
        false
    }

    fn start_act(&mut self, state: &mut State) {
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
        // The boost goes through the core like any other, without the summary (this run's config is
        // never saved); the engine knows the stand-in by its id "selftest".
        state.apply(Intent::SetOption { key: "confirm-boost".into(), value: "off".into() });
        state.add_selftest_game(crate::config::Game {
            id: "selftest".into(),
            name: "Self-test game".into(),
            launch: bat.to_string_lossy().to_string(),
            preset: crate::config::PRESET_MAX.into(),
            custom: Some(only_priority),
            ..Default::default()
        });
        self.bat = Some(bat);
        self.launched_at = procs::now_secs();
        state.apply(Intent::Boost { game: Some("selftest".into()) });
    }

    /// Stop only the stand-in we started (and the batch shell that started it), then delete its files.
    fn cleanup(&mut self, state: &mut State) {
        if let Some((pid, start)) = self.helper.take() {
            if procs::same_process(pid, start) {
                let snap = procs::latest(&state.snap);
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
