//! Games: your library. Add by browsing, by dropping a file on the window, or by importing what
//! Steam, Epic and GOG already installed. Each game has its own profile and real-program setting.

use crate::config::Game;
use crate::engine::Phase;
use crate::gui::icons::Ico;
use crate::gui::widgets::*;
use crate::gui::{CatalyxApp, EditTarget, Msg, PickFor, Tab};
use crate::{launcher, library};
use egui::{vec2, Align, Layout, RichText, ScrollArea, Ui};
use std::collections::HashMap;

#[derive(Default)]
pub struct GamesUi {
    pub adding: bool,
    pub name: String,
    pub path: String,
    pub args: String,
    pub installed: Option<Vec<library::Installed>>,
    pub scanning: bool,
    pub candidates: HashMap<String, Vec<(String, u64)>>,
    pub manual_exe: HashMap<String, String>,
}

/// Adds a game from a path or store link (used by the form, drag-and-drop and the importer).
pub fn add_game(app: &mut CatalyxApp, path: &str, name: Option<String>) {
    let path = path.trim().trim_matches('"').to_string();
    if path.is_empty() {
        return;
    }
    let is_link = path.contains("://");
    if !is_link && !std::path::Path::new(&path).exists() {
        app.toast_err(format!("File not found: {path}"));
        return;
    }
    let ext = std::path::Path::new(&path).extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    if !is_link && !matches!(ext.as_str(), "exe" | "bat" | "cmd" | "lnk" | "url") {
        app.toast_err("Games are .exe, .bat, .cmd, shortcut (.lnk/.url) files or store links");
        return;
    }
    if app.cfg.games.iter().any(|g| g.launch.eq_ignore_ascii_case(&path)) {
        app.toast_info("That game is already in your library");
        return;
    }
    let name = name
        .filter(|n| !n.trim().is_empty())
        .unwrap_or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(path.clone()));
    let id = app.cfg.new_id("game");
    let preset = app.cfg.default_preset.clone();
    app.cfg.games.push(Game { id: id.clone(), name: name.clone(), launch: path.clone(), preset, ..Default::default() });
    app.cfg.selected_game = app.cfg.games.len() - 1;
    app.mark_dirty();
    app.toast_ok(format!("Added {name}"));
    if !is_link {
        // Work out the real program behind a launcher or batch file while the person carries on.
        let p = path.clone();
        app.spawn(move || Some(Msg::Predicted { id, exe: launcher::predict_real_exe(&p).unwrap_or_default() }));
    }
}

pub fn show(app: &mut CatalyxApp, ui: &mut Ui) {
    let p = app.pal();
    app.drawn.insert("games");
    page(ui, "games", 1120.0, |ui| {
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            if ui.add(egui_components::Button::primary("Add a game")).clicked() {
                app.ui.games.adding = !app.ui.games.adding;
            }
            let scanning = app.ui.games.scanning;
            if ui
                .add_enabled(!scanning, egui_components::Button::secondary(if scanning { "Looking..." } else { "Find installed games" }))
                .on_hover_text("Reads what Steam, Epic Games and GOG have installed. Nothing is started or changed.")
                .clicked()
            {
                app.ui.games.scanning = true;
                app.spawn(|| Some(Msg::Installed(library::scan())));
            }
            ui.label(faint(ui, "or drop a game file on this window", 12.0));
        });

        if app.ui.games.adding {
            add_form(app, ui);
        }
        installed_list(app, ui);

        if app.cfg.games.is_empty() && !app.ui.games.adding {
            card(ui, |ui| {
                empty_state(ui, Ico::Gamepad, "No games yet", "Add one above, drop a file here, or find what is already installed.");
            });
        }
        let n = app.cfg.games.len();
        for i in 0..n {
            game_row(app, ui, i);
        }
        let _ = p;
    });
}

fn add_form(app: &mut CatalyxApp, ui: &mut Ui) {
    card_titled(
        ui,
        "Add a game",
        |_| {},
        |ui| {
            ui.label(faint(ui, "Pick the file you normally double-click: a game .exe, a .bat/.cmd launcher, a shortcut, or paste a store link such as steam://rungameid/620. CatalyX works out the real game program behind a launcher.", 12.0));
            ui.add_space(6.0);
            let w = ui.available_width();
            ui.label(dim(ui, "Name", 14.0));
            ui.add(egui_components::Input::new(&mut app.ui.games.name).placeholder("Shown in your library").width(w));
            ui.label(dim(ui, "Game file or link", 14.0));
            ui.horizontal(|ui| {
                ui.add(egui_components::Input::new(&mut app.ui.games.path).placeholder("C:\\Games\\Game\\launch.bat").width(w - 110.0));
                if ui.add(egui_components::Button::secondary("Browse...")).clicked() {
                    app.pick_file(PickFor::GameFile);
                }
            });
            ui.label(dim(ui, "Arguments (optional)", 14.0));
            ui.add(egui_components::Input::new(&mut app.ui.games.args).placeholder("-windowed").width(w));
            ui.add_space(6.0);
            ui.horizontal(|ui| {
                let ready = !app.ui.games.path.trim().is_empty();
                if ui.add_enabled(ready, egui_components::Button::primary("Add")).clicked() {
                    let path = std::mem::take(&mut app.ui.games.path);
                    let name = std::mem::take(&mut app.ui.games.name);
                    let args = std::mem::take(&mut app.ui.games.args);
                    add_game(app, &path, Some(name));
                    if !args.trim().is_empty() {
                        if let Some(g) = app.cfg.games.last_mut() {
                            g.args = args;
                        }
                    }
                    app.ui.games.adding = false;
                }
                if ui.add(egui_components::Button::ghost("Cancel")).clicked() {
                    app.ui.games.adding = false;
                }
            });
        },
    );
}

