//! Settings: look, behaviour, administrator start, the optional background watcher, your data.

use crate::engine::Cmd;
use crate::gui::control::Style;
use crate::gui::pages::Confirm;
use crate::gui::theme::{self, alpha, AccentId, ThemeId};
use crate::gui::widgets::*;
use crate::gui::{CatalyxApp, Msg, PickFor};
use crate::{config, elevate, ffi, procs};
use egui::{vec2, RichText, Sense, Stroke, Ui};

/// Opens CatalyX again with administrator rights (through the one-click task when it is set
/// up, otherwise with one Windows prompt) and closes this copy once the new one is starting.
pub fn relaunch_admin(app: &mut CatalyxApp) {
    let started = elevate::launch_ui_via_task() || ffi::relaunch_elevated();
    if started {
        let _ = app.tx.send(Cmd::Restore);
        std::thread::sleep(std::time::Duration::from_millis(400));
        std::process::exit(0);
    } else {
        app.toast_err("The administrator prompt was cancelled");
    }
}

pub fn show(app: &mut CatalyxApp, ui: &mut Ui) {
    page(ui, "settings", 760.0, |ui| {
        appearance(app, ui);
        behaviour(app, ui);
        admin(app, ui);
        data(app, ui);
        about(app, ui);
    });
}

fn appearance(app: &mut CatalyxApp, ui: &mut Ui) {
    let p = app.pal();
    card_titled(
        ui,
        "Appearance",
        |_| {},
        |ui| {
            let dark = app.dark_mode();
            let cur = app.theme_id();
            let accent = app.accent_id();
            let tint = app.cfg.tint;

            // ---- the look: three surface styles, each previewed with the chosen accent and tint
            ui.label(faint(ui, "Look", 12.0));
            let w = ((ui.available_width() - 24.0) / 3.0).min(250.0);
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 12.0;
                for t in ThemeId::ALL {
                    let pal = theme::palette(t, accent, tint, dark);
                    let (rect, resp) = ui.allocate_exact_size(vec2(w, 104.0), Sense::click());
                    let sel = cur == t;
                    ui.painter().rect_filled(rect, 12, pal.bg);
                    ui.painter().rect_stroke(
                        rect,
                        12,
                        Stroke::new(if sel { 2.0 } else { 1.0 }, if sel { p.accent } else { p.line_hi }),
                        egui::StrokeKind::Inside,
                    );
                    // A tiny mock of the window in that look.
                    let side = egui::Rect::from_min_size(rect.min + vec2(8.0, 8.0), vec2(26.0, rect.height() - 16.0));
                    ui.painter().rect_filled(side, 6, pal.sidebar);
                    let card = egui::Rect::from_min_size(rect.min + vec2(42.0, 8.0), vec2(rect.width() - 50.0, 50.0));
                    ui.painter().rect_filled(card, 7, pal.panel);
                    ui.painter().rect_stroke(card, 7, Stroke::new(1.0, pal.line), egui::StrokeKind::Inside);
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(card.min + vec2(8.0, 10.0), vec2(card.width() * 0.45, 7.0)),
                        3,
                        pal.text,
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(card.min + vec2(8.0, 24.0), vec2(card.width() * 0.3, 5.0)),
                        2,
                        pal.dim,
                    );
                    ui.painter().rect_filled(
                        egui::Rect::from_min_size(card.right_bottom() - vec2(46.0, 24.0), vec2(38.0, 16.0)),
                        6,
                        pal.accent,
                    );
                    ui.painter().text(
                        rect.left_bottom() + vec2(44.0, -12.0),
                        egui::Align2::LEFT_BOTTOM,
                        t.label(),
                        egui::FontId::new(14.0, egui::FontFamily::Name("bold".into())),
                        pal.text,
                    );
                    if resp.clicked() {
                        app.cfg.theme = t.key().into();
                        app.mark_dirty();
                    }
                    resp.on_hover_text(t.about());
                }
            });

            // ---- the accent: one swatch each, the selected one ringed
            ui.add_space(8.0);
            ui.label(faint(ui, "Accent", 12.0));
            ui.horizontal(|ui| {
                ui.spacing_mut().item_spacing.x = 10.0;
                for a in AccentId::ALL {
                    let col = a.swatch(cur, dark);
                    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
                    let sel = a == accent;
                    if sel {
                        ui.painter().circle_stroke(rect.center(), 14.5, Stroke::new(2.0, col));
                    }
                    ui.painter().circle_filled(rect.center(), if sel { 10.0 } else { 12.0 }, col);
                    if a == AccentId::Default {
                        // The look's own accent: a small hollow centre marks it as "no override".
                        ui.painter().circle_filled(rect.center(), 3.5, if sel { p.panel } else { alpha(p.panel, 200) });
                    }
                    if resp.hovered() {
                        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                    }
                    if resp.on_hover_text(a.label()).clicked() {
                        app.cfg.accent = a.key().into();
                        app.mark_dirty();
                    }
                }
            });
            ui.label(faint(ui, format!("{} - recolours the boost control, selections and switches", accent.label()), 12.0));

            ui.add_space(4.0);
            hairline(ui);
            setting_row(ui, "Surface tint", "How much of the accent bleeds into the window, sidebar and cards", |ui| {
                let mut i = (tint as usize).min(2);
                if segmented(ui, 214.0, &mut i, &theme::TINT_LABELS) {
                    app.cfg.tint = i as u8;
                    app.mark_dirty();
                }
            });
            hairline(ui);
            setting_row(ui, "Boost control", "A bar along the bottom, a gauge or a ring. All three do exactly the same thing", |ui| {
                let mut i = Style::ALL.iter().position(|s| *s == app.control_style()).unwrap_or(0);
                let labels: Vec<&str> = Style::ALL.iter().map(|s| s.label()).collect();
                if segmented(ui, 280.0, &mut i, &labels) {
                    app.cfg.boost_control = Style::ALL[i].key().into();
                    app.mark_dirty();
                }
            });
            hairline(ui);
            setting_row(ui, "Mode", "Dark, light, or follow Windows", |ui| {
                let mut i = match app.cfg.appearance.as_str() {
                    "light" => 1,
                    "system" => 2,
                    _ => 0,
                };
                if segmented(ui, 214.0, &mut i, &["Dark", "Light", "Windows"]) {
                    app.cfg.appearance = ["dark", "light", "system"][i].into();
                    app.mark_dirty();
                }
            });
        },
    );
}

