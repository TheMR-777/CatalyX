//! The seven pages, plus what they share: per-page state, confirmation dialogs and the results of
//! file pickers.

pub mod apps;
pub mod boost;
pub mod games;
pub mod profiles;
pub mod services_page;
pub mod settings;
pub mod tuning;

use super::icons::{self, Ico};
use super::widgets::{bold, faint};
use super::{CatalyxApp, PickFor};
use crate::config::{Config, Game, ProcAction};
use egui::Context;
use egui_components::{AlertChoice, AlertDialog, Button, Dialog};

#[derive(Default)]
pub struct PageState {
    pub games: games::GamesUi,
    pub apps: apps::AppsUi,
    pub svc: services_page::SvcUi,
    pub prof: profiles::ProfUi,
    pub tune: tuning::TuneUi,
}

pub enum Confirm {
    Welcome,
    /// The summary shown before a boost starts: the game to start, or none for gaming mode alone.
    Boost {
        idx: Option<usize>,
    },
    RemoveGame(usize),
    ResetConfig,
    EndApp {
        exe: String,
        name: String,
    },
    GpuTest,
    ClearApps,
    ClearServices,
}

/// Selects the catalog's gentle suggestions (for all games). Choices the person already made
/// are never overwritten.
pub fn apply_suggestions(app: &mut CatalyxApp) {
    for (exe, a) in crate::catalog::suggested_apps() {
        let cur = app.cfg.apps.get(&exe).copied().unwrap_or(ProcAction::Keep);
        if cur == ProcAction::Keep {
            app.cfg.apps.insert(exe, a);
        }
    }
    for s in crate::catalog::KNOWN_SERVICES.iter().filter(|s| s.recommended) {
        let key = app.cfg.services.keys().find(|k| k.eq_ignore_ascii_case(s.name)).cloned().unwrap_or_else(|| s.name.to_string());
        app.cfg.services.insert(key, true);
    }
    app.mark_dirty();
}

/// Sample content for screenshots of a fresh install.
pub fn demo_data(cfg: &mut Config) {
    for (name, exe, preset) in [
        ("Stellar Drift", "stellardrift-win64.exe", "balanced"),
        ("Iron Vale", "ironvale.exe", "max"),
        ("Cozy Farm", "cozyfarm.exe", "gentle"),
    ] {
        cfg.games.push(Game {
            id: cfg.new_id("game") + name,
            name: name.into(),
            launch: format!("C:\\Games\\{name}\\launch.exe"),
            real_exe: exe.into(),
            preset: preset.into(),
            plays: 3,
            ..Default::default()
        });
    }
    cfg.apps.insert("discord.exe".into(), ProcAction::Keep);
}

pub fn picked(app: &mut CatalyxApp, what: PickFor, path: String) {
    match what {
        PickFor::GameFile => {
            app.ui.games.path = path.clone();
            if app.ui.games.name.trim().is_empty() {
                app.ui.games.name = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            }
        }
        PickFor::ExportConfig => match serde_json::to_string_pretty(&app.cfg)
            .map_err(|e| e.to_string())
            .and_then(|s| std::fs::write(&path, s).map_err(|e| e.to_string()))
        {
            Ok(()) => app.toast_ok(format!("Settings saved to {path}")),
            Err(e) => app.toast_err(format!("Could not save: {e}")),
        },
        PickFor::ImportConfig => match std::fs::read_to_string(&path)
            .map_err(|e| e.to_string())
            .and_then(|t| serde_json::from_str::<Config>(&t).map_err(|e| format!("not a CatalyX settings file: {e}")))
        {
            Ok(c) => {
                let (g, p) = (c.games.len(), c.presets.len());
                app.cfg = c;
                app.mark_dirty();
                app.toast_ok(format!("Loaded {g} games and {p} presets"));
            }
            Err(e) => app.toast_err(e),
        },
    }
}

