//! Profiles: global presets (Gentle, Balanced, Max Squeeze and your own) and per-game settings.
//! A game follows a preset until you give it its own settings.

use crate::app::spec::{self, Group, Kind, Spec, Value};
use crate::app::Intent;
use crate::config::Preset;
use crate::gui::pages::Confirm;
use crate::gui::theme::alpha;
use crate::gui::widgets::*;
use crate::gui::{CatalyxApp, EditTarget};
use egui::{vec2, Align, Layout, ScrollArea, Sense, Stroke, Ui};

pub struct ProfUi {
    pub target: EditTarget,
}

impl Default for ProfUi {
    fn default() -> Self {
        ProfUi { target: EditTarget::Preset(1) }
    }
}

pub fn show(app: &mut CatalyxApp, ui: &mut Ui) {
    // A removed game or preset must not leave a dangling target.
    let cfg = app.v.config.clone();
    app.ui.prof.target = match app.ui.prof.target {
        EditTarget::Preset(i) if i >= cfg.presets.len() => EditTarget::Preset(0),
        EditTarget::Game(i) if i >= cfg.games.len() || cfg.games[i].custom.is_none() => EditTarget::Preset(1.min(cfg.presets.len() - 1)),
        t => t,
    };
    centered(ui, 1120.0, |ui, total| {
        if total > 820.0 {
            let list_w = 250.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                ui.vertical(|ui| {
                    ui.set_width(list_w);
                    list(app, ui);
                });
                ui.vertical(|ui| {
                    ui.set_width(total - list_w - 16.0);
                    editor(app, ui);
                });
            });
        } else {
            // A narrow window gets a selector instead of a tall list.
            selector(app, ui);
            editor(app, ui);
        }
    });
}

/// Which preset or game-with-own-settings to edit, as one drop-down (for narrow windows).
fn selector(app: &mut CatalyxApp, ui: &mut Ui) {
    let cfg = app.v.config.clone();
    let mut targets: Vec<EditTarget> = (0..cfg.presets.len()).map(EditTarget::Preset).collect();
    targets.extend(cfg.games.iter().enumerate().filter(|(_, g)| g.custom.is_some()).map(|(i, _)| EditTarget::Game(i)));
    let names: Vec<String> = targets
        .iter()
        .map(|t| match t {
            EditTarget::Preset(i) => cfg.presets[*i].name.clone(),
            EditTarget::Game(i) => format!("{} (own settings)", cfg.games[*i].name),
        })
        .collect();
    let mut cur = targets.iter().position(|t| *t == app.ui.prof.target).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.label(dim(ui, "Editing", 14.0));
        if dropdown(ui, "prof-select", 260.0, &mut cur, &names) {
            app.ui.prof.target = targets[cur];
        }
    });
}

