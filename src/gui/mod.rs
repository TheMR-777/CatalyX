//! The window. Built on egui (eframe, OpenGL) and the egui-components library.
//!
//! Footprint rules: the window draws only when something changed (input, a background result,
//! or a slow 2 s tick for the live meters); a minimised window asks for nothing; per-process
//! scans only run while the Apps page is open or a boost is active; every blocking job (file
//! dialogs, hardware probes, library scans) runs on its own short-lived thread.

mod checks;
mod chrome;
mod control;
mod icons;
pub mod mark;
mod pages;
mod theme;
mod widgets;

use crate::config::{Config, Game};
use crate::engine::{BoostRequest, Cmd, Phase, SharedStatus};
use crate::{ffi, gpu, library, power, procs, services};
use egui::{vec2, Align, Context, Layout, RichText};
use egui_components::Toasts;
use icons::Ico;
use std::collections::HashSet;
use std::sync::mpsc::{channel, Receiver, Sender};
use std::time::{Duration, Instant};
use theme::{AccentId, ThemeId};
use widgets::{bold, faint};

/// Height of the combined title bar / page header.
const TITLEBAR_H: f32 = 48.0;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Tab {
    Boost,
    Games,
    Apps,
    Services,
    Profiles,
    Tuning,
    Settings,
}

pub const TABS: [(Tab, &str, &str, Ico); 7] = [
    (Tab::Boost, "Boost", "Play boosted", Ico::Bolt),
    (Tab::Games, "Games", "Your library", Ico::Gamepad),
    (Tab::Apps, "Apps", "What stays on while you play", Ico::Apps),
    (Tab::Services, "Services", "Background services", Ico::Layers),
    (Tab::Profiles, "Profiles", "Presets and per-game settings", Ico::Sliders),
    (Tab::Tuning, "Tuning", "Power, CPU and graphics card", Ico::Gauge),
    (Tab::Settings, "Settings", "Behaviour and look", Ico::Gear),
];

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum EditTarget {
    Preset(usize),
    Game(usize),
}

pub enum PickFor {
    GameFile,
    ExportConfig,
    ImportConfig,
}

/// Results coming back from worker threads.
pub enum Msg {
    Hardware { gpu: Box<gpu::GpuState>, plans: Vec<power::Plan> },
    Services(Vec<services::ServiceInfo>),
    Picked(PickFor, String),
    Installed(Vec<library::Installed>),
    Predicted { id: String, exe: String },
    Candidates { id: String, list: Vec<(String, u64)> },
    Info(String),
    Error(String),
    Elevation { ui_task: bool, watcher: bool },
}

pub struct Options {
    pub demo_session: bool,
    pub selftest: Option<String>,
    pub screenshot: Option<String>,
    pub tour: Option<String>,
    pub tab: Option<Tab>,
    pub theme: Option<String>,
    pub appearance: Option<String>,
    pub accent: Option<String>,
    /// "bar", "gauge" or "ring": the boost control's look, for screenshots.
    pub control: Option<String>,
    /// "boost": open the boost summary dialog (for screenshots).
    pub dialog: Option<String>,
    pub size: Option<(f32, f32)>,
    pub demo: bool,
}

