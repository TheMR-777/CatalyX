//! The boost control: the one place a boost is started and undone.
//!
//! It has three interchangeable looks (a launch bar along the bottom of the page, a gauge, a power
//! ring) chosen in Settings. They share ONE state (`Ctl`: which stage the boost is at and what a click
//! would do) and ONE set of actions (`Act`), so they behave identically and only differ in how they
//! draw. Colours come from the palette (the accent), never from fixed hues: the boosted state is the
//! person's accent, not green.

use super::icons::{self, Ico};
use super::pages::Confirm;
use super::theme::{alpha, mix, pal};
use super::widgets::*;
use super::CatalyxApp;
use crate::config::{CpuLevel, GpuLevel, PowerChoice, ProcAction};
use crate::engine::{Phase, Status};
use egui::{pos2, vec2, Align, Align2, FontId, Id, Layout, Pos2, Rect, Sense, Shape, Stroke, Ui};
use std::f32::consts::{PI, TAU};

/// Height of the launch bar.
pub const BAR_H: f32 = 72.0;
/// Width of the column the gauge and the ring live in (inside the hero card).
pub const DIAL_W: f32 = 250.0;

/// Which look the control has.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Style {
    Bar,
    Gauge,
    Ring,
}

impl Style {
    pub const ALL: [Style; 3] = [Style::Bar, Style::Gauge, Style::Ring];
    pub fn key(self) -> &'static str {
        match self {
            Style::Bar => "bar",
            Style::Gauge => "gauge",
            Style::Ring => "ring",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Style::Bar => "Launch bar",
            Style::Gauge => "Gauge",
            Style::Ring => "Power ring",
        }
    }
    pub fn from_key(s: &str) -> Style {
        Style::ALL.into_iter().find(|k| k.key().eq_ignore_ascii_case(s)).unwrap_or(Style::Bar)
    }
}

/// What the person can ask for. Every look produces exactly these.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Act {
    /// Boost, and start the selected game (or just gaming mode when there is none).
    Boost,
    /// Boost without starting a game.
    BoostOnly,
    /// Put everything back.
    Restore,
}

