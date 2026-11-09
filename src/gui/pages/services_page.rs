//! Services: choose which Windows background services pause while you play. Each is stopped the
//! gentlest way Windows offers (dependents first, short time limit) and started again afterwards.

use crate::catalog::{self, Impact};
use crate::config::service_is_protected;
use crate::gui::icons::{self, Ico};
use crate::gui::pages::Confirm;
use crate::gui::widgets::*;
use crate::gui::CatalyxApp;
use egui::{pos2, vec2, Align, Align2, FontId, Layout, Rect, RichText, ScrollArea, Ui};

#[derive(Default)]
pub struct SvcUi {
    pub scope: usize,
    pub filter: String,
    pub view: usize,
}

fn scope_game(app: &CatalyxApp) -> Option<usize> {
    let s = app.ui.svc.scope;
    (s > 0 && s <= app.cfg.games.len()).then(|| s - 1)
}

fn effective(app: &CatalyxApp, game: Option<usize>, name: &str) -> bool {
    let rules = app.cfg.services_for(game.and_then(|i| app.cfg.games.get(i)));
    rules.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| *v).unwrap_or(false)
}

fn set_rule(app: &mut CatalyxApp, game: Option<usize>, name: &str, on: bool) {
    match game {
        None => {
            let key = app.cfg.services.keys().find(|k| k.eq_ignore_ascii_case(name)).cloned().unwrap_or_else(|| name.to_string());
            app.cfg.services.insert(key, on);
        }
        Some(i) => {
            let global = app.cfg.services.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| *v).unwrap_or(false);
            if on == global {
                app.cfg.games[i].services.retain(|k, _| !k.eq_ignore_ascii_case(name));
            } else {
                app.cfg.games[i].services.insert(name.to_string(), on);
            }
        }
    }
    app.mark_dirty();
}

pub fn show(app: &mut CatalyxApp, ui: &mut Ui) {
    if app.ui.svc.scope > app.cfg.games.len() {
        app.ui.svc.scope = 0;
    }
    let game = scope_game(app);
    centered(ui, 1120.0, |ui, w| {
        if !app.admin {
            let mut go = false;
            card(ui, |ui| {
                go = notice_row(ui, Tone::Warn, "Stopping a Windows service needs administrator rights", Some("Restart as admin"));
            });
            if go {
                super::settings::relaunch_admin(app);
            }
        }
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing = vec2(16.0, 8.0);
            let mut scopes: Vec<String> = vec!["All games".into()];
            scopes.extend(app.cfg.games.iter().map(|g| g.name.clone()));
            let mut s = app.ui.svc.scope;
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(faint(ui, "Rules apply to", 12.0));
                if dropdown(ui, "svc-scope", 180.0, &mut s, &scopes) {
                    app.ui.svc.scope = s;
                }
            });
            ui.vertical(|ui| {
                ui.spacing_mut().item_spacing.y = 4.0;
                ui.label(faint(ui, "Search", 12.0));
                ui.add(
                    egui_components::Input::new(&mut app.ui.svc.filter)
                        .placeholder("Filter by name")
                        .width((w - 180.0 - 290.0 - 32.0).clamp(140.0, 260.0)),
                );
            });
            if w >= 760.0 {
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 4.0;
                    ui.label(faint(ui, "Show", 12.0));
                    let mut v = app.ui.svc.view;
                    if segmented(ui, 290.0, &mut v, &["Recommended", "Running", "All"]) {
                        app.ui.svc.view = v;
                    }
                });
            }
        });
        if w < 760.0 {
            let mut v = app.ui.svc.view;
            if segmented(ui, 290.0, &mut v, &["Recommended", "Running", "All"]) {
                app.ui.svc.view = v;
            }
        }
        summary(app, ui, game, w);
        list(app, ui, game, w);
    });
}

fn summary(app: &mut CatalyxApp, ui: &mut Ui, game: Option<usize>, w: f32) {
    let p = app.pal();
    let rules = app.cfg.services_for(game.and_then(|i| app.cfg.games.get(i)));
    let on_count = rules.values().filter(|v| **v).count();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(format!("{on_count} service(s) pause while you play")).size(12.0).color(p.accent));
        ui.label(faint(ui, "\u{00b7} started again afterwards", 12.0));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui
                .add(egui_components::Button::secondary("Use recommended").small())
                .on_hover_text("Selects the services that are safe to pause for a play session. Your own choices are kept.")
                .clicked()
            {
                // Switches the recommended ones ON (for all games); nothing else is touched.
                for s in catalog::KNOWN_SERVICES.iter().filter(|s| s.recommended) {
                    set_rule(app, None, s.name, true);
                }
                app.toast_ok("Recommended services selected");
            }
            if w > 640.0 && ui.add(egui_components::Button::ghost("Keep all running").small()).clicked() {
                app.confirm = Some(Confirm::ClearServices);
            }
        });
    });
}