pub struct CatalyxApp {
    pub cfg: Config,
    pub ctx: Context,
    pub snap: procs::SharedSnapshot,
    pub tx: Sender<Cmd>,
    pub status: SharedStatus,
    pub admin: bool,
    pub tab: Tab,
    pub toasts: Toasts,
    msg_tx: Sender<Msg>,
    msg_rx: Receiver<Msg>,
    pub dirty_since: Option<Instant>,
    pub no_save: bool,
    sys_dark: bool,
    last_sys_check: Instant,
    palette_key: Option<(ThemeId, AccentId, u8, bool)>,
    titlebar_for: Option<(bool, u32)>,
    // hardware and services, filled by workers
    pub gpu: Option<gpu::GpuState>,
    pub plans: Vec<power::Plan>,
    pub svc_list: Vec<services::ServiceInfo>,
    pub live: Option<gpu::Live>,
    last_live: Instant,
    hw_busy: bool,
    pub elevation: Option<(bool, bool)>,
    pub elevation_busy: bool,
    svc_busy: bool,
    last_svc: Option<Instant>,
    // automatic boost
    pub auto_ignore: HashSet<u32>,
    last_auto_scan: Instant,
    // closing while a boost is active
    closing: Option<Instant>,
    last_trim: Instant,
    /// The minimised window has been trimmed once (reset when it is restored).
    trimmed_minimized: bool,
    // page state
    pub ui: pages::PageState,
    pub confirm: Option<pages::Confirm>,
    /// The "don't ask again" box in the boost summary (kept here while that dialog is open).
    pub boost_dont_ask: bool,
    /// The window starts hidden and is shown after its first frame is drawn (no empty frame on screen).
    /// 0 = nothing drawn yet, 1 = first frame drawn, 2 = shown.
    pub reveal_step: u8,
    // checks
    pub drawn: HashSet<&'static str>,
    selftest: Option<crate::selftest::SelfTest>,
    opts: Options,
    frames: u32,
    shot_requested: bool,
    tour_step: usize,
}

