//! State: everything the window used to own that is not drawing (the config and its saving, the engine
//! link, probes and their results, the boost policy), and `apply`, which carries out an intent.

use super::intent::{Event, Intent, Tone};
use super::jobs::Job;
use super::view::{Hardware, Library, System, View};
use crate::config::{Config, Game, ProcAction};
use crate::engine::{BoostRequest, Cmd, Phase, SharedStatus, Status};
use crate::procs::{self, SharedSnapshot, Snapshot};
use crate::services;
use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Events kept while nobody takes them (a minimised window draws nothing); the oldest go first.
const MAX_EVENTS: usize = 32;

pub struct State {
    pub(super) cfg: Arc<Config>,
    pub(super) dirty_since: Option<Instant>,
    pub(super) no_save: bool,
    /// The self-test drives this run: no memory trims (they would disturb its measurements).
    pub(super) selftest: bool,
    /// A check run (screenshot, tour, self-test): per-process figures always on.
    pub(super) check_run: bool,
    pub(super) tx: Sender<Cmd>,
    pub(super) status: SharedStatus,
    pub(super) snap: SharedSnapshot,
    // hardware and services, filled by jobs
    pub(super) hardware: Arc<Hardware>,
    pub(super) hw_busy: bool,
    pub(super) last_live: Instant,
    pub(super) services: Arc<Vec<services::ServiceInfo>>,
    pub(super) svc_busy: bool,
    pub(super) last_svc: Option<Instant>,
    pub(super) system: System,
    pub(super) library: Arc<Library>,
    // automatic boost
    pub(super) auto_ignore: HashSet<u32>,
    pub(super) last_auto_scan: Instant,
    // closing while a boost is active
    pub(super) closing: Option<Instant>,
    pub(super) last_trim: Instant,
    /// The minimised window has been trimmed once (reset when it is restored).
    pub(super) trimmed_minimized: bool,
    /// The boost the summary is being shown for: only that one may be confirmed.
    awaiting_confirm: Option<Option<usize>>,
    events: VecDeque<Event>,
    pub(super) rev: u64,
    // View sections that follow other threads: rebuilt when their source moved.
    status_seen: Option<u64>,
    status_view: Arc<Status>,
    snap_seen: Option<u64>,
    snap_view: Arc<Snapshot>,
    /// Jobs to start and whether to wake the skin, once the lock is released (see `Core::send`).
    pub(super) jobs: Vec<Job>,
    pub(super) wake: bool,
}

impl State {
    #[allow(clippy::too_many_arguments)]
    pub(super) fn new(
        cfg: Config,
        no_save: bool,
        selftest: bool,
        check_run: bool,
        tx: Sender<Cmd>,
        status: SharedStatus,
        snap: SharedSnapshot,
        admin: bool,
    ) -> State {
        State {
            cfg: Arc::new(cfg),
            dirty_since: None,
            no_save,
            selftest,
            check_run,
            tx,
            status,
            snap,
            hardware: Arc::new(Hardware::default()),
            hw_busy: false,
            last_live: Instant::now() - Duration::from_secs(10),
            services: Arc::new(Vec::new()),
            svc_busy: false,
            last_svc: None,
            system: System { admin, elevation: None, elevation_busy: false },
            library: Arc::new(Library::default()),
            auto_ignore: HashSet::new(),
            last_auto_scan: Instant::now(),
            closing: None,
            last_trim: Instant::now() - Duration::from_secs(10),
            trimmed_minimized: false,
            awaiting_confirm: None,
            events: VecDeque::new(),
            rev: 0,
            status_seen: None,
            status_view: Arc::new(Status::default()),
            snap_seen: None,
            snap_view: Arc::new(Snapshot::default()),
            jobs: Vec::new(),
            wake: false,
        }
    }

    // ------------------------------------------------------------------ helpers

    /// The config, for changing it. The view's copy is shared, so the first change after a view() copies
    /// it once (Arc::make_mut); later changes in the same frame do not.
    pub(super) fn cfg_mut(&mut self) -> &mut Config {
        self.rev += 1;
        Arc::make_mut(&mut self.cfg)
    }

