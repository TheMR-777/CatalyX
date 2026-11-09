//! Apps: everything running, with what it costs, and what should happen to it while you play.
//! Rules apply to all games or only to one; "Close" is the only choice that can lose work, and
//! the suggestions never use it for programs that hold your data.

use crate::catalog;
use crate::config::{app_is_protected, ProcAction};
use crate::gui::icons::{self, Ico};
use crate::gui::pages::Confirm;
use crate::gui::theme::alpha;
use crate::gui::widgets::*;
use crate::gui::CatalyxApp;
use crate::procs;
use egui::{pos2, vec2, Align, Align2, FontId, Layout, Rect, RichText, ScrollArea, Sense, Ui};
use std::collections::HashMap;

#[derive(Default)]
pub struct AppsUi {
    pub scope: usize,
    pub filter: String,
    pub view: usize,
    pub sort: usize,
}

struct Row {
    exe: String,
    name: String,
    file: String,
    category: catalog::Category,
    count: usize,
    cpu: f32,
    mem: u64,
    gpu: f32,
    protected: bool,
    running: bool,
}

pub fn scope_game(app: &CatalyxApp) -> Option<usize> {
    let s = app.ui.apps.scope;
    (s > 0 && s <= app.cfg.games.len()).then(|| s - 1)
}

pub fn effective(app: &CatalyxApp, game: Option<usize>, exe: &str) -> ProcAction {
    let global = app.cfg.apps.get(exe).copied().unwrap_or(ProcAction::Keep);
    match game.and_then(|i| app.cfg.games.get(i)) {
        Some(g) => g.apps.get(exe).copied().unwrap_or(global),
        None => global,
    }
}

fn set_action(app: &mut CatalyxApp, game: Option<usize>, exe: &str, a: ProcAction) {
    match game {
        None => {
            if a == ProcAction::Keep {
                app.cfg.apps.remove(exe);
            } else {
                app.cfg.apps.insert(exe.to_string(), a);
            }
        }
        Some(i) => {
            let global = app.cfg.apps.get(exe).copied().unwrap_or(ProcAction::Keep);
            if a == global {
                app.cfg.games[i].apps.remove(exe);
            } else {
                app.cfg.games[i].apps.insert(exe.to_string(), a);
            }
        }
    }
    app.mark_dirty();
}

fn build_rows(app: &CatalyxApp, game: Option<usize>) -> Vec<Row> {
    let snap = app.snap.lock().map(|s| s.clone()).unwrap_or_default();
    let mut map: HashMap<String, Row> = HashMap::new();
    for p in &snap.procs {
        if p.pid <= 4 || p.name_lower.is_empty() || p.name_lower == "catalyx.exe" || p.name_lower == "pureboost.exe" {
            continue;
        }
        let e = map.entry(p.name_lower.clone()).or_insert_with(|| Row {
            exe: p.name_lower.clone(),
            name: catalog::friendly(&p.name_lower, &p.name),
            file: p.name.clone(),
            category: catalog::category_of(&p.name_lower),
            count: 0,
            cpu: 0.0,
            mem: 0,
            gpu: 0.0,
            protected: app_is_protected(&p.name_lower),
            running: true,
        });
        e.count += 1;
        e.cpu += p.cpu;
        e.mem += p.mem;
        e.gpu = e.gpu.max(p.gpu);
    }
    // Rules for programs that are not running right now still belong in "With rules".
    let rules = app.cfg.apps_for(game.and_then(|i| app.cfg.games.get(i)));
    for (exe, a) in rules.iter().filter(|(_, a)| **a != ProcAction::Keep) {
        map.entry(exe.clone()).or_insert_with(|| Row {
            exe: exe.clone(),
            name: catalog::friendly(exe, exe),
            file: exe.clone(),
            category: catalog::category_of(exe),
            count: 0,
            cpu: 0.0,
            mem: 0,
            gpu: 0.0,
            protected: app_is_protected(exe),
            running: false,
        });
        let _ = a;
    }
    map.into_values().collect()
}

