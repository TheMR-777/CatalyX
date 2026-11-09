//! Boost: pick a game and a profile, press the button. Calm by default: only things that need
//! attention are shown, and the right-hand card is the plan while idle, the live session while
//! boosting.

use crate::config::{CpuLevel, GpuLevel, PowerChoice, ProcAction};
use crate::engine::{Phase, Status};
use crate::gui::icons::{self, Ico};
use crate::gui::theme::Palette;
use crate::gui::widgets::*;
use crate::gui::{CatalyxApp, EditTarget, Tab};
use crate::procs;
use egui::{vec2, Align, Layout, RichText, ScrollArea, Ui};

const SIDE_W: f32 = 320.0;
const GAP: f32 = 16.0;

pub fn show(app: &mut CatalyxApp, ui: &mut Ui) {
    let status = app.status.lock().map(|s| s.clone()).unwrap_or_default();
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
                    notices(app, ui, &status);
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
            notices(app, ui, &status);
            if session {
                session_card(app, ui, &status);
            } else {
                plan_card(app, ui);
            }
        }
    });
}

fn hero(app: &mut CatalyxApp, ui: &mut Ui, status: &Status) {
    let phase = status.phase;
    let games = app.cfg.games.len();
    let sel = app.cfg.selected_game.min(games.saturating_sub(1));
    let game = app.cfg.games.get(sel).cloned();
    let settings = app.cfg.settings_for(game.as_ref());
    card(ui, |ui| {
        let inner = ui.available_width();
        let boost_w = 160.0;
        let left_w = inner - boost_w - 24.0;
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
                            let names: Vec<String> = app.cfg.games.iter().map(|g| g.name.clone()).collect();
                            let mut i = sel;
                            if dropdown(ui, "boost-game", half, &mut i, &names) {
                                app.cfg.selected_game = i;
                                app.mark_dirty();
                            }
                        }
                    });
                    ui.vertical(|ui| {
                        ui.spacing_mut().item_spacing.y = 4.0;
                        ui.label(faint(ui, "Profile", 12.0));
                        let custom = game.as_ref().map(|g| g.custom.is_some()).unwrap_or(false);
                        let mut names: Vec<String> = app.cfg.presets.iter().map(|p| p.name.clone()).collect();
                        if custom {
                            names.push("Own settings".into());
                        }
                        let current = game.as_ref().map(|g| g.preset.clone()).unwrap_or_else(|| app.cfg.default_preset.clone());
                        let mut i =
                            if custom { app.cfg.presets.len() } else { app.cfg.presets.iter().position(|p| p.id == current).unwrap_or(0) };
                        if dropdown(ui, "boost-profile", half, &mut i, &names) {
                            if i < app.cfg.presets.len() {
                                let id = app.cfg.presets[i].id.clone();
                                if let Some(g) = app.cfg.games.get_mut(sel) {
                                    g.preset = id;
                                    g.custom = None;
                                } else {
                                    app.cfg.default_preset = id;
                                }
                                app.mark_dirty();
                            } else {
                                app.ui.prof.target = EditTarget::Game(sel);
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
            // ---- right: the one primary action
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 8.0;
                let active = phase.active();
                let label = if active { "RESTORE" } else { "BOOST" };
                let sub = if active {
                    "back to normal"
                } else if games > 0 {
                    "& play"
                } else {
                    "gaming mode"
                };
                app.drawn.insert("boost_button");
                if boost_button(ui, vec2(boost_w, boost_w), label, sub, active, true).clicked() {
                    if active {
                        app.restore();
                    } else {
                        app.boost_game(if games > 0 { Some(sel) } else { None });
                    }
                }
                ui.allocate_ui_with_layout(vec2(boost_w, 28.0), Layout::left_to_right(Align::Center), |ui| {
                    ui.spacing_mut().item_spacing.x = 0.0;
                    ui.add_space(14.0);
                    if ui
                        .add_enabled(!active, egui::Button::new(faint(ui, "Boost only", 12.0)).frame(false))
                        .on_hover_text("Tune the PC now without starting a game. Press Restore when done.")
                        .clicked()
                    {
                        app.boost_game(None);
                    }
                    ui.add_space(8.0);
                    ui.label(faint(ui, "\u{00b7}", 12.0));
                    ui.add_space(8.0);
                    if ui.add_enabled(active, egui::Button::new(faint(ui, "Restore", 12.0)).frame(false)).clicked() {
                        app.restore();
                    }
                });
            });
        });
    });
}

/// Only what needs attention. Nothing at all when everything is in order.
fn notices(app: &mut CatalyxApp, ui: &mut Ui, status: &Status) {
    let game = app.cfg.games.get(app.cfg.selected_game).cloned();
    let rules = app.cfg.apps_for(game.as_ref());
    let none_apps = rules.values().all(|a| *a == ProcAction::Keep);
    let none_svcs = !app.cfg.services_for(game.as_ref()).values().any(|v| *v);
    let idle = status.phase == Phase::Idle;
    #[derive(Clone, Copy)]
    enum Do {
        Restore,
        Admin,
        Suggest,
        AddGame,
    }
    let mut rows: Vec<(Tone, &str, &str, Do)> = Vec::new();
    if status.pending_journal && idle {
        rows.push((Tone::Warn, "A previous boost was interrupted: some changes may still be in place", "Restore now", Do::Restore));
    }
    if !app.admin {
        rows.push((Tone::Warn, "Not running as administrator: services and elevated games are off-limits", "Restart as admin", Do::Admin));
    }
    if none_apps && none_svcs && idle {
        rows.push((Tone::Accent, "No apps or services chosen yet", "Use suggestions", Do::Suggest));
    }
    if app.cfg.games.is_empty() {
        rows.push((Tone::Accent, "No game added yet", "Add a game", Do::AddGame));
    }
    if rows.is_empty() {
        return;
    }
    let mut act: Option<Do> = None;
    card(ui, |ui| {
        ui.spacing_mut().item_spacing.y = 0.0;
        let n = rows.len();
        for (i, (tone, text, action, what)) in rows.iter().enumerate() {
            if notice_row(ui, *tone, text, Some(action)) {
                act = Some(*what);
            }
            if i + 1 < n {
                hairline(ui);
            }
        }
    });
    match act {
        Some(Do::Restore) => app.restore(),
        Some(Do::Admin) => super::settings::relaunch_admin(app),
        Some(Do::Suggest) => {
            super::apply_suggestions(app);
            app.toast_ok("Suggestions selected: review them in Apps and Services");
        }
        Some(Do::AddGame) => app.tab = Tab::Games,
        None => {}
    }
}

