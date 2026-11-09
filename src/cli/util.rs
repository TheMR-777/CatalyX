//! Small helpers shared by the commands: finding games and presets by name, parsing on/off.

use super::*;

pub(super) enum Target {
    Game(usize),
    Preset(usize),
}

pub(super) fn trunc(s: &str, n: usize) -> String {
    if s.chars().count() <= n {
        s.to_string()
    } else {
        s.chars().take(n.saturating_sub(1)).collect::<String>() + "…"
    }
}

pub(super) fn profile_name(cfg: &Config, g: &Game) -> String {
    if g.custom.is_some() {
        "own settings".into()
    } else {
        cfg.preset(&g.preset).name.clone()
    }
}

/// Finds by number, id, exact name, then a unique part of the name.
pub(super) fn find_by<T>(items: &[T], q: &str, id: impl Fn(&T) -> &str, name: impl Fn(&T) -> &str, what: &str) -> Result<usize, String> {
    let ql = q.to_lowercase();
    if let Ok(n) = q.parse::<usize>() {
        if n >= 1 && n <= items.len() {
            return Ok(n - 1);
        }
    }
    if let Some(i) = items.iter().position(|t| id(t).eq_ignore_ascii_case(q) || name(t).eq_ignore_ascii_case(q)) {
        return Ok(i);
    }
    let hits: Vec<usize> = items.iter().enumerate().filter(|(_, t)| name(t).to_lowercase().contains(&ql)).map(|(i, _)| i).collect();
    match hits.as_slice() {
        [i] => Ok(*i),
        [] => Err(format!("no {what} matches '{q}'")),
        _ => Err(format!("'{q}' matches {} {what}s; be more specific", hits.len())),
    }
}

pub(super) fn find_game(cfg: &Config, q: &str) -> Result<usize, String> {
    find_by(&cfg.games, q, |g| &g.id, |g| &g.name, "game")
}

pub(super) fn find_preset(cfg: &Config, q: &str) -> Result<usize, String> {
    find_by(&cfg.presets, q, |p| &p.id, |p| &p.name, "preset")
}

pub(super) fn parse_on(v: &str) -> Result<bool, String> {
    match v.to_lowercase().as_str() {
        "on" | "yes" | "true" | "1" => Ok(true),
        "off" | "no" | "false" | "0" => Ok(false),
        o => Err(format!("expected on or off, got '{o}'")),
    }
}

pub(super) fn onoff(b: bool) -> String {
    if b {
        "on".into()
    } else {
        "off".into()
    }
}
