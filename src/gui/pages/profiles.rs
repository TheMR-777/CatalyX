//! Profiles: global presets (Gentle, Balanced, Max Squeeze and your own) and per-game settings.
//! A game follows a preset until you give it its own settings.

use crate::app::spec::{self, Group, Kind, Spec, Value};
use crate::app::Intent;
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
        // The fine default the window has always started on; `show` moves it if it is ever absent.
        ProfUi { target: EditTarget::Preset(crate::config::PRESET_BALANCED.to_string()) }
    }
}

pub fn show(app: &mut CatalyxApp, ui: &mut Ui) {
    // A removed game or preset must not leave a dangling target.
    let cfg = app.v.config.clone();
    // A removed game or preset must not leave a dangling target; with ids that is a lookup.
    let fallback = || {
        let id = if cfg.presets.iter().any(|p| p.id == crate::config::PRESET_BALANCED) {
            crate::config::PRESET_BALANCED.to_string()
        } else {
            cfg.presets.first().map(|p| p.id.clone()).unwrap_or_default()
        };
        EditTarget::Preset(id)
    };
    let target = match &app.ui.prof.target {
        EditTarget::Preset(id) if cfg.presets.iter().any(|p| p.id == *id) => app.ui.prof.target.clone(),
        EditTarget::Game(id) => match cfg.games.iter().find(|g| g.id == *id) {
            Some(g) if g.custom.is_some() => app.ui.prof.target.clone(),
            _ => fallback(),
        },
        _ => fallback(),
    };
    app.ui.prof.target = target;
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
    let mut targets: Vec<EditTarget> = cfg.presets.iter().map(|p| EditTarget::Preset(p.id.clone())).collect();
    targets.extend(cfg.games.iter().filter(|g| g.custom.is_some()).map(|g| EditTarget::Game(g.id.clone())));
    let names: Vec<String> = targets
        .iter()
        .map(|t| match t {
            EditTarget::Preset(id) => cfg.presets.iter().find(|p| p.id == *id).map(|p| p.name.clone()).unwrap_or_default(),
            EditTarget::Game(id) => match cfg.games.iter().find(|g| g.id == *id) {
                Some(g) => format!("{} (own settings)", g.name),
                None => String::new(),
            },
        })
        .collect();
    let mut cur = targets.iter().position(|t| *t == app.ui.prof.target).unwrap_or(0);
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 12.0;
        ui.label(dim(ui, "Editing", 14.0));
        if dropdown(ui, "prof-select", 260.0, &mut cur, &names) {
            app.ui.prof.target = targets[cur].clone();
        }
    });
}

fn list(app: &mut CatalyxApp, ui: &mut Ui) {
    let p = app.pal();
    ui.spacing_mut().item_spacing.y = 6.0;
    section_label(ui, "Presets");
    for i in 0..app.v.config.presets.len() {
        let pr = app.v.config.presets[i].clone();
        let selected = matches!(&app.ui.prof.target, EditTarget::Preset(id) if *id == pr.id);
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
            app.ui.prof.target = EditTarget::Preset(pr.id.clone());
        }
    }
    if ui.add(egui_components::Button::secondary("New preset from this one").full_width()).clicked() {
        app.send(Intent::NewPreset { from: app.ui.prof.target.clone() });
        // The core appends the new preset, so it is the last one.
        if let Some(id) = app.v.config.presets.last().map(|p| p.id.clone()) {
            app.ui.prof.target = EditTarget::Preset(id);
        }
    }
    let own: Vec<String> = app.v.config.games.iter().filter(|g| g.custom.is_some()).map(|g| g.id.clone()).collect();
    if !own.is_empty() {
        ui.add_space(8.0);
        section_label(ui, "Games with their own settings");
        for id in own {
            let selected = matches!(&app.ui.prof.target, EditTarget::Game(t) if *t == id);
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
            let name = app.v.config.games.iter().find(|g| g.id == id).map(|g| g.name.clone()).unwrap_or_default();
            ui.painter().text(
                rect.left_center() + vec2(14.0, 0.0),
                egui::Align2::LEFT_CENTER,
                short(&name, 28),
                egui::FontId::proportional(14.0),
                p.text,
            );
            if resp.clicked() {
                app.ui.prof.target = EditTarget::Game(id.clone());
            }
        }
    }
}

fn editor(app: &mut CatalyxApp, ui: &mut Ui) {
    let target = app.ui.prof.target.clone();
    ScrollArea::vertical().id_salt("profile-editor").auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            // The header shows the config as it was when this card started drawing: a button below may
            // change it (a deleted preset must not be read again in the same frame).
            let cfg = app.v.config.clone();
            match &target {
                EditTarget::Preset(id) => {
                    let Some(pr) = cfg.presets.iter().find(|p| p.id == *id) else { return };
                    let builtin = pr.builtin;
                    ui.horizontal(|ui| {
                        if builtin {
                            ui.label(bold(&pr.name, 16.0));
                        } else {
                            let mut name = pr.name.clone();
                            if ui.add(egui_components::Input::new(&mut name).width(240.0)).changed() && !name.trim().is_empty() {
                                app.send(Intent::RenamePreset { id: pr.id.clone(), name });
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !builtin && ui.add(egui_components::Button::ghost("Delete").small()).clicked() {
                                app.confirm = Some(Confirm::DeletePreset { id: pr.id.clone(), name: pr.name.clone() });
                            }
                            if builtin && ui.add(egui_components::Button::ghost("Reset to built-in").small()).clicked() {
                                // Only a built-in has a built-in version to go back to.
                                app.send(Intent::ResetPreset { id: pr.id.clone() });
                                app.toast_ok("Preset reset");
                            }
                            let is_default = app.v.config.default_preset == pr.id;
                            if ui
                                .add_enabled(!is_default, egui_components::Button::secondary("Make default").small())
                                .on_hover_text("Used for new games and for Boost only")
                                .clicked()
                            {
                                app.send(Intent::SetDefaultPreset { id: pr.id.clone() });
                            }
                        });
                    });
                    ui.label(faint(ui, pr.about.clone(), 12.0));
                    let users = cfg.games.iter().filter(|g| g.custom.is_none() && g.preset == pr.id).count();
                    ui.label(faint(ui, format!("{users} game(s) follow this preset"), 12.0));
                }
                EditTarget::Game(id) => {
                    let Some(g) = cfg.games.iter().find(|g| g.id == *id) else { return };
                    ui.horizontal(|ui| {
                        ui.label(bold(format!("{} - own settings", g.name), 16.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let pname = cfg.preset(&g.preset).name.clone();
                            if ui.add(egui_components::Button::secondary(format!("Follow {pname} again")).small()).clicked() {
                                app.send(Intent::FollowPreset { id: g.id.clone() });
                                app.ui.prof.target = EditTarget::Preset(g.preset.clone());
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
    let s = match &target {
        EditTarget::Preset(id) => app.v.config.presets.iter().find(|p| p.id == *id).map(|p| p.settings.clone()).unwrap_or_default(),
        EditTarget::Game(id) => app.v.config.games.iter().find(|g| g.id == *id).and_then(|g| g.custom.clone()).unwrap_or_default(),
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
                                    app.send(Intent::SetSetting {
                                        target: target.clone(),
                                        key: sp.key.into(),
                                        value: spec::onoff(on).into(),
                                    });
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
                                    app.send(Intent::SetSetting {
                                        target: target.clone(),
                                        key: sp.key.into(),
                                        value: choices[idx].key.into(),
                                    });
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
