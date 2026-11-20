//! The three looks (Focus, Slate, Mica), each in dark and light, as one palette that feeds both
//! egui's own style and the component library's theme.

use egui::{Color32, Context, FontData, FontDefinitions, FontFamily, FontId, Stroke, TextStyle};
use egui_components::theme::{Theme, ThemeColor, ThemeMetrics, ThemeMode};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum ThemeId {
    Focus,
    Slate,
    Mica,
}

impl ThemeId {
    pub const ALL: [ThemeId; 3] = [ThemeId::Focus, ThemeId::Slate, ThemeId::Mica];
    pub fn key(self) -> &'static str {
        match self {
            ThemeId::Focus => "focus",
            ThemeId::Slate => "slate",
            ThemeId::Mica => "mica",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ThemeId::Focus => "Focus",
            ThemeId::Slate => "Slate",
            ThemeId::Mica => "Mica",
        }
    }
    pub fn about(self) -> &'static str {
        match self {
            ThemeId::Focus => "Blue-grey with periwinkle",
            ThemeId::Slate => "Calm zinc with sage",
            ThemeId::Mica => "Windows 11 neutral",
        }
    }
    pub fn from_key(s: &str) -> ThemeId {
        ThemeId::ALL.into_iter().find(|t| t.key().eq_ignore_ascii_case(s)).unwrap_or(ThemeId::Focus)
    }
}

/// Every colour the interface uses. Surfaces come in three layers (window, card, well) separated
/// by hairlines instead of shadows; accents are pastel, never neon.
#[derive(Clone, Copy, Debug)]
pub struct Palette {
    pub dark: bool,
    pub bg: Color32,
    pub sidebar: Color32,
    pub panel: Color32,
    pub panel_hi: Color32,
    pub well: Color32,
    pub line: Color32,
    pub line_hi: Color32,
    pub popup: Color32,
    pub text: Color32,
    pub dim: Color32,
    pub faint: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub on_accent: Color32,
    pub ok: Color32,
    pub warn: Color32,
    pub danger: Color32,
    pub boost_top: Color32,
    pub boost_bottom: Color32,
}

const fn h(v: u32) -> Color32 {
    Color32::from_rgb(((v >> 16) & 0xff) as u8, ((v >> 8) & 0xff) as u8, (v & 0xff) as u8)
}