fn installed_list(app: &mut CatalyxApp, ui: &mut Ui) {
    let Some(list) = app.ui.games.installed.clone() else { return };
    let p = app.pal();
    let mut hide = false;
    card_titled(
        ui,
        &format!("Installed games found ({})", list.len()),
        |ui| {
            if ui.add(egui_components::Button::ghost("Hide").small()).clicked() {
                hide = true;
            }
        },
        |ui| {
            if list.is_empty() {
                ui.label(faint(ui, "No Steam, Epic or GOG games were found on this PC.", 12.0));
            }
            ScrollArea::vertical().id_salt("installed").max_height(280.0).auto_shrink([false, true]).show(ui, |ui| {
                for it in &list {
                    let have = app.cfg.games.iter().any(|g| g.launch.eq_ignore_ascii_case(&it.launch));
                    ui.horizontal(|ui| {
                        letter_tile(ui, &it.name, 28.0);
                        ui.vertical(|ui| {
                            ui.spacing_mut().item_spacing.y = 0.0;
                            ui.label(RichText::new(&it.name).size(14.0));
                            ui.label(faint(ui, if it.real_exe.is_empty() { it.folder.clone() } else { it.real_exe.clone() }, 12.0));
                        });
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            if have {
                                ui.label(RichText::new("Added").size(12.0).color(p.ok));
                            } else if ui.add(egui_components::Button::secondary("Add").small()).clicked() {
                                let launch = it.launch.clone();
                                let exe = it.real_exe.clone();
                                add_game(app, &launch, Some(it.name.clone()));
                                if let Some(g) = app.cfg.games.last_mut() {
                                    if g.launch == launch {
                                        g.real_exe = exe;
                                    }
                                }
                            }
                            chip(ui, it.store, Tone::Neutral);
                        });
                    });
                }
            });
            let missing: Vec<library::Installed> =
                list.iter().filter(|it| !app.cfg.games.iter().any(|g| g.launch.eq_ignore_ascii_case(&it.launch))).cloned().collect();
            if missing.len() > 1 {
                ui.add_space(6.0);
                if ui.add(egui_components::Button::secondary(format!("Add all {}", missing.len()))).clicked() {
                    for it in missing {
                        let launch = it.launch.clone();
                        add_game(app, &launch, Some(it.name.clone()));
                        if let Some(g) = app.cfg.games.last_mut() {
                            if g.launch == launch {
                                g.real_exe = it.real_exe;
                            }
                        }
                    }
                }
            }
        },
    );
    if hide {
        app.ui.games.installed = None;
    }
}

fn game_row(app: &mut CatalyxApp, ui: &mut Ui, i: usize) {
    let p = app.pal();
    let g = app.cfg.games[i].clone();
    let active = app.phase() != Phase::Idle;
    let selected = app.cfg.selected_game == i;
    egui::Frame::new()
        .fill(p.panel)
        .stroke(egui::Stroke::new(1.0, if selected { crate::gui::theme::alpha(p.accent, 90) } else { p.line }))
        .corner_radius(14)
        .inner_margin(20)
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.horizontal(|ui| {
                letter_tile(ui, &g.name, 44.0);
                ui.vertical(|ui| {
                    ui.spacing_mut().item_spacing.y = 2.0;
                    ui.label(bold(&g.name, 16.0));
                    ui.label(faint(ui, shorten(&g.launch, 70), 12.0));
                    ui.horizontal(|ui| {
                        if g.real_exe.is_empty() {
                            chip(ui, "program found on first launch", Tone::Neutral);
                        } else {
                            chip(ui, &g.real_exe, Tone::Accent);
                        }
                        let prof = if g.custom.is_some() { "Own settings".to_string() } else { app.cfg.preset(&g.preset).name.clone() };
                        chip(ui, &prof, Tone::Neutral);
                        if g.plays > 0 {
                            ui.label(faint(ui, format!("{} plays", g.plays), 12.0));
                        }
                    });
                });
                ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                    if icon_button(ui, Ico::Trash, "Remove from library").clicked() {
                        app.confirm = Some(crate::gui::pages::Confirm::RemoveGame(i));
                    }
                    if ui.add_enabled(!active, egui_components::Button::primary("Boost & play")).clicked() {
                        app.boost_game(Some(i));
                        app.tab = Tab::Boost;
                    }
                });
            });
            egui::CollapsingHeader::new(RichText::new("Details").size(12.0).color(p.dim)).id_salt(("game-details", &g.id)).show(ui, |ui| {
                details(app, ui, i);
            });
        });
}

