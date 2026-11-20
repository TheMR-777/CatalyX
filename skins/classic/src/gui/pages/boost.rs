//! Boost: pick a game and a profile, press the button. Calm by default: only things that need
//! attention are shown, and the right-hand card is the plan while idle, the live session while
//! boosting.

use crate::gui::control;
use crate::gui::icons::{self, Ico};
use crate::gui::widgets::*;
use crate::gui::{CatalyxApp, EditTarget, Tab};
use catalyx_core::app::{power_summary, Intent, NoticeKind};
use catalyx_core::engine::{Phase, Status, Tags};
use catalyx_core::procs;
use egui::{vec2, Align, Layout, RichText, ScrollArea, Ui};

const SIDE_W: f32 = 320.0;
const GAP: f32 = 16.0;

pub fn show(app: &mut CatalyxApp, ui: &mut Ui) {
    let status = app.v.status.clone();
    page(ui, "boost", 1120.0, |ui| {
        let total = ui.available_width();
        let wide = total >= 820.0;
        let session = status.phase.active() || !status.log.is_empty();
        if wide {
            let left_w = total - SIDE_W - GAP;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = GAP;
                ui.vertical(|ui| {
                    ui.set_width(left_w);
                    ui.spacing_mut().item_spacing.y = GAP;
                    hero(app, ui, &status);
                    notices(app, ui);
                });
                ui.vertical(|ui| {
                    ui.set_width(SIDE_W);
                    if session {
                        session_card(app, ui, &status);
                    } else {
                        plan_card(app, ui);
                    }
                });
            });
        } else {
            hero(app, ui, &status);
            notices(app, ui);
            if session {
                session_card(app, ui, &status);
            } else {
                plan_card(app, ui);
            }
        }
    });
}

