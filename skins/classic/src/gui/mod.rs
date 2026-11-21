//! The window. Built on egui (eframe, OpenGL) and the egui-components library.
//!
//! The window is a skin over the core (`catalyx_core::app`): it draws from `self.v` (the core's View, taken at
//! the start of each frame and after each change), changes things only through `send`, and shows the
//! core's events (toasts, the boost summary, closing).
//!
//! Footprint rules: the window draws only when something changed. The core wakes it for background
//! results and fresh readings, and never while it is minimised; per-process scans only run while the
//! Apps page is open or a boost is active; every blocking job (file dialogs, hardware probes, library
//! scans) runs on its own short-lived thread.

mod checks;
mod chrome;
mod control;
mod icons;
pub mod mark;
mod pages;
pub mod prefs;
mod theme;
mod widgets;

use catalyx_core::app::{self, CheckPage, Core, CoreOptions, Drawn, Event, Hint, Intent, Interest, Origin, Phase, View};
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

/// Which preset or game-with-own-settings a page edits. The core's target type, so this page state
/// and `Intent::SetSetting` cannot drift apart.
pub type EditTarget = catalyx_core::app::Target;

#[derive(Clone, Copy)]
pub enum PickFor {
    GameFile,
    ExportConfig,
    ImportConfig,
}

pub struct Options {
    /// The core's own flags (screenshot, tour, self-test, demo, size), parsed once in `app::Options`.
    pub run: app::Options,
    pub tab: Option<Tab>,
    pub theme: Option<String>,
    pub appearance: Option<String>,
    pub accent: Option<String>,
    /// "bar", "gauge" or "ring": the boost control's look, for screenshots.
    pub control: Option<String>,
    /// "boost": open the boost summary dialog (for screenshots).
    pub dialog: Option<String>,
    pub size: Option<(u32, u32)>,
}

pub struct CatalyxApp {
    pub core: Core,
    /// The core's state as of the start of this frame (or the last change made in it).
    pub v: View,
    pub ctx: Context,
    pub tab: Tab,
    pub toasts: Toasts,
    /// File dialogs answer here (they run on their own short-lived thread).
    picked_tx: Sender<(PickFor, String)>,
    picked_rx: Receiver<(PickFor, String)>,
    sys_dark: bool,
    last_sys_check: Instant,
    palette_key: Option<(ThemeId, AccentId, u8, bool)>,
    titlebar_for: Option<(bool, u32)>,
    // page state
    pub ui: pages::PageState,
    pub confirm: Option<pages::Confirm>,
    /// The "don't ask again" box in the boost summary (kept here while that dialog is open).
    pub boost_dont_ask: bool,
    /// The window starts hidden and is shown after its first frame is drawn (no empty frame on screen).
    /// 0 = nothing drawn yet, 1 = first frame drawn, 2 = shown.
    pub reveal_step: u8,
    /// What the core was last told this window shows, and whether it is minimised.
    last_interest: Interest,
    last_minimised: bool,
    /// The core said it is safe to close (`Event::ReadyToClose`/`ReadyToQuit`). Until then a close
    /// request is held back while a boost is put back.
    may_close: bool,
    // checks
    pub drawn: HashSet<&'static str>,
    opts: Options,
    /// When the window started, for check runs that must wait for the layout to settle.
    started: Instant,
    frames: u32,
    shot_requested: bool,
    tour_step: usize,
}