    pub(super) fn mark_dirty(&mut self) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
    }

    pub(super) fn save_now(&mut self) {
        if !self.no_save {
            self.cfg.save();
        }
        self.dirty_since = None;
    }

    pub(super) fn phase(&self) -> Phase {
        self.status.lock().map(|s| s.phase).unwrap_or(Phase::Idle)
    }

    pub(super) fn event(&mut self, e: Event) {
        if self.events.len() >= MAX_EVENTS {
            self.events.pop_front();
        }
        self.events.push_back(e);
        self.rev += 1;
    }

    pub(super) fn toast(&mut self, tone: Tone, text: impl Into<String>) {
        self.event(Event::Toast { tone, text: text.into() });
    }

    pub(super) fn take_events(&mut self) -> Vec<Event> {
        self.events.drain(..).collect()
    }

    /// Runs `job` on a worker thread once the lock is released; its result wakes the skin.
    pub(super) fn spawn(&mut self, job: impl FnOnce() -> Option<super::jobs::Done> + Send + 'static) {
        self.jobs.push(Box::new(job));
    }

    /// The current View, rebuilding only the sections whose source moved.
    pub(super) fn view(&mut self) -> View {
        let r = self.status.rev();
        if self.status_seen != Some(r) {
            self.status_view = Arc::new(self.status.lock().map(|s| s.clone()).unwrap_or_default());
            self.status_seen = Some(r);
            self.rev += 1;
        }
        if let Ok(s) = self.snap.lock() {
            if self.snap_seen != Some(s.serial) {
                self.snap_view = Arc::new(s.clone());
                self.snap_seen = Some(s.serial);
                self.rev += 1;
            }
        }
        View {
            rev: self.rev,
            config: self.cfg.clone(),
            status: self.status_view.clone(),
            snap: self.snap_view.clone(),
            hardware: self.hardware.clone(),
            services: self.services.clone(),
            library: self.library.clone(),
            system: self.system,
        }
    }

    // ------------------------------------------------------------------ intents

    pub(super) fn apply(&mut self, intent: Intent) {
        match intent {
            Intent::Boost { game } => self.request_boost(game),
            Intent::ConfirmBoost { game, dont_ask_again } => {
                // Only the boost the summary was shown for: a skin cannot start one without the core's say-so.
                if self.awaiting_confirm.take() == Some(game) {
                    if dont_ask_again {
                        // The person can bring the summary back in Settings > Behaviour.
                        self.cfg_mut().confirm_boost = false;
                        self.mark_dirty();
                    }
                    self.boost_game(game);
                }
            }
            Intent::CancelBoost => self.awaiting_confirm = None,
            Intent::Restore => self.restore(),
            Intent::ExitQuietDesktop => {
                let _ = self.tx.send(Cmd::ExitQuietDesktop);
            }
            Intent::AddGame { path, name, real_exe } => self.add_game(&path, name, real_exe),
            Intent::ScanLibrary => self.scan_library(),
            Intent::HideInstalled => {
                Arc::make_mut(&mut self.library).installed = None;
                self.rev += 1;
            }
            Intent::FindCandidates { id, folder } => self.find_candidates(id, folder),
            Intent::RefreshHardware => self.refresh_hardware(),
            Intent::RefreshServices => self.refresh_services(),
            Intent::TestGraphicsCard => self.test_graphics_card(),
            Intent::RunElevated { args, done } => self.run_elevated(args, done),
            Intent::ExportConfig(path) => self.export_config(&path),
            Intent::ImportConfig(path) => self.import_config(&path),
            Intent::Edit(f) => {
                f(self.cfg_mut());
                self.mark_dirty();
            }
        }
    }

    /// Starting a boost: first the summary of what will change (unless the person turned that off).
    fn request_boost(&mut self, game: Option<usize>) {
        if self.cfg.confirm_boost {
            self.show_boost_summary(game);
        } else {
            self.boost_game(game);
        }
    }

    /// Asks the skin to show the summary for this boost and waits for its answer.
    pub(super) fn show_boost_summary(&mut self, game: Option<usize>) {
        self.awaiting_confirm = Some(game);
        self.event(Event::ConfirmBoost { game });
    }

    pub(super) fn request_for(&self, game: Option<&Game>, attach: Option<u32>) -> BoostRequest {
        BoostRequest {
            game: game.cloned(),
            settings: self.cfg.settings_for(game),
            apps: self.cfg.apps_for(game),
            services: self.cfg.services_for(game),
            keep_apps_off: self.cfg.keep_apps_off,
            restore_when_game_closes: self.cfg.restore_when_game_closes,
            reopen_closed_apps: self.cfg.reopen_closed_apps,
            gpu_temp_limit: self.cfg.gpu_temp_limit_c,
            dry_run: false,
            attach_pid: attach,
        }
    }

    fn boost_game(&mut self, idx: Option<usize>) {
        let game = idx.and_then(|i| self.cfg.games.get(i)).cloned();
        if let Some(i) = idx {
            let cfg = self.cfg_mut();
            if let Some(g) = cfg.games.get_mut(i) {
                g.plays += 1;
            }
            cfg.selected_game = i;
            self.mark_dirty();
        }
        procs::set_detail(true);
        let req = self.request_for(game.as_ref(), None);
        let _ = self.tx.send(Cmd::Boost(Box::new(req)));
        match &game {
            Some(g) => self.toast(Tone::Info, format!("Boosting and starting {}", g.name)),
            None => self.toast(Tone::Info, "Gaming mode on"),
        }
        self.wake = true;
    }

    fn restore(&mut self) {
        let pid = self.status.lock().map(|s| s.game_pid).unwrap_or(0);
        if pid != 0 {
            self.auto_ignore.insert(pid);
        }
        let _ = self.tx.send(Cmd::Restore);
        self.wake = true;
    }

    /// Adds a game from a path or store link (used by the form, drag-and-drop and the importer).
    fn add_game(&mut self, path: &str, name: Option<String>, real_exe: Option<String>) {
        let path = path.trim().trim_matches('"').to_string();
        if path.is_empty() {
            return;
        }
        let is_link = path.contains("://");
        if !is_link && !std::path::Path::new(&path).exists() {
            self.toast(Tone::Error, format!("File not found: {path}"));
            return;
        }
        let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
        if !is_link && !matches!(ext.as_str(), "exe" | "bat" | "cmd" | "lnk" | "url") {
            self.toast(Tone::Error, "Games are .exe, .bat, .cmd, shortcut (.lnk/.url) files or store links");
            return;
        }
        if self.cfg.games.iter().any(|g| g.launch.eq_ignore_ascii_case(&path)) {
            self.toast(Tone::Info, "That game is already in your library");
            return;
        }
        let name = name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(path.clone()));
        let id = self.cfg.new_id("game");
        let preset = self.cfg.default_preset.clone();
        let cfg = self.cfg_mut();
        cfg.games.push(Game {
            id: id.clone(),
            name: name.clone(),
            launch: path.clone(),
            preset,
            real_exe: real_exe.unwrap_or_default(),
            ..Default::default()
        });
        cfg.selected_game = cfg.games.len() - 1;
        self.mark_dirty();
        self.toast(Tone::Ok, format!("Added {name}"));
        if !is_link {
            self.predict_real_exe(id, path);
        }
    }

    fn export_config(&mut self, path: &str) {
        match serde_json::to_string_pretty(&*self.cfg)
            .map_err(|e| e.to_string())
            .and_then(|s| std::fs::write(path, s).map_err(|e| e.to_string()))
        {
            Ok(()) => self.toast(Tone::Ok, format!("Settings saved to {path}")),
            Err(e) => self.toast(Tone::Error, format!("Could not save: {e}")),
        }
    }

    fn import_config(&mut self, path: &str) {
        match std::fs::read_to_string(path)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<Config>(&t).map_err(|e| format!("not a CatalyX settings file: {e}")))
        {
            Ok(c) => {
                let (g, p) = (c.games.len(), c.presets.len());
                *self.cfg_mut() = c;
                self.mark_dirty();
                self.toast(Tone::Ok, format!("Loaded {g} games and {p} presets"));
            }
            Err(e) => self.toast(Tone::Error, e),
        }
    }

    /// The self-test's stand-in game (check runs only; the engine treats id "selftest" specially).
    pub(super) fn add_selftest_game(&mut self, game: Game) -> usize {
        let cfg = self.cfg_mut();
        cfg.games.push(game);
        cfg.selected_game = cfg.games.len() - 1;
        cfg.selected_game
    }
}

