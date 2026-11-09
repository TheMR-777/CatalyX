//! Profiles: global presets (Gentle, Balanced, Max Squeeze and your own) and per-game settings.
//! A game follows a preset until you give it its own settings.

use crate::config::{CpuLevel, GpuLevel, PowerChoice, Preset, Priority, Settings};
use crate::gui::icons::Ico;
use crate::gui::theme::alpha;
use crate::gui::widgets::*;
use crate::gui::{CatalyxApp, EditTarget};
use egui::{vec2, Align, Layout, RichText, ScrollArea, Sense, Stroke, Ui};

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
    app.ui.prof.target = match app.ui.prof.target {
        EditTarget::Preset(i) if i >= app.cfg.presets.len() => EditTarget::Preset(0),
        EditTarget::Game(i) if i >= app.cfg.games.len() || app.cfg.games[i].custom.is_none() => {
            EditTarget::Preset(1.min(app.cfg.presets.len() - 1))
        }
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
    let mut targets: Vec<EditTarget> = (0..app.cfg.presets.len()).map(EditTarget::Preset).collect();
    targets.extend(app.cfg.games.iter().enumerate().filter(|(_, g)| g.custom.is_some()).map(|(i, _)| EditTarget::Game(i)));
    let names: Vec<String> = targets
        .iter()
        .map(|t| match t {
            EditTarget::Preset(i) => app.cfg.presets[*i].name.clone(),
            EditTarget::Game(i) => format!("{} (own settings)", app.cfg.games[*i].name),
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
    for i in 0..app.cfg.presets.len() {
        let pr = app.cfg.presets[i].clone();
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
        if app.cfg.default_preset == pr.id {
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
        let base = match app.ui.prof.target {
            EditTarget::Preset(i) => app.cfg.presets[i].settings.clone(),
            EditTarget::Game(i) => app.cfg.games[i].custom.clone().unwrap_or_default(),
        };
        let n = app.cfg.presets.iter().filter(|p| !p.builtin).count() + 1;
        app.cfg.presets.push(Preset {
            id: app.cfg.new_id("preset"),
            name: format!("My preset {n}"),
            about: "Your own preset".into(),
            builtin: false,
            settings: base,
        });
        app.ui.prof.target = EditTarget::Preset(app.cfg.presets.len() - 1);
        app.mark_dirty();
    }
    let own: Vec<usize> = app.cfg.games.iter().enumerate().filter(|(_, g)| g.custom.is_some()).map(|(i, _)| i).collect();
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
                short(&app.cfg.games[i].name, 28),
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
    let p = app.pal();
    let target = app.ui.prof.target;
    let admin = app.admin;
    ScrollArea::vertical().id_salt("profile-editor").auto_shrink([false, false]).show(ui, |ui| {
        card(ui, |ui| {
            ui.set_width(ui.available_width());
            match target {
                EditTarget::Preset(i) => {
                    let builtin = app.cfg.presets[i].builtin;
                    ui.horizontal(|ui| {
                        if builtin {
                            ui.label(bold(&app.cfg.presets[i].name, 16.0));
                        } else {
                            let mut name = app.cfg.presets[i].name.clone();
                            if ui.add(egui_components::Input::new(&mut name).width(240.0)).changed() && !name.trim().is_empty() {
                                app.cfg.presets[i].name = name;
                                app.mark_dirty();
                            }
                        }
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if !builtin && ui.add(egui_components::Button::ghost("Delete").small()).clicked() {
                                let id = app.cfg.presets[i].id.clone();
                                app.cfg.presets.remove(i);
                                let fallback = app.cfg.presets[1.min(app.cfg.presets.len() - 1)].id.clone();
                                for g in app.cfg.games.iter_mut().filter(|g| g.preset == id) {
                                    g.preset = fallback.clone();
                                }
                                if app.cfg.default_preset == id {
                                    app.cfg.default_preset = fallback;
                                }
                                app.ui.prof.target = EditTarget::Preset(1.min(app.cfg.presets.len() - 1));
                                app.mark_dirty();
                            }
                            if builtin && ui.add(egui_components::Button::ghost("Reset to built-in").small()).clicked() {
                                if let Some(b) = crate::config::builtin_presets().into_iter().find(|b| b.id == app.cfg.presets[i].id) {
                                    app.cfg.presets[i].settings = b.settings;
                                    app.mark_dirty();
                                    app.toast_ok("Preset reset");
                                }
                            }
                            let is_default = app.cfg.default_preset == app.cfg.presets[i].id;
                            if ui
                                .add_enabled(!is_default, egui_components::Button::secondary("Make default").small())
                                .on_hover_text("Used for new games and for Boost only")
                                .clicked()
                            {
                                app.cfg.default_preset = app.cfg.presets[i].id.clone();
                                app.mark_dirty();
                            }
                        });
                    });
                    ui.label(faint(ui, app.cfg.presets[i].about.clone(), 12.0));
                    let users = app.cfg.games.iter().filter(|g| g.custom.is_none() && g.preset == app.cfg.presets[i].id).count();
                    ui.label(faint(ui, format!("{users} game(s) follow this preset"), 12.0));
                }
                EditTarget::Game(i) => {
                    ui.horizontal(|ui| {
                        ui.label(bold(format!("{} - own settings", app.cfg.games[i].name), 16.0));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let pname = app.cfg.preset(&app.cfg.games[i].preset).name.clone();
                            if ui.add(egui_components::Button::secondary(format!("Follow {pname} again")).small()).clicked() {
                                app.cfg.games[i].custom = None;
                                app.ui.prof.target = EditTarget::Preset(1.min(app.cfg.presets.len() - 1));
                                app.mark_dirty();
                            }
                        });
                    });
                    ui.label(faint(ui, "These settings apply to this game only.", 12.0));
                }
            }
        });
        ui.add_space(10.0);
        let mut s: Settings = match app.ui.prof.target {
            EditTarget::Preset(i) => app.cfg.presets[i].settings.clone(),
            EditTarget::Game(i) => app.cfg.games[i].custom.clone().unwrap_or_default(),
        };
        let before = s.clone();
        settings_editor(ui, &mut s, admin, &p);
        if s != before {
            match app.ui.prof.target {
                EditTarget::Preset(i) => app.cfg.presets[i].settings = s,
                EditTarget::Game(i) => app.cfg.games[i].custom = Some(s),
            }
            app.mark_dirty();
        }
    });
}

