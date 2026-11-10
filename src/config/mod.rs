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
pub const CONFIG_VERSION: u32 = 5;

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
        // Suggestions are visible, but service shutdown is always opt-in.
        let mut services: BTreeMap<String, bool> = crate::catalog::suggested_services().into_keys().map(|n| (n, false)).collect();
        for (n, _) in COMMON_SERVICES {
            services.entry(n.to_string()).or_insert(false);
        }
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

    pub fn load() -> Config {
        let mut c: Config = std::fs::read_to_string(Self::path()).ok().and_then(|s| serde_json::from_str(&s).ok()).unwrap_or_default();
        c.migrate();
        c
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
}