/// Selects the catalog's gentle suggestions (for all games). Choices the person already made
/// are never overwritten.
pub fn apply_suggestions(cfg: &mut Config) {
    for (exe, a) in crate::catalog::suggested_apps() {
        let cur = cfg.apps.get(&exe).copied().unwrap_or(ProcAction::Keep);
        if cur == ProcAction::Keep {
            cfg.apps.insert(exe, a);
        }
    }
    for s in crate::catalog::KNOWN_SERVICES.iter().filter(|s| s.recommended) {
        let key = cfg.services.keys().find(|k| k.eq_ignore_ascii_case(s.name)).cloned().unwrap_or_else(|| s.name.to_string());
        cfg.services.insert(key, true);
    }
}

/// Sample content for screenshots of a fresh install.
pub(super) fn demo_data(cfg: &mut Config) {
    for (name, exe, preset) in [
        ("Stellar Drift", "stellardrift-win64.exe", "balanced"),
        ("Iron Vale", "ironvale.exe", "max"),
        ("Cozy Farm", "cozyfarm.exe", "gentle"),
    ] {
        cfg.games.push(Game {
            id: cfg.new_id("game") + name,
            name: name.into(),
            launch: format!("C:\\Games\\{name}\\launch.exe"),
            real_exe: exe.into(),
            preset: preset.into(),
            plays: 3,
            ..Default::default()
        });
    }
    cfg.apps.insert("discord.exe".into(), ProcAction::Keep);
}