pub fn palette_for(theme: ThemeId, dark: bool) -> Palette {
    match (theme, dark) {
        (ThemeId::Focus, true) => Palette {
            dark,
            bg: h(0x121419),
            sidebar: h(0x0f1115),
            panel: h(0x191c23),
            panel_hi: h(0x232731),
            well: h(0x15171d),
            line: h(0x252935),
            line_hi: h(0x3b4052),
            popup: h(0x1e212a),
            text: h(0xe8ebf0),
            dim: h(0xa3abb8),
            faint: h(0x757d8c),
            accent: h(0x9aa8ff),
            accent_soft: h(0x29304d),
            on_accent: h(0x0f1115),
            ok: h(0x80d4a8),
            warn: h(0xf0c26e),
            danger: h(0xf09494),
            boost_top: h(0xb8c4ff),
            boost_bottom: h(0x7d8ff7),
        },
        (ThemeId::Focus, false) => Palette {
            dark,
            bg: h(0xeef0f6),
            sidebar: h(0xe5e8f1),
            panel: h(0xfbfbfd),
            panel_hi: h(0xeceef6),
            well: h(0xf3f4f9),
            line: h(0xdcdfea),
            line_hi: h(0xc3c8da),
            popup: h(0xfdfdfe),
            text: h(0x1b1e27),
            dim: h(0x585e70),
            faint: h(0x8a90a2),
            accent: h(0x5664dc),
            accent_soft: h(0xe2e5fb),
            on_accent: h(0xfafbff),
            ok: h(0x2c9466),
            warn: h(0xad6f14),
            danger: h(0xc4463f),
            boost_top: h(0x8d98f5),
            boost_bottom: h(0x5664dc),
        },
        (ThemeId::Slate, true) => Palette {
            dark,
            bg: h(0x17171a),
            sidebar: h(0x131315),
            panel: h(0x1c1c20),
            panel_hi: h(0x27272c),
            well: h(0x18181b),
            line: h(0x2a2a30),
            line_hi: h(0x3c3c44),
            popup: h(0x212125),
            text: h(0xececef),
            dim: h(0xa1a1aa),
            faint: h(0x71717a),
            accent: h(0x8fd1b0),
            accent_soft: h(0x1e2f28),
            on_accent: h(0x101412),
            ok: h(0x8fd1b0),
            warn: h(0xe9c37a),
            danger: h(0xeb9a9a),
            boost_top: h(0xb3e3cb),
            boost_bottom: h(0x6fbf98),
        },
        (ThemeId::Slate, false) => Palette {
            dark,
            bg: h(0xf4f4f5),
            sidebar: h(0xebebed),
            panel: h(0xfcfcfc),
            panel_hi: h(0xeeeef0),
            well: h(0xf6f6f7),
            line: h(0xe1e1e5),
            line_hi: h(0xcacad0),
            popup: h(0xfdfdfd),
            text: h(0x18181b),
            dim: h(0x5d5d67),
            faint: h(0x8e8e98),
            accent: h(0x2f8a62),
            accent_soft: h(0xdcefe5),
            on_accent: h(0xf7fffb),
            ok: h(0x2f8a62),
            warn: h(0xa66a12),
            danger: h(0xbf4440),
            boost_top: h(0x6cc39b),
            boost_bottom: h(0x2f8a62),
        },
        (ThemeId::Mica, true) => Palette {
            dark,
            bg: h(0x202020),
            sidebar: h(0x1b1b1b),
            panel: h(0x2b2b2b),
            panel_hi: h(0x353535),
            well: h(0x262626),
            line: h(0x393939),
            line_hi: h(0x4b4b4b),
            popup: h(0x2e2e2e),
            text: h(0xededed),
            dim: h(0xadadad),
            faint: h(0x7c7c7c),
            accent: h(0x60cdff),
            accent_soft: h(0x1c3443),
            on_accent: h(0x0a1a22),
            ok: h(0x6ccb8f),
            warn: h(0xf0c060),
            danger: h(0xf08a8a),
            boost_top: h(0x9de0ff),
            boost_bottom: h(0x3fb3ec),
        },
        (ThemeId::Mica, false) => Palette {
            dark,
            bg: h(0xf3f3f3),
            sidebar: h(0xeaeaea),
            panel: h(0xfbfbfb),
            panel_hi: h(0xededed),
            well: h(0xf6f6f6),
            line: h(0xe0e0e0),
            line_hi: h(0xcbcbcb),
            popup: h(0xfcfcfc),
            text: h(0x1a1a1a),
            dim: h(0x5a5a5a),
            faint: h(0x8c8c8c),
            accent: h(0x0067c0),
            accent_soft: h(0xdbeaf7),
            on_accent: h(0xf8fbff),
            ok: h(0x0f7b45),
            warn: h(0x9d5d00),
            danger: h(0xc42b1c),
            boost_top: h(0x4aa3e6),
            boost_bottom: h(0x0067c0),
        },
    }
}

pub fn mix(a: Color32, b: Color32, t: f32) -> Color32 {
    egui_components::theme::mix(a, b, t)
}

/// `c` with its alpha replaced (0..=255).
pub fn alpha(c: Color32, a: u8) -> Color32 {
    Color32::from_rgba_unmultiplied(c.r(), c.g(), c.b(), a)
}