pub fn show(app: &mut CatalyxApp, ui: &mut Ui) {
    // Keep the scope inside the library.
    if app.ui.apps.scope > app.cfg.games.len() {
        app.ui.apps.scope = 0;
    }
    let game = scope_game(app);
    app.drawn.insert("apps");
    centered(ui, 1120.0, |ui, w| {
        toolbar(app, ui, w);
        summary(app, ui, game, w);
        list(app, ui, game, w);
    });
}

fn toolbar(app: &mut CatalyxApp, ui: &mut Ui, w: f32) {
    // One row when there is room, two when not. It never runs past the edge.
    let wide = w >= 900.0;
    let gap = 16.0;
    let mut scopes: Vec<String> = vec!["All games".into()];
    scopes.extend(app.cfg.games.iter().map(|g| g.name.clone()));
    let scope_w = 180.0;
    let seg_w = 290.0;
    let sort_w = 110.0;
    let search_w = if wide { (w - scope_w - seg_w - sort_w - gap * 3.0).clamp(160.0, 260.0) } else { (w - scope_w - gap).max(140.0) };
    let row_a = |app: &mut CatalyxApp, ui: &mut Ui| {
        let mut s = app.ui.apps.scope;
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(faint(ui, "Rules apply to", 12.0));
            if dropdown(ui, "apps-scope", scope_w, &mut s, &scopes) {
                app.ui.apps.scope = s;
            }
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(faint(ui, "Search", 12.0));
            ui.add(egui_components::Input::new(&mut app.ui.apps.filter).placeholder("Filter by name").width(search_w));
        });
    };
    let row_b = |app: &mut CatalyxApp, ui: &mut Ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(faint(ui, "Show", 12.0));
            let mut v = app.ui.apps.view;
            if segmented(ui, seg_w, &mut v, &["Running", "With rules", "Everything"]) {
                app.ui.apps.view = v;
            }
        });
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 4.0;
            ui.label(faint(ui, "Sort by", 12.0));
            let mut v = app.ui.apps.sort;
            if dropdown(ui, "apps-sort", sort_w, &mut v, &["CPU".to_string(), "Memory".into(), "GPU".into(), "Name".into()]) {
                app.ui.apps.sort = v;
            }
        });
    };
    if wide {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            row_a(app, ui);
            row_b(app, ui);
        });
    } else {
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            row_a(app, ui);
        });
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = gap;
            row_b(app, ui);
        });
    }
}

fn summary(app: &mut CatalyxApp, ui: &mut Ui, game: Option<usize>, w: f32) {
    let p = app.pal();
    let rules = app.cfg.apps_for(game.and_then(|i| app.cfg.games.get(i)));
    let count = |a: ProcAction| rules.values().filter(|x| **x == a).count();
    ui.horizontal(|ui| {
        ui.spacing_mut().item_spacing.x = 6.0;
        ui.label(RichText::new(format!("{} lowered", count(ProcAction::Lower))).size(12.0).color(p.warn));
        ui.label(faint(ui, "\u{00b7}", 12.0));
        ui.label(RichText::new(format!("{} paused", count(ProcAction::Suspend))).size(12.0).color(p.accent));
        ui.label(faint(ui, "\u{00b7}", 12.0));
        ui.label(RichText::new(format!("{} closed", count(ProcAction::Close))).size(12.0).color(p.danger));
        ui.label(faint(ui, "while you play", 12.0));
        ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
            ui.spacing_mut().item_spacing.x = 8.0;
            if ui.add(egui_components::Button::secondary("Use suggestions").small()).on_hover_text("Lowers browsers, chat and sync clients; pauses background helpers; closes only updaters and Windows extras. Never closes anything that holds your work.").clicked() {
                for (exe, a) in catalog::suggested_apps() {
                    if effective(app, None, &exe) == ProcAction::Keep {
                        set_action(app, None, &exe, a);
                    }
                }
                app.toast_ok("Suggestions added (your own choices were kept)");
            }
            if w > 640.0 && ui.add(egui_components::Button::ghost("Set all to Keep").small()).clicked() {
                app.confirm = Some(Confirm::ClearApps);
            }
        });
    });
}

