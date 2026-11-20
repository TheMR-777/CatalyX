//! The game library: `games`, `add`, `remove`, `rename`, `exe`.

use super::*;

impl Cli {
    pub(super) fn games(&self) -> Result<(), String> {
        let cfg = Config::load();
        if self.json {
            println!("{}", serde_json::to_string_pretty(&cfg.games).unwrap_or_default());
            return Ok(());
        }
        if cfg.games.is_empty() {
            println!("No games yet. Add one: catalyx add \"C:\\Games\\Game\\game.exe\"");
            return Ok(());
        }
        println!("{:>3}  {:<28} {:<16} {:<22} {:>5}", "#", "Game", "Profile", "Real program", "Plays");
        for (i, g) in cfg.games.iter().enumerate() {
            let real = if g.real_exe.is_empty() { "(found on first launch)" } else { &g.real_exe };
            println!(
                "{:>3}  {:<28} {:<16} {:<22} {:>5}",
                i + 1,
                trunc(&g.name, 28),
                trunc(&profile_name(&cfg, g), 16),
                trunc(real, 22),
                g.plays
            );
        }
        Ok(())
    }

    pub(super) fn add(&self) -> Result<(), String> {
        let path = self.arg(1, "path to the game")?.trim_matches('"').to_string();
        let mut cfg = Config::load();
        let lower = path.to_lowercase();
        let is_link = lower.contains("://");
        if !is_link && !std::path::Path::new(&path).exists() {
            return Err(format!("file not found: {path}"));
        }
        let name = self
            .value("--name")
            .unwrap_or_else(|| std::path::Path::new(&path).file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or(path.clone()));
        let preset = match self.value("--preset") {
            Some(p) => cfg.presets[find_preset(&cfg, &p)?].id.clone(),
            None => cfg.default_preset.clone(),
        };
        let real = if is_link { String::new() } else { crate::launcher::predict_real_exe(&path).unwrap_or_default() };
        if !real.is_empty() && config::file_name_lower(&path) != real {
            println!("  Real game program (from the launcher): {real}");
        }
        cfg.games.push(Game {
            id: cfg.new_id("game"),
            name: name.clone(),
            launch: path,
            args: self.value("--args").unwrap_or_default(),
            real_exe: real,
            preset,
            ..Default::default()
        });
        cfg.selected_game = cfg.games.len() - 1;
        cfg.save();
        println!("Added {name} (#{})", cfg.games.len());
        Ok(())
    }

    pub(super) fn edit_game(&self, f: impl FnOnce(&mut Config, usize) -> String) -> Result<(), String> {
        let mut cfg = Config::load();
        let i = find_game(&cfg, &self.arg(1, "game")?)?;
        let msg = f(&mut cfg, i);
        cfg.save();
        println!("{msg}");
        Ok(())
    }
}