fn component_theme(p: &Palette) -> Theme {
    let colors = ThemeColor {
        background: p.bg,
        foreground: p.text,
        border: p.line,
        ring: p.accent,
        overlay: alpha(Color32::BLACK, if p.dark { 120 } else { 50 }),
        caret: p.text,
        selection_background: alpha(p.accent, 90),

        primary_background: p.accent,
        primary_foreground: p.on_accent,
        primary_hover_background: mix(p.accent, p.text, 0.12),
        primary_active_background: mix(p.accent, p.bg, 0.18),

        secondary_background: p.panel_hi,
        secondary_foreground: p.text,
        secondary_hover_background: mix(p.panel_hi, p.text, 0.07),
        secondary_active_background: mix(p.panel_hi, p.text, 0.13),

        accent_background: p.panel_hi,
        accent_foreground: p.text,

        muted_background: p.well,
        muted_foreground: p.dim,

        danger_background: p.danger,
        danger_foreground: p.on_accent,
        success_background: p.ok,
        success_foreground: p.on_accent,
        warning_background: p.warn,
        warning_foreground: p.on_accent,
        info_background: p.accent,
        info_foreground: p.on_accent,

        input_border: p.line_hi,

        popover_background: p.popup,
        popover_foreground: p.text,

        slider_bar_background: p.accent,
        slider_thumb_background: p.text,
        switch_background: p.line_hi,

        link_foreground: p.accent,
        link_hover_foreground: mix(p.accent, p.text, 0.25),
        link_active_foreground: p.accent,
    };
    let metrics = ThemeMetrics {
        radius: 8.0,
        radius_sm: 6.0,
        radius_lg: 12.0,
        button_height_sm: 28.0,
        button_height_md: 32.0,
        button_height_lg: 40.0,
        input_height: 32.0,
        input_height_sm: 28.0,
        input_height_lg: 40.0,
        font_size_xs: 11.5,
        font_size_sm: 12.5,
        font_size_md: 14.0,
        font_size_lg: 16.0,
        ..Theme::dark().metrics
    };
    Theme { mode: if p.dark { ThemeMode::Dark } else { ThemeMode::Light }, colors, metrics }
}

/// The palette currently installed (widgets read their colours from here).
pub fn pal(ctx: &Context) -> Palette {
    ctx.data(|d| d.get_temp::<Palette>(egui::Id::new("catalyx-palette"))).unwrap_or_else(|| palette_for(ThemeId::Focus, true))
}

/// Install a palette: the component theme first, then egui's own visuals on top of it.
pub fn apply(ctx: &Context, p: &Palette) {
    component_theme(p).install(ctx);
    ctx.data_mut(|d| d.insert_temp(egui::Id::new("catalyx-palette"), *p));
    ctx.all_styles_mut(|s| {
        let v = &mut s.visuals;
        v.dark_mode = p.dark;
        v.override_text_color = None;
        v.window_fill = p.popup;
        v.panel_fill = p.bg;
        v.faint_bg_color = p.well;
        v.extreme_bg_color = p.well;
        v.code_bg_color = p.well;
        v.window_stroke = Stroke::new(1.0, p.line);
        v.window_corner_radius = egui::CornerRadius::same(12);
        v.menu_corner_radius = egui::CornerRadius::same(10);
        v.selection.bg_fill = alpha(p.accent, 80);
        v.selection.stroke = Stroke::new(1.0, p.accent);
        v.hyperlink_color = p.accent;
        v.warn_fg_color = p.warn;
        v.error_fg_color = p.danger;
        let r = egui::CornerRadius::same(8);
        for (w, fill, stroke) in [
            (&mut v.widgets.noninteractive, p.panel, Stroke::new(1.0, p.line)),
            (&mut v.widgets.inactive, p.panel_hi, Stroke::NONE),
            (&mut v.widgets.hovered, mix(p.panel_hi, p.text, 0.07), Stroke::new(1.0, p.line_hi)),
            (&mut v.widgets.active, mix(p.panel_hi, p.text, 0.13), Stroke::new(1.0, p.line_hi)),
            (&mut v.widgets.open, p.panel_hi, Stroke::new(1.0, p.line_hi)),
        ] {
            w.bg_fill = fill;
            w.weak_bg_fill = fill;
            w.bg_stroke = stroke;
            w.fg_stroke = Stroke::new(1.0, p.text);
            w.corner_radius = r;
        }
        // Plain labels are full-strength text; secondary text asks for dim/aint explicitly.
        v.widgets.noninteractive.fg_stroke = Stroke::new(1.0, p.text);
        // Text is not selectable: a press on a label (the title, the meters) must reach the title bar's
        // drag handler, and nothing in this UI is meant to be copied.
        s.interaction.selectable_labels = false;
        s.spacing.item_spacing = egui::vec2(8.0, 8.0);
        s.spacing.button_padding = egui::vec2(12.0, 6.0);
        s.spacing.interact_size.y = 30.0;
        s.spacing.scroll.bar_width = 6.0;
        s.spacing.scroll.floating = true;
        s.text_styles = [
            (TextStyle::Small, FontId::proportional(12.0)),
            (TextStyle::Body, FontId::proportional(14.0)),
            (TextStyle::Button, FontId::proportional(14.0)),
            (TextStyle::Heading, FontId::new(22.0, FontFamily::Name("bold".into()))),
            (TextStyle::Monospace, FontId::monospace(12.5)),
        ]
        .into();
    });
}

