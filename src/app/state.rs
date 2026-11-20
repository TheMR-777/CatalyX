//! State: everything the window used to own that is not drawing (the config and its saving, the engine
//! link, probes and their results, the boost policy), and `apply`, which carries out an intent.

use super::boost::{self, BoostView};
use super::intent::{Event, GameId, Hint, Intent, Interest, PresetId, Tone};
use super::jobs::{Done, Job};
use super::spec::{self, Target};
use super::view::{Hardware, Library, System, View};
use crate::config::{Config, Game, Preset, ProcAction};
use crate::engine::{BoostRequest, Cmd, LogLine, Phase, SharedStatus, Status, Tags};
use crate::procs::{self, SharedSnapshot, Snapshot};
use crate::services;
use crate::{elevate, ffi};
use std::collections::{HashSet, VecDeque};
use std::sync::mpsc::Sender;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Events kept while nobody takes them (a minimised window draws nothing); the oldest go first.
const MAX_EVENTS: usize = 32;

/// Everything the boost view is made from. Equal inputs mean the cached view is still current.
#[derive(Clone, Copy, PartialEq, Eq)]
struct BoostInputs {
    cfg: u64,
    status: u64,
    services: u64,
    admin: bool,
}

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
    pub(super) last_own: Instant,
    pub(super) services: Arc<Vec<services::ServiceInfo>>,
    pub(super) svc_busy: bool,
    pub(super) last_svc: Option<Instant>,
    pub(super) system: System,
    pub(super) library: Arc<Library>,
    // automatic boost
    pub(super) auto_ignore: HashSet<u32>,
    pub(super) last_auto_scan: Instant,
    // what the skin says about itself
    /// What the skin is showing (set by `Hint::Interest`), so only that is sampled.
    pub(super) interest: Interest,
    /// The skin's window is minimised: nothing is drawn, so nothing live is sampled and only real
    /// changes wake it.
    pub(super) minimised: bool,
    /// `Hint::Shown` has been answered: the probes start once, not on every hint.
    shown: bool,
    /// Closing while a boost is active
    pub(super) closing: Option<Instant>,
    pub(super) last_trim: Instant,
    /// When to trim the working set once after the window was minimised (None = it draws).
    pub(super) trim_due: Option<Instant>,
    /// The boost the summary is being shown for: only that one may be confirmed.
    awaiting_confirm: Option<Option<GameId>>,
    events: VecDeque<Event>,
    pub(super) rev: u64,
    /// The revision the skin last drew and when sampled readings may wake it again. The old window
    /// drew live readings at most every 2 s (0.6 s while boosting); two sampled sources must not
    /// add up to two draws.
    pub(super) drawn_rev: u64,
    pub(super) next_live_wake: Instant,
    // View sections that follow other threads: rebuilt when their source moved.
    status_seen: Option<u64>,
    status_view: Arc<Status>,
    snap_seen: Option<u64>,
    snap_view: Arc<Snapshot>,
    /// Bumped by every config edit and service-list replacement, so the boost view can tell when its
    /// inputs moved without walking the config on every frame.
    cfg_rev: u64,
    pub(super) services_rev: u64,
    boost_seen: Option<BoostInputs>,
    boost_view: Arc<BoostView>,
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
            last_own: Instant::now() - Duration::from_secs(10),
            services: Arc::new(Vec::new()),
            svc_busy: false,
            last_svc: None,
            system: System { admin, elevation: None, elevation_busy: false, own_working_set: crate::ffi::own_working_set() },
            library: Arc::new(Library::default()),
            auto_ignore: HashSet::new(),
            last_auto_scan: Instant::now(),
            interest: Interest::default(),
            minimised: false,
            shown: false,
            closing: None,
            last_trim: Instant::now() - Duration::from_secs(10),
            trim_due: None,
            awaiting_confirm: None,
            events: VecDeque::new(),
            rev: 0,
            drawn_rev: 0,
            next_live_wake: Instant::now() - Duration::from_secs(1),
            status_seen: None,
            status_view: Arc::new(Status::default()),
            snap_seen: None,
            snap_view: Arc::new(Snapshot::default()),
            cfg_rev: 0,
            services_rev: 0,
            boost_seen: None,
            boost_view: Arc::new(BoostView::default()),
            jobs: Vec::new(),
            wake: false,
        }
    }

    // ------------------------------------------------------------------ helpers

    /// The config, for changing it. The view's copy is shared, so the first change after a view() copies
    /// it once (Arc::make_mut); later changes in the same frame do not.
    pub(super) fn cfg_mut(&mut self) -> &mut Config {
        self.rev += 1;
        self.cfg_rev += 1;
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

    /// Asks the skin to draw again. A minimised window draws nothing, so it is not woken; the first
    /// frame after it is restored takes in everything at once.
    pub(super) fn wake_skin(&mut self) {
        if !self.minimised {
            self.wake = true;
        }
    }

    /// The cadence the old window showed live readings at: quick while a boost runs or a close is
    /// being answered, slow otherwise. Sampling keeps its own deadlines; this only caps the draws.
    pub(super) fn live_every(&self) -> Duration {
        if self.phase().active() || self.closing.is_some() {
            super::upkeep::LIVE_EVERY_BOOST
        } else {
            super::upkeep::LIVE_EVERY
        }
    }

    /// The skin has drawn (or is about to): everything in `rev` is accounted for, and sampled
    /// readings wait one cadence before waking it again.
    pub(super) fn drawn(&mut self, now: Instant) {
        self.drawn_rev = self.rev;
        self.next_live_wake = now + self.live_every();
    }

    /// A sampled reading changed: wake the skin only when the cadence allows it.
    pub(super) fn may_draw_live(&mut self, now: Instant) -> bool {
        if now < self.next_live_wake {
            return false;
        }
        self.drawn(now);
        true
    }

    /// Takes in a new engine status or monitor sample and moves `rev` on when either changed. The
    /// upkeep uses `rev` to tell whether the skin has something new to draw.
    pub(super) fn sync(&mut self) {
        let r = self.status.rev();
        if self.status_seen != Some(r) {
            self.status_view = Arc::new(self.status.lock().map(|s| s.clone()).unwrap_or_default());
            self.status_seen = Some(r);
            self.rev += 1;
        }
        // An Arc clone, not a copy: the view keeps the sample it drew from.
        let snap = procs::latest(&self.snap);
        if self.snap_seen != Some(snap.serial) {
            self.snap_seen = Some(snap.serial);
            self.snap_view = snap;
            self.rev += 1;
        }
    }

    /// Carries out a hint from the skin. Cheap: anything that samples, saves or trims is left to
    /// `upkeep`, which the caller kicks next.
    pub(super) fn apply_hint(&mut self, hint: Hint) {
        match hint {
            Hint::Interest(i) => self.interest = i,
            Hint::Shown => {
                if !self.shown {
                    self.shown = true;
                    // The window is on screen; probing now keeps the work off its first frames.
                    self.refresh_hardware();
                    self.refresh_services();
                }
            }
            Hint::Minimised(on) => {
                if self.minimised != on {
                    self.minimised = on;
                    if on {
                        // The window gets no more frames, so the 30 s timer must not be what does
                        // the trim that matters: trim once shortly after the last frame instead.
                        self.trim_due = (!self.selftest).then(|| Instant::now() + super::upkeep::TRIM_AFTER_MINIMISE);
                    } else {
                        self.trim_due = None;
                    }
                }
            }
            Hint::CloseRequested => self.close_now(),
        }
    }

    /// The window was asked to close: put Windows back first if a boost is running, then report it
    /// ready. The skin keeps the window up until `Event::ReadyToClose`.
    fn close_now(&mut self) {
        if self.closing.is_some() {
            return;
        }
        if self.phase().active() {
            self.closing = Some(Instant::now());
            let _ = self.tx.send(Cmd::Restore);
        } else {
            self.save_now();
            self.event(Event::ReadyToClose);
        }
    }

    /// The current View, rebuilding only the sections whose source moved.
    pub(super) fn view(&mut self) -> View {
        self.sync();
        let inputs =
            BoostInputs { cfg: self.cfg_rev, status: self.status_seen.unwrap_or(0), services: self.services_rev, admin: self.system.admin };
        if self.boost_seen != Some(inputs) {
            // The plan lines, stages and notices are derived from data that does not change per
            // frame; doing it here keeps a skin's draw loop free of config walks and Windows calls.
            self.boost_view = Arc::new(boost::build(&self.cfg, &self.status_view, &self.services, self.system.admin));
            self.boost_seen = Some(inputs);
        }
        // This call is the skin drawing the current revision: it is not shown again for a sample.
        self.drawn_rev = self.rev;
        View {
            rev: self.rev,
            config: self.cfg.clone(),
            status: self.status_view.clone(),
            snap: self.snap_view.clone(),
            hardware: self.hardware.clone(),
            services: self.services.clone(),
            library: self.library.clone(),
            system: self.system,
            boost: self.boost_view.clone(),
        }
    }

    // ------------------------------------------------------------------ intents

    pub(super) fn apply(&mut self, intent: Intent) {
        match intent {
            Intent::Boost { game } => self.request_boost(game),
            Intent::ConfirmBoost { game, dont_ask_again } => {
                // Only the boost the summary was shown for: a skin cannot start one without the core's say-so.
                if self.awaiting_confirm.take() == Some(game.clone()) {
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
            Intent::SelectGame { id } => self.select_game(&id),
            Intent::AddGame { path, name, args, real_exe } => self.add_game(&path, name, args, real_exe),
            Intent::RemoveGame { id } => self.remove_game(&id),
            Intent::EditGame { id, name, launch, args } => self.edit_game(&id, name, launch, args),
            Intent::SetRealProgram { id, exe } => self.set_real_program(&id, exe),
            Intent::SetGameProfile { id, preset } => self.set_game_profile(&id, preset),
            Intent::GiveGameOwnSettings { id } => self.give_own_settings(&id),
            Intent::FollowPreset { id } => self.follow_preset(&id),
            Intent::ScanLibrary => self.scan_library(),
            Intent::HideInstalled => {
                Arc::make_mut(&mut self.library).installed = None;
                self.rev += 1;
            }
            Intent::FindCandidates { id, folder } => self.find_candidates(id, folder),
            Intent::NewPreset { from } => self.new_preset(from),
            Intent::RenamePreset { id, name } => self.rename_preset(&id, name),
            Intent::DeletePreset { id } => self.delete_preset(&id),
            Intent::ResetPreset { id } => self.reset_preset(&id),
            Intent::SetDefaultPreset { id } => self.set_default_preset(&id),
            Intent::SetSetting { target, key, value } => self.set_setting(target, &key, &value),
            Intent::SetAppRule { scope, exe, action } => self.set_app_rule(scope, exe, action),
            Intent::UseSuggestedApps { scope } => self.use_suggested_apps(scope),
            Intent::ClearAppRules => self.clear_app_rules(),
            Intent::EndApp { exe, name } => self.end_app(&exe, &name),
            Intent::SetServiceRule { scope, name, stop } => self.set_service_rule(scope, name, stop),
            Intent::UseRecommendedServices { scope } => self.use_recommended_services(scope),
            Intent::ClearServiceRules => self.clear_service_rules(),
            Intent::UseSuggestions { scope } => self.use_suggestions(scope),
            Intent::RefreshHardware => self.refresh_hardware(),
            Intent::RefreshServices => self.refresh_services(),
            Intent::TestGraphicsCard => self.test_graphics_card(),
            Intent::UsePowerPlan { guid } => self.set_power_plan(guid),
            Intent::CreateBoostPlan { level } => self.create_boost_plan(level),
            Intent::SetHeatLimit { c } => {
                let cfg = self.cfg_mut();
                cfg.gpu_temp_limit_c = c;
                self.mark_dirty();
            }
            Intent::FinishWelcome { use_suggestions } => self.finish_welcome(use_suggestions),
            Intent::SetupOneClickStart => self.run_elevated("elevate setup", "One-click administrator start is ready"),
            Intent::RemoveOneClickStart => self.run_elevated("elevate remove", "One-click start removed"),
            Intent::SetWatcher { on } => self.set_watcher(on),
            Intent::RelaunchAsAdmin => self.relaunch_admin(),
            Intent::ExportConfig(path) => self.export_config(&path),
            Intent::ImportConfig(path) => self.import_config(&path),
            Intent::ResetConfig => self.reset_config(),
            Intent::SetOption { key, value } => {
                // A bad key or value changes nothing; the catalogue is the only judge.
                let cfg = self.cfg_mut();
                if spec::set_option(cfg, &key, &value).is_ok() {
                    self.mark_dirty();
                }
            }
            Intent::SetPref { key, value } => self.set_pref(&key, value),
        }
    }

    /// Changes one setting through the catalogue. Works on a copy first so a stale target or an
    /// unknown key leaves the config, its revision and its save timer untouched.
    fn set_setting(&mut self, target: Target, key: &str, value: &str) {
        let mut s = match &target {
            Target::Preset(id) => match self.cfg.presets.iter().find(|p| p.id == *id) {
                Some(p) => p.settings.clone(),
                None => return,
            },
            Target::Game(id) => match self.cfg.games.iter().find(|g| g.id == *id).and_then(|g| g.custom.clone()) {
                Some(s) => s,
                None => return,
            },
        };
        if spec::set(&mut s, key, value).is_err() {
            return;
        }
        let cfg = self.cfg_mut();
        match &target {
            Target::Preset(id) => {
                if let Some(p) = cfg.presets.iter_mut().find(|p| p.id == *id) {
                    p.settings = s;
                }
            }
            Target::Game(id) => {
                if let Some(g) = cfg.games.iter_mut().find(|g| g.id == *id) {
                    g.custom = Some(s);
                }
            }
        }
        self.mark_dirty();
    }

    /// The index of a game by id: ids survive removals, indexes do not.
    fn game_index(&self, id: &str) -> Option<usize> {
        self.cfg.games.iter().position(|g| g.id == id)
    }

    /// A rule scope as an index. No scope means all games; a scope whose game is gone is an error,
    /// because treating it as "all games" would write the wrong list.
    fn scope_index(&self, scope: &Option<GameId>) -> Result<Option<usize>, ()> {
        match scope {
            None => Ok(None),
            Some(id) => self.game_index(id).map(Some).ok_or(()),
        }
    }

    /// Selects a game by id (the Boost page's game picker). A stale id changes nothing.
    fn select_game(&mut self, id: &str) {
        let Some(i) = self.game_index(id) else { return };
        if self.cfg.selected_game != i {
            self.cfg_mut().selected_game = i;
            self.mark_dirty();
        }
    }

    fn remove_game(&mut self, id: &str) {
        let Some(i) = self.game_index(id) else { return };
        let cfg = self.cfg_mut();
        cfg.games.remove(i);
        // The library may have shrunk: keep every reader of the selected game safe.
        cfg.clamp_selected();
        self.mark_dirty();
    }

    fn edit_game(&mut self, id: &str, name: String, launch: String, args: String) {
        let Some(i) = self.game_index(id) else { return };
        let cfg = self.cfg_mut();
        cfg.games[i].name = name;
        cfg.games[i].launch = launch;
        cfg.games[i].args = args;
        self.mark_dirty();
    }

    fn set_real_program(&mut self, id: &str, exe: Option<String>) {
        let Some(i) = self.game_index(id) else { return };
        self.cfg_mut().games[i].real_exe = exe.unwrap_or_default();
        self.mark_dirty();
    }

    fn set_game_profile(&mut self, id: &str, preset: PresetId) {
        if !self.cfg.presets.iter().any(|p| p.id == preset) {
            return;
        }
        let Some(i) = self.game_index(id) else { return };
        let cfg = self.cfg_mut();
        cfg.games[i].preset = preset;
        // Choosing a preset means following it again; own settings would hide the choice.
        cfg.games[i].custom = None;
        self.mark_dirty();
    }

    fn give_own_settings(&mut self, id: &str) {
        let Some(g) = self.cfg.games.iter().find(|g| g.id == id) else { return };
        if g.custom.is_some() {
            return;
        }
        let s = self.cfg.settings_for(Some(g));
        let cfg = self.cfg_mut();
        if let Some(g) = cfg.games.iter_mut().find(|g| g.id == id) {
            g.custom = Some(s);
        }
        self.mark_dirty();
    }

    fn follow_preset(&mut self, id: &str) {
        let Some(i) = self.game_index(id) else { return };
        self.cfg_mut().games[i].custom = None;
        self.mark_dirty();
    }

    fn new_preset(&mut self, from: Target) {
        let base = match &from {
            Target::Preset(id) => match self.cfg.presets.iter().find(|p| p.id == *id) {
                Some(p) => p.settings.clone(),
                None => return,
            },
            Target::Game(id) => match self.cfg.games.iter().find(|g| g.id == *id) {
                Some(g) => g.custom.clone().unwrap_or_default(),
                None => return,
            },
        };
        let cfg = self.cfg_mut();
        let n = cfg.presets.iter().filter(|p| !p.builtin).count() + 1;
        let id = cfg.new_id("preset");
        cfg.presets.push(Preset { id, name: format!("My preset {n}"), about: "Your own preset".into(), builtin: false, settings: base });
        self.mark_dirty();
    }

    fn rename_preset(&mut self, id: &str, name: String) {
        // A built-in's name is the catalogue's, not the person's to change.
        let Some(i) = self.cfg.presets.iter().position(|p| p.id == id && !p.builtin) else { return };
        self.cfg_mut().presets[i].name = name;
        self.mark_dirty();
    }

    fn delete_preset(&mut self, id: &str) {
        let Some(i) = self.cfg.presets.iter().position(|p| p.id == id && !p.builtin) else { return };
        let cfg = self.cfg_mut();
        cfg.presets.remove(i);
        // Its games and the default move to a built-in preset rather than pointing at nothing.
        let fallback = cfg.presets.get(1.min(cfg.presets.len().saturating_sub(1))).map(|p| p.id.clone()).unwrap_or_default();
        for g in cfg.games.iter_mut().filter(|g| g.preset == id) {
            g.preset = fallback.clone();
        }
        if cfg.default_preset == id {
            cfg.default_preset = fallback;
        }
        self.mark_dirty();
    }

    fn reset_preset(&mut self, id: &str) {
        let Some(b) = crate::config::builtin_presets().into_iter().find(|b| b.id == id) else { return };
        let Some(i) = self.cfg.presets.iter().position(|p| p.id == id) else { return };
        self.cfg_mut().presets[i].settings = b.settings;
        self.mark_dirty();
    }

    fn set_default_preset(&mut self, id: &str) {
        if !self.cfg.presets.iter().any(|p| p.id == id) {
            return;
        }
        self.cfg_mut().default_preset = id.to_string();
        self.mark_dirty();
    }

    fn set_app_rule(&mut self, scope: Option<GameId>, exe: String, action: ProcAction) {
        let Ok(game) = self.scope_index(&scope) else { return };
        self.cfg_mut().set_app_rule(game, &exe, action);
        self.mark_dirty();
    }

    fn use_suggested_apps(&mut self, scope: Option<GameId>) {
        let Ok(game) = self.scope_index(&scope) else { return };
        apply_app_suggestions_to(self.cfg_mut(), game);
        self.mark_dirty();
    }

    fn clear_app_rules(&mut self) {
        let cfg = self.cfg_mut();
        // "For all games" has to mean it: per-game rules would otherwise survive.
        cfg.apps.clear();
        for g in &mut cfg.games {
            g.apps.clear();
        }
        self.mark_dirty();
    }

    fn set_service_rule(&mut self, scope: Option<GameId>, name: String, stop: bool) {
        let Ok(game) = self.scope_index(&scope) else { return };
        self.cfg_mut().set_service_rule(game, &name, stop);
        self.mark_dirty();
    }

    fn use_recommended_services(&mut self, scope: Option<GameId>) {
        let Ok(game) = self.scope_index(&scope) else { return };
        apply_service_suggestions_to(self.cfg_mut(), game);
        self.mark_dirty();
    }

    fn clear_service_rules(&mut self) {
        let cfg = self.cfg_mut();
        cfg.services.clear();
        for g in &mut cfg.games {
            g.services.clear();
        }
        self.mark_dirty();
    }

    fn use_suggestions(&mut self, scope: Option<GameId>) {
        let Ok(game) = self.scope_index(&scope) else { return };
        apply_suggestions_to(self.cfg_mut(), game);
        self.mark_dirty();
    }

    fn finish_welcome(&mut self, use_suggestions: bool) {
        let cfg = self.cfg_mut();
        if use_suggestions {
            apply_suggestions(cfg);
        }
        cfg.onboarded = true;
        self.mark_dirty();
    }

    fn set_watcher(&mut self, on: bool) {
        if on {
            self.run_elevated("autostart on", "Background watcher enabled");
        } else {
            self.run_elevated("autostart off", "Background watcher removed");
        }
    }

    fn reset_config(&mut self) {
        let cfg = self.cfg_mut();
        *cfg = Config::default();
        cfg.onboarded = true;
        // The dialog promises the suggestions, so apply them (they fill, never overwrite).
        apply_suggestions(cfg);
        self.mark_dirty();
        self.toast(Tone::Ok, "Settings reset");
    }

    /// A look preference: stored as it comes, but only for the five fields the classic skin uses
    /// until Phase 1b moves them under `skins.<name>`.
    fn set_pref(&mut self, key: &str, value: String) {
        let tint = value.parse::<u8>().ok().filter(|n| *n <= 2);
        let known = match key {
            "theme" | "accent" | "boost-control" => true,
            "appearance" => matches!(value.as_str(), "dark" | "light" | "system"),
            "tint" => tint.is_some(),
            _ => false,
        };
        if !known {
            return;
        }
        let cfg = self.cfg_mut();
        match key {
            "theme" => cfg.theme = value,
            "accent" => cfg.accent = value,
            "boost-control" => cfg.boost_control = value,
            "appearance" => cfg.appearance = value,
            _ => cfg.tint = tint.unwrap_or(0),
        }
        self.mark_dirty();
    }

    /// Starting a boost: first the summary of what will change (unless the person turned that off).
    fn request_boost(&mut self, game: Option<GameId>) {
        if self.cfg.confirm_boost {
            self.show_boost_summary(game);
        } else {
            self.boost_game(game);
        }
    }

    /// Asks the skin to show the summary for this boost and waits for its answer. The plan is made
    /// for exactly this game now, so a skin never has to work out which game the summary belongs to.
    pub(super) fn show_boost_summary(&mut self, game: Option<GameId>) {
        let idx = match &game {
            Some(id) => match self.game_index(id) {
                Some(i) => Some(i),
                None => return,
            },
            None => None,
        };
        let target = idx.and_then(|i| self.cfg.games.get(i));
        let plan = boost::plan(&self.cfg, target, &self.services, self.system.admin);
        self.awaiting_confirm = Some(game.clone());
        self.event(Event::ConfirmBoost { game, plan });
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

    fn boost_game(&mut self, id: Option<GameId>) {
        let idx = match &id {
            // A game that is gone cannot be started: gaming mode would be a surprising substitute.
            Some(id) => match self.game_index(id) {
                Some(i) => Some(i),
                None => return,
            },
            None => None,
        };
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
        self.wake_skin();
    }

    fn restore(&mut self) {
        let pid = self.status.lock().map(|s| s.game_pid).unwrap_or(0);
        if pid != 0 {
            self.auto_ignore.insert(pid);
        }
        let _ = self.tx.send(Cmd::Restore);
        self.wake_skin();
    }

    /// Ends a program the person pointed at. The scan is fresh and every pid is checked against its
    /// start time, so a recycled id is never touched; it all happens on a job thread.
    fn end_app(&mut self, exe: &str, name: &str) {
        let exe = exe.to_string();
        let name = name.to_string();
        self.spawn(move || Some(Done::EndedApp { name, count: procs::terminate_by_name(&exe) }));
    }

    /// Opens the elevated copy. Pending changes are written first (the old copy used to exit and
    /// lose them), Windows is put back before the window goes, and the launch runs on a job thread
    /// so a UAC prompt never freezes the window.
    fn relaunch_admin(&mut self) {
        self.save_now();
        self.restore();
        let status = self.status.clone();
        self.spawn(move || {
            let started = elevate::launch_ui_via_task() || ffi::relaunch_elevated();
            if !started {
                return Some(Done::Relaunch(false));
            }
            // Let the restore finish rather than ending the process in the middle of it; the new
            // copy then gets the same short head start the old code gave it.
            let t = Instant::now();
            while status.lock().map(|s| s.phase.active()).unwrap_or(false) && t.elapsed() < Duration::from_secs(20) {
                std::thread::sleep(Duration::from_millis(100));
            }
            std::thread::sleep(Duration::from_millis(400));
            Some(Done::Relaunch(true))
        });
    }

    /// Adds a game from a path or store link (used by the form, drag-and-drop and the importer).
    fn add_game(&mut self, path: &str, name: Option<String>, args: String, real_exe: Option<String>) {
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
            args,
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
            .and_then(|t| Config::from_json(&t).map_err(|e| format!("not a CatalyX settings file: {e}")))
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

/// Selects the catalog's gentle suggestions for every game. Choices the person already made are
/// never overwritten.
fn apply_suggestions(cfg: &mut Config) {
    apply_suggestions_to(cfg, None);
}

/// Selects the catalog's gentle suggestions for one scope (None = all games). App suggestions only
/// fill actions still on Keep; service suggestions only fill services nobody has answered for.
fn apply_suggestions_to(cfg: &mut Config, game: Option<usize>) {
    apply_app_suggestions_to(cfg, game);
    apply_service_suggestions_to(cfg, game);
}

/// The app half of `apply_suggestions_to`: never touches a program that already has an action,
/// whether the rule sits on the game or in the global list.
fn apply_app_suggestions_to(cfg: &mut Config, game: Option<usize>) {
    let current = cfg.apps_for(game.and_then(|i| cfg.games.get(i)));
    for (exe, a) in crate::catalog::suggested_apps() {
        // A game's own rule is always explicit, even when it says Keep (the game map only stores
        // differences); a global Keep is the unanswered state, where suggestions may fill in.
        let explicit = game.and_then(|i| cfg.games.get(i)).map(|g| g.apps.contains_key(&exe)).unwrap_or(false);
        if current.get(&exe).copied().unwrap_or(ProcAction::Keep) == ProcAction::Keep && !explicit {
            cfg.set_app_rule(game, &exe, a);
        }
    }
}

/// The service half: an answer recorded in this scope is the person's choice and stays as it is.
/// For a game, an answer recorded globally counts too: the game map only holds differences, so
/// silence there means "follow the global list", not "nobody has answered".
fn apply_service_suggestions_to(cfg: &mut Config, game: Option<usize>) {
    for s in crate::catalog::KNOWN_SERVICES.iter().filter(|s| s.recommended) {
        // Windows protects these: they are never stopped, so a rule for one would only be noise.
        if crate::config::service_is_protected(s.name) {
            continue;
        }
        let answered_here = cfg.service_choice(game, s.name).is_some();
        let answered_globally = game.is_some() && cfg.service_choice(None, s.name).is_some();
        if !answered_here && !answered_globally {
            cfg.set_service_rule(game, s.name, true);
        }
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
        // The same lines a real boost writes, with the same tags: the picture is the truth.
        st.log = [
            (Tags::GAME, "Boosting for Stellar Drift"),
            (Tags::POWER.and(Tags::OK), "Power plan: CatalyX plan, CPU Boost (checked: turbo aggressive, cores unparked)"),
            (Tags::WINDOWS, "Windows: Game Mode on"),
            (Tags::SERVICES, "Stopped service Connected User Experiences and Telemetry"),
            (Tags::MEMORY, "Freed 640 MB of memory from 32 apps"),
            (Tags::GAME, "Starting Stellar Drift"),
            (Tags::GAME.and(Tags::OK), "Found the game: stellardrift-win64.exe (pid 4242)"),
            (Tags::GAME.and(Tags::OK), "Game priority: Above normal"),
            (Tags::GAME.and(Tags::OK), "Game disk I/O priority: High (checked with Windows)"),
            (Tags::GAME, "Windows efficiency mode kept off for the game"),
        ]
        .into_iter()
        .map(|(tags, text)| LogLine::new(tags, text))
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
        let snap = Arc::new(Mutex::new(Arc::new(Snapshot::default())));
        (State::new(Config::default(), true, false, false, tx, status, snap, false), rx)
    }

    #[test]
    fn a_boost_asks_first_when_the_person_wants_a_summary() {
        let (mut st, rx) = state();
        st.apply(Intent::Boost { game: None });
        assert!(rx.try_recv().is_err(), "nothing may start before the summary is confirmed");
        assert!(matches!(st.take_events().as_slice(), [Event::ConfirmBoost { game: None, .. }]));
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
        st.apply(Intent::SetOption { key: "confirm-boost".into(), value: "off".into() });
        assert!(st.dirty_since.is_some(), "an edit is saved later");
        st.apply(Intent::Boost { game: None });
        assert!(matches!(rx.try_recv(), Ok(Cmd::Boost(_))));
        let events = st.take_events();
        assert!(events.iter().all(|e| !matches!(e, Event::ConfirmBoost { .. })));
    }

    #[test]
    fn the_summary_plan_is_for_the_game_that_asked_for_it() {
        let (mut st, _rx) = state();
        let cfg = st.cfg_mut();
        cfg.games.push(Game { id: "one".into(), name: "One".into(), ..Default::default() });
        cfg.games.push(Game {
            id: "two".into(),
            name: "Two".into(),
            custom: Some(crate::config::Settings {
                power: crate::config::PowerChoice::Keep,
                cpu: crate::config::CpuLevel::Off,
                ..Default::default()
            }),
            ..Default::default()
        });
        // The selected game is still the first; the summary is asked for the second, by id.
        st.apply(Intent::Boost { game: Some("two".into()) });
        let events = st.take_events();
        match events.as_slice() {
            [Event::ConfirmBoost { game: Some(id), plan }] => {
                assert_eq!(id, "two", "the event names the game that asked for the summary");
                assert!(
                    plan.lines.iter().all(|l| !l.contains("power plan")),
                    "the plan belongs to the game being confirmed, not the selected one"
                );
            }
            other => panic!("expected a summary for game two, got {other:?}"),
        }
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

    #[test]
    fn suggestions_respect_the_scope_and_keep_explicit_choices() {
        let mut c = Config::default();
        c.games.push(Game { name: "One".into(), ..Default::default() });
        let mut suggested = crate::catalog::suggested_apps().into_iter();
        let (kept, _) = suggested.next().unwrap();
        let (filled, action) = suggested.next().unwrap();
        let (global, _) = suggested.next().unwrap();
        // An explicit Keep on the game itself, and an explicit action in the global list.
        c.games[0].apps.insert(kept.clone(), ProcAction::Keep);
        c.apps.insert(global.clone(), ProcAction::Close);

        apply_app_suggestions_to(&mut c, Some(0));
        assert_eq!(c.games[0].apps.get(&kept), Some(&ProcAction::Keep), "an explicit game choice is kept");
        assert_eq!(c.apps.get(&global), Some(&ProcAction::Close), "a global choice is not overwritten");
        assert_eq!(c.games[0].apps.get(&filled), Some(&action), "the game scope gets its own rule");
        assert_eq!(c.apps.get(&filled), Some(&ProcAction::Keep), "a game scope never writes the global list");
    }

    #[test]
    fn service_suggestions_fill_unanswered_services_only() {
        let mut names =
            crate::catalog::KNOWN_SERVICES.iter().filter(|s| s.recommended && !crate::config::service_is_protected(s.name)).map(|s| s.name);
        let off = names.next().unwrap();
        let on = names.next().unwrap();
        let mine = names.next().unwrap();

        let mut c = Config::default();
        c.services.insert(off.to_string(), false);
        apply_suggestions_to(&mut c, None);
        assert_eq!(c.services.get(off), Some(&false), "a service turned off is not turned back on");
        assert_eq!(c.services.get(on), Some(&true), "an unanswered service is filled");

        // In a game scope an unanswered service gets a game rule, while a global answer is
        // inherited: the game map only holds differences, so silence means "follow the global
        // list", and the global choice must not be silently reversed for one game.
        let mut c = Config::default();
        c.games.push(Game { name: "One".into(), ..Default::default() });
        c.services.insert(off.to_string(), false);
        c.games[0].services.insert(mine.to_string(), false);
        apply_suggestions_to(&mut c, Some(0));
        assert_eq!(c.games[0].services.get(on), Some(&true), "the game scope gets its own rule");
        assert_eq!(c.services.get(on), None, "a game scope never writes the global list");
        assert_eq!(c.games[0].services.get(off), None, "a global off choice is inherited");
        assert_eq!(c.services.get(off), Some(&false), "the global choice is untouched");
        assert_eq!(c.games[0].services.get(mine), Some(&false), "the game's own choice is kept");
    }

    #[test]
    fn shown_starts_the_probes_once() {
        let (mut st, _rx) = state();
        st.apply_hint(Hint::Shown);
        assert_eq!(st.jobs.len(), 2, "the card and the service list are read once the window is up");
        st.jobs.clear();
        st.apply_hint(Hint::Shown);
        assert!(st.jobs.is_empty(), "a second frame must not probe again");
    }

    #[test]
    fn a_close_request_without_a_boost_is_answered_at_once() {
        let (mut st, rx) = state();
        st.apply_hint(Hint::CloseRequested);
        assert!(rx.try_recv().is_err(), "nothing to put back");
        assert!(matches!(st.take_events().as_slice(), [Event::ReadyToClose]));
    }

    #[test]
    fn a_close_request_during_a_boost_restores_before_answering() {
        let (mut st, rx) = state();
        if let Ok(mut s) = st.status.lock() {
            s.phase = Phase::Boosted;
        }
        st.apply_hint(Hint::CloseRequested);
        assert!(matches!(rx.try_recv(), Ok(Cmd::Restore)), "the boost is put back first");
        assert!(st.take_events().is_empty(), "the window stays up while it happens");
        assert!(st.closing.is_some());

        if let Ok(mut s) = st.status.lock() {
            s.phase = Phase::Idle;
        }
        st.upkeep(Instant::now());
        assert!(matches!(st.take_events().as_slice(), [Event::ReadyToClose]));
        assert!(st.closing.is_none(), "the answer is given once");
    }

    #[test]
    fn a_minimised_window_is_not_woken_but_is_trimmed_once() {
        let (mut st, _rx) = state();
        st.apply_hint(Hint::Minimised(true));
        let due = st.trim_due.expect("a trim is due shortly after the last frame");
        st.wake_skin();
        assert!(!st.wake, "a minimised window draws nothing, so it is not woken");

        let next = st.upkeep(due - Duration::from_millis(1)).expect("the trim has a deadline");
        assert!(next <= due);
        assert!(st.trim_due.is_some(), "the trim happens when it is due, not before");

        st.apply_hint(Hint::Minimised(false));
        assert!(st.trim_due.is_none());
        st.wake_skin();
        assert!(st.wake, "a restored window is woken again");
    }

    #[test]
    fn live_sources_share_one_draw_cadence() {
        let (mut st, _rx) = state();
        let t0 = Instant::now();
        st.drawn(t0);
        let cadence = st.live_every();
        st.rev += 1; // the monitor published a sample
        assert!(!st.may_draw_live(t0 + Duration::from_millis(100)), "a sample waits for the cadence");
        assert!(st.may_draw_live(t0 + cadence), "the first sample after it is drawn");
        let next = st.next_live_wake;
        st.rev += 1; // the card was read a moment later
        assert!(!st.may_draw_live(t0 + cadence + Duration::from_millis(100)), "two sources share one draw");
        assert!(st.may_draw_live(next), "and the next cadence shows everything new");
    }

    #[test]
    fn drawing_marks_the_revision_it_shows() {
        let (mut st, _rx) = state();
        st.rev = 7;
        let _ = st.view();
        assert_eq!(st.drawn_rev, st.rev, "a drawn revision is not drawn again for a sample");
    }

    #[test]
    fn the_boost_view_is_rebuilt_only_when_its_inputs_change() {
        let (mut st, _rx) = state();
        let first = st.view().boost.clone();
        assert!(Arc::ptr_eq(&first, &st.view().boost), "a second frame reuses the boost view");

        // A monitor sample is not a boost input: it must not rebuild the plan.
        let mut next = (*procs::latest(&st.snap)).clone();
        next.serial += 1;
        procs::publish(&st.snap, next);
        assert!(Arc::ptr_eq(&first, &st.view().boost), "a sample does not rebuild the plan");

        // Any config edit rebuilds it: deciding which fields a plan reads would be fragile, and the
        // rebuild is cheap compared with the frame that follows an edit.
        st.apply(Intent::SetPref { key: "boost-control".into(), value: "gauge".into() });
        let after_edit = st.view().boost.clone();
        assert!(!Arc::ptr_eq(&first, &after_edit));

        // A new service list is: its cached can_start facts decide which services are promised.
        st.finish(Done::Services(Vec::new()));
        let after_services = st.view().boost.clone();
        assert!(!Arc::ptr_eq(&after_edit, &after_services));

        // The engine's status is: stages finish while a boost is applied.
        if let Ok(mut s) = st.status.lock() {
            s.phase = Phase::Boosted;
        }
        let after_status = st.view().boost.clone();
        assert!(!Arc::ptr_eq(&after_services, &after_status));
        assert_eq!(after_status.progress, 1.0);
    }

    #[test]
    fn a_stale_id_does_nothing_instead_of_hitting_the_wrong_entry() {
        let (mut st, rx) = state();
        st.cfg_mut().games.push(Game { id: "one".into(), name: "One".into(), ..Default::default() });
        st.apply(Intent::Boost { game: Some("gone".into()) });
        assert!(rx.try_recv().is_err(), "a removed game is not started, and not swapped for gaming mode");
        st.apply(Intent::SelectGame { id: "gone".into() });
        assert_eq!(st.cfg.selected_game, 0, "a dangling selection is ignored");
        st.apply(Intent::SetSetting { target: Target::Preset("gone".into()), key: "priority".into(), value: "high".into() });
        assert!(st.dirty_since.is_none(), "a stale target leaves the config and its save timer untouched");
        st.apply(Intent::RemoveGame { id: "one".into() });
        assert!(st.cfg.games.is_empty(), "a real id removes exactly that game");
        assert!(st.take_events().is_empty(), "an id from a skin is not a licence to toast");
    }

    #[test]
    fn a_rule_for_a_removed_game_never_becomes_the_global_rule() {
        let (mut st, _rx) = state();
        st.apply(Intent::SetAppRule { scope: Some("gone".into()), exe: "chat.exe".into(), action: ProcAction::Close });
        st.apply(Intent::SetServiceRule { scope: Some("gone".into()), name: "DiagTrack".into(), stop: true });
        // A fresh config carries suggested app rules, so the check is that this one did not land.
        assert!(!st.cfg.apps.contains_key("chat.exe"), "a stale scope must not be read as 'all games'");
        assert!(st.cfg.services.is_empty(), "the same for services");
    }
}