pub fn dialogs(app: &mut CatalyxApp, ctx: &Context) {
    let Some(c) = app.confirm.take() else { return };
    let mut open = true;
    match &c {
        Confirm::Welcome => {
            let mut done = false;
            let mut use_suggestions = false;
            Dialog::new("Welcome to CatalyX").width(500.0).show(ctx, &mut open, |ui| {
                ui.label(bold("One button. Nothing running behind your back.", 16.0));
                ui.add_space(6.0);
                for line in [
                    "Add your games (or let CatalyX find the ones you already have installed).",
                    "Press Boost & play: your power plan, CPU, apps and services are tuned for the game, and put back when it closes.",
                    "Everything it changes is written down first, so even a crash can be undone with Restore.",
                    "No ads, no updater, no telemetry, no tray icon, no background service.",
                ] {
                    ui.horizontal_top(|ui| {
                        ui.label("•");
                        ui.label(faint(ui, line, 12.0));
                    });
                }
                ui.add_space(8.0);
                ui.label(faint(ui, "Nothing is closed or stopped until you choose it. You can start from CatalyX's suggestions - browsers, chat and sync clients are only lowered, helpers are paused, only updaters and Windows extras are closed, and nothing that holds your work is ever closed - or pick everything yourself.", 12.0));
                ui.add_space(14.0);
                ui.horizontal(|ui| {
                    if ui.add(Button::primary("Use the suggestions").large()).clicked() {
                        use_suggestions = true;
                        done = true;
                    }
                    if ui.add(Button::secondary("I'll choose myself").large()).clicked() {
                        done = true;
                    }
                });
            });
            if use_suggestions {
                apply_suggestions(app);
            }
            if done || !open {
                app.cfg.onboarded = true;
                app.mark_dirty();
                return;
            }
        }
        Confirm::Boost { idx } => {
            let game = idx.and_then(|i| app.cfg.games.get(i)).cloned();
            let p = app.pal();
            let (lines, off) = boost::plan_lines(app, game.as_ref());
            let title = match &game {
                Some(g) => format!("Boost & play {}?", g.name),
                None => "Start gaming mode?".to_string(),
            };
            let verb = if game.is_some() { "Boost & play" } else { "Boost" };
            let mut dont = app.boost_dont_ask;
            let (mut go, mut cancel) = (false, false);
            Dialog::new(title).width(460.0).show(ctx, &mut open, |ui| {
                ui.label(faint(ui, "This is what will change on this PC. Everything is put back when the game closes, or when you press Restore.", 12.0));
                ui.add_space(10.0);
                for text in &lines {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 22.0), egui::Sense::hover());
                        icons::paint_at(ui.painter(), Ico::Check, rect.center(), 13.0, p.ok);
                        ui.label(egui::RichText::new(text).size(14.0));
                    });
                }
                if off > 0 {
                    ui.add_space(4.0);
                    ui.label(faint(ui, format!("{off} more option(s) are off"), 12.0));
                }
                ui.add_space(14.0);
                ui.add(egui_components::Checkbox::new(&mut dont, "Don't ask me again"));
                ui.add_space(10.0);
                ui.horizontal(|ui| {
                    if ui.add(Button::primary(verb).large()).clicked() {
                        go = true;
                    }
                    if ui.add(Button::secondary("Cancel").large()).clicked() {
                        cancel = true;
                    }
                });
            });
            app.boost_dont_ask = dont;
            if go {
                if dont {
                    // The person can bring the summary back in Settings > Behaviour.
                    app.cfg.confirm_boost = false;
                    app.mark_dirty();
                }
                app.boost_game(*idx);
                return;
            }
            if cancel || !open {
                return;
            }
        }        Confirm::RemoveGame(i) => {
            let name = app.cfg.games.get(*i).map(|g| g.name.clone()).unwrap_or_default();
            match AlertDialog::new(format!("Remove {name}?")).description("Only CatalyX's entry is removed. The game itself is not touched.").confirm_label("Remove").danger().show(ctx, &mut open) {
                Some(AlertChoice::Confirm) => {
                    if *i < app.cfg.games.len() {
                        app.cfg.games.remove(*i);
                        app.cfg.selected_game = app.cfg.selected_game.min(app.cfg.games.len().saturating_sub(1));
                        app.mark_dirty();
                    }
                    return;
                }
                Some(AlertChoice::Cancel) => return,
                None => {}
            }
        }
        Confirm::ResetConfig => match AlertDialog::new("Reset everything?").description("Forgets all games, rules and presets and starts again with the suggestions. This cannot be undone.").confirm_label("Reset").danger().show(ctx, &mut open) {
            Some(AlertChoice::Confirm) => {
                app.cfg = Config::default();
                app.cfg.onboarded = true;
                app.mark_dirty();
                app.toast_ok("Settings reset");
                return;
            }
            Some(AlertChoice::Cancel) => return,
            None => {}
        },
        Confirm::EndApp { exe, name } => {
            match AlertDialog::new(format!("End {name} now?")).description("Ends every copy of the program at once. Anything unsaved in it is lost, and CatalyX cannot get it back.").confirm_label("End now").danger().show(ctx, &mut open) {
                Some(AlertChoice::Confirm) => {
                    let snap = app.snap.lock().map(|s| s.clone()).unwrap_or_default();
                    let n = snap.procs.iter().filter(|p| p.name_lower == *exe).map(|p| crate::procs::terminate(p.pid)).filter(|ok| *ok).count();
                    app.toast_info(format!("Ended {n} process(es) of {name}"));
                    return;
                }
                Some(AlertChoice::Cancel) => return,
                None => {}
            }
        }
        Confirm::GpuTest => {
            match AlertDialog::new("Test the graphics card tuning?").description("Applies the Safe step for about 3 seconds, reads it back from the driver, then puts the old values back. Stops at once if the card is warm.").confirm_label("Run test").show(ctx, &mut open) {
                Some(AlertChoice::Confirm) => {
                    let limit = app.cfg.gpu_temp_limit_c;
                    app.spawn(move || Some(match crate::gpu::roundtrip(crate::config::GpuLevel::Safe, limit) {
                        Ok(s) => super::Msg::Info(s),
                        Err(e) => super::Msg::Error(format!("Graphics test: {e}")),
                    }));
                    return;
                }
                Some(AlertChoice::Cancel) => return,
                None => {}
            }
        }
        Confirm::ClearApps => match AlertDialog::new("Set every app back to Keep?").description("Clears all app rules (for all games). You can bring the suggestions back with one click.").confirm_label("Clear rules").show(ctx, &mut open) {
            Some(AlertChoice::Confirm) => {
                app.cfg.apps.clear();
                app.mark_dirty();
                return;
            }
            Some(AlertChoice::Cancel) => return,
            None => {}
        },
        Confirm::ClearServices => match AlertDialog::new("Keep every service running?").description("Clears all service rules (for all games).").confirm_label("Clear rules").show(ctx, &mut open) {
            Some(AlertChoice::Confirm) => {
                for v in app.cfg.services.values_mut() {
                    *v = false;
                }
                app.mark_dirty();
                return;
            }
            Some(AlertChoice::Cancel) => return,
            None => {}
        },
    }
    if open {
        app.confirm = Some(c);
    }
}
