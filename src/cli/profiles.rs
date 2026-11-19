//! Presets and per-game settings: `presets`, `preset`, `show`, `set`.

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

    /// A game or a preset, by name. The target carries its id, as the API does everywhere.
    pub(super) fn target(&self, cfg: &Config, q: &str) -> Result<Target, String> {
        if let Ok(i) = find_game(cfg, q) {
            return Ok(Target::Game(cfg.games[i].id.clone()));
        }
        match find_preset(cfg, q) {
            Ok(i) => Ok(Target::Preset(cfg.presets[i].id.clone())),
            Err(_) => Err(format!("no game or preset matches '{q}'")),
        }
    }

    /// The game a target names. The target was made from this very config, so it is always there.
    fn game_of<'a>(&self, cfg: &'a Config, target: &Target) -> Result<&'a crate::config::Game, String> {
        match target {
            Target::Game(id) => cfg.games.iter().find(|g| g.id == *id).ok_or_else(|| format!("no game with id '{id}'")),
            Target::Preset(_) => Err("not a game".into()),
        }
    }

    pub(super) fn show(&self) -> Result<(), String> {
        let cfg = Config::load();
        let q = self.arg(1, "game or preset")?;
        let target = self.target(&cfg, &q)?;
        let (title, s) = match &target {
            Target::Game(_) => {
                let g = self.game_of(&cfg, &target)?;
                (format!("{} ({})", g.name, profile_name(&cfg, g)), cfg.settings_for(Some(g)))
            }
            Target::Preset(id) => {
                let p = cfg.presets.iter().find(|p| p.id == *id).ok_or_else(|| format!("no preset with id '{id}'"))?;
                (format!("Preset {}", p.name), p.settings.clone())
            }
        };
        if self.json {
            println!("{}", serde_json::to_string_pretty(&s).unwrap_or_default());
            return Ok(());
        }
        println!("{title}");
        for (k, v) in spec::describe(&s) {
            println!("  {k:<20} {v}");
        }
        Ok(())
    }

    pub(super) fn set(&self) -> Result<(), String> {
        let mut cfg = Config::load();
        let q = self.arg(1, "game or preset")?;
        let key = self.arg(2, "setting")?.to_lowercase();
        let val = self.arg(3, "value")?.to_lowercase();
        let target = self.target(&cfg, &q)?;
        let mut s = match &target {
            Target::Game(_) => cfg.settings_for(self.game_of(&cfg, &target).ok()),
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