fn behaviour(app: &mut CatalyxApp, ui: &mut Ui) {
    card_titled(
        ui,
        "Behaviour",
        |_| {},
        |ui| {
            let mut changed = false;
            setting_row(ui, "Restore when the game closes", "Everything is put back automatically, after a short grace period", |ui| {
                changed |= ui.add(egui_components::Switch::new(&mut app.cfg.restore_when_game_closes)).changed();
            });
            hairline(ui);
            setting_row(
                ui,
                "Keep chosen apps and services off",
                "If one starts again while you play, it is closed or stopped again",
                |ui| {
                    changed |= ui.add(egui_components::Switch::new(&mut app.cfg.keep_apps_off)).changed();
                },
            );
            hairline(ui);
            setting_row(ui, "Reopen closed apps afterwards", "Closed apps start again, but unsaved work is gone either way", |ui| {
                changed |= ui.add(egui_components::Switch::new(&mut app.cfg.reopen_closed_apps)).changed();
            });
            hairline(ui);
            setting_row(ui, "Ask before boosting", "A short summary of what is about to change, with a way to switch it off", |ui| {
                changed |= ui.add(egui_components::Switch::new(&mut app.cfg.confirm_boost)).changed();
            });
            hairline(ui);
            setting_row(ui, "Boost known games when they start", "Boosts games started outside CatalyX while this window is open", |ui| {
                changed |= ui.add(egui_components::Switch::new(&mut app.cfg.auto_boost_known_games)).changed();
            });
            if changed {
                app.mark_dirty();
            }
        },
    );
}

