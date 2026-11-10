//! Small reusable pieces of the interface. Anything the component library already does well
//! (buttons, switches, selects, tabs, inputs, dialogs, toasts) comes from `egui_components`;
//! this file holds what it does not have: cards, chips, stat tiles, meters, the action picker
//! and the small pieces shared by the pages.

use super::icons::{self, Ico};
use super::theme::{alpha, mix, pal, Palette};
use crate::config::ProcAction;
use egui::{pos2, vec2, Align, Color32, CursorIcon, FontFamily, FontId, Layout, Rect, Response, RichText, Sense, Stroke, Ui};

/// The one width for drop-downs and segmented controls that sit on the right of a settings row,
/// so their right and left edges line up all the way down a card.
pub const CONTROL_W: f32 = 220.0;

pub fn bold(text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text.into()).font(FontId::new(size, FontFamily::Name("bold".into())))
}

pub fn dim(ui: &Ui, text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text.into()).size(size).color(pal(ui.ctx()).dim)
}

pub fn faint(ui: &Ui, text: impl Into<String>, size: f32) -> RichText {
    RichText::new(text.into()).size(size).color(pal(ui.ctx()).faint)
}

/// A bordered surface: the "card" layer between the window and the wells inside it.
pub fn card<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui.ctx());
    egui::Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(14)
        .inner_margin(20)
        .show(ui, |ui| {
            // Cards always fill their column, so neighbouring cards share the same edges.
            ui.set_min_width(ui.available_width());
            ui.set_max_width(ui.available_width());
            ui.spacing_mut().item_spacing = vec2(8.0, 8.0);
            body(ui)
        })
        .inner
}

/// A card with a title row (and optional right-hand content).
pub fn card_titled<R>(ui: &mut Ui, title: &str, right: impl FnOnce(&mut Ui), body: impl FnOnce(&mut Ui) -> R) -> R {
    card(ui, |ui| {
        ui.horizontal(|ui| {
            ui.label(bold(title, 16.0));
            ui.with_layout(Layout::right_to_left(Align::Center), right);
        });
        ui.add_space(8.0);
        body(ui)
    })
}

/// Small upper-case group heading.
pub fn section_label(ui: &mut Ui, text: &str) {
    ui.add_space(4.0);
    ui.label(faint(ui, text.to_uppercase(), 11.0));
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Neutral,
    Accent,
    Ok,
    Warn,
}

pub fn tone_color(p: &Palette, t: Tone) -> Color32 {
    match t {
        Tone::Neutral => p.dim,
        Tone::Accent => p.accent,
        Tone::Ok => p.ok,
        Tone::Warn => p.warn,
    }
}

/// A small tinted pill ("Admin", "Running", "Recommended").
pub fn chip(ui: &mut Ui, text: &str, tone: Tone) -> Response {
    let p = pal(ui.ctx());
    let c = tone_color(&p, tone);
    let font = FontId::proportional(12.0);
    let galley = ui.painter().layout_no_wrap(text.to_string(), font, c);
    let size = vec2(galley.size().x + 16.0, 20.0);
    let (rect, resp) = ui.allocate_exact_size(size, Sense::hover());
    ui.painter().rect_filled(rect, 10, alpha(c, if p.dark { 34 } else { 28 }));
    ui.painter().galley(rect.center() - galley.size() * 0.5, galley, c);
    resp
}

/// A well with a big value and a caption beneath it.
pub fn stat_tile(ui: &mut Ui, width: f32, value: &str, caption: &str) {
    let p = pal(ui.ctx());
    // A fixed 64 px well: every tile in a row has exactly the same top and bottom edge.
    let (rect, _) = ui.allocate_exact_size(vec2(width, 64.0), Sense::hover());
    ui.painter().rect_filled(rect, 10, p.well);
    let clip = rect.shrink2(vec2(14.0, 0.0));
    let painter = ui.painter().with_clip_rect(clip);
    painter.text(pos2(clip.left(), rect.top() + 24.0), egui::Align2::LEFT_CENTER, value, FontId::proportional(18.0), p.text);
    painter.text(pos2(clip.left(), rect.top() + 46.0), egui::Align2::LEFT_CENTER, caption, FontId::proportional(12.0), p.faint);
}

