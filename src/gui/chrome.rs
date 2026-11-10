//! The window's own chrome: the title bar (brand, page title, live meters, window buttons) and the sidebar.

use super::*;

impl CatalyxApp {
    pub(super) fn sidebar(&mut self, ui: &mut egui::Ui, compact: bool) {
        let p = self.pal();
        ui.add_space(6.0);
        let groups: [(&str, &[Tab]); 3] = [
            ("Play", &[Tab::Boost, Tab::Games]),
            ("Control", &[Tab::Apps, Tab::Services, Tab::Profiles]),
            ("System", &[Tab::Tuning, Tab::Settings]),
        ];
        let pending = self.status.lock().map(|s| s.pending_journal && s.phase == Phase::Idle).unwrap_or(false);
        for (name, tabs) in groups {
            if !compact {
                ui.add_space(8.0);
                ui.horizontal(|ui| {
                    ui.add_space(8.0);
                    ui.label(faint(ui, name.to_uppercase(), 12.0));
                });
            } else {
                ui.add_space(8.0);
            }
            for t in tabs {
                let (_, label, _, icon) = TABS.iter().find(|(tt, ..)| tt == t).copied().unwrap();
                let badge = (*t == Tab::Boost && pending).then_some("!");
                let r = if compact {
                    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 38.0), egui::Sense::click());
                    let sel = self.tab == *t;
                    if sel {
                        ui.painter().rect_filled(rect, 8, p.panel_hi);
                        ui.painter().rect_filled(egui::Rect::from_min_size(rect.left_top() + vec2(0.0, 9.0), vec2(3.0, 20.0)), 2, p.accent);
                    } else if resp.hovered() {
                        ui.painter().rect_filled(rect, 8, theme::alpha(p.panel_hi, 140));
                    }
                    icons::paint_at(ui.painter(), icon, rect.center(), 19.0, if sel { p.accent } else { p.dim });
                    resp.on_hover_text(label)
                } else {
                    widgets::nav_item(ui, icon, label, self.tab == *t, badge)
                };
                if r.clicked() {
                    self.tab = *t;
                }
            }
        }
        // Bottom: status and quick light/dark switch.
        ui.with_layout(Layout::bottom_up(Align::Min), |ui| {
            ui.add_space(12.0);
            let st = self.status.lock().map(|s| (s.phase, s.dry_run)).unwrap_or((Phase::Idle, false));
            egui::Frame::new().fill(p.well).corner_radius(10).inner_margin(egui::Margin::symmetric(12, 10)).show(ui, |ui| {
                ui.set_width(ui.available_width());
                if compact {
                    let c = if st.0.active() { p.accent } else { p.faint };
                    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 14.0), egui::Sense::hover());
                    ui.painter().circle_filled(rect.center(), 5.0, c);
                } else {
                    ui.horizontal(|ui| {
                        let c = if st.0.active() { p.accent } else { p.faint };
                        let (rect, _) = ui.allocate_exact_size(vec2(10.0, 14.0), egui::Sense::hover());
                        ui.painter().circle_filled(rect.center(), 4.0, c);
                        let short = match st.0 {
                            Phase::Idle => "Normal mode",
                            Phase::Boosting => "Boosting...",
                            Phase::Finding => "Finding game...",
                            Phase::Running | Phase::Boosted => "Boosted",
                            Phase::Restoring => "Restoring...",
                        };
                        ui.label(RichText::new(short).size(14.0).color(if st.0.active() { p.text } else { p.dim }));
                        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                            let (i, tip) = if self.dark_mode() { (Ico::Sun, "Switch to light") } else { (Ico::Moon, "Switch to dark") };
                            if widgets::icon_button(ui, i, tip).clicked() {
                                self.cfg.appearance = if self.dark_mode() { "light".into() } else { "dark".into() };
                                self.mark_dirty();
                            }
                        });
                    });
                    ui.add_space(2.0);
                    if self.admin {
                        widgets::chip(ui, "Administrator", widgets::Tone::Ok);
                    } else {
                        widgets::chip(ui, "Limited rights", widgets::Tone::Warn);
                    }
                }
            });
        });
    }

    /// The window's title bar and the page header in one. Left to right: the brand (over the
    /// sidebar, whose colour continues up through the bar), the current page, then the live
    /// meters and the window buttons. Dragging its empty space moves the window.
    pub(super) fn title_bar(&mut self, ui: &mut egui::Ui, compact: bool, nav_w: f32) {
        let p = self.pal();
        let (_, title, sub, _) = TABS.iter().find(|(t, ..)| *t == self.tab).copied().unwrap();
        let s = self.snap.lock().map(|s| (s.cpu_total, s.mem_used, s.mem_total, s.gpu_total, s.serial)).unwrap_or_default();
        // Screenshots show steady, believable meters, not whatever this PC happens to be doing right then.
        let s = if self.opts.screenshot.is_some() || self.opts.tour.is_some() {
            const GIB: f64 = 1_073_741_824.0;
            (23.0, (5.7 * GIB) as u64, (15.9 * GIB) as u64, 6.0, 3)
        } else {
            s
        };
        self.drawn.insert("stats");
        let h = TITLEBAR_H;
        let win_w = ui.ctx().content_rect().width();
        widgets::title_bar_drag(ui);
        let full = ui.max_rect();
        // The sidebar's colour and edge continue up through the bar: one surface, not two.
        ui.painter().rect_filled(egui::Rect::from_min_size(full.min, vec2(nav_w, full.height())), 0, p.sidebar);
        let edge = full.left() + nav_w - 0.5;
        ui.painter().line_segment([egui::pos2(edge, full.top()), egui::pos2(edge, full.bottom())], egui::Stroke::new(1.0, p.line));
        ui.allocate_ui_with_layout(vec2(full.width(), h), Layout::left_to_right(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            // ---- brand, over the sidebar (it always takes exactly the sidebar's width)
            let brand_start = ui.cursor().min.x;
            ui.allocate_ui_with_layout(vec2(nav_w, h), Layout::left_to_right(Align::Center), |ui| {
                ui.add_space(if compact { 17.0 } else { 18.0 });
                let (rect, _) = ui.allocate_exact_size(vec2(30.0, 30.0), egui::Sense::hover());
                ui.painter().rect_filled(rect, 9, p.accent_soft);
                mark::paint(ui.painter(), rect.center(), 22.0, p.accent_soft, theme::alpha(p.text, 190), p.accent);
                if !compact {
                    ui.add_space(10.0);
                    // The wordmark: the X wears the accent, so it changes with the person's choice.
                    ui.label(bold("Cataly", 15.0));
                    ui.label(bold("X", 15.0).color(p.accent));
                }
            });
            // ---- the page you are on: starts where the page's content starts (sidebar + 24)
            let used = ui.cursor().min.x - brand_start;
            ui.add_space((nav_w - used).max(0.0) + 24.0);
            ui.label(bold(title, 16.0));
            if win_w >= 1000.0 {
                ui.add_space(10.0);
                ui.add(egui::Label::new(faint(ui, sub, 12.0)).truncate());
            }
            // ---- meters, then the window buttons
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                widgets::window_buttons(ui, h);
                ui.add_space(8.0);
                let (cpu, used, total, gpu_pct) = (s.0, s.1, s.2, s.3);
                let ready = s.4 > 0;
                let dash = "\u{2013}".to_string();
                let mem_frac = if total > 0 { used as f32 / total as f32 } else { 0.0 };
                let mut items: Vec<(&str, String, f32)> = vec![
                    ("GPU", if ready { format!("{gpu_pct:.0}%") } else { dash.clone() }, gpu_pct / 100.0),
                    (
                        "RAM",
                        if !ready {
                            dash.clone()
                        } else if win_w < 1000.0 {
                            format!("{:.0}%", mem_frac * 100.0)
                        } else {
                            format!("{} / {}", procs::fmt_mem(used), procs::fmt_mem(total))
                        },
                        mem_frac,
                    ),
                    ("CPU", if ready { format!("{cpu:.0}%") } else { dash.clone() }, cpu / 100.0),
                ];
                // Drop the least important readings first as the window narrows.
                if win_w < 900.0 {
                    items.remove(0);
                }
                if win_w < 740.0 {
                    items.remove(0);
                }
                for (i, (name, value, frac)) in items.into_iter().enumerate() {
                    if i > 0 {
                        ui.add_space(18.0);
                    }
                    // name, value, meter: read left to right even though the row is laid out from the right.
                    ui.horizontal(|ui| {
                        ui.spacing_mut().item_spacing.x = 8.0;
                        ui.label(faint(ui, name, 12.0));
                        ui.label(bold(value, 14.0));
                        widgets::meter(ui, frac, widgets::load_color(&p, frac), 36.0);
                    });
                }
                ui.add_space(8.0);
            });
        });
    }

    pub(super) fn body(&mut self, ui: &mut egui::Ui) {
        match self.tab {
            Tab::Boost => pages::boost::show(self, ui),
            Tab::Games => pages::games::show(self, ui),
            Tab::Apps => pages::apps::show(self, ui),
            Tab::Services => pages::services_page::show(self, ui),
            Tab::Profiles => pages::profiles::show(self, ui),
            Tab::Tuning => pages::tuning::show(self, ui),
            Tab::Settings => pages::settings::show(self, ui),
        }
    }
}
