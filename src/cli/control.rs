//! App and service rules: `apps`, `app`, `services`, `service`.

use super::*;

impl Cli {
    pub(super) fn scope(&self, cfg: &Config) -> Result<Option<usize>, String> {
        match self.value("--game") {
            Some(g) => find_game(cfg, &g).map(Some),
            None => Ok(None),
        }
    }

    pub(super) fn apps(&self) -> Result<(), String> {
        let cfg = Config::load();
        let g = self.scope(&cfg)?;
        let list = cfg.apps_for(g.and_then(|i| cfg.games.get(i)));
        if self.json {
            println!("{}", serde_json::to_string_pretty(&list).unwrap_or_default());
            return Ok(());
        }
        for (k, v) in list.iter().filter(|(_, v)| **v != ProcAction::Keep) {
            let own = g.map(|i| cfg.games[i].apps.contains_key(k)).unwrap_or(false);
            println!("{:<7} {}{}", v.label(), k, if own { "  (this game only)" } else { "" });
        }
        Ok(())
    }

    pub(super) fn app(&self) -> Result<(), String> {
        let mut cfg = Config::load();
        let exe = config::file_name_lower(&self.arg(1, "program name")?);
        let exe = if exe.ends_with(".exe") { exe } else { format!("{exe}.exe") };
        if config::app_is_protected(&exe) {
            return Err(format!("{exe} is protected (Windows, security or the shell) and is never touched"));
        }
        let action = match self.arg(2, "keep, pause, lower or close")?.to_lowercase().as_str() {
            "keep" => ProcAction::Keep,
            "pause" | "suspend" => ProcAction::Suspend,
            "lower" => ProcAction::Lower,
            "close" | "end" | "kill" => ProcAction::Close,
            o => return Err(format!("unknown action '{o}' (keep, pause, lower, close)")),
        };
        match self.scope(&cfg)? {
            None => {
                if action == ProcAction::Keep {
                    cfg.apps.remove(&exe);
                } else {
                    cfg.apps.insert(exe.clone(), action);
                }
                println!("{exe}: {} for all games", action.label());
            }
            Some(i) => {
                let global = cfg.apps.get(&exe).copied().unwrap_or(ProcAction::Keep);
                if action == global {
                    cfg.games[i].apps.remove(&exe);
                } else {
                    cfg.games[i].apps.insert(exe.clone(), action);
                }
                println!("{exe}: {} for {}", action.label(), cfg.games[i].name);
            }
        }
        cfg.save();
        Ok(())
    }

    pub(super) fn services(&self) -> Result<(), String> {
        let cfg = Config::load();
        let g = self.scope(&cfg)?;
        let rules = cfg.services_for(g.and_then(|i| cfg.games.get(i)));
        let live = services::list();
        if self.json {
            let v: Vec<_> = live
                .iter()
                .map(|s| {
                    let stop = rules.iter().find(|(k, _)| k.eq_ignore_ascii_case(&s.name)).map(|(_, v)| *v).unwrap_or(false);
                    json!({"name": s.name, "display": s.display, "running": s.running, "stop_while_gaming": stop, "protected": config::service_is_protected(&s.name), "can_start_again": services::can_start(&s.name)})
                })
                .collect();
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            return Ok(());
        }
        let all = self.flag("--all");
        println!("{:<6} {:<8} {:<26} Name", "Rule", "State", "Service");
        for s in &live {
            let stop = rules.iter().find(|(k, _)| k.eq_ignore_ascii_case(&s.name)).map(|(_, v)| *v).unwrap_or(false);
            if !all && !stop {
                continue;
            }
            let rule = if config::service_is_protected(&s.name) {
                "kept"
            } else if stop && !services::can_start(&s.name) {
                // Windows lets anybody stop it, but lets nobody start it again: stopping it would
                // leave a change that can never be undone.
                "safe"
            } else if stop {
                "stop"
            } else {
                "-"
            };
            println!("{:<6} {:<8} {:<26} {}", rule, if s.running { "running" } else { "stopped" }, trunc(&s.name, 26), s.display);
        }
        if !all {
            println!("(services that stop while gaming; add --all for every service)");
        }
        Ok(())
    }

    pub(super) fn service(&self) -> Result<(), String> {
        let mut cfg = Config::load();
        let name = self.arg(1, "service name")?;
        if config::service_is_protected(&name) {
            return Err(format!("{name} is needed by Windows or games and is never stopped"));
        }
        let stop = match self.arg(2, "stop or keep")?.to_lowercase().as_str() {
            "stop" | "on" | "off-while-gaming" => true,
            "keep" | "off" => false,
            o => return Err(format!("unknown choice '{o}' (stop, keep)")),
        };
        let live = services::list();
        let real = live
            .iter()
            .find(|s| s.name.eq_ignore_ascii_case(&name) || s.display.eq_ignore_ascii_case(&name))
            .map(|s| s.name.clone())
            .unwrap_or(name.clone());
        match self.scope(&cfg)? {
            None => {
                let key = cfg.services.keys().find(|k| k.eq_ignore_ascii_case(&real)).cloned().unwrap_or(real.clone());
                cfg.services.insert(key, stop);
                println!("{real}: {} for all games", if stop { "stops while gaming" } else { "kept running" });
            }
            Some(i) => {
                cfg.games[i].services.insert(real.clone(), stop);
                println!("{real}: {} for {}", if stop { "stops while gaming" } else { "kept running" }, cfg.games[i].name);
            }
        }
        cfg.save();
        Ok(())
    }
}