/// A thin horizontal meter (0..=1).
pub fn meter(ui: &mut Ui, frac: f32, color: Color32, width: f32) {
    let p = pal(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(width, 5.0), Sense::hover());
    ui.painter().rect_filled(rect, 3, p.line);
    let mut f = rect;
    f.set_width((rect.width() * frac.clamp(0.0, 1.0)).max(if frac > 0.0 { 3.0 } else { 0.0 }));
    ui.painter().rect_filled(f, 3, color);
}

/// Heat colour for a 0..=1 load: calm, busy, hot.
pub fn load_color(p: &Palette, frac: f32) -> Color32 {
    if frac > 0.9 {
        p.danger
    } else if frac > 0.7 {
        p.warn
    } else {
        p.accent
    }
}

/// A sidebar entry: icon, label and the accent bar when selected.
pub fn nav_item(ui: &mut Ui, icon: Ico, label: &str, selected: bool, badge: Option<&str>) -> Response {
    let p = pal(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 36.0), Sense::click());
    if ui.is_rect_visible(rect) {
        let bg = if selected {
            p.panel_hi
        } else if resp.hovered() {
            alpha(p.panel_hi, 140)
        } else {
            Color32::TRANSPARENT
        };
        if bg != Color32::TRANSPARENT {
            ui.painter().rect_filled(rect, 8, bg);
        }
        if selected {
            ui.painter().rect_filled(Rect::from_min_size(pos2(rect.left(), rect.top() + 8.0), vec2(3.0, 20.0)), 2, p.accent);
        }
        let fg = if selected { p.text } else { p.dim };
        icons::paint_at(ui.painter(), icon, pos2(rect.left() + 22.0, rect.center().y), 18.0, if selected { p.accent } else { fg });
        ui.painter().text(pos2(rect.left() + 40.0, rect.center().y), egui::Align2::LEFT_CENTER, label, FontId::proportional(14.0), fg);
        if let Some(b) = badge {
            let g = ui.painter().layout_no_wrap(b.to_string(), FontId::proportional(12.0), p.on_accent);
            let r = Rect::from_center_size(pos2(rect.right() - 20.0, rect.center().y), vec2(g.size().x + 10.0, 17.0));
            ui.painter().rect_filled(r, 9, p.warn);
            ui.painter().galley(r.center() - g.size() * 0.5, g, p.on_accent);
        }
    }
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

/// A settings row: title and one-line hint on the left, the control on the right.
pub fn setting_row(ui: &mut Ui, title: &str, hint: &str, control: impl FnOnce(&mut Ui)) {
    // Every row is at least 56 px tall, so rows with and without a hint line up on one rhythm.
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 56.0), Layout::left_to_right(Align::Center), |ui| {
        ui.vertical(|ui| {
            ui.spacing_mut().item_spacing.y = 2.0;
            ui.set_max_width((ui.available_width() - CONTROL_W - 24.0).max(120.0));
            ui.label(RichText::new(title).size(14.0));
            if !hint.is_empty() {
                ui.label(faint(ui, hint, 12.0));
            }
        });
        ui.with_layout(Layout::right_to_left(Align::Center), control);
    });
}

pub fn action_color(p: &Palette, a: ProcAction) -> Color32 {
    match a {
        ProcAction::Keep => p.ok,
        ProcAction::Suspend => p.accent,
        ProcAction::Lower => p.warn,
        ProcAction::Close => p.danger,
    }
}

/// The same picker with a chosen segment width (narrow windows use smaller segments).
pub fn action_picker_sized(ui: &mut Ui, value: &mut ProcAction, seg: f32) -> bool {
    let p = pal(ui.ctx());
    let mut changed = false;
    let (rect, _) = ui.allocate_exact_size(vec2(seg * 4.0 + 6.0, 28.0), Sense::hover());
    ui.painter().rect_filled(rect, 8, p.well);
    for (i, a) in ProcAction::ALL.iter().enumerate() {
        let r = Rect::from_min_size(pos2(rect.left() + 3.0 + i as f32 * seg, rect.top() + 3.0), vec2(seg, 22.0));
        let resp = ui.interact(r, ui.id().with(("act", i, rect.min.x as i32, rect.min.y as i32)), Sense::click());
        let sel = *value == *a;
        let col = action_color(&p, *a);
        if sel {
            // Selected: a soft tint of its own colour, no outline.
            ui.painter().rect_filled(r, 6, alpha(col, if p.dark { 52 } else { 44 }));
        } else if resp.hovered() {
            ui.painter().rect_filled(r, 6, alpha(p.panel_hi, 220));
        }
        ui.painter().text(r.center(), egui::Align2::CENTER_CENTER, a.label(), FontId::proportional(12.0), if sel { col } else { p.dim });
        if resp.hovered() {
            ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
        }
        let resp = resp.on_hover_text(a.hint());
        if resp.clicked() && !sel {
            *value = *a;
            changed = true;
        }
    }
    changed
}