fn list(app: &mut CatalyxApp, ui: &mut Ui) {
    let p = app.pal();
    ui.spacing_mut().item_spacing.y = 6.0;
    section_label(ui, "Presets");
    for i in 0..app.v.config.presets.len() {
        let pr = app.v.config.presets[i].clone();
        let selected = app.ui.prof.target == EditTarget::Preset(i);
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 58.0), Sense::click());
        ui.painter().rect_filled(
            rect,
            12,
            if selected {
                p.panel_hi
            } else if resp.hovered() {
                alpha(p.panel_hi, 120)
            } else {
                p.panel
            },
        );
        if selected {
            ui.painter().rect_stroke(rect, 12, Stroke::new(1.0, alpha(p.accent, 120)), egui::StrokeKind::Inside);
        }
        ui.painter().text(
            rect.left_top() + vec2(14.0, 19.0),
            egui::Align2::LEFT_CENTER,
            &pr.name,
            egui::FontId::new(14.5, egui::FontFamily::Name("bold".into())),
            p.text,
        );
        ui.painter().text(
            rect.left_top() + vec2(14.0, 40.0),
            egui::Align2::LEFT_CENTER,
            short(&pr.about, 36),
            egui::FontId::proportional(12.0),
            p.faint,
        );
        if app.v.config.default_preset == pr.id {
            let g = ui.painter().layout_no_wrap("Default".into(), egui::FontId::proportional(12.0), p.accent);
            let cr = egui::Rect::from_min_size(rect.right_top() + vec2(-g.size().x - 22.0, 10.0), vec2(g.size().x + 14.0, 18.0));
            ui.painter().rect_filled(cr, 9, p.accent_soft);
            ui.painter().text(cr.center(), egui::Align2::CENTER_CENTER, "Default", egui::FontId::proportional(12.0), p.accent);
        }
        if resp.clicked() {
            app.ui.prof.target = EditTarget::Preset(i);
        }
    }
    if ui.add(egui_components::Button::secondary("New preset from this one").full_width()).clicked() {
        let target = app.ui.prof.target;
        app.edit(move |cfg| {
            let base = match target {
                EditTarget::Preset(i) => cfg.presets[i].settings.clone(),
                EditTarget::Game(i) => cfg.games[i].custom.clone().unwrap_or_default(),
            };
            let n = cfg.presets.iter().filter(|p| !p.builtin).count() + 1;
            let id = cfg.new_id("preset");
            cfg.presets.push(Preset {
                id,
                name: format!("My preset {n}"),
                about: "Your own preset".into(),
                builtin: false,
                settings: base,
            });
        });
        app.ui.prof.target = EditTarget::Preset(app.v.config.presets.len() - 1);
    }
    let own: Vec<usize> = app.v.config.games.iter().enumerate().filter(|(_, g)| g.custom.is_some()).map(|(i, _)| i).collect();
    if !own.is_empty() {
        ui.add_space(8.0);
        section_label(ui, "Games with their own settings");
        for i in own {
            let selected = app.ui.prof.target == EditTarget::Game(i);
            let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 40.0), Sense::click());
            ui.painter().rect_filled(
                rect,
                10,
                if selected {
                    p.panel_hi
                } else if resp.hovered() {
                    alpha(p.panel_hi, 150)
                } else {
                    p.panel
                },
            );
            ui.painter().rect_stroke(
                rect,
                10,
                Stroke::new(1.0, if selected { alpha(p.accent, 130) } else { p.line }),
                egui::StrokeKind::Inside,
            );
            ui.painter().text(
                rect.left_center() + vec2(14.0, 0.0),
                egui::Align2::LEFT_CENTER,
                short(&app.v.config.games[i].name, 28),
                egui::FontId::proportional(14.0),
                p.text,
            );
            if resp.clicked() {
                app.ui.prof.target = EditTarget::Game(i);
            }
        }
    }
}

fn editor(app: &mut CatalyxApp, ui: &mut Ui) {
    let target = app.ui.prof.target;
    ScrollArea::vertical().id_salt("profile-editor").auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            // The header shows the config as it was when this card started drawing: a button below may
            // change it (a deleted preset must not be read again in the same frame).
            let cfg = app.v.config.clone();
            match target {
                EditTarget::Preset(i) => {
                    let pr = &cfg.presets[i];
                    let builtin = pr.builtin;
                    ui.horizontal(|ui| {
                        if builtin {
                            ui.label(bold(&pr.name, 16.0));
                        } else {
                            let mut name = pr.name.clone();
                            if ui.add(egui_components::Input::new(&mut name).width(240.0)).changed() && !name.trim().is_empty() {
                                app.edit(move |cfg| cfg.presets[i].name = name);
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !builtin && ui.add(egui_components::Button::ghost("Delete").small()).clicked() {
                                app.confirm = Some(Confirm::DeletePreset { id: pr.id.clone(), name: pr.name.clone() });
                            }
                            if builtin && ui.add(egui_components::Button::ghost("Reset to built-in").small()).clicked() {
                                if let Some(b) = crate::config::builtin_presets().into_iter().find(|b| b.id == pr.id) {
                                    app.edit(move |cfg| cfg.presets[i].settings = b.settings);
                                    app.toast_ok("Preset reset");
                                }
                            }
                            let is_default = app.v.config.default_preset == pr.id;
                            if ui
                                .add_enabled(!is_default, egui_components::Button::secondary("Make default").small())
                                .on_hover_text("Used for new games and for Boost only")
                                .clicked()
                            {
                                let id = pr.id.clone();
                                app.edit(move |cfg| cfg.default_preset = id);
                            }
                        });
                    });
                    ui.label(faint(ui, pr.about.clone(), 12.0));
                    let users = cfg.games.iter().filter(|g| g.custom.is_none() && g.preset == pr.id).count();
                    ui.label(faint(ui, format!("{users} game(s) follow this preset"), 12.0));
                }
                EditTarget::Game(i) => {
                    ui.horizontal(|ui| {
                        ui.label(bold(format!("{} - own settings", cfg.games[i].name), 16.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let pname = cfg.preset(&cfg.games[i].preset).name.clone();
                            if ui.add(egui_components::Button::secondary(format!("Follow {pname} again")).small()).clicked() {
                                app.edit(move |cfg| cfg.games[i].custom = None);
                                app.ui.prof.target = EditTarget::Preset(1.min(app.v.config.presets.len() - 1));
                            }
                        });
                    });
                    ui.label(faint(ui, "These settings apply to this game only.", 12.0));
                }
            }
        });
        ui.add_space(10.0);
        settings_editor(app, ui, target);
    });
}

