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
        for (k, v) in describe(&s) {
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
        apply_setting(&mut s, &key, &val)?;
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

fn describe(s: &Settings) -> Vec<(&'static str, String)> {
    vec![
        ("priority", s.priority.label().into()),
        ("power", s.power.label().into()),
        ("gpu", s.gpu.label().into()),
        ("cpu", s.cpu.label().into()),
        ("apps", onoff(s.apps)),
        ("services", onoff(s.services)),
        ("clean-ram", onoff(s.clean_ram)),
        ("io-boost", onoff(s.io_boost)),
        ("purge-standby", onoff(s.purge_standby)),
        ("game-mode", onoff(s.windows_game_mode)),
        ("dvr-off", onoff(s.game_dvr_off)),
        ("notifications-off", onoff(s.notifications_off)),
        ("transparency-off", onoff(s.transparency_off)),
        ("maintenance-off", onoff(s.pause_maintenance)),
        ("mmcss", onoff(s.mmcss_games)),
        ("no-throttling", onoff(s.no_power_throttling)),
        ("lower-others", onoff(s.lower_others)),
        ("quiet-desktop", onoff(s.quiet_desktop)),
    ]
}

fn apply_setting(s: &mut Settings, key: &str, val: &str) -> Result<(), String> {
    match key {
        "priority" => {
            s.priority = match val {
                "normal" => Priority::Normal,
                "above" | "abovenormal" | "above-normal" => Priority::AboveNormal,
                "high" => Priority::High,
                o => return Err(format!("priority: normal, above or high (got '{o}'); realtime can freeze Windows input")),
            }
        }
        "power" => {
            s.power = match val {
                "keep" => PowerChoice::Keep,
                "high" | "high-performance" => PowerChoice::HighPerformance,
                "catalyx" | "pureboost" | "boost" => PowerChoice::BoostPlan,
                o => return Err(format!("power: keep, high or catalyx (got '{o}')")),
            }
        }
        "cpu" => {
            s.cpu = match val {
                "off" => CpuLevel::Off,
                "boost" => CpuLevel::Boost,
                "max" | "max-sustained" => CpuLevel::Max,
                o => return Err(format!("cpu: off, boost or max (got '{o}')")),
            }
        }
        "cpu-boost" => s.cpu = if parse_on(val)? { CpuLevel::Boost } else { CpuLevel::Off },
        "gpu" => {
            s.gpu = match val {
                "off" => GpuLevel::Off,
                "safe" => GpuLevel::Safe,
                "balanced" => GpuLevel::Balanced,
                o => return Err(format!("gpu: off, safe or balanced (got '{o}')")),
            }
        }
        _ => {
            let b = parse_on(val)?;
            let field = match key {
                "apps" => &mut s.apps,
                "services" => &mut s.services,
                "clean-ram" => &mut s.clean_ram,
                "io-boost" => &mut s.io_boost,
                "purge-standby" => &mut s.purge_standby,
                "game-mode" => &mut s.windows_game_mode,
                "dvr-off" => &mut s.game_dvr_off,
                "notifications-off" => &mut s.notifications_off,
                "transparency-off" => &mut s.transparency_off,
                "maintenance-off" => &mut s.pause_maintenance,
                "mmcss" => &mut s.mmcss_games,
                "no-throttling" => &mut s.no_power_throttling,
                "lower-others" => &mut s.lower_others,
                "quiet-desktop" => &mut s.quiet_desktop,
                o => return Err(format!("unknown setting '{o}' (see `catalyx help`)")),
            };
            *field = b;
        }
    }
    Ok(())
}