/// The state every look draws from.
pub struct Ctl {
    pub phase: Phase,
    /// The control waits: the boost is being applied or undone.
    pub busy: bool,
    /// A session is on (anything but idle).
    pub boosted: bool,
    /// Whether a game is selected (otherwise it is gaming mode without a game).
    pub has_game: bool,
    /// The game's name, or "Gaming mode".
    pub name: String,
    pub profile: String,
    /// The stages this profile will go through, and whether each is done.
    pub stages: Vec<(&'static str, bool)>,
    /// 0 (idle) to 1 (boosted): what the needle, the bar's line and the ring's fill show.
    pub progress: f32,
    /// A short headline for the dial's caption.
    pub title: String,
    /// The line under it (or under the game's name in the launch bar).
    pub sub: String,
}

/// How far along a boost is. Pure, so it can be tested: applying fills 8-80% as stages finish, waiting
/// for the game is 85%, a running boost is 100%, and restoring falls back to 40%.
pub fn progress_for(phase: Phase, done: usize, total: usize) -> f32 {
    match phase {
        Phase::Idle => 0.0,
        Phase::Boosting => 0.08 + 0.72 * done as f32 / total.max(1) as f32,
        Phase::Finding => 0.85,
        Phase::Running | Phase::Boosted => 1.0,
        Phase::Restoring => 0.4,
    }
}

pub fn state(app: &CatalyxApp, st: &Status) -> Ctl {
    let games = app.cfg.games.len();
    let sel = app.cfg.selected_game.min(games.saturating_sub(1));
    let game = app.cfg.games.get(sel);
    let s = app.cfg.settings_for(game);
    let rules = app.cfg.apps_for(game);
    let n_rules = rules.values().filter(|a| **a != ProcAction::Keep).count();
    let n_svc = app.cfg.services_for(game).values().filter(|v| **v).count();
    let k = &st.counts;
    let phase = st.phase;
    let applied = matches!(phase, Phase::Finding | Phase::Running | Phase::Boosted | Phase::Restoring);
    let finished = matches!(phase, Phase::Running | Phase::Boosted);

    let mut stages: Vec<(&'static str, bool)> = Vec::new();
    if s.power != PowerChoice::Keep || s.cpu != CpuLevel::Off {
        stages.push(("Power plan", applied || !k.power_plan.is_empty()));
    }
    if s.windows_game_mode || s.game_dvr_off || s.notifications_off || s.transparency_off || s.pause_maintenance || s.mmcss_games {
        stages.push(("Windows", applied || k.tweaks > 0));
    }
    if (s.apps && n_rules > 0) || (s.services && n_svc > 0) || s.lower_others {
        stages.push(("Apps and services", applied || k.apps_closed + k.apps_paused + k.apps_lowered + k.services_stopped > 0));
    }
    if s.clean_ram || s.purge_standby {
        stages.push(("Memory", applied || k.ram_freed > 0 || k.standby_purged));
    }
    if s.gpu != GpuLevel::Off {
        stages.push(("Graphics card", applied || k.gpu_core_mhz > 0));
    }
    if game.is_some() {
        stages.push(("Game", finished || st.game_pid != 0));
    }
    let done = stages.iter().filter(|s| s.1).count();
    let progress = progress_for(phase, done, stages.len());

    let name = game.map(|g| g.name.clone()).unwrap_or_else(|| "Gaming mode".into());
    let profile = match game {
        Some(g) if g.custom.is_some() => "Own settings".to_string(),
        Some(g) => app.cfg.preset(&g.preset).name.clone(),
        None => app.cfg.preset(&app.cfg.default_preset).name.clone(),
    };
    let pending = stages.iter().find(|s| !s.1).map(|s| s.0);
    let (title, sub) = match phase {
        Phase::Idle => (if game.is_some() { "Boost & play" } else { "Boost" }.to_string(), profile.clone()),
        Phase::Boosting => (pending.map(|n| format!("{n}...")).unwrap_or_else(|| "Applying...".into()), "boosting".to_string()),
        Phase::Finding => (format!("Starting {name}..."), "looking for the game".to_string()),
        Phase::Running | Phase::Boosted => ("Boosted".to_string(), "click to restore".to_string()),
        Phase::Restoring => ("Restoring...".to_string(), "putting everything back".to_string()),
    };
    Ctl {
        phase,
        busy: matches!(phase, Phase::Boosting | Phase::Restoring),
        boosted: phase != Phase::Idle,
        has_game: game.is_some(),
        name,
        profile,
        stages,
        progress,
        title,
        sub,
    }
}

/// What a click on the main part of the control does right now.
pub fn click_act(c: &Ctl) -> Option<Act> {
    if c.busy {
        None
    } else if c.boosted {
        Some(Act::Restore)
    } else {
        Some(Act::Boost)
    }
}

impl CatalyxApp {
    pub fn control_style(&self) -> Style {
        Style::from_key(&self.cfg.boost_control)
    }

    /// Carries out what the control asked for.
    pub(super) fn control_act(&mut self, a: Act) {
        let games = self.cfg.games.len();
        let sel = self.cfg.selected_game.min(games.saturating_sub(1));
        match a {
            Act::Restore => self.restore(),
            Act::Boost => self.request_boost(if games > 0 { Some(sel) } else { None }),
            Act::BoostOnly => self.request_boost(None),
        }
    }

    /// Starting a boost: first the summary of what will change (unless the person turned that off).
    pub(super) fn request_boost(&mut self, idx: Option<usize>) {
        if self.cfg.confirm_boost {
            self.boost_dont_ask = false;
            self.confirm = Some(Confirm::Boost { idx });
        } else {
            self.boost_game(idx);
        }
    }
}

// ------------------------------------------------------------------ painting helpers

fn smooth(t: f32) -> f32 {
    let t = t.clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn arc(c: Pos2, r: f32, a0: f32, a1: f32) -> Vec<Pos2> {
    let n = (((a1 - a0).abs() * r / 3.0).ceil() as usize).max(2);
    (0..=n)
        .map(|i| {
            let a = a0 + (a1 - a0) * i as f32 / n as f32;
            c + vec2(a.cos(), a.sin()) * r
        })
        .collect()
}

/// The icon in the middle of a dial: the bolt waiting, a check when boosted, breathing while working.
fn core_icon(ui: &Ui, centre: Pos2, size: f32, c: &Ctl, color: egui::Color32) {
    let icon = if c.boosted && !c.busy { Ico::Check } else { Ico::Bolt };
    let mut s = size;
    if c.busy {
        let t = ui.input(|i| i.time) as f32;
        s *= 1.0 + 0.07 * (t * 6.0).sin();
        ui.ctx().request_repaint();
    }
    icons::paint_at(ui.painter(), icon, centre, s, color);
}

// ------------------------------------------------------------------ the power ring

/// A thin ring with the bolt in the middle. One segment per stage of this profile; each fills as the
/// stage is done, and the whole ring glows in the accent once the boost is on.
pub fn ring(ui: &mut Ui, c: &Ctl) -> Option<Act> {
    let p = pal(ui.ctx());
    let size = 150.0;
    let k = size / 100.0;
    let (rect, resp) = ui.allocate_exact_size(vec2(size, size), Sense::click());
    let live = click_act(c).is_some();
    if resp.hovered() && live {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let hot = resp.hovered() && live;
    let centre = rect.center();
    let (r, w) = (46.0 * k, 5.0 * k);
    let glow = ui.ctx().animate_bool_with_time(resp.id.with("glow"), c.progress >= 1.0, 0.6);
    let painter = ui.painter();
    if glow > 0.0 {
        // A soft falloff outside the ring: wide, overlapping, ever fainter bands.
        for i in 1..=7 {
            let a = (22.0 - i as f32 * 3.0).max(2.0) * glow;
            painter.circle_stroke(centre, r + w / 2.0 + i as f32 * 2.6, Stroke::new(3.4, alpha(p.accent, a as u8)));
        }
    }
    painter.circle_stroke(centre, r, Stroke::new(w * 0.6, p.line));
    let n = c.stages.len().max(1);
    let gap = 0.16;
    for i in 0..n {
        let done = if c.stages.is_empty() { c.progress >= 0.85 } else { c.stages[i].1 };
        let t = ui.ctx().animate_bool_with_time(resp.id.with(("seg", i)), done, 0.4);
        let a0 = -PI / 2.0 + gap / 2.0 + TAU * i as f32 / n as f32;
        let a1 = a0 + TAU / n as f32 - gap;
        let col = alpha(p.accent, (46.0 + 209.0 * t) as u8);
        painter.add(Shape::line(arc(centre, r, a0, a1), Stroke::new(w, col)));
    }
    let core = r - w / 2.0 - 6.0 * k * 0.5;
    let fill = if hot || c.boosted { p.accent_soft } else { p.well };
    painter.circle_filled(centre, core, fill);
    painter.circle_stroke(centre, core, Stroke::new(1.0, if hot { mix(p.line_hi, p.accent, 0.5) } else { p.line_hi }));
    core_icon(ui, centre, 42.0, c, p.accent);
    if resp.clicked() {
        return click_act(c);
    }
    None
}

// ------------------------------------------------------------------ the gauge

/// A semicircular dial. The needle sweeps from NORMAL to BOOSTED as the stages finish, the arc fills in
/// the accent, and the cap in the middle is the button.
pub fn gauge(ui: &mut Ui, c: &Ctl) -> Option<Act> {
    let p = pal(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(DIAL_W, 150.0), Sense::click());
    let live = click_act(c).is_some();
    if resp.hovered() && live {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    let hot = resp.hovered() && live;
    let centre = pos2(rect.center().x, rect.top() + 125.0);
    let radius = 100.0;
    let v = ui.ctx().animate_value_with_time(resp.id.with("needle"), c.progress, 0.9);
    let shown = smooth(v);
    let painter = ui.painter();

    // The track, then the filled part with a gradient along the arc.
    painter.add(Shape::line(arc(centre, radius, PI, TAU), Stroke::new(8.0, p.line_hi)));
    for end in [PI, TAU] {
        painter.circle_filled(centre + vec2(end.cos(), end.sin()) * radius, 4.0, p.line_hi);
    }
    let steps = 72;
    let filled = (shown * steps as f32).round() as usize;
    let colour_at = |t: f32| if t < 0.55 { mix(p.boost_bottom, p.accent, t / 0.55) } else { mix(p.accent, p.boost_top, (t - 0.55) / 0.45) };
    for i in 0..filled {
        let (t0, t1) = (i as f32 / steps as f32, (i + 1) as f32 / steps as f32);
        let (a0, a1) = (PI + t0 * PI, PI + t1 * PI + 0.012);
        painter.add(Shape::line(arc(centre, radius, a0, a1), Stroke::new(8.0, colour_at(t0))));
    }
    if filled > 0 {
        for t in [0.0, filled as f32 / steps as f32] {
            let a = PI + t * PI;
            painter.circle_filled(centre + vec2(a.cos(), a.sin()) * radius, 4.0, colour_at(t));
        }
    }
    // Ticks.
    for i in 0..=20 {
        let a = PI + i as f32 * PI / 20.0;
        let big = i % 5 == 0;
        let d = vec2(a.cos(), a.sin());
        painter.line_segment(
            [centre + d * if big { 82.0 } else { 86.0 }, centre + d * 93.0],
            Stroke::new(if big { 1.8 } else { 1.0 }, p.faint),
        );
    }
    // Needle and cap.
    let a = PI + shown * PI;
    let d = vec2(a.cos(), a.sin());
    painter.line_segment([centre, centre + d * 70.0], Stroke::new(3.0, p.text));
    painter.circle_filled(centre + d * 70.0, 1.5, p.text);
    let cap = 26.0;
    painter.circle_filled(centre, cap, if hot || c.boosted { p.accent_soft } else { p.well });
    painter.circle_stroke(centre, cap, Stroke::new(2.0, if hot { mix(p.line_hi, p.accent, 0.5) } else { p.line_hi }));
    core_icon(ui, centre, 24.0, c, p.accent);
    // The two ends.
    let painter = ui.painter();
    painter.text(centre + vec2(-radius, 14.0), Align2::CENTER_TOP, "NORMAL", FontId::proportional(11.0), p.faint);
    painter.text(
        centre + vec2(radius, 14.0),
        Align2::CENTER_TOP,
        "BOOSTED",
        FontId::proportional(11.0),
        if shown > 0.95 { p.accent } else { p.faint },
    );
    if resp.clicked() {
        return click_act(c);
    }
    None
}

/// The headline and the line under it, for the gauge and the ring.
pub fn caption(ui: &mut Ui, c: &Ctl) {
    ui.vertical_centered(|ui| {
        ui.spacing_mut().item_spacing.y = 1.0;
        ui.label(bold(c.title.clone(), 15.0));
        ui.label(faint(ui, c.sub.clone(), 12.0));
    });
}

/// "Boost only" under a dial, while idle. Returns the action when it is clicked.
pub fn boost_only_link(ui: &mut Ui, c: &Ctl) -> Option<Act> {
    if c.boosted {
        return None;
    }
    let mut act = None;
    ui.vertical_centered(|ui| {
        if ui
            .add(egui::Button::new(faint(ui, "Boost only", 12.0)).frame(false))
            .on_hover_text("Tune the PC now without starting a game. Press Restore when done.")
            .clicked()
        {
            act = Some(Act::BoostOnly);
        }
    });
    act
}

// ------------------------------------------------------------------ the launch bar

/// The bar along the bottom of the Boost page: the game, a line of status, and the action. While a boost
/// runs it shows the stages as they finish and a progress line along its top edge.
pub fn launch_bar(app: &mut CatalyxApp, ui: &mut Ui) {
    let p = app.pal();
    let status = app.status.lock().map(|s| s.clone()).unwrap_or_default();
    let c = state(app, &status);
    app.drawn.insert("boost_control");
    let full = ui.max_rect();
    ui.painter().line_segment([full.left_top(), full.right_top()], Stroke::new(1.0, p.line));
    let prog = ui.ctx().animate_value_with_time(Id::new("launch-bar-progress"), c.progress, 0.6);
    if prog > 0.001 {
        ui.painter().rect_filled(Rect::from_min_size(full.left_top(), vec2(full.width() * prog, 2.0)), 0, p.accent);
    }
    let mut act: Option<Act> = None;
    let wide = full.width() >= 900.0;
    ui.allocate_ui_with_layout(vec2(full.width(), full.height()), Layout::left_to_right(Align::Center), |ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.add_space(24.0);
        letter_tile(ui, &c.name, 40.0);
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 1.0;
            ui.add_space(15.0);
            ui.add(egui::Label::new(bold(c.name.clone(), 15.0)).truncate());
            ui.add(egui::Label::new(faint(ui, bar_line(&c), 12.0)).truncate());
        });
        if c.boosted && wide {
            ui.add_space(8.0);
            for (name, done) in &c.stages {
                let (fill, text, line) = if *done { (p.accent_soft, p.accent, alpha(p.accent, 70)) } else { (p.panel, p.faint, p.line) };
                let galley = ui.painter().layout_no_wrap(name.to_string(), FontId::proportional(12.0), text);
                let (rect, _) = ui.allocate_exact_size(vec2(galley.size().x + 20.0, 24.0), Sense::hover());
                ui.painter().rect_filled(rect, 12, fill);
                ui.painter().rect_stroke(rect, 12, Stroke::new(1.0, line), egui::StrokeKind::Inside);
                ui.painter().galley(rect.center() - galley.size() / 2.0, galley, text);
            }
        }
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.add_space(24.0);
            ui.spacing_mut().item_spacing.x = 12.0;
            if c.boosted {
                if ui.add_enabled(!c.busy, egui_components::Button::secondary("Restore").large()).clicked() {
                    act = Some(Act::Restore);
                }
            } else {
                let label = if c.has_game { "Boost & play" } else { "Boost" };
                if ui.add(egui_components::Button::primary(label).large()).clicked() {
                    act = Some(Act::Boost);
                }
                if ui
                    .add(egui::Button::new(faint(ui, "Boost only", 12.0)).frame(false))
                    .on_hover_text("Tune the PC now without starting a game. Press Restore when done.")
                    .clicked()
                {
                    act = Some(Act::BoostOnly);
                }
            }
        });
    });
    if let Some(a) = act {
        app.control_act(a);
    }
}

