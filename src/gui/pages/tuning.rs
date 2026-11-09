//! Tuning: power plan, CPU performance and graphics card clocks, with live readings from the
//! processor and the driver as proof that a setting does what it says.

use crate::config::{CpuLevel, GpuLevel, PowerChoice};
use crate::gui::icons::Ico;
use crate::gui::pages::Confirm;
use crate::gui::theme::alpha;
use crate::gui::widgets::*;
use crate::gui::CatalyxApp;
use crate::{gpu, power};
use egui::{vec2, Align, Layout, RichText, Ui};

#[derive(Default)]
pub struct TuneUi {
    /// Which preset the CPU / power / graphics choices below edit.
    pub preset: usize,
    pub chosen: bool,
}

pub fn show(app: &mut CatalyxApp, ui: &mut Ui) {
    if !app.ui.tune.chosen || app.ui.tune.preset >= app.cfg.presets.len() {
        app.ui.tune.chosen = true;
        app.ui.tune.preset = app.cfg.presets.iter().position(|p| p.id == app.cfg.default_preset).unwrap_or(0);
    }
    page(ui, "tuning", 1120.0, |ui| {
        let total = ui.available_width();
        // Which profile do these choices edit?
        ui.horizontal(|ui| {
            ui.spacing_mut().item_spacing.x = 12.0;
            ui.label(dim(ui, "Tune profile", 14.0));
            let names: Vec<String> = app.cfg.presets.iter().map(|p| p.name.clone()).collect();
            let mut i = app.ui.tune.preset;
            if dropdown(ui, "tune-profile", 200.0, &mut i, &names) {
                app.ui.tune.preset = i;
                app.ui.tune.chosen = true;
            }
            ui.label(faint(ui, "The CPU, power and graphics choices below are saved into this profile.", 12.0));
        });
        if total > 900.0 {
            let w = (total - 16.0) / 2.0;
            ui.horizontal_top(|ui| {
                ui.spacing_mut().item_spacing.x = 16.0;
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing.y = 16.0;
                    cpu_card(app, ui);
                    power_card(app, ui);
                });
                ui.vertical(|ui| {
                    ui.set_width(w);
                    ui.spacing_mut().item_spacing.y = 16.0;
                    gpu_card(app, ui);
                });
            });
        } else {
            cpu_card(app, ui);
            power_card(app, ui);
            gpu_card(app, ui);
        }
    });
}

fn cpu_card(app: &mut CatalyxApp, ui: &mut Ui) {
    let p = app.pal();
    let snap = app.snap.lock().map(|s| s.clone()).unwrap_or_default();
    let idx = app.ui.tune.preset;
    card_titled(
        ui,
        "Processor",
        |ui| {
            ui.label(faint(ui, format!("{} threads", snap.cores), 12.0));
        },
        |ui| {
            ui.label(RichText::new(if snap.cpu_name.is_empty() { "Reading...".into() } else { snap.cpu_name.clone() }).size(14.0));
            ui.add_space(4.0);
            let half = (ui.available_width() - 16.0) / 3.0;
            ui.horizontal(|ui| {
                stat_tile(ui, half, &ghz(snap.cpu_mhz), "clock now");
                stat_tile(ui, half, &ghz(snap.cpu_base_mhz), "rated base");
                stat_tile(ui, half, &format!("{:.0}%", snap.cpu_total), "load");
            });
            let turbo = if snap.cpu_base_mhz > 0 && snap.cpu_mhz > snap.cpu_base_mhz + 30 {
                format!(
                    "Running {:.0}% above its rated speed (turbo is working).",
                    (snap.cpu_mhz as f32 / snap.cpu_base_mhz as f32 - 1.0) * 100.0
                )
            } else {
                "At or near its rated speed right now (idle CPUs rest; turbo shows under load).".to_string()
            };
            ui.label(faint(ui, turbo, 12.0));
            ui.add_space(6.0);
            hairline(ui);
            let level = app.cfg.presets[idx].settings.cpu;
            setting_row(ui, "CPU performance", level.hint(), |ui| {
                let mut i = CpuLevel::ALL.iter().position(|x| *x == level).unwrap_or(0);
                if dropdown(ui, "tune-cpu", CONTROL_W, &mut i, &CpuLevel::ALL.iter().map(|x| x.label().to_string()).collect::<Vec<_>>()) {
                    app.cfg.presets[idx].settings.cpu = CpuLevel::ALL[i];
                    app.mark_dirty();
                }
            });
            ui.add_space(4.0);
            egui::Frame::new().fill(p.well).stroke(egui::Stroke::new(1.0, p.line)).corner_radius(10).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                ui.horizontal_top(|ui| {
                let (r, _) = ui.allocate_exact_size(vec2(18.0, 18.0), egui::Sense::hover());
                crate::gui::icons::paint_at(ui.painter(), Ico::Info, r.center(), 16.0, p.accent);
                ui.vertical(|ui| {
                    ui.label(RichText::new("What this really does").size(14.0).color(p.text));
                    ui.label(faint(ui, "Holds turbo at its highest allowed level, keeps every core awake and tells Windows to favour speed over saving power. Those are the processor's own limits, so it is safe and fully undone afterwards. Raising the multiplier or voltage (true overclocking) is not done here: it needs an unlocked CPU and BIOS tools, and can damage hardware.", 12.0));
                });
            });
        });
        },
    );
}

