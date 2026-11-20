//! Presets and per-game settings: `presets`, `preset`, `show`, `set`.
//!
//! `show` builds its text as a string instead of printing it line by line, so the test below can pin
//! every line of it: the text is what scripts and people read, and a refactor must not reword it.
//!
//! `show-balanced.txt` is that pinned text. A deliberate change to what a preset shows means updating
//! the file in the same commit (the test prints the new text when it fails).

use super::*;

impl Cli {
    pub(super) fn presets(&self) -> Result<(), String> {
        let cfg = Config::load();
        if self.json {
            println!("{}", serde_json::to_string_pretty(&cfg.presets).unwrap_or_default());
            return Ok(());
        }
        for p in &cfg.presets {
            let d = if p.id == cfg.default_preset { " (default)" } else { "" };
            println!("{:<16} {}{}", p.name, p.about, d);
        }
        Ok(())
    }

    pub(super) fn preset(&self) -> Result<(), String> {
        let mut cfg = Config::load();
        let g = find_game(&cfg, &self.arg(1, "game")?)?;
        let p = find_preset(&cfg, &self.arg(2, "preset")?)?;
        cfg.games[g].preset = cfg.presets[p].id.clone();
        cfg.games[g].custom = None;
        cfg.save();
        println!("{} uses {}", cfg.games[g].name, cfg.presets[p].name);
        Ok(())
    }

    pub(super) fn show(&self) -> Result<(), String> {
        let cfg = Config::load();
        let q = self.arg(1, "game or preset")?;
        if self.json {
            let (_, s) = show_target(&cfg, &q)?;
            println!("{}", serde_json::to_string_pretty(&s).unwrap_or_default());
            return Ok(());
        }
        print!("{}", show_text(&cfg, &q)?);
        Ok(())
    }

    pub(super) fn set(&self) -> Result<(), String> {
        let mut cfg = Config::load();
        let q = self.arg(1, "game or preset")?;
        let key = self.arg(2, "setting")?.to_lowercase();
        let val = self.arg(3, "value")?.to_lowercase();
        let target = target_of(&cfg, &q)?;
        let mut s = match &target {
            Target::Game(_) => cfg.settings_for(game_of(&cfg, &target).ok()),
            Target::Preset(id) => cfg.presets.iter().find(|p| p.id == *id).map(|p| p.settings.clone()).unwrap_or_default(),
        };
        spec::set(&mut s, &key, &val)?;
        let who = match &target {
            Target::Game(id) => {
                let g = cfg.games.iter_mut().find(|g| g.id == *id).ok_or_else(|| format!("no game with id '{id}'"))?;
                g.custom = Some(s);
                g.name.clone()
            }
            Target::Preset(id) => {
                let p = cfg.presets.iter_mut().find(|p| p.id == *id).ok_or_else(|| format!("no preset with id '{id}'"))?;
                p.settings = s;
                p.name.clone()
            }
        };
        cfg.save();
        println!("{who}: {key} = {val}");
        Ok(())
    }
}

/// A game or a preset, by name. The target carries its id, as the API does everywhere.
fn target_of(cfg: &Config, q: &str) -> Result<Target, String> {
    if let Ok(i) = find_game(cfg, q) {
        return Ok(Target::Game(cfg.games[i].id.clone()));
    }
    match find_preset(cfg, q) {
        Ok(i) => Ok(Target::Preset(cfg.presets[i].id.clone())),
        Err(_) => Err(format!("no game or preset matches '{q}'")),
    }
}

/// The game a target names. The target was made from this very config, so it is always there.
fn game_of<'a>(cfg: &'a Config, target: &Target) -> Result<&'a crate::config::Game, String> {
    match target {
        Target::Game(id) => cfg.games.iter().find(|g| g.id == *id).ok_or_else(|| format!("no game with id '{id}'")),
        Target::Preset(_) => Err("not a game".into()),
    }
}

/// What the target is called and the settings it uses.
fn show_target(cfg: &Config, q: &str) -> Result<(String, crate::config::Settings), String> {
    let target = target_of(cfg, q)?;
    match &target {
        Target::Game(_) => {
            let g = game_of(cfg, &target)?;
            Ok((format!("{} ({})", g.name, profile_name(cfg, g)), cfg.settings_for(Some(g))))
        }
        Target::Preset(id) => {
            let p = cfg.presets.iter().find(|p| p.id == *id).ok_or_else(|| format!("no preset with id '{id}'"))?;
            Ok((format!("Preset {}", p.name), p.settings.clone()))
        }
    }
}

/// The text `show` prints: a title, then one line per setting in catalogue order.
fn show_text(cfg: &Config, q: &str) -> Result<String, String> {
    let (title, s) = show_target(cfg, q)?;
    let mut out = format!("{title}\n");
    for (k, v) in spec::describe(&s) {
        out.push_str(&format!("  {k:<20} {v}\n"));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn show_pins_every_line_of_a_preset() {
        let text = show_text(&Config::default(), "Balanced").expect("the built-in preset is always there");
        let pinned = include_str!("show-balanced.txt");
        if text != pinned {
            eprintln!("--- what `show Balanced` prints now ---\n{text}--- end ---");
        }
        assert_eq!(text, pinned, "the text `show` prints changed; if that was intended, update show-balanced.txt");
    }

    #[test]
    fn a_target_that_matches_nothing_is_refused_in_the_same_words() {
        let err = target_of(&Config::default(), "Nope").unwrap_err();
        assert_eq!(err, "no game or preset matches 'Nope'");
    }
}