/// Windows' own Segoe UI (regular, semibold/bold) and Consolas, read from the system fonts folder:
/// nothing is bundled, and the program looks native. Falls back to egui's built-in fonts.
pub fn install_fonts(ctx: &Context) {
    let mut fonts = FontDefinitions::default();
    let dir = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let load = |file: &str| std::fs::read(format!("{dir}\\Fonts\\{file}")).ok();
    if let Some(bytes) = load("segoeui.ttf") {
        fonts.font_data.insert("segoe".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Proportional).or_default().insert(0, "segoe".into());
    }
    let bold = load("segoeuib.ttf").or_else(|| load("seguisb.ttf"));
    let bold_name = if let Some(bytes) = bold {
        fonts.font_data.insert("segoe-bold".into(), Arc::new(FontData::from_owned(bytes)));
        "segoe-bold".to_string()
    } else {
        "Ubuntu-Light".to_string()
    };
    let mut bold_family = vec![bold_name];
    bold_family.extend(fonts.families.get(&FontFamily::Proportional).cloned().unwrap_or_default());
    fonts.families.insert(FontFamily::Name("bold".into()), bold_family);
    if let Some(bytes) = load("consola.ttf") {
        fonts.font_data.insert("consolas".into(), Arc::new(FontData::from_owned(bytes)));
        fonts.families.entry(FontFamily::Monospace).or_default().insert(0, "consolas".into());
    }
    ctx.set_fonts(fonts);
}

// ------------------------------------------------------------------ accents

/// An accent colour on top of a look. `Default` keeps the look's own accent and surfaces exactly
/// as designed; every other accent recolours the boost control, selections and switches, and lets a
/// little of its hue bleed into the surfaces (window, sidebar, cards) so the whole app feels of
/// one colour without any single element shouting.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum AccentId {
    Default,
    Periwinkle,
    Ember,
    Copper,
    Rose,
    Sage,
    Ocean,
    Violet,
    Gold,
}

