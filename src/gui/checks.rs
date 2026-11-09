//! Built-in checks that drive the real window: `--screenshot`, `--tour` and `--self-test` (see docs/DEVELOPING.md).

use super::*;

impl CatalyxApp {
    pub(super) fn run_checks(&mut self, ctx: &Context) {
        self.frames += 1;
        // The tour walks every page and both modes, saving a picture of each.
        if let Some(dir) = self.opts.tour.clone() {
            const SETTLE: u32 = 14;
            let steps: Vec<(Tab, bool)> = TABS.iter().flat_map(|(t, ..)| [(*t, true), (*t, false)]).collect();
            if self.frames % SETTLE == 1 && !self.shot_requested {
                if self.tour_step >= steps.len() {
                    std::process::exit(0);
                }
                let (t, dark) = steps[self.tour_step];
                self.tab = t;
                self.cfg.appearance = if dark { "dark".into() } else { "light".into() };
            }
            if self.frames % SETTLE == SETTLE - 2 && !self.shot_requested {
                self.shot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            let shots: Vec<_> = ctx.input(|i| {
                i.events
                    .iter()
                    .filter_map(|e| match e {
                        egui::Event::Screenshot { image, .. } => Some(image.clone()),
                        _ => None,
                    })
                    .collect()
            });
            for img in shots {
                if self.tour_step < steps.len() {
                    let (t, dark) = steps[self.tour_step];
                    let name = format!("{}-{}.png", format!("{:?}", t).to_lowercase(), if dark { "dark" } else { "light" });
                    let _ = std::fs::create_dir_all(&dir);
                    save_png(&format!("{dir}\\{name}"), &img);
                    self.tour_step += 1;
                }
                self.shot_requested = false;
            }
            ctx.request_repaint_after(Duration::from_millis(40));
            return;
        }
        if let Some(path) = self.opts.screenshot.clone() {
            if self.frames == 16 && !self.shot_requested {
                self.shot_requested = true;
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::UserData::default()));
            }
            let shot = ctx.input(|i| {
                i.events.iter().find_map(|e| match e {
                    egui::Event::Screenshot { image, .. } => Some(image.clone()),
                    _ => None,
                })
            });
            if let Some(img) = shot {
                save_png(&path, &img);
                std::process::exit(0);
            }
            ctx.request_repaint_after(Duration::from_millis(40));
        }
    }
}