/// The line under the game's name in the launch bar.
fn bar_line(c: &Ctl) -> String {
    match c.phase {
        Phase::Idle => format!("{} \u{00b7} ready", c.profile),
        Phase::Boosting | Phase::Finding | Phase::Restoring => c.title.clone(),
        Phase::Running | Phase::Boosted => format!("{} \u{00b7} boosted", c.profile),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_looks_round_trip_through_their_saved_names() {
        for s in Style::ALL {
            assert_eq!(Style::from_key(s.key()), s);
        }
        assert_eq!(Style::from_key("nonsense"), Style::Bar, "an unknown name falls back to the launch bar");
        assert_eq!(Style::from_key("GAUGE"), Style::Gauge);
    }

    #[test]
    fn progress_rises_while_applying_and_ends_full() {
        assert_eq!(progress_for(Phase::Idle, 0, 5), 0.0);
        let mut last = 0.0;
        for done in 0..=5 {
            let p = progress_for(Phase::Boosting, done, 5);
            assert!(p > last || done == 0, "progress must not go backwards");
            assert!(p <= 0.8 + 1e-6, "applying never looks finished");
            last = p;
        }
        assert!(progress_for(Phase::Finding, 5, 5) > progress_for(Phase::Boosting, 5, 5));
        assert_eq!(progress_for(Phase::Running, 5, 5), 1.0);
        assert_eq!(progress_for(Phase::Boosted, 0, 0), 1.0);
        assert!(progress_for(Phase::Restoring, 5, 5) < 1.0);
        // A profile with no stages at all must not divide by zero.
        assert!(progress_for(Phase::Boosting, 0, 0).is_finite());
    }

    #[test]
    fn a_click_does_the_same_thing_in_every_look() {
        let mk = |phase: Phase| Ctl {
            phase,
            busy: matches!(phase, Phase::Boosting | Phase::Restoring),
            boosted: phase != Phase::Idle,
            has_game: true,
            name: String::new(),
            profile: String::new(),
            stages: Vec::new(),
            progress: 0.0,
            title: String::new(),
            sub: String::new(),
        };
        assert_eq!(click_act(&mk(Phase::Idle)), Some(Act::Boost));
        assert_eq!(click_act(&mk(Phase::Boosting)), None, "no clicking while it is applying");
        assert_eq!(click_act(&mk(Phase::Finding)), Some(Act::Restore), "waiting for the game can be cancelled");
        assert_eq!(click_act(&mk(Phase::Boosted)), Some(Act::Restore));
        assert_eq!(click_act(&mk(Phase::Restoring)), None);
    }
}