impl CatalyxApp {
    pub fn new(cc: &eframe::CreationContext<'_>, opts: Options) -> Self {
        crate::trace::mark("window and GL context ready");
        theme::install_fonts(&cc.egui_ctx);
        crate::trace::mark("fonts installed");
        let no_save = opts.screenshot.is_some() || opts.tour.is_some() || opts.demo || opts.selftest.is_some();
        let snap = procs::start_monitor();
        let (tx, status) = crate::engine::start(snap.clone());
        crate::trace::mark("monitor and engine started");
        let mut cfg = if no_save { Config::default() } else { Config::load() };
        if opts.demo && cfg.games.is_empty() {
            pages::demo_data(&mut cfg);
        }
        if let Some(t) = &opts.theme {
            cfg.theme = ThemeId::from_key(t).key().to_string();
        }
        if let Some(a) = &opts.appearance {
            cfg.appearance = a.to_lowercase();
        }
        if opts.dialog.as_deref() == Some("boost") {
            cfg.onboarded = true;
        }
        if let Some(c) = &opts.control {
            cfg.boost_control = control::Style::from_key(c).key().to_string();
        }
        if let Some(a) = &opts.accent {
            cfg.accent = AccentId::from_key(a).key().to_string();
        }
        if opts.demo_session {
            // For visual checks only: a believable running session (no real boost is made).
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
        let (msg_tx, msg_rx) = channel();
        let mut app = CatalyxApp {
            cfg,
            ctx: cc.egui_ctx.clone(),
            snap,
            tx,
            status,
            admin: ffi::is_admin(),
            tab: opts.tab.unwrap_or(Tab::Boost),
            toasts: Toasts::new(),
            msg_tx,
            msg_rx,
            dirty_since: None,
            no_save,
            sys_dark: ffi::system_prefers_dark(),
            last_sys_check: Instant::now(),
            palette_key: None,
            titlebar_for: None,
            gpu: None,
            plans: Vec::new(),
            svc_list: Vec::new(),
            live: None,
            last_live: Instant::now() - Duration::from_secs(10),
            hw_busy: false,
            elevation: None,
            elevation_busy: false,
            svc_busy: false,
            last_svc: None,
            auto_ignore: HashSet::new(),
            last_auto_scan: Instant::now(),
            closing: None,
            last_trim: Instant::now() - Duration::from_secs(10),
            trimmed_minimized: false,
            ui: pages::PageState::default(),
            confirm: None,
            boost_dont_ask: false,
            reveal_step: if opts.screenshot.is_some() || opts.tour.is_some() || opts.selftest.is_some() { 2 } else { 0 },
            drawn: HashSet::new(),
            selftest: opts.selftest.clone().map(crate::selftest::SelfTest::new),
            opts,
            frames: 0,
            shot_requested: false,
            tour_step: 0,
        };
        crate::trace::mark("app state built");
        if app.reveal_step == 2 {
            // A check run draws at once and needs the data now; a normal start probes after the window is shown.
            app.refresh_hardware();
            app.refresh_services();
        }
        if !app.cfg.onboarded && !app.no_save {
            app.confirm = Some(pages::Confirm::Welcome);
        }
        if app.opts.dialog.as_deref() == Some("boost") {
            app.confirm = Some(pages::Confirm::Boost { idx: Some(0) });
        }
        app
    }

    // ------------------------------------------------------------------ helpers

    /// Shows the window once one frame has really been drawn, then starts the hardware probes (so they never
    /// compete with the first frames). Called at the start of every frame.
    fn reveal_when_ready(&mut self, ctx: &Context) {
        match self.reveal_step {
            0 => {
                self.reveal_step = 1;
                ctx.request_repaint(); // make sure a second frame follows the first
            }
            1 => {
                self.reveal_step = 2;
                ctx.send_viewport_cmd(egui::ViewportCommand::Visible(true));
                ctx.send_viewport_cmd(egui::ViewportCommand::Focus);
                crate::trace::mark("window shown");
                self.refresh_hardware();
                self.refresh_services();
            }
            _ => {}
        }
    }

    pub fn dark_mode(&self) -> bool {
        match self.cfg.appearance.as_str() {
            "light" => false,
            "system" => self.sys_dark,
            _ => true,
        }
    }

    pub fn theme_id(&self) -> ThemeId {
        ThemeId::from_key(&self.cfg.theme)
    }

    pub fn accent_id(&self) -> AccentId {
        AccentId::from_key(&self.cfg.accent)
    }

    pub fn pal(&self) -> theme::Palette {
        theme::pal(&self.ctx)
    }

    pub fn mark_dirty(&mut self) {
        if self.dirty_since.is_none() {
            self.dirty_since = Some(Instant::now());
        }
    }

    pub fn save_now(&mut self) {
        if !self.no_save {
            self.cfg.save();
        }
        self.dirty_since = None;
    }

    pub fn phase(&self) -> Phase {
        self.status.lock().map(|s| s.phase).unwrap_or(Phase::Idle)
    }

    pub fn toast_ok(&mut self, text: impl Into<String>) {
        self.toasts.add(egui_components::Variant::Success, None, text.into());
    }
    pub fn toast_info(&mut self, text: impl Into<String>) {
        self.toasts.add(egui_components::Variant::Info, None, text.into());
    }
    pub fn toast_err(&mut self, text: impl Into<String>) {
        self.toasts.add(egui_components::Variant::Danger, None, text.into());
    }

    /// Run `job` on a worker thread; its message wakes the window.
    pub fn spawn(&self, job: impl FnOnce() -> Option<Msg> + Send + 'static) {
        let tx = self.msg_tx.clone();
        let ctx = self.ctx.clone();
        std::thread::spawn(move || {
            if let Some(m) = job() {
                let _ = tx.send(m);
                ctx.request_repaint();
            }
        });
    }

    pub fn refresh_hardware(&mut self) {
        if self.hw_busy {
            return;
        }
        self.hw_busy = true;
        self.spawn(|| Some(Msg::Hardware { gpu: Box::new(gpu::probe()), plans: power::list() }));
    }

    /// Which Task Scheduler entries exist (asked off the UI thread; it runs a helper program).
    pub fn refresh_elevation(&mut self) {
        if self.elevation_busy {
            return;
        }
        self.elevation_busy = true;
        self.spawn(|| Some(Msg::Elevation { ui_task: crate::elevate::ui_task_ready(), watcher: crate::elevate::watcher_enabled() }));
    }

    pub fn refresh_services(&mut self) {
        if self.svc_busy {
            return;
        }
        self.svc_busy = true;
        self.last_svc = Some(Instant::now());
        self.spawn(|| Some(Msg::Services(services::list())));
    }

    pub fn pick_file(&self, what: PickFor) {
        let (save, title, filter, ext): (bool, &str, &str, &str) = match what {
            PickFor::GameFile => (
                false,
                "Choose a game, launcher, batch file or shortcut",
                "Games and launchers\0*.exe;*.bat;*.cmd;*.lnk;*.url\0All files\0*.*\0\0",
                "",
            ),
            PickFor::ExportConfig => (true, "Save CatalyX settings", "CatalyX settings\0*.json\0\0", "json"),
            PickFor::ImportConfig => (false, "Load CatalyX settings", "CatalyX settings\0*.json\0All files\0*.*\0\0", "json"),
        };
        let (title, filter, ext) = (title.to_string(), filter.to_string(), ext.to_string());
        self.spawn(move || ffi::pick_file(&title, &filter, &ext, save).map(|p| Msg::Picked(what, p)));
    }

    // ------------------------------------------------------------------ boosting

    pub fn request_for(&self, game: Option<&Game>, attach: Option<u32>) -> BoostRequest {
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

    pub fn boost_game(&mut self, idx: Option<usize>) {
        let game = idx.and_then(|i| self.cfg.games.get(i)).cloned();
        if let Some(i) = idx {
            if let Some(g) = self.cfg.games.get_mut(i) {
                g.plays += 1;
            }
            self.cfg.selected_game = i;
            self.mark_dirty();
        }
        procs::set_detail(true);
        let req = self.request_for(game.as_ref(), None);
        let _ = self.tx.send(Cmd::Boost(Box::new(req)));
        match &game {
            Some(g) => self.toast_info(format!("Boosting and starting {}", g.name)),
            None => self.toast_info("Gaming mode on"),
        }
        self.ctx.request_repaint();
    }

    pub fn restore(&mut self) {
        let pid = self.status.lock().map(|s| s.game_pid).unwrap_or(0);
        if pid != 0 {
            self.auto_ignore.insert(pid);
        }
        let _ = self.tx.send(Cmd::Restore);
        self.ctx.request_repaint();
    }

    // ------------------------------------------------------------------ per-frame upkeep

    fn drain_messages(&mut self) {
        while let Ok(m) = self.msg_rx.try_recv() {
            match m {
                Msg::Hardware { gpu, plans } => {
                    self.gpu = Some(*gpu);
                    self.plans = plans;
                    self.hw_busy = false;
                }
                Msg::Services(l) => {
                    self.svc_list = l;
                    self.svc_busy = false;
                }
                Msg::Picked(what, path) => pages::picked(self, what, path),
                Msg::Installed(list) => {
                    self.ui.games.installed = Some(list);
                    self.ui.games.scanning = false;
                }
                Msg::Predicted { id, exe } => {
                    if let Some(g) = self.cfg.games.iter_mut().find(|g| g.id == id) {
                        if g.real_exe.is_empty() && !exe.is_empty() {
                            g.real_exe = exe;
                            self.mark_dirty();
                        }
                    }
                }
                Msg::Candidates { id, list } => {
                    self.ui.games.candidates.insert(id, list);
                }
                Msg::Info(s) => {
                    self.toast_info(s);
                    // A finished administrator job may have changed the tasks.
                    self.elevation = None;
                }
                Msg::Error(s) => {
                    self.toast_err(s);
                    self.elevation = None;
                }
                Msg::Elevation { ui_task, watcher } => {
                    self.elevation = Some((ui_task, watcher));
                    self.elevation_busy = false;
                }
            }
        }
    }

    fn learn_game_program(&mut self) {
        let learned = self.status.lock().ok().and_then(|mut s| s.learned.take());
        if let Some((gid, exe)) = learned {
            if let Some(g) = self.cfg.games.iter_mut().find(|g| g.id == gid) {
                if g.real_exe != exe {
                    g.real_exe = exe.clone();
                    self.toast_ok(format!("Learned the real game program: {exe}"));
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
            self.toast_info(format!("{} started - boosting automatically", g.name));
        }
    }

    /// Rounded corners and the drop shadow for the borderless window, once the window exists.
    fn sync_titlebar(&mut self) {
        if self.titlebar_for.is_none() && ffi::style_borderless_window() {
            self.titlebar_for = Some((true, 0));
        }
    }

    fn apply_theme_if_needed(&mut self) {
        let key = (self.theme_id(), self.accent_id(), self.cfg.tint, self.dark_mode());
        if self.palette_key != Some(key) {
            theme::apply(&self.ctx, &theme::palette(key.0, key.1, key.2, key.3));
            self.palette_key = Some(key);
        }
    }
}

fn save_png(path: &str, img: &egui::ColorImage) {
    if let Some(dir) = std::path::Path::new(path).parent() {
        let _ = std::fs::create_dir_all(dir);
    }
    let Ok(file) = std::fs::File::create(path) else { return };
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), img.width() as u32, img.height() as u32);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    if let Ok(mut w) = enc.write_header() {
        let bytes: Vec<u8> = img.pixels.iter().flat_map(|c| [c.r(), c.g(), c.b(), 255]).collect();
        let _ = w.write_image_data(&bytes);
    }
}

impl eframe::App for CatalyxApp {
    fn logic(&mut self, ctx: &Context, _frame: &mut eframe::Frame) {
        self.apply_theme_if_needed();
        self.drain_messages();
        self.learn_game_program();
        self.auto_boost_scan();

        let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        let phase = self.phase();
        // Follow Windows' light/dark choice.
        if self.cfg.appearance == "system" && self.last_sys_check.elapsed() > Duration::from_secs(3) {
            self.last_sys_check = Instant::now();
            let d = ffi::system_prefers_dark();
            if d != self.sys_dark {
                self.sys_dark = d;
            }
        }
        // Per-process scans only where they are shown (Apps) or needed (a boost).
        procs::set_detail(
            self.tab == Tab::Apps
                || phase.active()
                || self.opts.screenshot.is_some()
                || self.opts.tour.is_some()
                || self.opts.selftest.is_some(),
        );
        procs::set_sample_interval(if minimized && !phase.active() {
            8000
        } else if phase.active() || self.tab == Tab::Apps {
            1500
        } else {
            2500
        });
        // Live graphics card readings, only while they are on screen.
        if (self.tab == Tab::Tuning || self.tab == Tab::Boost) && !minimized && self.last_live.elapsed() > Duration::from_millis(1500) {
            self.last_live = Instant::now();
            self.live = gpu::live();
        }
        // Services change slowly: refresh every few seconds while the page is open.
        if self.tab == Tab::Services && self.last_svc.map(|t| t.elapsed() > Duration::from_secs(5)).unwrap_or(true) {
            self.refresh_services();
        }
        // Save a moment after the last change.
        if let Some(t) = self.dirty_since {
            if t.elapsed() > Duration::from_millis(500) {
                self.save_now();
            }
        }
        // Closing during a boost puts everything back first.
        if ctx.input(|i| i.viewport().close_requested()) && self.closing.is_none() {
            if phase.active() {
                ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
                self.closing = Some(Instant::now());
                let _ = self.tx.send(Cmd::Restore);
            } else {
                self.save_now();
            }
        }
        if let Some(t) = self.closing {
            if phase == Phase::Idle || t.elapsed() > Duration::from_secs(20) {
                self.save_now();
                ctx.send_viewport_cmd(egui::ViewportCommand::Close);
            }
        }
        // Dropped files become games.
        let dropped: Vec<String> =
            ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.as_ref().map(|p| p.to_string_lossy().to_string())).collect());
        for path in dropped {
            pages::games::add_game(self, &path, None);
        }
        // Footprint: hand back memory the window is not using right now (driver and font pages
        // that were only needed while drawing). They come back on demand, a few at a time.
        if self.last_trim.elapsed() > Duration::from_secs(30) && self.opts.selftest.is_none() {
            self.last_trim = Instant::now();
            procs::trim(unsafe { ffi::GetCurrentProcessId() });
        }
        // A minimised window is not asked to draw again, so the 30 s timer above never runs for it
        // - and minimised is exactly where it stays during a game. Trim once, just after the last
        // frame, from a short-lived thread (nothing wakes this window again until it is restored).
        if minimized && !self.trimmed_minimized && self.opts.selftest.is_none() {
            self.trimmed_minimized = true;
            std::thread::spawn(|| {
                std::thread::sleep(Duration::from_millis(1500));
                procs::trim(unsafe { ffi::GetCurrentProcessId() });
            });
        } else if !minimized {
            self.trimmed_minimized = false;
        }
        self.run_checks(ctx);
        if let Some(mut t) = self.selftest.take() {
            // The self-test keeps the window drawing so its stages advance.
            ctx.request_repaint_after(Duration::from_millis(40));
            if t.step_frame(self) {
                std::process::exit(0);
            }
            self.selftest = Some(t);
        }
        // Slow tick for the live meters: nothing while minimised, quick while a boost runs.
        if !minimized {
            ctx.request_repaint_after(if phase.active() || self.closing.is_some() {
                Duration::from_millis(600)
            } else {
                Duration::from_millis(2000)
            });
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        crate::trace::frame();
        self.reveal_when_ready(ui.ctx());
        self.frames = self.frames.max(1);
        self.sync_titlebar();
        let p = self.pal();
        let width = ui.available_width();
        let compact = width < 880.0;
        let nav_w = if compact { 64.0 } else { 212.0 };
        egui::Panel::top("titlebar")
            .exact_size(TITLEBAR_H)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.bg))
            .show(ui, |ui| self.title_bar(ui, compact, nav_w));
        egui::Panel::left("nav")
            .exact_size(nav_w)
            .resizable(false)
            .show_separator_line(false)
            .frame(egui::Frame::new().fill(p.sidebar).inner_margin(egui::Margin::symmetric(8, 0)).stroke(egui::Stroke::new(1.0, p.line)))
            .show(ui, |ui| self.sidebar(ui, compact));
        // The launch bar is pinned to the bottom of the Boost page, before the page itself so it takes its space.
        if self.tab == Tab::Boost && self.control_style() == control::Style::Bar {
            egui::Panel::bottom("boost-bar")
                .exact_size(control::BAR_H)
                .resizable(false)
                .show_separator_line(false)
                .frame(egui::Frame::new().fill(p.sidebar))
                .show(ui, |ui| control::launch_bar(self, ui));
        }
        egui::CentralPanel::default()
            .frame(egui::Frame::new().fill(p.bg).inner_margin(egui::Margin { left: 24, right: 24, top: 12, bottom: 16 }))
            .show(ui, |ui| self.body(ui));
        pages::dialogs(self, ui.ctx());
        self.toasts.show(ui.ctx());
        // Last, so the resize handles and the hairline are above everything.
        widgets::window_edges(ui.ctx());
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        self.save_now();
        if self.phase().active() {
            let _ = self.tx.send(Cmd::Restore);
            std::thread::sleep(Duration::from_millis(1500));
        }
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }
}

/// Opens the window.
pub fn run(opts: Options) -> Result<(), String> {
    let size = opts.size.unwrap_or((1180.0, 760.0));
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CatalyX")
            .with_inner_size(vec2(size.0, size.1))
            .with_min_inner_size(vec2(760.0, 560.0))
            .with_decorations(false)
            .with_visible(opts.screenshot.is_some() || opts.tour.is_some() || opts.selftest.is_some())
            .with_icon(app_icon())
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Glow,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        persist_window: false,
        ..Default::default()
    };
    crate::trace::mark("opening the window");
    eframe::run_native("CatalyX", native, Box::new(move |cc| Ok(Box::new(CatalyxApp::new(cc, opts))))).map_err(|e| e.to_string())
}

fn app_icon() -> egui::IconData {
    mark::icon_data()
}