/// The full settings form, shared by presets and per-game settings, generated from the catalogue.
/// Every change goes through `Intent::SetSetting`, so a skin never writes the config itself.
fn settings_editor(app: &mut CatalyxApp, ui: &mut Ui, target: EditTarget) {
    let admin = app.v.system.admin;
    let gpu = app.v.hardware.gpu.clone();
    let s = match target {
        EditTarget::Preset(i) => app.v.config.presets[i].settings.clone(),
        EditTarget::Game(i) => app.v.config.games[i].custom.clone().unwrap_or_default(),
    };
    let mut first = true;
    for group in Group::ALL {
        let rows: Vec<&Spec> = spec::SETTINGS.iter().filter(|sp| sp.group == group).collect();
        if rows.is_empty() {
            continue;
        }
        // One gap between cards, as before: a trailing gap would make the last card taller.
        if !first {
            ui.add_space(10.0);
        }
        first = false;
        card_titled(
            ui,
            group.label(),
            |_| {},
            |ui| {
                ui.set_width(ui.available_width());
                for (i, sp) in rows.iter().enumerate() {
                    let title = if sp.needs_admin && !admin { format!("{} (needs administrator)", sp.label) } else { sp.label.to_string() };
                    let hint = sp.hint_text(&s, gpu.as_ref());
                    match sp.kind {
                        Kind::Switch => {
                            let Value::Switch(mut on) = sp.get(&s) else { continue };
                            setting_row(ui, &title, &hint, |ui| {
                                if ui.add(egui_components::Switch::new(&mut on)).changed() {
                                    app.send(Intent::SetSetting { target, key: sp.key.into(), value: spec::onoff(on).into() });
                                }
                            });
                        }
                        Kind::Choice(choices) => {
                            let current = match sp.get(&s) {
                                Value::Choice(k) => k,
                                Value::Switch(_) => "",
                            };
                            let mut idx = choices.iter().position(|c| c.key == current).unwrap_or(0);
                            let labels: Vec<String> = choices.iter().map(|c| c.label.to_string()).collect();
                            setting_row(ui, &title, &hint, |ui| {
                                if dropdown(ui, &format!("pr-{}", sp.key), CONTROL_W, &mut idx, &labels) {
                                    app.send(Intent::SetSetting { target, key: sp.key.into(), value: choices[idx].key.into() });
                                }
                            });
                        }
                    }
                    if i + 1 < rows.len() {
                        hairline(ui);
                    }
                }
            },
        );
    }
}

fn short(s: &str, n: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= n {
        s.to_string()
    } else {
        c[..n - 1].iter().collect::<String>() + "\u{2026}"
    }
}
