//! The command line. `catalyx <command> ...` works without opening the window, so boosts can be
//! started from shortcuts, scripts, Steam launch options or a terminal. No arguments opens the window.

mod admin;
mod control;
mod help;
mod library;
mod play;
mod profiles;
mod system;
mod util;

use crate::config::{self, Config, CpuLevel, Game, GpuLevel, PowerChoice, Priority, ProcAction, Settings};
use crate::engine::{self, BoostRequest, Cmd, Phase};
use crate::{ffi, gpu, power, procs, services};
use help::HELP;
use serde_json::json;
use std::time::{Duration, Instant};
use util::*;

/// True when rgs is a command that prints something, rather than a request to open the window.
pub fn wants_console(args: &[String]) -> bool {
    match args.first() {
        None => false,
        Some(f) => !(f.eq_ignore_ascii_case("gui") || (f.starts_with("--") && !matches!(f.as_str(), "--help" | "--version" | "-h"))),
    }
}

/// Runs a CLI command. `None` means "open the window" (no command, or window-only flags).
pub fn run(args: &[String]) -> Option<i32> {
    if !wants_console(args) {
        return None; // no command, "gui", or window-only flags (--screenshot, --self-test, --tab, --theme ...)
    }
    let mut c = Cli::new(args);
    let code = match c.dispatch() {
        Ok(()) => 0,
        Err(e) => {
            eprintln!("catalyx: {e}");
            if e.starts_with("unknown command") {
                eprintln!("Run `catalyx help` for the list of commands.");
            }
            2
        }
    };
    Some(code)
}

struct Cli {
    pos: Vec<String>,
    flags: Vec<(String, Option<String>)>,
    json: bool,
}

impl Cli {
    fn new(args: &[String]) -> Cli {
        let mut pos = Vec::new();
        let mut flags = Vec::new();
        let valued = ["--preset", "--name", "--args", "--game", "--for"];
        let mut i = 0;
        while i < args.len() {
            let a = &args[i];
            if a.starts_with("--") {
                if valued.contains(&a.as_str()) {
                    flags.push((a.clone(), args.get(i + 1).cloned()));
                    i += 1;
                } else {
                    flags.push((a.clone(), None));
                }
            } else {
                pos.push(a.clone());
            }
            i += 1;
        }
        let json = flags.iter().any(|(f, _)| f == "--json");
        Cli { pos, flags, json }
    }

    fn flag(&self, name: &str) -> bool {
        self.flags.iter().any(|(f, _)| f == name)
    }

    fn value(&self, name: &str) -> Option<String> {
        self.flags.iter().find(|(f, _)| f == name).and_then(|(_, v)| v.clone())
    }

    fn arg(&self, i: usize, what: &str) -> Result<String, String> {
        self.pos.get(i).cloned().ok_or_else(|| format!("missing {what}"))
    }

    fn dispatch(&mut self) -> Result<(), String> {
        let cmd = self.pos.first().cloned().unwrap_or_else(|| "help".into()).to_lowercase();
        let cmd = match cmd.as_str() {
            "--help" | "-h" => "help".to_string(),
            "--version" => "version".to_string(),
            _ => cmd,
        };
        match cmd.as_str() {
            "help" | "?" => {
                print!("{HELP}");
                Ok(())
            }
            "version" => {
                println!("CatalyX {}", env!("CARGO_PKG_VERSION"));
                Ok(())
            }
            "play" => self.play(),
            "boost" => self.boost(),
            "attach" => self.attach(),
            "doctor" => self.doctor(),
            "restore" => self.restore(),
            "status" | "top" => self.status(),
            "games" | "list" => self.games(),
            "add" => self.add(),
            "remove" | "rm" => self.edit_game(|cfg, i| {
                let g = cfg.games.remove(i);
                if cfg.selected_game >= cfg.games.len() {
                    cfg.selected_game = cfg.games.len().saturating_sub(1);
                }
                format!("Removed {}", g.name)
            }),
            "rename" => {
                let name = self.pos[2..].join(" ");
                if name.trim().is_empty() {
                    return Err("missing new name".into());
                }
                self.edit_game(move |cfg, i| {
                    cfg.games[i].name = name.trim().to_string();
                    format!("Renamed to {}", cfg.games[i].name)
                })
            }
            "exe" => {
                let exe = self.arg(2, "program name")?;
                self.edit_game(move |cfg, i| {
                    cfg.games[i].real_exe = config::file_name_lower(&exe);
                    format!("{} now boosts {}", cfg.games[i].name, cfg.games[i].real_exe)
                })
            }
            "presets" => self.presets(),
            "preset" => self.preset(),
            "default-preset" => {
                let mut cfg = Config::load();
                let p = find_preset(&cfg, &self.arg(1, "preset")?)?;
                cfg.default_preset = cfg.presets[p].id.clone();
                cfg.save();
                println!("Default preset: {}", cfg.presets[p].name);
                Ok(())
            }
            "show" => self.show(),
            "set" => self.set(),
            "own" => self.edit_game(|cfg, i| {
                let s = cfg.settings_for(cfg.games.get(i));
                cfg.games[i].custom = Some(s);
                format!("{} now has its own settings", cfg.games[i].name)
            }),
            "inherit" => self.edit_game(|cfg, i| {
                cfg.games[i].custom = None;
                let p = cfg.preset(&cfg.games[i].preset).name.clone();
                format!("{} follows the {} preset again", cfg.games[i].name, p)
            }),
            "apps" => self.apps(),
            "app" => self.app(),
            "services" => self.services(),
            "service" => self.service(),
            "plans" => self.plans(),
            "plan" => self.plan(),
            "gpu" => self.gpu(),
            "theme" => self.theme(),
            "option" => self.option(),
            "heat-limit" => {
                let v: u32 = self
                    .arg(1, "temperature")?
                    .trim_end_matches(|c: char| !c.is_ascii_digit())
                    .parse()
                    .map_err(|_| "temperature must be a number")?;
                if ![75, 80, 83, 87].contains(&v) {
                    return Err("heat limit must be 75, 80, 83 or 87".into());
                }
                let mut cfg = Config::load();
                cfg.gpu_temp_limit_c = v;
                cfg.save();
                println!("Heat limit: {v} °C");
                Ok(())
            }
            "config" => self.config(),
            "elevate" => self.elevate_cmd(),
            "autostart" => self.autostart_cmd(),
            "watch" => crate::watch::run(self.flag("--quiet")),
            "desktop" => self.desktop_cmd(),
            "shell-guard" => {
                // Internal: started by a session that switched the desktop off (see shell.rs).
                let pid: u32 = self.arg(1, "pid")?.parse().map_err(|_| "bad pid")?;
                crate::engine::run_guard(pid);
                Ok(())
            }
            other => Err(format!("unknown command '{other}'")),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn a(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn commands_want_a_console_and_window_requests_do_not() {
        assert!(!wants_console(&a(&[])), "no arguments opens the window");
        assert!(!wants_console(&a(&["gui"])));
        assert!(!wants_console(&a(&["--screenshot", "x.png"])));
        assert!(!wants_console(&a(&["--self-test", "r.json"])));
        assert!(wants_console(&a(&["help"])));
        assert!(wants_console(&a(&["--help"])));
        assert!(wants_console(&a(&["--version"])));
        assert!(wants_console(&a(&["play", "My Game"])));
    }
}
