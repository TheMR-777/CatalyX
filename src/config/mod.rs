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

/// The only settings version this build reads. There is no release to stay compatible with before
/// 1.0, so a file with another version is set aside and defaults are used (docs/DECISIONS.md).
/// Bump it when the saved shape changes in a way a reader must not guess at.
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
    /// The settings version the file was written with; a file with any other version is not read.
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
/// `CATALYX_DATA` points check runs and tests at a scratch folder.
pub fn data_dir() -> PathBuf {
    let d = match std::env::var("CATALYX_DATA") {
        Ok(s) => PathBuf::from(s),
        Err(_) => {
            let appdata = std::env::var("APPDATA").map(PathBuf::from).unwrap_or_else(|_| std::env::temp_dir());
            appdata.join("CatalyX")
        }
    };
    let _ = std::fs::create_dir_all(&d);
    d
}

impl Config {
    pub fn path() -> PathBuf {
        data_dir().join("config.json")
    }

    /// A config read from settings text (the saved file or an imported one). A file written by
    /// another version is refused rather than guessed at: defaults are used instead of a
    /// half-understood mixture of old and new meaning.
    pub fn from_json(text: &str) -> Result<Config, String> {
        let mut c: Config = serde_json::from_str(text).map_err(|e| e.to_string())?;
        if c.version != CONFIG_VERSION {
            return Err(format!("the file was saved by version {}; this build reads version {CONFIG_VERSION}", c.version));
        }
        c.fix_up();
        Ok(c)
    }

    pub fn load() -> Config {
        let path = Self::path();
        let Ok(text) = std::fs::read_to_string(&path) else { return Config::default() };
        match Self::from_json(&text) {
            Ok(c) => c,
            Err(_) => {
                set_aside(&path);
                Config::default()
            }
        }
    }

    /// Whatever the file said, the program needs the three built-in presets, a default preset that
    /// exists, and a remembered game inside the library.
    fn fix_up(&mut self) {
        for b in builtin_presets() {
            if !self.presets.iter().any(|p| p.id == b.id) {
                self.presets.push(b);
            }
        }
        if !self.presets.iter().any(|p| p.id == self.default_preset) {
            self.default_preset = PRESET_BALANCED.into();
        }
        // The library may have shrunk since the file was written (or it came from another PC).
        self.clamp_selected();
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

    /// The profile a game follows, worded for people: "Own settings", the preset's name, or the
    /// default preset's name when no game is selected. Shared by the core's boost view and any skin.
    pub fn profile_label(&self, game: Option<&Game>) -> String {
        match game {
            Some(g) if g.custom.is_some() => "Own settings".to_string(),
            Some(g) => self.preset(&g.preset).name.clone(),
            None => self.preset(&self.default_preset).name.clone(),
        }
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

/// Moves a settings file this build cannot read out of the way: the next save writes fresh
/// defaults, and whatever the old file held stays readable next to it as config.json.old.
fn set_aside(path: &std::path::Path) {
    let _ = std::fs::rename(path, path.with_extension("json.old"));
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
    fn a_current_file_keeps_what_the_person_chose() {
        let mut c = Config::default();
        c.apps.insert("chrome.exe".into(), ProcAction::Close);
        c.services.insert("MapsBroker".into(), true);
        c.games.push(Game { name: "One".into(), ..Default::default() });
        c.selected_game = 7;
        let text = serde_json::to_string(&c).unwrap();

        let read = Config::from_json(&text).unwrap();
        assert_eq!(read.apps.get("chrome.exe"), Some(&ProcAction::Close));
        assert_eq!(read.services.get("MapsBroker"), Some(&true));
        assert_eq!(read.selected_game, 0, "the remembered game is clamped to the library");
        assert_eq!(read.selected().map(|g| g.name.as_str()), Some("One"));
    }

    #[test]
    fn a_file_from_another_version_is_refused_and_kept_aside() {
        // No release to stay compatible with before 1.0: an old file is not upgraded, it is put
        // out of the way so defaults are used and nothing the person wrote is destroyed.
        let mut old = Config { version: CONFIG_VERSION - 1, ..Default::default() };
        old.games.push(Game { name: "One".into(), ..Default::default() });
        let text = serde_json::to_string(&old).unwrap();
        let why = Config::from_json(&text).unwrap_err();
        assert!(why.contains("version"), "the refusal says why: {why}");

        let dir = std::env::temp_dir().join(format!("catalyx-aside-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("config.json");
        std::fs::write(&file, &text).unwrap();
        set_aside(&file);
        assert!(!file.exists(), "the unreadable file is moved, so the next save writes defaults");
        assert_eq!(std::fs::read_to_string(dir.join("config.json.old")).unwrap(), text, "and it is still readable");
        let _ = std::fs::remove_dir_all(&dir);
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