fn power_card(app: &mut CatalyxApp, ui: &mut Ui) {
    let idx = app.ui.tune.preset;
    let mut refresh = false;
    card_titled(
        ui,
        "Power plan",
        |ui| {
            if icon_button(ui, Ico::Refresh, "Read the plans again").clicked() {
                refresh = true;
            }
        },
        |ui| {
            let choice = app.cfg.presets[idx].settings.power;
            setting_row(ui, "Power plan", choice.hint(), |ui| {
                let mut i = PowerChoice::ALL.iter().position(|x| *x == choice).unwrap_or(0);
                if dropdown(
                    ui,
                    "tune-power",
                    CONTROL_W,
                    &mut i,
                    &PowerChoice::ALL.iter().map(|x| x.label().to_string()).collect::<Vec<_>>(),
                ) {
                    app.cfg.presets[idx].settings.power = PowerChoice::ALL[i];
                    app.mark_dirty();
                }
            });
            hairline(ui);
            ui.label(faint(ui, "Plans on this PC", 12.0));
            let plans = app.plans.clone();
            for pl in &plans {
                ui.horizontal(|ui| {
                    let p = app.pal();
                    let (r, _) = ui.allocate_exact_size(vec2(16.0, 20.0), egui::Sense::hover());
                    ui.painter().circle_stroke(r.center(), 6.0, egui::Stroke::new(1.3, if pl.active { p.accent } else { p.faint }));
                    if pl.active {
                        ui.painter().circle_filled(r.center(), 3.0, p.accent);
                    }
                    ui.label(RichText::new(&pl.name).size(14.0));
                    ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                        if pl.active {
                            chip(ui, "Active", Tone::Ok);
                        } else if ui
                            .add(egui_components::Button::ghost("Use now").small())
                            .on_hover_text("Switch Windows to this plan right now")
                            .clicked()
                        {
                            if power::set_active(&pl.guid) {
                                app.refresh_hardware();
                            } else {
                                app.toast_err("Windows refused the change");
                            }
                        }
                    });
                });
            }
            ui.add_space(4.0);
            if ui
                .add(egui_components::Button::secondary("Create / refresh the CatalyX plan"))
                .on_hover_text("Builds a copy of your current plan with CatalyX's settings; your own plan is never edited.")
                .clicked()
            {
                let level = app.cfg.presets[idx].settings.cpu;
                let cur = power::active().unwrap_or_default();
                if power::ensure_boost_plan(if level == CpuLevel::Off { CpuLevel::Boost } else { level }, &cur) {
                    app.toast_ok("CatalyX plan is ready");
                } else {
                    app.toast_err("Windows would not create the plan");
                }
                app.refresh_hardware();
            }
        },
    );
    if refresh {
        app.refresh_hardware();
    }
}