impl CatalyxApp {
    pub fn new(cc: &eframe::CreationContext<'_>, opts: Options) -> Self {
        catalyx_core::trace::mark("window and GL context ready");
        theme::install_fonts(&cc.egui_ctx);
        catalyx_core::trace::mark("fonts installed");
        let origin = opts.run.origin();
        let no_save = origin != Origin::User;
        let check_run = origin.check();
        let dialog_boost = opts.dialog.as_deref() == Some("boost");
        // The look flags of check runs are this skin's own settings, applied before the first frame
        // and never saved for the run: they go through the same keys the Settings page writes.
        let mut start_prefs: Vec<(String, String)> = Vec::new();
        if let Some(t) = &opts.theme {
            start_prefs.push((prefs::THEME.into(), ThemeId::from_key(t).key().into()));
        }
        if let Some(a) = &opts.appearance {
            start_prefs.push((prefs::APPEARANCE.into(), prefs::Appearance::from_key(a).key().into()));
        }
        if let Some(c) = &opts.control {
            start_prefs.push((prefs::CONTROL.into(), control::Style::from_key(c).key().into()));
        }
        if let Some(a) = &opts.accent {
            start_prefs.push((prefs::ACCENT.into(), AccentId::from_key(a).key().into()));
        }
        let core = Core::start(CoreOptions { origin, skin: prefs::NAME, prefs: start_prefs });
        let wake_ctx = cc.egui_ctx.clone();
        core.connect(move || wake_ctx.request_repaint());
        let v = core.view();
        let (picked_tx, picked_rx) = channel();
        let mut app = CatalyxApp {
            core,
            v,
            ctx: cc.egui_ctx.clone(),
            tab: opts.tab.unwrap_or(Tab::Boost),
            toasts: Toasts::new(),
            picked_tx,
            picked_rx,
            sys_dark: app::window::system_prefers_dark(),
            last_sys_check: Instant::now(),
            palette_key: None,
            titlebar_for: None,
            ui: pages::PageState::default(),
            confirm: None,
            boost_dont_ask: false,
            reveal_step: if check_run { 2 } else { 0 },
            last_interest: Interest::default(),
            last_minimised: false,
            may_close: false,
            drawn: HashSet::new(),
            opts,
            started: Instant::now(),
            frames: 0,
            shot_requested: false,
            tour_step: 0,
        };
        catalyx_core::trace::mark("app state built");
        if app.reveal_step == 2 {
            // A check run draws at once and needs the data now; a normal start probes after the window is shown.
            app.hint(Hint::Shown);
        }
        if !app.v.config.onboarded && !no_save {
            app.confirm = Some(pages::Confirm::Welcome);
        }
        if dialog_boost {
            // A real game, by id: the summary has to describe something. It goes through the core's
            // own policy, exactly as a person's click does.
            let first = app.v.config.games.first().map(|g| g.id.clone());
            app.send(Intent::Boost { game: first });
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
                catalyx_core::trace::mark("window shown");
                self.hint(Hint::Shown);
            }
            _ => {}
        }
    }

    /// Asks the core for a change, then takes its new state at once, so the rest of the frame sees it.
    pub fn send(&mut self, intent: Intent) {
        self.core.send(intent);
        self.v = self.core.view();
        // Toasts come out in the order the actions happened, as when the window made them itself.
        self.drain_events();
    }

    /// Tells the core something about the window itself (what it shows, that it is minimised, that it
    /// was asked to close), then takes whatever that made due.
    pub fn hint(&mut self, hint: Hint) {
        self.core.hint(hint);
        self.v = self.core.view();
        self.drain_events();
    }

    /// Shows what the core asked for.
    fn drain_events(&mut self) {
        for e in self.core.take_events() {
            match e {
                Event::Toast { tone, text } => {
                    // One tone vocabulary for toasts and the log: OK is good news, WARN and ERROR
                    // need attention, no tone bit is the plain message.
                    if tone.has(app::Tags::OK) {
                        self.toast_ok(text);
                    } else if tone.has(app::Tags::WARN) || tone.has(app::Tags::ERROR) {
                        self.toast_err(text);
                    } else {
                        self.toast_info(text);
                    }
                }
                Event::ConfirmBoost { game, plan } => {
                    self.boost_dont_ask = false;
                    self.confirm = Some(pages::Confirm::Boost { game, plan });
                }
                Event::ReadyToClose => {
                    self.may_close = true;
                    self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
                Event::ReadyToQuit => {
                    self.may_close = true;
                    self.ctx.send_viewport_cmd(egui::ViewportCommand::Close);
                }
            }
        }
    }

    /// What this frame shows, as data the core can act on.
    fn interest(&self) -> Interest {
        Interest {
            processes: self.tab == Tab::Apps,
            gpu_live: self.tab == Tab::Tuning || self.tab == Tab::Boost,
            services: self.tab == Tab::Services,
            elevation: self.tab == Tab::Settings,
            own_usage: self.tab == Tab::Settings,
        }
    }

    pub fn dark_mode(&self) -> bool {
        match self.look().appearance {
            prefs::Appearance::Light => false,
            prefs::Appearance::System => self.sys_dark,
            prefs::Appearance::Dark => true,
        }
    }

    /// The look this skin is wearing, read from its own saved preferences (`skins.classic`).
    pub fn look(&self) -> prefs::Prefs {
        prefs::Prefs::from_config(&self.v.config)
    }

    pub fn pal(&self) -> theme::Palette {
        theme::pal(&self.ctx)
    }

    pub fn phase(&self) -> Phase {
        self.v.phase()
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
        let dialog = app::window::FileDialog { title: title.to_string(), filter: filter.to_string(), ext: ext.to_string(), save };
        let (answer, ctx) = (self.picked_tx.clone(), self.ctx.clone());
        app::window::pick_file(dialog, move |path| {
            if let Some(p) = path {
                let _ = answer.send((what, p));
                ctx.request_repaint();
            }
        });
    }

    /// Rounded corners and the drop shadow for the borderless window, once the window exists.
    fn sync_titlebar(&mut self) {
        if self.titlebar_for.is_none() && app::window::style_borderless_window() {
            self.titlebar_for = Some((true, 0));
        }
    }

    /// A check run (screenshot, tour or self-test) photographs a fixed frame.
    fn checking(&self) -> bool {
        self.opts.run.check()
    }

    fn apply_theme_if_needed(&mut self) {
        let look = self.look();
        let key = (look.theme, look.accent, look.tint, self.dark_mode());
        if self.palette_key != Some(key) {
            theme::apply(&self.ctx, &theme::palette(key.0, key.1, key.2, key.3));
            self.palette_key = Some(key);
            // A check run captures a fixed frame, so nothing may still be moving when it does: a
            // fading scroll bar made the same build photograph differently from one run to the
            // next, and a golden image that changes on its own is not a golden image.
            if self.checking() {
                self.ctx.all_styles_mut(|s| s.animation_time = 0.0);
            }
        }
    }
}

