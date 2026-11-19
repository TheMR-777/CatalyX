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
use super::{CatalyxApp, EditTarget, PickFor};
use crate::app::Intent;
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
    /// The summary shown before a boost starts: the game to start, or none for gaming mode alone,
    /// and the core's plan for exactly that game.
    Boost {
        game: Option<crate::app::GameId>,
        plan: crate::app::Plan,
    },
    RemoveGame {
        id: String,
        name: String,
    },
    ResetConfig,
    EndApp {
        exe: String,
        name: String,
    },
    GpuTest,
    /// A person's own preset, by id: deleting it moves its games (and the default) back to a
    /// built-in one.
    DeletePreset {
        id: String,
        name: String,
    },
    ClearApps,
    ClearServices,
}

/// A file dialog answered: a game file fills the form, a settings file goes to the core.
pub fn picked(app: &mut CatalyxApp, what: PickFor, path: String) {
    match what {
        PickFor::GameFile => {
            app.ui.games.path = path.clone();
            if app.ui.games.name.trim().is_empty() {
                app.ui.games.name = std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            }
        }
        PickFor::ExportConfig => app.send(Intent::ExportConfig(path)),
        PickFor::ImportConfig => app.send(Intent::ImportConfig(path)),
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
            if done || !open {
                // One intent: the welcome is over, taking the suggestions when that was the choice.
                app.send(Intent::FinishWelcome { use_suggestions });
                return;
            }
        }
        Confirm::Boost { game: game_id, plan } => {
            let game = game_id.as_ref().and_then(|id| app.v.config.games.iter().find(|g| g.id == *id)).cloned();
            let p = app.pal();
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
                for text in &plan.lines {
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 10.0;
                        let (rect, _) = ui.allocate_exact_size(egui::vec2(14.0, 22.0), egui::Sense::hover());
                        icons::paint_at(ui.painter(), Ico::Check, rect.center(), 13.0, p.ok);
                        ui.label(egui::RichText::new(text).size(14.0));
                    });
                }
                if plan.off > 0 {
                    ui.add_space(4.0);
                    ui.label(faint(ui, format!("{} more option(s) are off", plan.off), 12.0));
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
                // "Don't ask me again" is applied by the core (the person can bring the summary back in
                // Settings > Behaviour).
                app.send(Intent::ConfirmBoost { game: game_id.clone(), dont_ask_again: dont });
                return;
            }
            if cancel || !open {
                app.send(Intent::CancelBoost);
                return;
            }
        }        Confirm::RemoveGame { id, name } => {
            match AlertDialog::new(format!("Remove {name}?")).description("Only CatalyX's entry is removed. The game itself is not touched.").confirm_label("Remove").danger().show(ctx, &mut open) {
                Some(AlertChoice::Confirm) => {
                    app.send(Intent::RemoveGame { id: id.clone() });
                    return;
                }
                Some(AlertChoice::Cancel) => return,
                None => {}
            }
        }
        Confirm::ResetConfig => match AlertDialog::new("Reset everything?").description("Forgets all games, rules and presets and starts again with the suggestions. This cannot be undone.").confirm_label("Reset").danger().show(ctx, &mut open) {
            Some(AlertChoice::Confirm) => {
                app.send(Intent::ResetConfig);
                return;
            }
            Some(AlertChoice::Cancel) => return,
            None => {}
        },
        Confirm::EndApp { exe, name } => {
            match AlertDialog::new(format!("End {name} now?")).description("Ends every copy of the program at once. Anything unsaved in it is lost, and CatalyX cannot get it back.").confirm_label("End now").danger().show(ctx, &mut open) {
                Some(AlertChoice::Confirm) => {
                    // Off the UI thread, with a fresh scan and an identity check per process.
                    app.send(Intent::EndApp { exe: exe.clone(), name: name.clone() });
                    return;
                }
                Some(AlertChoice::Cancel) => return,
                None => {}
            }
        }
        Confirm::GpuTest => {
            match AlertDialog::new("Test the graphics card tuning?").description("Applies the Safe step for about 3 seconds, reads it back from the driver, then puts the old values back. Stops at once if the card is warm.").confirm_label("Run test").show(ctx, &mut open) {
                Some(AlertChoice::Confirm) => {
                    app.send(Intent::TestGraphicsCard);
                    return;
                }
                Some(AlertChoice::Cancel) => return,
                None => {}
            }
        }
        Confirm::DeletePreset { id, name } => match AlertDialog::new(format!("Delete {name}?")).description("Games using it go back to a built-in preset. This cannot be undone.").confirm_label("Delete").danger().show(ctx, &mut open) {
            Some(AlertChoice::Confirm) => {
                app.send(Intent::DeletePreset { id: id.clone() });
                // The core moves its games and the default to a built-in preset; edit that one next.
                app.ui.prof.target = EditTarget::Preset(app.v.config.default_preset.clone());
                return;
            }
            Some(AlertChoice::Cancel) => return,
            None => {}
        },
        Confirm::ClearApps => match AlertDialog::new("Set every app back to Keep?").description("Clears all app rules (for all games). You can bring the suggestions back with one click.").confirm_label("Clear rules").show(ctx, &mut open) {
            Some(AlertChoice::Confirm) => {
                app.send(Intent::ClearAppRules);
                return;
            }
            Some(AlertChoice::Cancel) => return,
            None => {}
        },
        Confirm::ClearServices => match AlertDialog::new("Keep every service running?").description("Clears all service rules (for all games).").confirm_label("Clear rules").show(ctx, &mut open) {
            Some(AlertChoice::Confirm) => {
                app.send(Intent::ClearServiceRules);
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