/// A small icon-only button with a tooltip.
pub fn icon_button(ui: &mut Ui, icon: Ico, tip: &str) -> Response {
    let p = pal(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(30.0, 30.0), Sense::click());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 8, p.panel_hi);
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    icons::paint_at(ui.painter(), icon, rect.center(), 17.0, if resp.hovered() { p.text } else { p.dim });
    resp.on_hover_text(tip)
}

/// A friendly empty state.
pub fn empty_state(ui: &mut Ui, icon: Ico, title: &str, hint: &str) {
    let p = pal(ui.ctx());
    ui.vertical_centered(|ui| {
        ui.add_space(28.0);
        let (rect, _) = ui.allocate_exact_size(vec2(44.0, 44.0), Sense::hover());
        icons::paint_at(ui.painter(), icon, rect.center(), 36.0, p.faint);
        ui.add_space(6.0);
        ui.label(bold(title, 16.0));
        ui.label(faint(ui, hint, 12.0));
        ui.add_space(20.0);
    });
}

/// Letter tile for a game or app (first letter on a soft accent square).
pub fn letter_tile(ui: &mut Ui, name: &str, size: f32) {
    let p = pal(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(size, size), Sense::hover());
    ui.painter().rect_filled(rect, size * 0.26, p.accent_soft);
    ui.painter().rect_stroke(rect, size * 0.26, Stroke::new(1.0, p.line), egui::StrokeKind::Inside);
    let c = name.chars().find(|c| c.is_alphanumeric()).map(|c| c.to_uppercase().to_string()).unwrap_or_else(|| "?".into());
    ui.painter().text(rect.center(), egui::Align2::CENTER_CENTER, c, FontId::new(size * 0.46, FontFamily::Name("bold".into())), p.accent);
}

/// A segmented control of a fixed width. Returns true when the selection changed.
pub fn segmented(ui: &mut Ui, width: f32, selected: &mut usize, labels: &[&str]) -> bool {
    let before = *selected;
    ui.allocate_ui(vec2(width, 38.0), |ui| {
        egui_components::Tabs::new(selected).segmented().small().tabs(labels.iter().map(|s| s.to_string())).show(ui);
    });
    *selected != before
}

/// A drop-down over a list of labels. Returns true when the choice changed.
pub fn dropdown(ui: &mut Ui, id: &str, width: f32, selected: &mut usize, options: &[String]) -> bool {
    let mut sel = Some((*selected).min(options.len().saturating_sub(1)));
    let before = sel;
    egui_components::Select::new(id, &mut sel).options(options.iter().cloned()).width(width).show(ui);
    if let Some(i) = sel {
        *selected = i;
    }
    sel != before
}

/// Formats a clock in GHz.
pub fn ghz(mhz: u32) -> String {
    if mhz == 0 {
        "-".into()
    } else {
        format!("{:.2} GHz", mhz as f32 / 1000.0)
    }
}

pub fn hairline(ui: &mut Ui) {
    let p = pal(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 1.0), Sense::hover());
    ui.painter().line_segment([rect.left_center(), rect.right_center()], Stroke::new(1.0, p.line));
}

/// A page body: scrolls, is centred, and never grows wider than `max_w` (text and controls stay
/// at a comfortable measure on big windows instead of stretching edge to edge).
pub fn page(ui: &mut Ui, id: &str, max_w: f32, body: impl FnOnce(&mut Ui)) {
    egui::ScrollArea::vertical().id_salt(id).auto_shrink([false, false]).show(ui, |ui| {
        let avail = ui.available_width();
        let w = avail.min(max_w);
        let off = ((avail - w) / 2.0).max(0.0);
        ui.horizontal_top(|ui| {
            ui.spacing_mut().item_spacing.x = 0.0;
            ui.add_space(off);
            ui.vertical(|ui| {
                ui.set_width(w - 3.0);
                // 16 px between cards; 8 px between things inside them (see card).
                ui.spacing_mut().item_spacing = vec2(8.0, 16.0);
                body(ui);
                ui.add_space(8.0);
            });
        });
    });
}