fn gpu_card(app: &mut CatalyxApp, ui: &mut Ui) {
    let p = app.pal();
    let idx = app.ui.tune.preset;
    let st = app.gpu.clone();
    let live = app.live.clone();
    let limit = app.cfg.gpu_temp_limit_c;
    let app_admin = app.admin;
    let mut refresh = false;
    card_titled(
        ui,
        "Graphics card",
        |ui| {
            if icon_button(ui, Ico::Refresh, "Read the card again").clicked() {
                refresh = true;
            }
        },
        |ui| {
            let Some(st) = st else {
                ui.label(faint(ui, "Reading the graphics card...", 12.0));
                return;
            };
            ui.horizontal(|ui| {
                ui.label(RichText::new(if st.name.is_empty() { "No graphics card found".into() } else { st.name.clone() }).size(14.0));
                chip(ui, &st.vendor, Tone::Neutral);
            });
            // Live readings straight from the driver.
            let third = (ui.available_width() - 16.0) / 3.0;
            let l = live.unwrap_or_default();
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                stat_tile(ui, third, &l.temp_c.map(|t| format!("{t} \u{00b0}C")).unwrap_or("-".into()), "temperature");
                stat_tile(ui, third, &l.core_mhz.map(|m| format!("{m} MHz")).unwrap_or("-".into()), "core clock");
                stat_tile(ui, third, &l.util_pct.map(|u| format!("{u}%")).unwrap_or("-".into()), "in use");
            });
            ui.horizontal(|ui| {
                stat_tile(ui, third, &l.power_w.map(|w| format!("{w:.0} W")).unwrap_or("-".into()), "power draw");
                stat_tile(ui, third, &l.mem_mhz.map(|m| format!("{m} MHz")).unwrap_or("-".into()), "memory clock");
                stat_tile(
                    ui,
                    third,
                    &match (l.vram_used, l.vram_total) {
                        (Some(u), Some(t)) => format!("{:.1} / {:.0} GB", u as f64 / 1e9, t as f64 / 1e9),
                        _ => "-".into(),
                    },
                    "video memory",
                );
            });
            if !st.supported {
                ui.add_space(6.0);
                ui.label(faint(
                    ui,
                    format!(
                        "Clock tuning is not available: {}",
                        if st.note.is_empty() { "this card does not allow offsets" } else { &st.note }
                    ),
                    12.0,
                ));
                ui.label(faint(
                    ui,
                    "Clock offsets need an NVIDIA card whose driver allows them. Intel and AMD cards are not supported yet.",
                    12.0,
                ));
                return;
            }
            hairline(ui);
            let half = (ui.available_width() - 8.0) / 2.0;
            ui.horizontal(|ui| {
                stat_tile(ui, half, &format!("{:+} MHz", st.core.current_khz / 1000), "core offset");
                stat_tile(ui, half, &format!("{:+} MHz", st.mem.current_khz / 1000), "memory offset");
            });
            ui.label(faint(
                ui,
                format!(
                    "The driver allows core {} to {} MHz and memory {} to {} MHz.",
                    st.core.min_khz / 1000,
                    st.core.max_khz / 1000,
                    st.mem.min_khz / 1000,
                    st.mem.max_khz / 1000
                ),
                12.0,
            ));
            let (sc, sm) = gpu::planned(&st, GpuLevel::Safe);
            let (bc, bm) = gpu::planned(&st, GpuLevel::Balanced);
            let level = app.cfg.presets[idx].settings.gpu;
            let hint = match level {
                GpuLevel::Off => "Clocks untouched".to_string(),
                GpuLevel::Safe => format!("Core +{} / memory +{} MHz", sc / 1000, sm / 1000),
                GpuLevel::Balanced => format!("Core +{} / memory +{} MHz", bc / 1000, bm / 1000),
            };
            setting_row(ui, "Graphics tuning", &hint, |ui| {
                let mut i = GpuLevel::ALL.iter().position(|x| *x == level).unwrap_or(0);
                if dropdown(ui, "tune-gpu", CONTROL_W, &mut i, &GpuLevel::ALL.iter().map(|x| x.label().to_string()).collect::<Vec<_>>()) {
                    app.cfg.presets[idx].settings.gpu = GpuLevel::ALL[i];
                    app.mark_dirty();
                }
            });
            hairline(ui);
            setting_row(ui, "Heat limit", "Clocks go back to normal at once above this temperature", |ui| {
                let limits = [75u32, 80, 83, 87];
                let mut i = limits.iter().position(|x| *x == app.cfg.gpu_temp_limit_c).unwrap_or(2);
                if dropdown(ui, "tune-heat", CONTROL_W, &mut i, &limits.iter().map(|x| format!("{x} \u{00b0}C")).collect::<Vec<_>>()) {
                    app.cfg.gpu_temp_limit_c = limits[i];
                    app.mark_dirty();
                }
            });
            if !app_admin {
                ui.label(RichText::new("Changing graphics clocks needs administrator rights: restart CatalyX as administrator (Settings, or the button on the Boost page).").size(12.0).color(p.warn));
            }
            ui.add_space(4.0);
            ui.horizontal(|ui| {
                if ui
                    .add(egui_components::Button::secondary("Test now (3 seconds)"))
                    .on_hover_text("Applies the Safe step briefly, reads it back from the driver, then undoes it.")
                    .clicked()
                {
                    app.confirm = Some(Confirm::GpuTest);
                }
            });
            ui.label(faint(
                ui,
                "Applied only while a game runs, always undone, and backed off if the card heats up (limit {limit} \u{00b0}C)."
                    .replace("{limit}", &limit.to_string()),
                12.0,
            ));
            let _ = alpha(p.accent, 0);
        },
    );
    if refresh {
        app.refresh_hardware();
    }
}