/// Whether this frame's close request is held back while the core saves or puts a boost back.
/// Once the core is ready, the close must go through: eframe cancels any close in a frame that also
/// sends `CancelClose`, so cancelling after `ReadyToClose` would ask itself to close forever.
fn cancel_close(requested: bool, may_close: bool) -> bool {
    requested && !may_close
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
        self.v = self.core.view();
        self.apply_theme_if_needed();
        while let Ok((what, path)) = self.picked_rx.try_recv() {
            pages::picked(self, what, path);
        }

        let minimized = ctx.input(|i| i.viewport().minimized).unwrap_or(false);
        // Tell the core what is on screen and when the window stops drawing: it samples only what is
        // shown, and keeps working while a minimised window gets no frames.
        let interest = self.interest();
        if interest != self.last_interest {
            self.last_interest = interest;
            self.core.hint(Hint::Interest(interest));
        }
        if minimized != self.last_minimised {
            self.last_minimised = minimized;
            self.core.hint(Hint::Minimised(minimized));
        }
        // Follow Windows' light/dark choice. Only this mode needs asking again, so only it asks for
        // a frame: everything else is woken by the core when there is something new.
        if self.look().appearance == prefs::Appearance::System {
            let now = Instant::now();
            if now.duration_since(self.last_sys_check) > Duration::from_secs(3) {
                self.last_sys_check = now;
                self.sys_dark = app::window::system_prefers_dark();
            }
            ctx.request_repaint_after((self.last_sys_check + Duration::from_secs(3)).saturating_duration_since(now));
        }
        self.v = self.core.view();
        // The core answers a close request: the window stays up while a boost is put back, then
        // Event::ReadyToClose lets it through. eframe cancels a close whenever CancelClose appears
        // in the same frame's commands, so never cancel after the core is ready: that is how the
        // window used to ask itself to close forever.
        let close_requested = ctx.input(|i| i.viewport().close_requested());
        if cancel_close(close_requested, self.may_close) {
            self.hint(Hint::CloseRequested);
        }
        if cancel_close(close_requested, self.may_close) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }
        self.drain_events();
        // Dropped files become games.
        let dropped: Vec<String> =
            ctx.input(|i| i.raw.dropped_files.iter().filter_map(|f| f.path.as_ref().map(|p| p.to_string_lossy().to_string())).collect());
        for path in dropped {
            self.send(Intent::AddGame { path, name: None, args: String::new(), real_exe: None });
        }
        self.run_checks(ctx);
        if self.opts.run.selftest() {
            // The self-test keeps the window drawing while its stages advance; the steps, the stand-in
            // game and the report live in the core. This skin only shows the page it asks for and says
            // which pages it has drawn.
            ctx.request_repaint_after(Duration::from_millis(40));
            if let Some(page) = self.core.selftest_page() {
                self.tab = match page {
                    CheckPage::Apps => Tab::Apps,
                    CheckPage::Games => Tab::Games,
                    CheckPage::Boost => Tab::Boost,
                };
            }
            let d = &self.drawn;
            let drawn = Drawn {
                apps: d.contains("apps"),
                games: d.contains("games"),
                boost: d.contains("stats") && d.contains("session") && d.contains("boost_control"),
            };
            if self.core.selftest_step(drawn) {
                std::process::exit(0);
            }
        }
    }

    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        catalyx_core::trace::frame();
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
        self.core.exit();
    }

    fn clear_color(&self, visuals: &egui::Visuals) -> [f32; 4] {
        visuals.panel_fill.to_normalized_gamma_f32()
    }
}

/// Opens the window.
pub fn run(opts: Options) -> Result<(), String> {
    let size = opts.size.map(|(w, h)| (w as f32, h as f32)).unwrap_or((1180.0, 760.0));
    let native = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_title("CatalyX")
            .with_inner_size(vec2(size.0, size.1))
            .with_min_inner_size(vec2(760.0, 560.0))
            .with_decorations(false)
            .with_visible(opts.run.check())
            .with_icon(app_icon())
            .with_drag_and_drop(true),
        renderer: eframe::Renderer::Glow,
        multisampling: 0,
        depth_buffer: 0,
        stencil_buffer: 0,
        persist_window: false,
        ..Default::default()
    };
    catalyx_core::trace::mark("opening the window");
    eframe::run_native("CatalyX", native, Box::new(move |cc| Ok(Box::new(CatalyxApp::new(cc, opts))))).map_err(|e| e.to_string())
}

fn app_icon() -> egui::IconData {
    mark::icon_data()
}

#[cfg(test)]
mod tests {
    use super::cancel_close;

    #[test]
    fn a_close_is_only_held_back_until_the_core_is_ready() {
        assert!(!cancel_close(false, false), "nothing to answer");
        assert!(!cancel_close(false, true), "already allowed: let it through");
        assert!(cancel_close(true, false), "a boost is being put back");
        assert!(!cancel_close(true, true), "the core said safe: never cancel now");
    }
}