impl AccentId {
    pub const ALL: [AccentId; 9] = [
        AccentId::Default,
        AccentId::Periwinkle,
        AccentId::Ember,
        AccentId::Copper,
        AccentId::Rose,
        AccentId::Sage,
        AccentId::Ocean,
        AccentId::Violet,
        AccentId::Gold,
    ];
    pub fn key(self) -> &'static str {
        match self {
            AccentId::Default => "default",
            AccentId::Periwinkle => "periwinkle",
            AccentId::Ember => "ember",
            AccentId::Copper => "copper",
            AccentId::Rose => "rose",
            AccentId::Sage => "sage",
            AccentId::Ocean => "ocean",
            AccentId::Violet => "violet",
            AccentId::Gold => "gold",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            AccentId::Default => "Look's own",
            AccentId::Periwinkle => "Periwinkle",
            AccentId::Ember => "Ember",
            AccentId::Copper => "Copper",
            AccentId::Rose => "Rose",
            AccentId::Sage => "Sage",
            AccentId::Ocean => "Ocean",
            AccentId::Violet => "Violet",
            AccentId::Gold => "Gold",
        }
    }
    pub fn from_key(s: &str) -> AccentId {
        AccentId::ALL.into_iter().find(|a| a.key().eq_ignore_ascii_case(s)).unwrap_or(AccentId::Default)
    }
    /// (colour on dark surfaces, colour on light surfaces): pastel on dark, deeper on light so
    /// text and icons stay readable on both.
    fn colors(self) -> Option<(Color32, Color32)> {
        Some(match self {
            AccentId::Default => return None,
            AccentId::Periwinkle => (h(0x9aa8ff), h(0x5664dc)),
            AccentId::Ember => (h(0xff9a62), h(0xc4501b)),
            AccentId::Copper => (h(0xe3a47c), h(0xa6612f)),
            AccentId::Rose => (h(0xf59db8), h(0xc2386a)),
            AccentId::Sage => (h(0x9ccfb0), h(0x3f8a63)),
            AccentId::Ocean => (h(0x6cc4e8), h(0x0b73a8)),
            AccentId::Violet => (h(0xb8a4ff), h(0x6a4ce0)),
            AccentId::Gold => (h(0xebcb72), h(0x9a7410)),
        })
    }
    /// The swatch shown in the picker for this accent under the given look.
    pub fn swatch(self, look: ThemeId, dark: bool) -> Color32 {
        match self.colors() {
            Some((d, l)) => {
                if dark {
                    d
                } else {
                    l
                }
            }
            None => palette_for(look, dark).accent,
        }
    }
}

/// How strongly the accent's hue bleeds into the surfaces: 0 off, 1 subtle, 2 rich.
pub const TINT_LABELS: [&str; 3] = ["Off", "Subtle", "Rich"];

/// The full palette for a look, an accent, a tint strength and light/dark.
pub fn palette(look: ThemeId, accent: AccentId, tint: u8, dark: bool) -> Palette {
    let mut p = palette_for(look, dark);
    let Some((ad, al)) = accent.colors() else { return p };
    let a = if dark { ad } else { al };
    let s = match tint {
        0 => 0.0,
        1 => 1.0,
        _ => 1.9,
    };
    let k = |base: f32| (base * s).min(0.45);
    // Share of the accent mixed into each surface. Dark surfaces carry a little more hue;
    // hairlines carry the most so edges pick the colour up without any surface looking dyed.
    let (bg, side, panel, hi, well, line, line_hi, popup) = if dark {
        (0.030, 0.040, 0.045, 0.070, 0.030, 0.075, 0.090, 0.055)
    } else {
        (0.045, 0.060, 0.030, 0.060, 0.030, 0.070, 0.090, 0.030)
    };
    p.bg = mix(p.bg, a, k(bg));
    p.sidebar = mix(p.sidebar, a, k(side));
    p.panel = mix(p.panel, a, k(panel));
    p.panel_hi = mix(p.panel_hi, a, k(hi));
    p.well = mix(p.well, a, k(well));
    p.line = mix(p.line, a, k(line));
    p.line_hi = mix(p.line_hi, a, k(line_hi));
    p.popup = mix(p.popup, a, k(popup));
    p.accent = a;
    p.accent_soft = mix(p.panel, a, if dark { 0.22 } else { 0.14 });
    p.on_accent = if dark { mix(h(0x111113), a, 0.10) } else { mix(Color32::WHITE, a, 0.05) };
    p.boost_top = mix(a, Color32::WHITE, if dark { 0.30 } else { 0.26 });
    p.boost_bottom = if dark { mix(a, Color32::BLACK, 0.10) } else { a };
    p
}
