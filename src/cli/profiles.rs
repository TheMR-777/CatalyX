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

    /// A game or a preset, by name.
    pub(super) fn target(&self, cfg: &Config, q: &str) -> Result<Target, String> {
        if let Ok(i) = find_game(cfg, q) {
            return Ok(Target::Game(i));
        }
        find_preset(cfg, q).map(Target::Preset).map_err(|_| format!("no game or preset matches '{q}'"))
    }

    pub(super) fn show(&self) -> Result<(), String> {
        let cfg = Config::load();
        let q = self.arg(1, "game or preset")?;
        let (title, s) = match self.target(&cfg, &q)? {
            Target::Game(i) => {
                (format!("{} ({})", cfg.games[i].name, profile_name(&cfg, &cfg.games[i])), cfg.settings_for(cfg.games.get(i)))
            }
            Target::Preset(i) => (format!("Preset {}", cfg.presets[i].name), cfg.presets[i].settings.clone()),
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
        let mut s = match target {
            Target::Game(i) => cfg.settings_for(cfg.games.get(i)),
            Target::Preset(i) => cfg.presets[i].settings.clone(),
        };
        spec::set(&mut s, &key, &val)?;
        let who = match target {
            Target::Game(i) => {
                cfg.games[i].custom = Some(s);
                cfg.games[i].name.clone()
            }
            Target::Preset(i) => {
                cfg.presets[i].settings = s;
                cfg.presets[i].name.clone()
            }
        };
        cfg.save();
        println!("{who}: {key} = {val}");
        Ok(())
    }
}