/// A quiet text button in the accent colour, for secondary actions inside a sentence-like row.
pub fn link_button(ui: &mut Ui, text: &str) -> Response {
    let p = pal(ui.ctx());
    let g = ui.painter().layout_no_wrap(text.to_string(), FontId::proportional(14.0), p.accent);
    let (rect, resp) = ui.allocate_exact_size(vec2(g.size().x + 8.0, 28.0), Sense::click());
    let col = if resp.hovered() { mix(p.accent, p.text, 0.3) } else { p.accent };
    ui.painter().galley(pos2(rect.left() + 4.0, rect.center().y - g.size().y / 2.0), g, col);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(CursorIcon::PointingHand);
    }
    resp
}

/// One line of a "heads-up" list: a coloured dot, the message, and an optional action on the right.
pub fn notice_row(ui: &mut Ui, tone: Tone, text: &str, action: Option<&str>) -> bool {
    let p = pal(ui.ctx());
    let mut clicked = false;
    ui.allocate_ui_with_layout(vec2(ui.available_width(), 32.0), Layout::left_to_right(Align::Center), |ui| {
        let (r, _) = ui.allocate_exact_size(vec2(14.0, 14.0), Sense::hover());
        ui.painter().circle_filled(r.center(), 4.0, tone_color(&p, tone));
        ui.add_space(4.0);
        // The message gets whatever the action does not need; it never runs into the action.
        let action_w = action.map(|a| a.chars().count() as f32 * 8.0 + 24.0).unwrap_or(0.0);
        let text_w = (ui.available_width() - action_w).max(80.0);
        ui.allocate_ui_with_layout(vec2(text_w, 32.0), Layout::left_to_right(Align::Center), |ui| {
            ui.add(egui::Label::new(RichText::new(text).size(14.0)).truncate());
        });
        if let Some(a) = action {
            ui.with_layout(Layout::right_to_left(Align::Center), |ui| {
                clicked = link_button(ui, a).clicked();
            });
        }
    });
    clicked
}

/// A centred column no wider than `max_w` that does not scroll by itself (for pages whose list
/// scrolls inside a card). The body receives the column's width.
pub fn centered(ui: &mut Ui, max_w: f32, body: impl FnOnce(&mut Ui, f32)) {
    let avail = ui.available_width();
    let w = avail.min(max_w);
    let off = ((avail - w) / 2.0).max(0.0);
    ui.horizontal_top(|ui| {
        ui.spacing_mut().item_spacing.x = 0.0;
        ui.add_space(off);
        ui.vertical(|ui| {
            ui.set_width(w);
            ui.spacing_mut().item_spacing = vec2(8.0, 12.0);
            body(ui, w);
        });
    });
}

/// The card that holds a long scrolling list: flat rows inside, one border outside.
pub fn list_card<R>(ui: &mut Ui, body: impl FnOnce(&mut Ui) -> R) -> R {
    let p = pal(ui.ctx());
    let h = ui.available_height();
    egui::Frame::new()
        .fill(p.panel)
        .stroke(Stroke::new(1.0, p.line))
        .corner_radius(14)
        .inner_margin(egui::Margin::symmetric(8, 8))
        .show(ui, |ui| {
            ui.set_min_width(ui.available_width());
            ui.set_min_height((h - 18.0).max(120.0));
            ui.spacing_mut().item_spacing = vec2(0.0, 0.0);
            body(ui)
        })
        .inner
}

/// A flat list row: hover shade, and a hairline under it (inset, so it reads as a divider and not a box).
pub fn flat_row(ui: &mut Ui, width: f32, height: f32) -> (egui::Rect, bool) {
    let p = pal(ui.ctx());
    let (rect, resp) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
    if resp.hovered() {
        ui.painter().rect_filled(rect, 10, alpha(p.panel_hi, if p.dark { 150 } else { 200 }));
    }
    ui.painter().line_segment(
        [pos2(rect.left() + 12.0, rect.bottom()), pos2(rect.right() - 12.0, rect.bottom())],
        Stroke::new(1.0, alpha(p.line, 160)),
    );
    (rect, resp.hovered())
}