fn hero(app: &mut CatalyxApp, ui: &mut Ui, status: &Status) {
    let games = app.v.config.games.len();
    let sel = app.v.config.selected_index().unwrap_or(0);
    let game = app.v.config.games.get(sel).cloned();
    let settings = app.v.config.settings_for(game.as_ref());
    // The gauge and the ring sit in this card; the launch bar is drawn along the bottom of the page instead.
    let style = app.control_style();
    let ctl = control::state(app, status);
    let dial = style != control::Style::Bar;
    let mut act: Option<control::Act> = None;
    card(ui, |ui| {
        let inner = ui.available_width();
        let left_w = if dial { inner - control::DIAL_W - 24.0 } else { inner };
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 24.0;
            // ---- left: who, which game, which profile
            ui.allocate_ui_with_layout(vec2(left_w, 196.0), Layout::top_down(Align::Min), |ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 14.0;
                    letter_tile(ui, game.as_ref().map(|g| g.name.as_str()).unwrap_or("Gaming mode"), 56.0);
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 2.0;
                        ui.add_space(6.0);
                        ui.add(
                            egui::Label::new(bold(game.as_ref().map(|g| g.name.clone()).unwrap_or_else(|| "Gaming mode".into()), 22.0))
                                .truncate(),
                        );
                        let sub = match &game {
                            Some(g) if !g.real_exe.is_empty() => format!("Plays as {}", g.real_exe),
                            Some(_) => "The game program is found on first launch".to_string(),
                            None => "Tune your PC without starting a game".to_string(),
                        };
                        ui.add(egui::Label::new(faint(ui, sub, 12.0)).truncate());
                    });
                });
                ui.add_space(20.0);
                let half = (left_w - 12.0) / 2.0;
                ui.horizontal_top(|ui| {
                    ui.spacing_mut().item_spacing.x = 12.0;
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.label(faint(ui, "Game", 12.0));
                        if games == 0 {
                            if ui.add(egui_components::Button::secondary("Add a game").min_width(half)).clicked() {
                                app.tab = Tab::Games;
                            }
                        } else {
                            let names: Vec<String> = app.v.config.games.iter().map(|g| g.name.clone()).collect();
                            let ids: Vec<catalyx_core::app::GameId> = app.v.config.games.iter().map(|g| g.id.clone()).collect();
                            let mut i = sel;
                            if dropdown(ui, "boost-game", half, &mut i, &names) {
                                app.send(Intent::SelectGame { id: ids[i].clone() });
                            }
                        }
                    });
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.label(faint(ui, "Profile", 12.0));
                        let custom = game.as_ref().map(|g| g.custom.is_some()).unwrap_or(false);
                        let mut names: Vec<String> = app.v.config.presets.iter().map(|p| p.name.clone()).collect();
                        if custom {
                            names.push("Own settings".into());
                        }
                        let current = game.as_ref().map(|g| g.preset.clone()).unwrap_or_else(|| app.v.config.default_preset.clone());
                        let mut i = if custom {
                            app.v.config.presets.len()
                        } else {
                            app.v.config.presets.iter().position(|p| p.id == current).unwrap_or(0)
                        };
                        if dropdown(ui, "boost-profile", half, &mut i, &names) {
                            if i < app.v.config.presets.len() {
                                let preset = app.v.config.presets[i].id.clone();
                                match game.as_ref() {
                                    Some(g) => app.send(Intent::SetGameProfile { id: g.id.clone(), preset }),
                                    // No game: the choice is the default for new games and Boost only.
                                    None => app.send(Intent::SetDefaultPreset { id: preset }),
                                }
                            } else if let Some(g) = game.as_ref() {
                                app.ui.prof.target = EditTarget::Game(g.id.clone());
                                app.tab = Tab::Profiles;
                            }
                        }
                    });
                });
                // One quiet summary line instead of a wall of tiles.
                ui.add_space(12.0);
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 4.0;
                    ui.add(
                        egui::Label::new(faint(
                            ui,
                            format!("{} priority \u{00b7} {}", settings.priority.label(), power_summary(&settings)),
                            12.0,
                        ))
                        .truncate(),
                    );
                });
            });
            // ---- right: the boost control (gauge or ring)
            if dial {
                ui.vertical(|ui| {
                    ui.set_width(control::DIAL_W);
                    ui.spacing_mut().item_spacing.y = 8.0;
                    app.drawn.insert("boost_control");
                    act = if style == control::Style::Gauge {
                        control::gauge(ui, &ctl)
                    } else {
                        ui.vertical_centered(|ui| control::ring(ui, &ctl)).inner
                    };
                    control::caption(ui, &ctl);
                    act = act.or(control::boost_only_link(ui, &ctl));
                });
            }
        });
    });
    if let Some(a) = act {
        app.control_act(a);
    }
}
/// Only what needs attention. Nothing at all when everything is in order. The core decides what is
/// worth saying; this skin chooses the tone, the action label and what the action does.
fn notices(app: &mut CatalyxApp, ui: &mut Ui) {
    // A cheap Arc clone, not a copy of the notices: the core made them when its inputs changed.
    let boost = app.v.boost.clone();
    if boost.notices.is_empty() {
        return;
    }
    let sel = app.v.config.selected().map(|g| g.id.clone());
    #[derive(Clone, Copy)]
    enum Do {
        Restore,
        Admin,
        Suggest,
        AddGame,
    }
    let mut act: Option<Do> = None;
    card(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let n = boost.notices.len();
        for (i, notice) in boost.notices.iter().enumerate() {
            let (tone, action, what) = match notice.kind {
                NoticeKind::Interrupted => (Tone::Warn, "Restore now", Do::Restore),
                NoticeKind::NotAdmin => (Tone::Warn, "Restart as admin", Do::Admin),
                NoticeKind::NothingChosen => (Tone::Accent, "Use suggestions", Do::Suggest),
                NoticeKind::NoGame => (Tone::Accent, "Add a game", Do::AddGame),
            };
            if notice_row(ui, tone, notice.text, Some(action)) {
                act = Some(what);
            }
            if i + 1 < n {
                hairline(ui);
            }
        }
    });
    match act {
        Some(Do::Restore) => app.send(Intent::Restore),
        Some(Do::Admin) => super::settings::relaunch_admin(app),
        Some(Do::Suggest) => {
            // The notice describes the selected game's plan, so the suggestions fill that scope.
            app.send(Intent::UseSuggestions { scope: sel });
            app.toast_ok("Suggestions selected: review them in Apps and Services");
        }
        Some(Do::AddGame) => app.tab = Tab::Games,
        None => {}
    }
}

/// What this boost will do: only the things that are switched on.
fn plan_card(app: &mut CatalyxApp, ui: &mut Ui) {
    let p = app.pal();
    let game = app.v.config.selected().cloned();
    // A cheap Arc clone: the plan itself was made when its inputs last changed, not now.
    let boost = app.v.boost.clone();
    let mut edit = false;
    card_titled(
        ui,
        "This boost will",
        |ui| {
            if link_button(ui, "Edit").clicked() {
                edit = true;
            }
        },
        |ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            for text in &boost.plan.lines {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (rect, _) = ui.allocate_exact_size(vec2(14.0, 22.0), egui::Sense::hover());
                    icons::paint_at(ui.painter(), Ico::Check, rect.center(), 13.0, p.ok);
                    ui.add(egui::Label::new(RichText::new(text).size(14.0)).truncate());
                });
            }
            if boost.plan.off > 0 {
                ui.add_space(6.0);
                ui.label(faint(ui, format!("{} more option(s) are off", boost.plan.off), 12.0));
            }
        },
    );
    if edit {
        let preset = game.as_ref().map(|g| g.preset.clone()).unwrap_or_else(|| app.v.config.default_preset.clone());
        app.ui.prof.target = match game.as_ref() {
            Some(g) if g.custom.is_some() => EditTarget::Game(g.id.clone()),
            _ => EditTarget::Preset(preset),
        };
        app.tab = Tab::Profiles;
    }
}

