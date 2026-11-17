//! Presets, game profiles and the lists of apps and services to turn off.
//! Saved as plain JSON in %APPDATA%\CatalyX\config.json.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::path::PathBuf;

mod lists;
mod presets;
mod settings;

pub use lists::*;
pub use presets::*;
pub use settings::*;

/// Bumped when saved settings need a one-time fix-up on load.
pub const CONFIG_VERSION: u32 = 6;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug, Default)]
#[serde(default)]
pub struct Game {
    pub id: String,
    pub name: String,
    /// What the person picked: an .exe, .bat, .cmd, .lnk, .url or a store link.
    pub launch: String,
    pub args: String,
    /// The real game program found behind the launcher (learned on first run, editable).
    pub real_exe: String,
    pub preset: String,
    /// When set, this game uses its own settings instead of the preset.
    pub custom: Option<Settings>,
    /// Per-game app choices that differ from the global list.
    pub apps: BTreeMap<String, ProcAction>,
    /// Per-game service choices that differ from the global list (true = stop).
    pub services: BTreeMap<String, bool>,
    pub plays: u32,
}

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Config {
    pub presets: Vec<Preset>,
    pub games: Vec<Game>,
    pub default_preset: String,
    /// Global app list: lower-case exe name -> action.
    pub apps: BTreeMap<String, ProcAction>,
    /// Global service list: service name -> stop while gaming.
    pub services: BTreeMap<String, bool>,
    pub restore_when_game_closes: bool,
    /// Start closed apps again afterwards. Off by default: reopening is the person's choice.
    pub reopen_closed_apps: bool,
    pub keep_apps_off: bool,
    pub auto_boost_known_games: bool,
    /// Open CatalyX with administrator rights (one Windows prompt) so it can boost games that
    /// themselves run as administrator. Off = never ask.
    pub run_as_admin: bool,
    pub gpu_temp_limit_c: u32,
    pub selected_game: usize,
    /// Look: "focus", "slate" or "mica".
    pub theme: String,
    /// "dark", "light" or "system" (follow Windows).
    pub appearance: String,
    /// Accent colour: "default" (the look's own) or one of ACCENT_KEYS.
    #[serde(default = "default_accent")]
    pub accent: String,
    /// How much of the accent bleeds into the surfaces: 0 off, 1 subtle, 2 rich.
    #[serde(default = "default_tint")]
    pub tint: u8,
    /// The look of the boost control: "bar" (launch bar), "gauge" or "ring" (see gui/control.rs).
    pub boost_control: String,
    /// Show a summary of what will change before a boost starts. The summary has a "don't ask again" box.
    pub confirm_boost: bool,
    /// Missing in files saved before version 2 (reads as 0, not the struct default).
    #[serde(default)]
    pub version: u32,
    /// The first-run welcome has been shown.
    #[serde(default)]
    pub onboarded: bool,
}

impl Default for Config {
    fn default() -> Self {
        // Service rules start empty: off is the default, and a recorded `false` is then a real
        // choice (something the person turned off) which suggestions must never overwrite.
        let services: BTreeMap<String, bool> = BTreeMap::new();
        Config {
            onboarded: false,
            presets: builtin_presets(),
            games: Vec::new(),
            default_preset: PRESET_BALANCED.into(),
            apps: default_apps(),
            services,
            restore_when_game_closes: true,
            reopen_closed_apps: false,
            keep_apps_off: true,
            auto_boost_known_games: true,
            run_as_admin: true,
            gpu_temp_limit_c: 83,
            selected_game: 0,
            theme: "focus".into(),
            appearance: "dark".into(),
            accent: default_accent(),
            tint: default_tint(),
            boost_control: BOOST_CONTROL_KEYS[0].into(),
            confirm_boost: true,
            version: CONFIG_VERSION,
        }
    }
}

fn default_accent() -> String {
    "default".into()
}

fn default_tint() -> u8 {
    1
}

/// The accent colours a person can pick (besides default, the look's own). The colours
/// themselves live with the theme code in gui/theme.rs.
pub const ACCENT_KEYS: [&str; 8] = ["periwinkle", "ember", "copper", "rose", "sage", "ocean", "violet", "gold"];

/// The looks the boost control can have; the first is the default.
pub const BOOST_CONTROL_KEYS: [&str; 3] = ["bar", "gauge", "ring"];