// ---------------------------------------------------------------- custom title bar

/// How far in from the window's edges the window resizes. The bar's own controls leave this band alone.
const RESIZE_EDGE: f32 = 5.0;

/// The width of the resize band right now: none while maximized or full screen (the edges are the
/// screen's, so the caption buttons reach all the way to the corner).
fn resize_band(ctx: &egui::Context) -> f32 {
    let (maximized, fullscreen) = ctx.input(|i| (i.viewport().maximized.unwrap_or(false), i.viewport().fullscreen.unwrap_or(false)));
    if maximized || fullscreen {
        0.0
    } else {
        RESIZE_EDGE
    }
}

/// Dragging the bar's empty space moves the window; double-clicking it maximizes or restores.
/// Call before drawing the bar's contents so their controls stay on top.
pub fn title_bar_drag(ui: &mut Ui) {
    let r = ui.max_rect();
    let band = resize_band(ui.ctx());
    let rect = egui::Rect::from_min_max(r.min + vec2(band, band), r.max - vec2(band, 0.0));
    let resp = ui.interact(rect, ui.id().with("title-bar-drag"), Sense::click_and_drag());
    if resp.drag_started_by(egui::PointerButton::Primary) {
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
    }
    if resp.double_clicked() {
        let maximized = ui.input(|i| i.viewport().maximized.unwrap_or(false));
        ui.ctx().send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Caption {
    Minimize,
    Maximize,
    Restore,
    Close,
}

/// Minimize, maximize/restore and close for a right-to-left layout (close ends up rightmost).
pub fn window_buttons(ui: &mut Ui, height: f32) {
    let ctx = ui.ctx().clone();
    let maximized = ctx.input(|i| i.viewport().maximized.unwrap_or(false));
    // The first frame after a restore still hit-tests the maximized layout: ignore presses on the edge.
    let inside = ctx.content_rect().shrink(resize_band(&ctx));
    let clicked = |r: Response| r.clicked() && r.interact_pointer_pos().is_none_or(|p| inside.contains(p));
    if clicked(caption_button(ui, Caption::Close, height)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Close);
    }
    let mid = if maximized { Caption::Restore } else { Caption::Maximize };
    if clicked(caption_button(ui, mid, height)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Maximized(!maximized));
    }
    if clicked(caption_button(ui, Caption::Minimize, height)) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
    }
}

fn caption_button(ui: &mut Ui, which: Caption, height: f32) -> Response {
    let p = pal(ui.ctx());
    let (rect, _) = ui.allocate_exact_size(vec2(46.0, height), Sense::hover());
    // Hit area excludes the top resize band (and the right band for Close) so edges stay grabbable.
    let band = resize_band(ui.ctx());
    let right = if which == Caption::Close { band } else { 0.0 };
    let hit = egui::Rect::from_min_max(rect.min + vec2(0.0, band), rect.max - vec2(right, 0.0));
    let resp = ui.interact(hit, ui.id().with(("caption", which as u8)), Sense::click());
    let (fill, ink) = match (which, resp.hovered()) {
        (Caption::Close, true) => (Color32::from_rgb(0xc4, 0x2b, 0x1c), Color32::WHITE),
        (_, true) => (alpha(p.text, 22), p.text),
        (_, false) => (Color32::TRANSPARENT, p.dim),
    };
    let painter = ui.painter();
    painter.rect_filled(rect, 0, fill);
    let c = rect.center();
    let s = 5.0;
    let stroke = Stroke::new(1.0, ink);
    // Snap strokes to the pixel grid so the 1 px glyphs are crisp at any scale.
    let ppp = ui.ctx().pixels_per_point();
    let snap = |v: f32| ((v * ppp).round() + 0.5) / ppp;
    let (cx, cy) = (snap(c.x), snap(c.y));
    match which {
        Caption::Minimize => {
            painter.line_segment([pos2(cx - s, cy), pos2(cx + s, cy)], stroke);
        }
        Caption::Maximize => {
            painter.rect_stroke(egui::Rect::from_min_max(pos2(cx - s, cy - s), pos2(cx + s, cy + s)), 0, stroke, egui::StrokeKind::Inside);
        }
        Caption::Restore => {
            let front = egui::Rect::from_min_max(pos2(cx - s, cy - s + 2.0), pos2(cx + s - 2.0, cy + s));
            painter.rect_stroke(front, 0, stroke, egui::StrokeKind::Inside);
            painter.line_segment([pos2(cx - s + 2.0, cy - s), pos2(cx + s, cy - s)], stroke);
            painter.line_segment([pos2(cx + s, cy - s), pos2(cx + s, cy + s - 2.0)], stroke);
        }
        Caption::Close => {
            painter.line_segment([pos2(cx - s, cy - s), pos2(cx + s, cy + s)], stroke);
            painter.line_segment([pos2(cx - s, cy + s), pos2(cx + s, cy - s)], stroke);
        }
    }
    let label = match which {
        Caption::Minimize => "Minimize",
        Caption::Maximize => "Maximize",
        Caption::Restore => "Restore",
        Caption::Close => "Close",
    };
    resp.on_hover_text(label)
}