fn admin(app: &mut CatalyxApp, ui: &mut Ui) {
    // Task Scheduler is asked once (off the UI thread), not on every frame.
    if app.elevation.is_none() && !app.elevation_busy {
        app.refresh_elevation();
    }
    let (task, watcher) = app.elevation.unwrap_or((false, false));
    let is_admin = app.admin;
    let mut changed = false;
    let mut job: Option<(&'static str, &'static str)> = None;
    card_titled(
        ui,
        "Administrator and background",
        |ui| {
            if is_admin {
                chip(ui, "Running as administrator", Tone::Ok);
            } else {
                chip(ui, "Limited rights", Tone::Warn);
            }
        },
        |ui| {
            setting_row(
                ui,
                "Open CatalyX as administrator",
                "Needed to stop services and to boost games that run as administrator",
                |ui| {
                    changed |= ui.add(egui_components::Switch::new(&mut app.cfg.run_as_admin)).changed();
                },
            );
            hairline(ui);
            setting_row(
                ui,
                "One-click administrator start",
                if task {
                    "Set up: CatalyX opens elevated with no Windows prompt"
                } else {
                    "Skips the Windows prompt every time. Needs one approval now."
                },
                |ui| {
                    if task {
                        if ui.add(egui_components::Button::secondary("Remove")).clicked() {
                            job = Some(("elevate remove", "One-click start removed"));
                        }
                    } else if ui.add(egui_components::Button::primary("Set up")).clicked() {
                        job = Some(("elevate setup", "One-click administrator start is ready"));
                    }
                },
            );
            hairline(ui);
            setting_row(
                ui,
                "Boost games without opening the window",
                if watcher {
                    "On: a tiny hidden watcher starts at sign-in (under 10 MB) and boosts known games"
                } else {
                    "Off. A tiny hidden watcher can start at sign-in and boost your games automatically."
                },
                |ui| {
                    let mut on = watcher;
                    if ui.add(egui_components::Switch::new(&mut on)).changed() {
                        job = Some(if on {
                            ("autostart on", "Background watcher enabled")
                        } else {
                            ("autostart off", "Background watcher removed")
                        });
                    }
                },
            );
        },
    );
    if changed {
        app.mark_dirty();
    }
    if let Some((args, done)) = job {
        run_elevated_job(app, args, done);
    }
}
/// Runs a CatalyX command as administrator (one prompt) off the UI thread and reports back.
fn run_elevated_job(app: &mut CatalyxApp, args: &'static str, done: &'static str) {
    let admin = app.admin;
    app.toast_info(if admin { "Working..." } else { "Approve the Windows prompt to continue" });
    app.spawn(move || {
        let ok = if admin { ffi::run_self_as_current(args) } else { ffi::run_self_elevated_and_wait(args, 120_000) };
        Some(if ok { Msg::Info(done.to_string()) } else { Msg::Error("That did not complete (the prompt may have been cancelled)".into()) })
    });
}

fn data(app: &mut CatalyxApp, ui: &mut Ui) {
    card_titled(
        ui,
        "Your data",
        |_| {},
        |ui| {
            ui.label(faint(ui, format!("Saved as plain JSON in {}", config::data_dir().display()), 12.0));
            ui.add_space(4.0);
            ui.horizontal_wrapped(|ui| {
                if ui.add(egui_components::Button::secondary("Open folder")).clicked() {
                    let _ = std::process::Command::new("explorer").arg(config::data_dir()).spawn();
                }
                if ui.add(egui_components::Button::secondary("Export settings...")).clicked() {
                    app.pick_file(PickFor::ExportConfig);
                }
                if ui.add(egui_components::Button::secondary("Import settings...")).clicked() {
                    app.pick_file(PickFor::ImportConfig);
                }
                if ui.add(egui_components::Button::danger("Reset everything...")).clicked() {
                    app.confirm = Some(Confirm::ResetConfig);
                }
            });
        },
    );
}

fn about(app: &mut CatalyxApp, ui: &mut Ui) {
    let _ = app;
    card_titled(
        ui,
        "About",
        |ui| {
            ui.label(faint(ui, format!("Version {}", env!("CARGO_PKG_VERSION")), 12.0));
        },
        |ui| {
            ui.label(RichText::new("A lean game booster: no ads, no updater, no telemetry, no tray icon, nothing running in the background unless you turn the watcher on.").size(14.0));
            ui.add_space(4.0);
            let ws = ffi::own_working_set();
            ui.label(faint(ui, format!("This window uses {} of memory right now.", procs::fmt_mem(ws)), 12.0));
            ui.label(faint(ui, "Every change is written to a recovery journal before it is made, and undone when the game closes - or with `catalyx restore` after a crash.", 12.0));
        },
    );
}