/// Where settings, the recovery journal and caches live: `%APPDATA%\CatalyX`.
/// `CATALYX_DATA` (or the older `PUREBOOST_DATA`) points check runs and tests at a scratch folder.
pub fn data_dir() -> PathBuf {
    static MIGRATED: std::sync::Once = std::sync::Once::new();
    let scratch = std::env::var("CATALYX_DATA").or_else(|_| std::env::var("PUREBOOST_DATA")).ok();
    let d = match scratch {
        Some(s) => PathBuf::from(s),
        None => {
            let appdata = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
            let d = appdata.join("CatalyX");
            MIGRATED.call_once(|| migrate_old_folder(&appdata.join("PureBoost"), &d));
            d
        }
    };
    let _ = std::fs::create_dir_all(&d);
    d
}

/// The app used to be called PureBoost and kept its files in `%APPDATA%\PureBoost`. The first time the
/// new name starts, whatever the old folder has (settings, a recovery journal still waiting) is COPIED over,
/// never moved: the old folder stays exactly as it was. Files already present in the new folder win.
fn migrate_old_folder(old: &std::path::Path, new: &std::path::Path) {
    if !old.join("config.json").exists() || new.join("config.json").exists() {
        return;
    }
    fn copy_missing(from: &std::path::Path, to: &std::path::Path) {
        let _ = std::fs::create_dir_all(to);
        let Ok(rd) = std::fs::read_dir(from) else { return };
        for e in rd.flatten() {
            let (src, dst) = (e.path(), to.join(e.file_name()));
            if src.is_dir() {
                copy_missing(&src, &dst);
            } else if !dst.exists() && !e.file_name().to_string_lossy().ends_with(".tmp") {
                let _ = std::fs::copy(&src, &dst);
            }
        }
    }
    copy_missing(old, new);
}
impl Config {
    pub fn path() -> PathBuf {
        data_dir().join("config.json")
    }

    /// A config read from settings text (the saved file or an imported one). Both go through the
    /// same migration, so an imported file can never smuggle in an old default.
    pub fn from_json(text: &str) -> Result<Config, serde_json::Error> {
        let mut c: Config = serde_json::from_str(text)?;
        c.migrate();
        Ok(c)
    }

    pub fn load() -> Config {
        std::fs::read_to_string(Self::path()).ok().and_then(|s| Self::from_json(&s).ok()).unwrap_or_default()
    }

    fn migrate(&mut self) {
        let c = self;
        // Version 2: the default game priority moved from Above normal to Normal. Built-in
        // presets still holding the old default follow along; anything the person set stays.
        if c.version < 2 {
            for p in c.presets.iter_mut().filter(|p| p.builtin && p.id != PRESET_MAX) {
                if p.settings.priority == Priority::AboveNormal {
                    p.settings.priority = DEFAULT_PRIORITY;
                }
            }
            c.version = 2;
        }
        // Version 3: the built-in presets were saved with "do nothing" values (Normal priority,
        // power plan kept), so a boost changed almost nothing. They are reset to the current
        // definitions once; the person's own presets and per-game settings are untouched.
        if c.version < 3 {
            c.presets.retain(|p| !p.builtin);
            c.version = 3;
        }
        // Version 4 refreshes built-in settings; never silently opt the person into closing
        // programs or stopping services when a new catalog becomes available.
        if c.version < 4 {
            c.presets.retain(|p| !p.builtin);
            c.version = 4;
        }
        // Version 5: Max Squeeze also switches Windows' desktop off while the game runs.
        // Only the built-in Max Squeeze preset follows; the person's own settings stay as they are.
        if c.version < 5 {
            for p in c.presets.iter_mut().filter(|p| p.builtin && p.id == PRESET_MAX) {
                p.settings.quiet_desktop = true;
            }
            c.version = 5;
        }
        // Version 6: old files listed every service CatalyX knew about as `false` (the default),
        // so a saved `false` could not be told apart from "never answered". Dropping those
        // defaults keeps every real choice (a `true`, a per-game rule, an unknown hand-written
        // name) and lets "Use recommendations" fill the rest again. Nothing is stopped by this:
        // absence and `false` both mean "keep it running".
        if c.version < 6 {
            c.services.retain(|name, on| *on || crate::catalog::known_service(name).is_none());
            c.version = CONFIG_VERSION;
        }
        // Make sure the three built-in presets are always there.
        for b in builtin_presets() {
            if !c.presets.iter().any(|p| p.id == b.id) {
                c.presets.push(b);
            }
        }
        if !c.presets.iter().any(|p| p.id == c.default_preset) {
            c.default_preset = PRESET_BALANCED.into();
        }
        // The library may have shrunk since the file was written (or it came from another PC).
        c.clamp_selected();
    }

    /// Keeps the remembered game index inside the library. Called after a load/import and after
    /// every edit, so every reader can trust it.
    pub fn clamp_selected(&mut self) {
        self.selected_game = self.selected_game.min(self.games.len().saturating_sub(1));
    }