fn power_summary(s: &crate::config::Settings) -> String {
    let plan = match s.power {
        PowerChoice::Keep => "your power plan",
        PowerChoice::HighPerformance => "High performance plan",
        PowerChoice::BoostPlan => "CatalyX power plan",
    };
    match s.cpu {
        CpuLevel::Off => plan.to_string(),
        c => format!("{plan}, CPU {}", c.label().split(' ').next().unwrap_or("").to_lowercase()),
    }
}

/// What this boost will do: only the things that are switched on.
fn plan_card(app: &mut CatalyxApp, ui: &mut Ui) {
    let p = app.pal();
    let game = app.cfg.games.get(app.cfg.selected_game).cloned();
    let s = app.cfg.settings_for(game.as_ref());
    let rules = app.cfg.apps_for(game.as_ref());
    let count = |a: ProcAction| rules.values().filter(|x| **x == a).count();
    let n_svc = app.cfg.services_for(game.as_ref()).values().filter(|v| **v).count();
    let mut on: Vec<String> = Vec::new();
    let mut off = 0;
    let mut add = |enabled: bool, text: String| {
        if enabled {
            on.push(text)
        } else {
            off += 1
        }
    };
    add(s.power != PowerChoice::Keep || s.cpu != CpuLevel::Off, power_summary(&s));
    add(true, format!("{} game priority{}", s.priority.label(), if s.io_boost { " + disk" } else { "" }));
    let (lo, pa, cl) = (count(ProcAction::Lower), count(ProcAction::Suspend), count(ProcAction::Close));
    add(s.apps && lo + pa + cl > 0, format!("Apps: {lo} lowered, {pa} paused, {cl} closed"));
    add(s.services && n_svc > 0, format!("{n_svc} services paused{}", if app.admin { "" } else { " (needs admin)" }));
    add(s.lower_others, "Everything else at lowest priority".into());
    add(s.clean_ram, format!("Free memory first{}", if s.purge_standby { " + cached memory" } else { "" }));
    add(s.windows_game_mode, "Windows Game Mode on".into());
    add(s.notifications_off || s.game_dvr_off, "Quiet notifications and recording".into());
    add(s.gpu != GpuLevel::Off, format!("Graphics card: {}", s.gpu.label().to_lowercase()));
    add(s.quiet_desktop && game.is_some(), "Taskbar off while you play (Ctrl+Alt+End brings it back)".into());
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
            for text in &on {
                ui.horizontal(|ui| {
                    ui.spacing_mut().item_spacing.x = 10.0;
                    let (rect, _) = ui.allocate_exact_size(vec2(14.0, 22.0), egui::Sense::hover());
                    icons::paint_at(ui.painter(), Ico::Check, rect.center(), 13.0, p.ok);
                    ui.add(egui::Label::new(RichText::new(text).size(14.0)).truncate());
                });
            }
            if off > 0 {
                ui.add_space(6.0);
                ui.label(faint(ui, format!("{off} more option(s) are off"), 12.0));
            }
        },
    );
    if edit {
        app.ui.prof.target = match game.as_ref() {
            Some(g) if g.custom.is_some() => EditTarget::Game(app.cfg.selected_game),
            _ => EditTarget::Preset(
                app.cfg
                    .presets
                    .iter()
                    .position(|pr| pr.id == game.as_ref().map(|g| g.preset.clone()).unwrap_or_else(|| app.cfg.default_preset.clone()))
                    .unwrap_or(0),
            ),
        };
        app.tab = Tab::Profiles;
    }
}

fn session_card(app: &mut CatalyxApp, ui: &mut Ui, st: &Status) {
    let p = app.pal();
    let phase = st.phase;
    let live = app.live.clone();
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
            ui.add(egui::Label::new(RichText::new(format!("{} (pid {})", st.game_exe, st.game_pid)).size(14.0).color(p.accent)).truncate());
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
                    if link_button(ui, "Bring it back").on_hover_text(format!("Or press {}", crate::shell::EXIT_KEYS)).clicked() {
                        let _ = app.tx.send(crate::engine::Cmd::ExitQuietDesktop);
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
                ui.label(RichText::new(line).size(12.0).color(log_color(&p, line)));
            }
        });
    });
}

fn log_color(p: &Palette, line: &str) -> egui::Color32 {
    let l = line.to_lowercase();
    if l.contains("could not") || l.contains("failed") || l.contains("refused") || l.contains("blocked") {
        p.danger
    } else if l.contains("needs administrator") || l.contains("skipped") || l.contains("unchanged") {
        p.warn
    } else if l.starts_with("found the game") || l.contains("game priority:") || l.contains("checked") {
        p.ok
    } else {
        p.dim
    }
}

fn ghz_short(mhz: u32) -> String {
    if mhz == 0 {
        "-".into()
    } else {
        format!("{:.2}", mhz as f32 / 1000.0)
    }
}