/// The full settings form, shared by presets and per-game settings. Returns nothing: the caller
/// compares before and after.
pub fn settings_editor(ui: &mut Ui, s: &mut Settings, admin: bool, p: &crate::gui::theme::Palette) {
    let need_admin = if admin { "" } else { " (needs administrator)" };
    let w = CONTROL_W;
    card_titled(
        ui,
        "Performance",
        |_| {},
        |ui| {
            ui.set_width(ui.available_width());
            setting_row(ui, "Game priority", s.priority.hint(), |ui| {
                let mut i = Priority::ALL.iter().position(|x| *x == s.priority).unwrap_or(0);
                if dropdown(ui, "pr-priority", w, &mut i, &Priority::ALL.iter().map(|x| x.label().to_string()).collect::<Vec<_>>()) {
                    s.priority = Priority::ALL[i];
                }
            });
            hairline(ui);
            setting_row(ui, "Power plan", s.power.hint(), |ui| {
                let mut i = PowerChoice::ALL.iter().position(|x| *x == s.power).unwrap_or(0);
                if dropdown(ui, "pr-power", w, &mut i, &PowerChoice::ALL.iter().map(|x| x.label().to_string()).collect::<Vec<_>>()) {
                    s.power = PowerChoice::ALL[i];
                }
            });
            hairline(ui);
            setting_row(ui, "CPU performance", s.cpu.hint(), |ui| {
                let mut i = CpuLevel::ALL.iter().position(|x| *x == s.cpu).unwrap_or(0);
                if dropdown(ui, "pr-cpu", w, &mut i, &CpuLevel::ALL.iter().map(|x| x.label().to_string()).collect::<Vec<_>>()) {
                    s.cpu = CpuLevel::ALL[i];
                }
            });
            hairline(ui);
            setting_row(ui, "Graphics card tuning", s.gpu.hint(), |ui| {
                let mut i = GpuLevel::ALL.iter().position(|x| *x == s.gpu).unwrap_or(0);
                if dropdown(ui, "pr-gpu", w, &mut i, &GpuLevel::ALL.iter().map(|x| x.label().to_string()).collect::<Vec<_>>()) {
                    s.gpu = GpuLevel::ALL[i];
                }
            });
            hairline(ui);
            toggle(
                ui,
                "Disk and memory priority for the game",
                "The game gets the disk first: less stutter while it streams textures",
                &mut s.io_boost,
            );
        },
    );
    ui.add_space(10.0);
    card_titled(
        ui,
        "Apps, services and memory",
        |_| {},
        |ui| {
            ui.set_width(ui.available_width());
            toggle(ui, "Handle my app rules", "Lower, pause or close the apps you chose in Apps", &mut s.apps);
            hairline(ui);
            toggle(
                ui,
                &format!("Pause my chosen services{need_admin}"),
                "Stopped for the session, started again afterwards",
                &mut s.services,
            );
            hairline(ui);
            toggle(ui, "Lower everything else", "Every other app of yours runs at the lowest priority", &mut s.lower_others);
            hairline(ui);
            toggle(ui, "Free memory before the game starts", "Trims what background apps are holding", &mut s.clean_ram);
            hairline(ui);
            toggle(
                ui,
                &format!("Also flush cached memory{need_admin}"),
                "Empties Windows' standby list so the game's memory is free at once",
                &mut s.purge_standby,
            );
        },
    );
    ui.add_space(10.0);
    card_titled(
        ui,
        "Windows while you play",
        |_| {},
        |ui| {
            ui.set_width(ui.available_width());
            toggle(
                ui,
                "Windows Game Mode",
                "Let Windows itself favour the game and hold back updates and drivers",
                &mut s.windows_game_mode,
            );
            hairline(ui);
            toggle(ui, "Background recording off", "Stops Game Bar capture from recording behind the game", &mut s.game_dvr_off);
            hairline(ui);
            toggle(ui, "Quiet notifications", "No pop-ups on top of the game", &mut s.notifications_off);
            hairline(ui);
            toggle(
                ui,
                "Keep Windows efficiency mode off for the game",
                "Stops Windows slowing the game down to save power",
                &mut s.no_power_throttling,
            );
            hairline(ui);
            toggle(
                ui,
                &format!("Pause automatic maintenance{need_admin}"),
                "No scheduled defrag or scans during the game",
                &mut s.pause_maintenance,
            );
            hairline(ui);
            toggle(ui, "Transparency effects off", "Saves a little graphics power on weak PCs", &mut s.transparency_off);
            hairline(ui);
            toggle(
                ui,
                &format!("Multimedia scheduler favours games{need_admin}"),
                "Windows' own game scheduling class, tuned up",
                &mut s.mmcss_games,
            );
            hairline(ui);
            toggle(ui, "Quiet desktop", "Taskbar off while you play; Ctrl+Alt+End brings it back", &mut s.quiet_desktop);
        },
    );
    let _ = (p, RichText::new(""), Ico::Check);
}

fn toggle(ui: &mut Ui, title: &str, hint: &str, value: &mut bool) {
    setting_row(ui, title, hint, |ui| {
        ui.add(egui_components::Switch::new(value));
    });
}

fn short(s: &str, n: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= n {
        s.to_string()
    } else {
        c[..n - 1].iter().collect::<String>() + "\u{2026}"
    }
}
