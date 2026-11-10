//! Administrator start, the quiet desktop, the background watcher and the config file:
//! `elevate`, `desktop`, `autostart`, `config`.

use super::*;

impl Cli {
    /// Runs `args` as administrator and waits (one Windows prompt) unless this is already elevated.
    pub(super) fn as_admin(&self, args: &str) -> Result<bool, String> {
        if ffi::is_admin() {
            return Ok(false); // caller does the work itself
        }
        println!("Approve the Windows prompt to continue...");
        if ffi::run_self_elevated_and_wait(args, 120_000) {
            Ok(true)
        } else {
            Err("the administrator prompt was cancelled, or the step did not finish".into())
        }
    }

    pub(super) fn elevate_cmd(&self) -> Result<(), String> {
        use crate::elevate as el;
        for m in el::migrate_legacy() {
            println!("Renamed scheduled task: {m}");
        }
        match self.arg(1, "setup, remove or status")?.as_str() {
            "status" => {
                println!("Administrator now:      {}", if ffi::is_admin() { "yes" } else { "no" });
                println!(
                    "One-click admin start:  {}",
                    if el::ui_task_ready() {
                        "set up"
                    } else if el::exists(el::TASK_UI) {
                        "set up for a different copy of CatalyX (run `elevate setup` again)"
                    } else {
                        "not set up"
                    }
                );
                println!("Background watcher:     {}", if el::watcher_enabled() { "on" } else { "off" });
                Ok(())
            }
            "setup" => {
                if self.as_admin("elevate setup")? {
                    return if el::ui_task_ready() {
                        println!("One-click administrator start is ready.");
                        Ok(())
                    } else {
                        Err("the task was not created".into())
                    };
                }
                el::create(el::TASK_UI, "", false)?;
                println!("One-click administrator start is ready: CatalyX now opens elevated without a prompt.");
                Ok(())
            }
            "remove" => {
                if self.as_admin("elevate remove")? {
                    println!("Removed.");
                    return Ok(());
                }
                el::delete(el::TASK_UI);
                println!("One-click administrator start removed.");
                Ok(())
            }
            "xml" => {
                // Exactly what setup registers, for review (and for validating with schtasks).
                let exe = el::gui_exe().ok_or("cannot tell where this program is")?;
                let watcher = self.pos.get(2).map(|s| s == "watch").unwrap_or(false);
                let user =
                    format!("{}\\{}", std::env::var("USERDOMAIN").unwrap_or_default(), std::env::var("USERNAME").unwrap_or_default());
                print!(
                    "{}",
                    if watcher {
                        el::task_xml(&exe.to_string_lossy(), "watch --quiet", true, &user)
                    } else {
                        el::task_xml(&exe.to_string_lossy(), "", false, &user)
                    }
                );
                Ok(())
            }
            o => Err(format!("unknown action '{o}' (setup, remove, status)")),
        }
    }

    /// `catalyx desktop ...`: look at, restore or briefly try the quiet desktop.
    pub(super) fn desktop_cmd(&self) -> Result<(), String> {
        let sub = self.pos.get(1).map(|s| s.to_lowercase()).unwrap_or_else(|| "status".into());
        match sub.as_str() {
            "status" => {
                println!("Windows desktop (taskbar): {}", if crate::shell::running() { "on" } else { "OFF" });
                if engine::journal_has_shell() {
                    println!("A game session switched it off and has not brought it back yet (`catalyx desktop restore` does).");
                }
                println!("Shortcut that brings it back during a game: {}", crate::shell::EXIT_KEYS);
                Ok(())
            }
            "restore" | "on" => {
                let how = crate::shell::start()?;
                engine::clear_shell_journal();
                println!("Windows desktop is back ({how}).");
                Ok(())
            }
            "test" => {
                let secs: u64 = self.pos.get(2).and_then(|s| s.parse().ok()).unwrap_or(8).clamp(3, 120);
                println!("Switching the Windows desktop off for {secs} seconds ({} ends it sooner)...", crate::shell::EXIT_KEYS);
                for l in engine::desktop_test(secs)? {
                    println!("  {l}");
                }
                Ok(())
            }
            o => Err(format!("desktop: status, restore or test [seconds] (got '{o}')")),
        }
    }
    pub(super) fn autostart_cmd(&self) -> Result<(), String> {
        use crate::elevate as el;
        for m in el::migrate_legacy() {
            println!("Renamed scheduled task: {m}");
        }
        match self.arg(1, "on, off or status")?.to_lowercase().as_str() {
            "status" => {
                println!("Background watcher: {}", if el::watcher_enabled() { "on (starts at sign-in)" } else { "off" });
                Ok(())
            }
            "on" => {
                if self.as_admin("autostart on")? {
                    return if el::watcher_enabled() {
                        println!("Background watcher enabled.");
                        Ok(())
                    } else {
                        Err("the task was not created".into())
                    };
                }
                el::create(el::TASK_WATCH, "watch --quiet", true)?;
                println!("Background watcher enabled: it starts at sign-in, uses under 10 MB, and boosts your games when they start.");
                Ok(())
            }
            "off" => {
                if self.as_admin("autostart off")? {
                    println!("Background watcher removed.");
                    return Ok(());
                }
                el::delete(el::TASK_WATCH);
                println!("Background watcher removed.");
                Ok(())
            }
            o => Err(format!("unknown choice '{o}' (on, off, status)")),
        }
    }
    pub(super) fn config(&self) -> Result<(), String> {
        match self.arg(1, "path, export, import or reset")?.as_str() {
            "path" => println!("{}", Config::path().display()),
            "export" => {
                let f = self.arg(2, "file")?;
                let s = serde_json::to_string_pretty(&Config::load()).map_err(|e| e.to_string())?;
                std::fs::write(&f, s).map_err(|e| e.to_string())?;
                println!("Saved to {f}");
            }
            "import" => {
                let f = self.arg(2, "file")?;
                let text = std::fs::read_to_string(&f).map_err(|e| e.to_string())?;
                let c: Config = serde_json::from_str(&text).map_err(|e| format!("not a CatalyX settings file: {e}"))?;
                c.save();
                println!("Loaded {} games and {} presets", c.games.len(), c.presets.len());
            }
            "reset" => {
                if !self.flag("--yes") {
                    return Err("this forgets all games and rules; repeat with --yes".into());
                }
                Config::default().save();
                println!("Settings reset");
            }
            o => return Err(format!("unknown config action '{o}'")),
        }
        Ok(())
    }
}