/// For visual checks only: a believable running session (no real boost is made).
pub(super) fn demo_session(status: &SharedStatus) {
    if let Ok(mut st) = status.lock() {
        st.phase = Phase::Running;
        st.game_name = "Stellar Drift".into();
        st.game_exe = "stellardrift-win64.exe".into();
        st.game_pid = 4242;
        st.counts.free_before = 5_200_000_000;
        st.counts.free_now = 6_900_000_000;
        st.counts.mhz_before = 3400;
        st.counts.mhz_peak = 3830;
        st.counts.apps_lowered = 7;
        st.counts.apps_paused = 3;
        st.counts.apps_closed = 4;
        st.counts.services_stopped = 12;
        st.counts.tweaks = 5;
        st.counts.ram_freed = 640 * 1024 * 1024;
        st.counts.power_plan = "CatalyX plan".into();
        st.counts.cpu_level = "Boost".into();
        st.counts.game_priority = true;
        st.counts.io_boost = true;
        st.log = [
            "Boosting for Stellar Drift",
            "Power plan: CatalyX plan, CPU Boost (checked: turbo aggressive, cores unparked)",
            "Windows: Game Mode on",
            "Stopped service Connected User Experiences and Telemetry",
            "Freed 640 MB of memory from 32 apps",
            "Starting Stellar Drift",
            "Found the game: stellardrift-win64.exe (pid 4242)",
            "Game priority: Above normal",
            "Game disk I/O priority: High (checked with Windows)",
            "Windows efficiency mode kept off for the game",
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::StatusCell;
    use std::sync::mpsc::{channel, Receiver};
    use std::sync::Mutex;

    /// A State that saves nothing and whose engine is just the receiving end of a channel.
    fn state() -> (State, Receiver<Cmd>) {
        let (tx, rx) = channel();
        let status = Arc::new(StatusCell::new(Status::default()));
        let snap = Arc::new(Mutex::new(Snapshot::default()));
        (State::new(Config::default(), true, false, false, tx, status, snap, false), rx)
    }

    #[test]
    fn a_boost_asks_first_when_the_person_wants_a_summary() {
        let (mut st, rx) = state();
        st.apply(Intent::Boost { game: None });
        assert!(rx.try_recv().is_err(), "nothing may start before the summary is confirmed");
        assert!(matches!(st.take_events().as_slice(), [Event::ConfirmBoost { game: None }]));
        st.apply(Intent::ConfirmBoost { game: None, dont_ask_again: false });
        assert!(matches!(rx.try_recv(), Ok(Cmd::Boost(_))));
        assert!(st.cfg.confirm_boost, "asking stays on unless the box was ticked");
    }

    #[test]
    fn a_boost_starts_at_once_without_a_summary_and_never_from_a_stray_confirmation() {
        let (mut st, rx) = state();
        st.apply(Intent::ConfirmBoost { game: None, dont_ask_again: false });
        assert!(rx.try_recv().is_err(), "a confirmation with no summary shown does nothing");
        st.apply(Intent::Boost { game: None });
        st.apply(Intent::CancelBoost);
        st.apply(Intent::ConfirmBoost { game: None, dont_ask_again: false });
        assert!(rx.try_recv().is_err(), "a cancelled summary cannot be confirmed later");
        st.take_events();
        st.apply(Intent::Edit(Box::new(|c| c.confirm_boost = false)));
        assert!(st.dirty_since.is_some(), "an edit is saved later");
        st.apply(Intent::Boost { game: None });
        assert!(matches!(rx.try_recv(), Ok(Cmd::Boost(_))));
        let events = st.take_events();
        assert!(events.iter().all(|e| !matches!(e, Event::ConfirmBoost { .. })));
    }

    #[test]
    fn dont_ask_again_turns_the_summary_off() {
        let (mut st, rx) = state();
        st.apply(Intent::Boost { game: None });
        st.apply(Intent::ConfirmBoost { game: None, dont_ask_again: true });
        assert!(matches!(rx.try_recv(), Ok(Cmd::Boost(_))));
        assert!(!st.cfg.confirm_boost);
    }

    #[test]
    fn restore_never_asks() {
        let (mut st, rx) = state();
        st.apply(Intent::Restore);
        assert!(matches!(rx.try_recv(), Ok(Cmd::Restore)));
        assert!(st.take_events().is_empty());
    }
}