fn list(app: &mut CatalyxApp, ui: &mut Ui, game: Option<usize>, _w: f32) {
    let f = app.ui.svc.filter.trim().to_lowercase();
    let view = app.ui.svc.view;
    let all = app.svc_list.clone();
    let mut rows: Vec<&crate::services::ServiceInfo> = all
        .iter()
        .filter(|s| {
            let by_view = match view {
                0 => catalog::known_service(&s.name).is_some() || effective(app, game, &s.name),
                1 => s.running,
                _ => true,
            };
            by_view && (f.is_empty() || s.name.to_lowercase().contains(&f) || s.display.to_lowercase().contains(&f))
        })
        .collect();
    // Selected first, then recommended, then by name.
    rows.sort_by_key(|s| (!effective(app, game, &s.name), catalog::known_service(&s.name).is_none(), s.display.to_lowercase()));
    if rows.is_empty() {
        card(ui, |ui| {
            empty_state(
                ui,
                Ico::Layers,
                if all.is_empty() { "Reading services..." } else { "Nothing matches" },
                "Try another filter, or switch to All.",
            );
        });
        return;
    }
    let row_h = 56.0;
    list_card(ui, |ui| {
        ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, rows.len(), |ui, range| {
            let rw = ui.available_width() - 6.0;
            for s in &rows[range] {
                row(app, ui, s, game, rw, row_h);
            }
        });
    });
}

fn row(app: &mut CatalyxApp, ui: &mut Ui, s: &crate::services::ServiceInfo, game: Option<usize>, width: f32, h: f32) {
    let p = app.pal();
    let (rect, _) = flat_row(ui, width, h);
    let on = effective(app, game, &s.name);
    let known = catalog::known_service(&s.name);
    let protected = service_is_protected(&s.name);

    // Right to left: the switch, then a quiet one-word hint. `x` is the left edge placed so far.
    let mut x = rect.right() - 16.0;
    // Windows only reports whether a service accepts Stop while it is running.
    if protected || (s.running && !s.can_stop) {
        let msg = if protected { "Needed by Windows" } else { "Cannot be stopped" };
        let g = ui.painter().layout_no_wrap(msg.into(), FontId::proportional(12.0), p.faint);
        x -= g.size().x;
        ui.painter().galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.faint);
        icons::paint_at(ui.painter(), Ico::Lock, pos2(x - 12.0, rect.center().y), 14.0, p.faint);
        x -= 28.0;
    } else {
        let sr = Rect::from_min_size(pos2(x - 40.0, rect.center().y - 12.0), vec2(40.0, 24.0));
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(sr).id_salt(("svc", &s.name)).layout(Layout::left_to_right(Align::Center)));
        let mut v = on;
        if child.add(egui_components::Switch::new(&mut v)).changed() {
            set_rule(app, game, &s.name, v);
        }
        x -= 56.0;
        if let Some(k) = known {
            let (text, tone) = match (k.impact, k.recommended) {
                (Impact::Safe, true) => ("Recommended", Tone::Ok),
                (Impact::Noticeable, true) => ("You may notice", Tone::Warn),
                (_, false) => ("Optional", Tone::Neutral),
            };
            let c = tone_color(&p, tone);
            let g = ui.painter().layout_no_wrap(text.into(), FontId::proportional(12.0), c);
            x -= g.size().x;
            ui.painter().galley(pos2(x, rect.center().y - g.size().y / 2.0), g, c);
            x -= 16.0;
        }
    }
    let maxw = (x - rect.left() - 28.0).max(120.0);
    ui.painter().text(
        pos2(rect.left() + 16.0, rect.top() + h * 0.38),
        Align2::LEFT_CENTER,
        fit(ui, &s.display, 14.0, maxw),
        FontId::proportional(14.0),
        p.text,
    );
    let state = if s.running { "running" } else { "stopped" };
    let sub = match known {
        Some(k) => format!("{} \u{00b7} {state} \u{00b7} {}", s.name, k.why),
        None => format!("{} \u{00b7} {state}", s.name),
    };
    ui.painter().text(
        pos2(rect.left() + 16.0, rect.top() + h * 0.68),
        Align2::LEFT_CENTER,
        fit(ui, &sub, 12.0, maxw),
        FontId::proportional(12.0),
        p.faint,
    );
}

fn fit(ui: &Ui, text: &str, size: f32, max: f32) -> String {
    let font = FontId::proportional(size);
    let w = |s: &str| ui.painter().layout_no_wrap(s.to_string(), font.clone(), egui::Color32::WHITE).size().x;
    if w(text) <= max {
        return text.to_string();
    }
    let chars: Vec<char> = text.chars().collect();
    let mut n = chars.len();
    while n > 1 {
        n -= 1;
        let s: String = chars[..n].iter().collect::<String>() + "\u{2026}";
        if w(&s) <= max {
            return s;
        }
    }
    "\u{2026}".into()
}