fn session_card(app: &mut CatalyxApp, ui: &mut Ui, st: &Status) {
    let p = app.pal();
    let phase = st.phase;
    let live = app.v.hardware.live.clone();
    app.drawn.insert("session");
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(bold("This session", 16.0));
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                chip(ui, phase.label(), if phase.active() { Tone::Accent } else { Tone::Neutral });
            });
        });
        ui.add_space(4.0);
        if !st.game_exe.is_empty() {
            ui.add(
                egui::Label::new(
                    RichText::new(if st.game_pid != 0 { format!("{} (pid {})", st.game_exe, st.game_pid) } else { st.game_exe.clone() })
                        .size(14.0)
                        .color(p.accent),
                )
                .truncate(),
            );
        } else if phase == Phase::Finding && !st.game_name.is_empty() {
            ui.add(
                egui::Label::new(RichText::new(format!("Waiting for {} to start...", st.game_name)).size(14.0).color(p.warn)).truncate(),
            );
        }
        let k = &st.counts;
        ui.add_space(8.0);
        // The proof: before and after, measured.
        if k.mhz_before > 0 || k.free_before > 0 {
            let half = (ui.available_width() - 8.0) / 2.0;
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 8.0;
                stat_tile(
                    ui,
                    half,
                    &format!("{:.1} \u{2192} {:.1} GB", k.free_before as f64 / 1_073_741_824.0, k.free_now as f64 / 1_073_741_824.0),
                    "memory available",
                );
                let peak = k.mhz_peak.max(k.mhz_now);
                stat_tile(ui, half, &format!("{} \u{2192} {}", ghz_short(k.mhz_before), ghz_short(peak)), "CPU clock, GHz");
            });
            ui.add_space(8.0);
        }
        // Only what actually happened: zeros are noise.
        let rows: Vec<(&str, String)> = [
            ("Apps closed", (k.apps_closed > 0).then(|| k.apps_closed.to_string())),
            ("Apps paused", (k.apps_paused > 0).then(|| k.apps_paused.to_string())),
            ("Apps lowered", (k.apps_lowered > 0).then(|| k.apps_lowered.to_string())),
            ("Services stopped", (k.services_stopped > 0).then(|| k.services_stopped.to_string())),
            ("Windows features quietened", (k.tweaks > 0).then(|| k.tweaks.to_string())),
            ("Memory freed", (k.ram_freed > 0).then(|| procs::fmt_mem(k.ram_freed))),
            (
                "Power plan",
                (!k.power_plan.is_empty()).then(|| {
                    if k.cpu_level.is_empty() {
                        k.power_plan.clone()
                    } else {
                        format!("{} + CPU {}", k.power_plan, k.cpu_level)
                    }
                }),
            ),
            ("Game priority", k.game_priority.then(|| if k.io_boost { "raised + disk".to_string() } else { "raised".to_string() })),
            ("Graphics card", (k.gpu_core_mhz > 0).then(|| format!("+{} / +{} MHz", k.gpu_core_mhz, k.gpu_mem_mhz))),
        ]
        .into_iter()
        .filter_map(|(n, v)| v.map(|v| (n, v)))
        .collect();
        for (name, val) in rows {
            ui.horizontal(|ui| {
                ui.set_height(24.0);
                ui.label(dim(ui, name, 14.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| ui.label(RichText::new(val).size(14.0)));
            });
        }
        if st.quiet_desktop {
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                ui.set_height(28.0);
                ui.label(dim(ui, "Windows desktop is off", 14.0));
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if link_button(ui, "Bring it back").on_hover_text(format!("Or press {}", catalyx_core::shell::EXIT_KEYS)).clicked() {
                        app.send(Intent::ExitQuietDesktop);
                    }
                });
            });
        }
        if let Some(l) = &live {
            if phase.active() && (l.temp_c.is_some() || l.core_mhz.is_some()) {
                ui.add_space(4.0);
                ui.label(faint(
                    ui,
                    format!(
                        "Graphics card now: {} \u{00b7} {} MHz \u{00b7} {}%",
                        l.temp_c.map(|t| format!("{t} \u{00b0}C")).unwrap_or_default(),
                        l.core_mhz.unwrap_or(0),
                        l.util_pct.unwrap_or(0)
                    ),
                    12.0,
                ));
            }
        }
        ui.add_space(8.0);
        hairline(ui);
        ui.add_space(8.0);
        ui.label(faint(ui, "ACTIVITY", 12.0));
        ui.add_space(2.0);
        ScrollArea::vertical().id_salt("activity").max_height(240.0).stick_to_bottom(true).auto_shrink([false, true]).show(ui, |ui| {
            ui.spacing_mut().item_spacing.y = 3.0;
            for line in &st.log {
                let tone = if line.tags.has(Tags::ERROR) {
                    p.danger
                } else if line.tags.has(Tags::WARN) {
                    p.warn
                } else if line.tags.has(Tags::OK) {
                    p.ok
                } else {
                    p.dim
                };
                ui.label(RichText::new(&line.text).size(12.0).color(tone));
            }
        });
    });
}

fn ghz_short(mhz: u32) -> String {
    if mhz == 0 {
        "-".into()
    } else {
        format!("{:.2}", mhz as f32 / 1000.0)
    }
}