fn list(app: &mut CatalyxApp, ui: &mut Ui, game: Option<usize>, w: f32) {
    let mut rows = build_rows(app, game);
    let f = app.ui.apps.filter.trim().to_lowercase();
    let view = app.ui.apps.view;
    rows.retain(|r| {
        let by_view = match view {
            0 => r.running && !r.protected,
            1 => effective(app, game, &r.exe) != ProcAction::Keep,
            _ => true,
        };
        by_view && (f.is_empty() || r.exe.contains(&f) || r.name.to_lowercase().contains(&f))
    });
    match app.ui.apps.sort {
        0 => rows.sort_by(|a, b| b.cpu.partial_cmp(&a.cpu).unwrap_or(std::cmp::Ordering::Equal).then(b.mem.cmp(&a.mem))),
        1 => rows.sort_by_key(|r| std::cmp::Reverse(r.mem)),
        2 => rows.sort_by(|a, b| b.gpu.partial_cmp(&a.gpu).unwrap_or(std::cmp::Ordering::Equal)),
        _ => rows.sort_by_key(|a| a.name.to_lowercase()),
    }
    if rows.is_empty() {
        card(ui, |ui| {
            let (t, h) = match view {
                1 => ("No rules yet", "Press \"Use suggestions\", or choose an action for any running app."),
                _ => ("Nothing matches", "Try another filter, or switch to Everything."),
            };
            empty_state(ui, Ico::Apps, t, h);
        });
        return;
    }
    let show_gpu = w > 860.0;
    let show_cat = w > 740.0;
    let show_cpu = w > 600.0;
    let row_h = 56.0;
    list_card(ui, |ui| {
        ScrollArea::vertical().auto_shrink([false, false]).show_rows(ui, row_h, rows.len(), |ui, range| {
            let rw = ui.available_width() - 6.0;
            for r in &rows[range] {
                row(app, ui, r, game, rw, show_gpu, show_cat, show_cpu, row_h);
            }
        });
    });
}