fn details(app: &mut CatalyxApp, ui: &mut Ui, i: usize) {
    let g = app.cfg.games[i].clone();
    let w = ui.available_width().min(560.0);
    ui.spacing_mut().item_spacing.y = 6.0;
    ui.label(dim(ui, "Name", 14.0));
    let mut name = g.name.clone();
    if ui.add(egui_components::Input::new(&mut name).width(w)).changed() && !name.trim().is_empty() {
        app.cfg.games[i].name = name;
        app.mark_dirty();
    }
    ui.label(dim(ui, "Game file or link", 14.0));
    let mut launch = g.launch.clone();
    if ui.add(egui_components::Input::new(&mut launch).width(w)).changed() {
        app.cfg.games[i].launch = launch;
        app.mark_dirty();
    }
    ui.label(dim(ui, "Arguments", 14.0));
    let mut args = g.args.clone();
    if ui.add(egui_components::Input::new(&mut args).placeholder("(none)").width(w)).changed() {
        app.cfg.games[i].args = args;
        app.mark_dirty();
    }
    ui.label(dim(ui, "Real game program (what gets the boost)", 14.0));
    // Candidates are read from the game's folder once, off the UI thread.
    let folder = game_folder(&g);
    if !folder.is_empty() && !app.ui.games.candidates.contains_key(&g.id) {
        app.ui.games.candidates.insert(g.id.clone(), Vec::new());
        let id = g.id.clone();
        let scan = folder.clone();
        app.spawn(move || Some(Msg::Candidates { list: launcher::candidates(&scan), id }));
    }
    let mut options: Vec<String> = vec!["Find it automatically on launch".into()];
    let cands = app.ui.games.candidates.get(&g.id).cloned().unwrap_or_default();
    for (n, size) in cands.iter().take(40) {
        options.push(format!("{n}  ({:.0} MB)", *size as f64 / 1048576.0));
    }
    let mut cur =
        if g.real_exe.is_empty() { 0 } else { cands.iter().take(40).position(|(n, _)| *n == g.real_exe).map(|x| x + 1).unwrap_or(0) };
    if dropdown(ui, &format!("real-{}", g.id), w, &mut cur, &options) {
        app.cfg.games[i].real_exe = if cur == 0 { String::new() } else { cands[cur - 1].0.clone() };
        app.mark_dirty();
    }
    let mut manual = app.ui.games.manual_exe.get(&g.id).cloned().unwrap_or_else(|| g.real_exe.clone());
    let mut use_manual = false;
    ui.horizontal(|ui| {
        ui.add(egui_components::Input::new(&mut manual).placeholder("or type a program name, e.g. game-win64.exe").width(w - 110.0));
        if ui.add(egui_components::Button::secondary("Use it")).clicked() {
            use_manual = true;
        }
    });
    if use_manual {
        app.cfg.games[i].real_exe = crate::config::file_name_lower(&manual);
        app.mark_dirty();
    }
    app.ui.games.manual_exe.insert(g.id.clone(), manual);
    ui.horizontal(|ui| {
        if ui.add(egui_components::Button::secondary("Edit this game's settings")).clicked() {
            if app.cfg.games[i].custom.is_none() {
                let s = app.cfg.settings_for(Some(&app.cfg.games[i]));
                app.cfg.games[i].custom = Some(s);
                app.mark_dirty();
            }
            app.ui.prof.target = EditTarget::Game(i);
            app.tab = Tab::Profiles;
        }
        if !folder.is_empty() && ui.add(egui_components::Button::ghost("Open folder")).clicked() {
            let _ = std::process::Command::new("explorer").arg(&folder).spawn();
        }
    });
    let _ = vec2(0.0, 0.0);
}

fn game_folder(g: &Game) -> String {
    if g.launch.contains("://") {
        return String::new();
    }
    std::path::Path::new(&g.launch).parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default()
}

fn shorten(s: &str, n: usize) -> String {
    let c: Vec<char> = s.chars().collect();
    if c.len() <= n {
        s.to_string()
    } else {
        let head: String = c[..n / 3].iter().collect();
        let tail: String = c[c.len() - (n - n / 3 - 1)..].iter().collect();
        format!("{head}\u{2026}{tail}")
    }
}