/// An undecorated window has no frame to grab: this draws a hairline round the window and turns
/// its edges and corners into resize handles. Call once per frame, last.
pub fn window_edges(ctx: &egui::Context) {
    use egui::viewport::ResizeDirection as D;
    use egui::CursorIcon as C;
    let (maximized, fullscreen) = ctx.input(|i| (i.viewport().maximized.unwrap_or(false), i.viewport().fullscreen.unwrap_or(false)));
    if maximized || fullscreen {
        return;
    }
    let p = pal(ctx);
    let r = ctx.content_rect();
    ctx.layer_painter(egui::LayerId::new(egui::Order::Foreground, egui::Id::new("window-border"))).rect_stroke(
        r,
        8,
        Stroke::new(1.0, alpha(p.line_hi, 200)),
        egui::StrokeKind::Inside,
    );
    let e = RESIZE_EDGE;
    let bands = [r.with_max_y(r.top() + e), r.with_min_y(r.bottom() - e), r.with_max_x(r.left() + e), r.with_min_x(r.right() - e)];
    // Above every other layer, so a resize press is never handed to a widget nearby.
    for (i, rect) in bands.into_iter().enumerate() {
        egui::Area::new(egui::Id::new(("window-edge", i)))
            .order(egui::Order::Tooltip)
            .movable(false)
            .constrain(false)
            .fixed_pos(rect.min)
            .show(ctx, |ui| ui.allocate_response(rect.size(), Sense::CLICK));
    }
    let Some(pos) = ctx.input(|i| i.pointer.hover_pos()).filter(|q| r.contains(*q)) else { return };
    let near = |d: f32| d < RESIZE_EDGE;
    let corner = |d: f32| d < 3.0 * RESIZE_EDGE;
    let (left, right, top, bottom) = (pos.x - r.left(), r.right() - pos.x, pos.y - r.top(), r.bottom() - pos.y);
    let hit = if (near(top) && corner(left)) || (near(left) && corner(top)) {
        Some((D::NorthWest, C::ResizeNorthWest))
    } else if (near(top) && corner(right)) || (near(right) && corner(top)) {
        Some((D::NorthEast, C::ResizeNorthEast))
    } else if (near(bottom) && corner(left)) || (near(left) && corner(bottom)) {
        Some((D::SouthWest, C::ResizeSouthWest))
    } else if (near(bottom) && corner(right)) || (near(right) && corner(bottom)) {
        Some((D::SouthEast, C::ResizeSouthEast))
    } else if near(top) {
        Some((D::North, C::ResizeNorth))
    } else if near(bottom) {
        Some((D::South, C::ResizeSouth))
    } else if near(left) {
        Some((D::West, C::ResizeWest))
    } else if near(right) {
        Some((D::East, C::ResizeEast))
    } else {
        None
    };
    if let Some((direction, cursor)) = hit {
        ctx.set_cursor_icon(cursor);
        if ctx.input(|i| i.pointer.primary_pressed()) {
            ctx.send_viewport_cmd(egui::ViewportCommand::BeginResize(direction));
        }
    }
}