    /// The remembered game, if there is one. The index is clamped here too: a list that shrank
    /// without an edit (a job, a demo) must not point past the end.
    pub fn selected(&self) -> Option<&Game> {
        self.games.get(self.selected_game.min(self.games.len().saturating_sub(1)))
    }

    /// The remembered game's index, clamped the same way (None = no games).
    pub fn selected_index(&self) -> Option<usize> {
        (!self.games.is_empty()).then(|| self.selected_game.min(self.games.len() - 1))
    }

    /// Writes one app choice at one scope. Keep is "no rule" for the global list and "follow the
    /// global list" for a game, so a game's map only ever holds real differences.
    pub fn set_app_rule(&mut self, game: Option<usize>, exe: &str, a: ProcAction) {
        match game {
            None => {
                if a == ProcAction::Keep {
                    self.apps.remove(exe);
                } else {
                    self.apps.insert(exe.to_string(), a);
                }
            }
            Some(i) => {
                let global = self.apps.get(exe).copied().unwrap_or(ProcAction::Keep);
                let Some(g) = self.games.get_mut(i) else { return };
                if a == global {
                    g.apps.remove(exe);
                } else {
                    g.apps.insert(exe.to_string(), a);
                }
            }
        }
    }

    /// Writes one service choice at one scope, case-insensitively (imported files may differ), and
    /// never stores a game rule that merely repeats the global one.
    pub fn set_service_rule(&mut self, game: Option<usize>, name: &str, on: bool) {
        match game {
            None => {
                let key = self.services.keys().find(|k| k.eq_ignore_ascii_case(name)).cloned().unwrap_or_else(|| name.to_string());
                self.services.insert(key, on);
            }
            Some(i) => {
                let global = self.services.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| *v).unwrap_or(false);
                let Some(g) = self.games.get_mut(i) else { return };
                g.services.retain(|k, _| !k.eq_ignore_ascii_case(name));
                if on != global {
                    g.services.insert(name.to_string(), on);
                }
            }
        }
    }

    /// The service choice recorded in one scope's own map (None = never answered there).
    pub fn service_choice(&self, game: Option<usize>, name: &str) -> Option<bool> {
        let map = match game {
            None => &self.services,
            Some(i) => &self.games.get(i)?.services,
        };
        map.iter().find(|(k, _)| k.eq_ignore_ascii_case(name)).map(|(_, v)| *v)
    }

    pub fn save(&self) {
        if let Ok(s) = serde_json::to_string_pretty(self) {
            let tmp = Self::path().with_extension("json.tmp");
            if std::fs::write(&tmp, s).is_ok() {
                let _ = std::fs::rename(&tmp, Self::path());
            }
        }
    }

    pub fn preset(&self, id: &str) -> &Preset {
        self.presets
            .iter()
            .find(|p| p.id == id)
            .or_else(|| self.presets.iter().find(|p| p.id == self.default_preset))
            .unwrap_or(&self.presets[0])
    }

    pub fn settings_for(&self, game: Option<&Game>) -> Settings {
        match game {
            Some(g) => g.custom.clone().unwrap_or_else(|| self.preset(&g.preset).settings.clone()),
            None => self.preset(&self.default_preset).settings.clone(),
        }
    }

    /// The final app list for a game: global list with that game's own changes on top.
    pub fn apps_for(&self, game: Option<&Game>) -> BTreeMap<String, ProcAction> {
        let mut m = self.apps.clone();
        if let Some(g) = game {
            for (k, v) in &g.apps {
                m.insert(k.clone(), *v);
            }
        }
        m
    }

    pub fn services_for(&self, game: Option<&Game>) -> BTreeMap<String, bool> {
        let mut m = self.services.clone();
        if let Some(g) = game {
            for (k, v) in &g.services {
                m.insert(k.clone(), *v);
            }
        }
        m
    }

    pub fn new_id(&self, prefix: &str) -> String {
        let t = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_millis()).unwrap_or(0);
        format!("{prefix}-{t:x}")
    }
}