#[allow(clippy::too_many_arguments)]
fn row(
    app: &mut CatalyxApp,
    ui: &mut Ui,
    r: &Row,
    game: Option<usize>,
    width: f32,
    show_gpu: bool,
    show_cat: bool,
    show_cpu: bool,
    h: f32,
) {
    let p = app.pal();
    let (rect, _hover) = flat_row(ui, width, h);
    let current = effective(app, game, &r.exe);
    let seg = if width < 560.0 { 48.0 } else { 56.0 };
    let picker_w = seg * 4.0 + 6.0;

    // Left: letter tile, name, and a quiet second line.
    let tile = Rect::from_min_size(rect.left_top() + vec2(12.0, (h - 32.0) / 2.0), vec2(32.0, 32.0));
    ui.painter().rect_filled(tile, 9, p.accent_soft);
    let c = r.name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or("?".into());
    ui.painter().text(tile.center(), Align2::CENTER_CENTER, c, FontId::new(14.0, egui::FontFamily::Name("bold".into())), p.accent);
    let name_x = tile.right() + 12.0;

    // Right to left: picker, end-now, metrics, category. `x` is the left edge of what is already placed.
    let mut x = rect.right() - 12.0;
    if r.protected {
        let g = ui.painter().layout_no_wrap("Protected".into(), FontId::proportional(12.0), p.faint);
        x -= g.size().x;
        ui.painter().galley(pos2(x, rect.center().y - g.size().y / 2.0), g, p.faint);
        icons::paint_at(ui.painter(), Ico::Lock, pos2(x - 12.0, rect.center().y), 14.0, p.faint);
        x -= 28.0;
    } else {
        x -= picker_w;
        let pr = Rect::from_min_size(pos2(x, rect.top() + (h - 28.0) / 2.0), vec2(picker_w, 28.0));
        let mut child =
            ui.new_child(egui::UiBuilder::new().max_rect(pr).id_salt(("picker", &r.exe)).layout(Layout::left_to_right(Align::Center)));
        let mut a = current;
        if action_picker_sized(&mut child, &mut a, seg) {
            if a == ProcAction::Close
                && matches!(
                    r.category,
                    catalog::Category::Browser | catalog::Category::Chat | catalog::Category::Sync | catalog::Category::Dev
                )
            {
                app.toast_info(format!("{} can hold unsaved work: Lower keeps it running without the cost.", r.name));
            }
            set_action(app, game, &r.exe, a);
        }
        x -= 8.0;
        if r.running {
            let br = Rect::from_min_size(pos2(x - 28.0, rect.top() + (h - 28.0) / 2.0), vec2(28.0, 28.0));
            let resp = ui.interact(br, ui.id().with(("end", &r.exe)), Sense::click());
            if resp.hovered() {
                ui.painter().rect_filled(br, 8, alpha(p.danger, 40));
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            icons::paint_at(ui.painter(), Ico::Power, br.center(), 15.0, if resp.hovered() { p.danger } else { p.faint });
            if resp.on_hover_text("End now (once)").clicked() {
                app.confirm = Some(Confirm::EndApp { exe: r.exe.clone(), name: r.name.clone() });
            }
            x -= 36.0;
        }
    }
    // Metrics: right-aligned numbers in fixed columns, so every row lines up.
    if r.running {
        let mut cols: Vec<(String, f32, egui::Color32)> = Vec::new();
        if show_cpu {
            cols.push((format!("{:.1}%", r.cpu), 52.0, if r.cpu > 15.0 { p.warn } else { p.dim }));
        }
        cols.push((procs::fmt_mem(r.mem), 72.0, if r.mem > 1_500_000_000 { p.warn } else { p.dim }));
        if show_gpu {
            cols.push((format!("{:.0}%", r.gpu), 44.0, if r.gpu > 20.0 { p.warn } else { p.dim }));
        }
        for (text, w, col) in cols.iter().rev() {
            ui.painter().text(pos2(x - 4.0, rect.center().y), Align2::RIGHT_CENTER, text, FontId::proportional(12.0), *col);
            x -= w + 8.0;
        }
    }
    if show_cat && r.category != catalog::Category::Other {
        let g = ui.painter().layout_no_wrap(r.category.label().into(), FontId::proportional(12.0), p.dim);
        let cr = Rect::from_min_size(pos2(x - g.size().x - 24.0, rect.center().y - 10.0), vec2(g.size().x + 16.0, 20.0));
        ui.painter().rect_filled(cr, 10, p.well);
        ui.painter().galley(pos2(cr.left() + 8.0, cr.center().y - g.size().y / 2.0), g, p.dim);
        x = cr.left();
    }
    // Names get what is left (truncated with an ellipsis, never overlapping the columns).
    let maxw = (x - name_x - 12.0).max(80.0);
    let name = fit(ui, &r.name, 14.0, maxw);
    ui.painter().text(pos2(name_x, rect.top() + h * 0.38), Align2::LEFT_CENTER, name, FontId::proportional(14.0), p.text);
    let sub = if r.running {
        if r.count > 1 {
            format!("{} \u{00b7} {} processes", r.file, r.count)
        } else {
            r.file.clone()
        }
    } else {
        format!("{} \u{00b7} not running", r.file)
    };
    ui.painter().text(
        pos2(name_x, rect.top() + h * 0.68),
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
