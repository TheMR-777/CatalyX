//! The classic skin's look preferences: the only place these five choices, their defaults and their
//! stored keys live.
//!
//! The core keeps them under `skins.classic` and never interprets a key or a value (docs/SKINS.md),
//! so a stored value this build no longer accepts simply reads as the default: the window never
//! comes up broken because of something written by another build.

use super::control::Style;
use super::theme::{AccentId, ThemeId};
use catalyx_core::config::Config;

/// The skin's name: namespaces its preferences in the config and names it in an `Intent::SetPref`.
pub const NAME: &str = "classic";

// The stored keys, in one place so a writer and a reader cannot drift apart.
pub const THEME: &str = "theme";
pub const APPEARANCE: &str = "appearance";
pub const ACCENT: &str = "accent";
pub const TINT: &str = "tint";
pub const CONTROL: &str = "boost_control";

/// Which mode the window follows.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Appearance {
    Dark,
    Light,
    /// Follow Windows' own light/dark choice.
    System,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::Dark, Appearance::Light, Appearance::System];

    pub fn key(self) -> &'static str {
        match self {
            Appearance::Dark => "dark",
            Appearance::Light => "light",
            Appearance::System => "system",
        }
    }

    pub fn from_key(s: &str) -> Appearance {
        match s.to_lowercase().as_str() {
            "light" => Appearance::Light,
            "system" => Appearance::System,
            _ => Appearance::Dark,
        }
    }
}

/// The look this skin is wearing, as typed choices.
#[derive(Clone, Copy, PartialEq, Debug)]
pub struct Prefs {
    pub theme: ThemeId,
    pub accent: AccentId,
    pub tint: u8,
    pub control: Style,
    pub appearance: Appearance,
}

impl Prefs {
    /// Reads the skin's preferences, falling back to its own defaults for anything unset or no
    /// longer understood.
    pub fn from_config(cfg: &Config) -> Prefs {
        fn get<'a>(cfg: &'a Config, key: &str, default: &'a str) -> &'a str {
            cfg.pref(NAME, key).unwrap_or(default)
        }
        Prefs {
            theme: ThemeId::from_key(get(cfg, THEME, "focus")),
            accent: AccentId::from_key(get(cfg, ACCENT, "default")),
            tint: get(cfg, TINT, "1").parse::<u8>().map(|n| n.min(2)).unwrap_or(1),
            control: Style::from_key(get(cfg, CONTROL, "bar")),
            appearance: Appearance::from_key(get(cfg, APPEARANCE, "dark")),
        }
    }
}