pub fn file_name_lower(path: &str) -> String {
    std::path::Path::new(path).file_name().map(|s| s.to_string_lossy().to_lowercase()).unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_users_keep_their_apps_and_services_until_opted_in() {
        let c = Config::default();
        assert!(c.apps.values().all(|v| *v == ProcAction::Keep));
        assert!(c.services.values().all(|v| !v));
        assert!(c.restore_when_game_closes);
        assert!(!c.settings_for(None).clean_ram);
        assert_eq!(c.settings_for(None).priority, Priority::AboveNormal);
    }

    #[test]
    fn migration_preserves_explicit_choices_and_does_not_opt_in() {
        let mut old = Config::default();
        old.version = 3;
        old.apps.insert("chrome.exe".into(), ProcAction::Close);
        old.services.insert("MapsBroker".into(), true);
        old.migrate();
        assert_eq!(old.version, CONFIG_VERSION);
        assert_eq!(old.apps.get("chrome.exe"), Some(&ProcAction::Close));
        assert_eq!(old.services.get("MapsBroker"), Some(&true));
        let mut untouched = Config::default();
        untouched.version = 3;
        untouched.migrate();
        assert!(untouched.apps.values().all(|v| *v == ProcAction::Keep));
        assert!(untouched.services.values().all(|v| !v));
    }

    #[test]
    fn max_squeeze_gets_the_quiet_desktop_once_and_other_choices_stay() {
        // A version 4 file: built-in presets saved before the setting existed, one custom preset.
        let mut old = Config::default();
        old.version = 4;
        for p in old.presets.iter_mut() {
            p.settings.quiet_desktop = false;
        }
        let mut mine = old.preset(PRESET_MAX).clone();
        mine.id = "mine".into();
        mine.name = "Mine".into();
        mine.builtin = false;
        old.presets.push(mine);
        old.migrate();
        assert!(old.preset(PRESET_MAX).settings.quiet_desktop, "Max Squeeze switches the desktop off");
        assert!(!old.preset(PRESET_BALANCED).settings.quiet_desktop);
        assert!(!old.preset(PRESET_GENTLE).settings.quiet_desktop);
        assert!(!old.preset("mine").settings.quiet_desktop, "a person's own preset is never changed");
        // Turning it off in Max Squeeze afterwards must stick: migration runs once.
        old.presets.iter_mut().find(|p| p.id == PRESET_MAX).unwrap().settings.quiet_desktop = false;
        old.migrate();
        assert!(!old.preset(PRESET_MAX).settings.quiet_desktop);
    }

    #[test]
    fn game_rules_override_global_rules_without_modifying_them() {
        let mut c = Config::default();
        c.apps.insert("example.exe".into(), ProcAction::Lower);
        c.services.insert("ExampleService".into(), false);
        let mut g = Game::default();
        g.apps.insert("example.exe".into(), ProcAction::Keep);
        g.services.insert("ExampleService".into(), true);
        assert_eq!(c.apps_for(Some(&g))["example.exe"], ProcAction::Keep);
        assert!(c.services_for(Some(&g))["ExampleService"]);
        assert_eq!(c.apps["example.exe"], ProcAction::Lower);
        assert!(!c.services["ExampleService"]);
    }

    #[test]
    fn imported_settings_get_the_same_migration_as_a_saved_file() {
        // A file written by version 4: Max Squeeze predates the quiet desktop, and the remembered
        // game points past the end (the library shrank on another PC).
        let mut old = Config::default();
        old.version = 4;
        old.presets.iter_mut().find(|p| p.id == PRESET_MAX).unwrap().settings.quiet_desktop = false;
        old.games.push(Game { name: "One".into(), ..Default::default() });
        old.selected_game = 7;
        let text = serde_json::to_string(&old).unwrap();

        let c = Config::from_json(&text).unwrap();
        assert!(c.preset(PRESET_MAX).settings.quiet_desktop, "an import is migrated like a load");
        assert_eq!(c.selected_game, 0, "the remembered game is clamped to the library");
        assert_eq!(c.selected().map(|g| g.name.as_str()), Some("One"));
    }

    #[test]
    fn old_service_defaults_are_not_mistaken_for_choices() {
        // Version 5 files listed every known service as `false` (the old default), so a saved
        // false cannot be told apart from "never answered". The upgrade drops those defaults;
        // a real opt-in, a per-game rule and an unknown hand-written name stay.
        let mut old = Config::default();
        old.version = 5;
        old.services.insert("WSearch".into(), false);
        old.services.insert("Spooler".into(), true);
        old.services.insert("SomeCustomService".into(), false);
        old.games.push(Game { name: "One".into(), ..Default::default() });
        old.games[0].services.insert("WSearch".into(), false);
        old.migrate();
        assert_eq!(old.services.get("WSearch"), None, "a saved default is not a choice");
        assert_eq!(old.services.get("Spooler"), Some(&true), "an opt-in is kept");
        assert_eq!(old.services.get("SomeCustomService"), Some(&false), "an unknown rule is kept");
        assert_eq!(old.games[0].services.get("WSearch"), Some(&false), "a per-game rule is a real choice");
    }
}
