//! Finds games already installed through Steam, Epic Games and GOG, so adding them is one click.
//! Read-only: it only reads the launchers' own install records; nothing is started or changed.

use crate::launcher;
use serde_json::Value;
use std::path::{Path, PathBuf};
use winreg::enums::*;
use winreg::RegKey;

#[derive(Clone, Debug)]
pub struct Installed {
    pub name: String,
    /// What CatalyX starts: a store link (Steam) or the game's own program.
    pub launch: String,
    pub folder: String,
    pub store: &'static str,
    /// The most likely real game program (the biggest non-helper program in the folder).
    pub real_exe: String,
}

pub fn scan() -> Vec<Installed> {
    let mut out = Vec::new();
    out.extend(steam());
    out.extend(epic());
    out.extend(gog());
    out.sort_by_key(|a| a.name.to_lowercase());
    out.dedup_by(|a, b| a.name.eq_ignore_ascii_case(&b.name) && a.store == b.store);
    out
}

fn best_exe(folder: &str) -> String {
    launcher::candidates(folder).into_iter().next().map(|(n, _)| n).unwrap_or_default()
}

/// The value of a `"key"  "value"` line in Valve's text format.
fn vdf_value(line: &str, key: &str) -> Option<String> {
    let l = line.trim();
    let rest = l.strip_prefix('"')?.strip_prefix(key)?.strip_prefix('"')?;
    let v = rest.trim().strip_prefix('"')?;
    let end = v.rfind('"')?;
    Some(v[..end].replace("\\\\", "\\"))
}

fn steam_root() -> Option<PathBuf> {
    let from = |hive, path: &str, name: &str| RegKey::predef(hive).open_subkey(path).ok().and_then(|k| k.get_value::<String, _>(name).ok());
    from(HKEY_CURRENT_USER, "Software\\Valve\\Steam", "SteamPath")
        .or_else(|| from(HKEY_LOCAL_MACHINE, "SOFTWARE\\WOW6432Node\\Valve\\Steam", "InstallPath"))
        .map(|p| PathBuf::from(p.replace('/', "\\")))
        .filter(|p| p.exists())
}

fn steam() -> Vec<Installed> {
    let Some(root) = steam_root() else { return Vec::new() };
    let mut libs = vec![root.clone()];
    if let Ok(text) = std::fs::read_to_string(root.join("steamapps").join("libraryfolders.vdf")) {
        for line in text.lines() {
            if let Some(p) = vdf_value(line, "path") {
                let p = PathBuf::from(p);
                if !libs.contains(&p) {
                    libs.push(p);
                }
            }
        }
    }
    let mut out = Vec::new();
    for lib in libs {
        let apps = lib.join("steamapps");
        let Ok(rd) = std::fs::read_dir(&apps) else { continue };
        for e in rd.flatten() {
            let fname = e.file_name().to_string_lossy().to_string();
            if !(fname.starts_with("appmanifest_") && fname.ends_with(".acf")) {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
            let (mut appid, mut name, mut dir) = (String::new(), String::new(), String::new());
            for line in text.lines() {
                if let Some(v) = vdf_value(line, "appid") {
                    appid = v;
                } else if let Some(v) = vdf_value(line, "name") {
                    name = v;
                } else if let Some(v) = vdf_value(line, "installdir") {
                    dir = v;
                }
            }
            let tool = name.starts_with("Steamworks")
                || name.starts_with("Steam Linux Runtime")
                || name.starts_with("Proton")
                || appid == "228980";
            if appid.is_empty() || name.is_empty() || tool {
                continue;
            }
            let folder = apps.join("common").join(&dir);
            if !folder.exists() {
                continue;
            }
            let folder = folder.to_string_lossy().to_string();
            out.push(Installed { name, launch: format!("steam://rungameid/{appid}"), real_exe: best_exe(&folder), folder, store: "Steam" });
        }
    }
    out
}

fn epic() -> Vec<Installed> {
    let dir = Path::new(&std::env::var("ProgramData").unwrap_or_else(|_| "C:\\ProgramData".into()))
        .join("Epic\\EpicGamesLauncher\\Data\\Manifests");
    let Ok(rd) = std::fs::read_dir(dir) else { return Vec::new() };
    let mut out = Vec::new();
    for e in rd.flatten() {
        if e.path().extension().map(|x| x != "item").unwrap_or(true) {
            continue;
        }
        let Ok(text) = std::fs::read_to_string(e.path()) else { continue };
        let Ok(v) = serde_json::from_str::<Value>(&text) else { continue };
        let s = |k: &str| v.get(k).and_then(|x| x.as_str()).unwrap_or("").to_string();
        let is_game =
            v.get("AppCategories").and_then(|c| c.as_array()).map(|c| c.iter().any(|x| x.as_str() == Some("games"))).unwrap_or(true);
        let (name, folder, exe) = (s("DisplayName"), s("InstallLocation"), s("LaunchExecutable"));
        if !is_game || name.is_empty() || exe.is_empty() {
            continue;
        }
        let launch = Path::new(&folder).join(&exe);
        if !launch.exists() {
            continue;
        }
        let real = crate::config::file_name_lower(&exe);
        out.push(Installed { name, launch: launch.to_string_lossy().to_string(), folder, real_exe: real, store: "Epic" });
    }
    out
}

fn gog() -> Vec<Installed> {
    let Ok(games) = RegKey::predef(HKEY_LOCAL_MACHINE).open_subkey("SOFTWARE\\WOW6432Node\\GOG.com\\Games") else { return Vec::new() };
    let mut out = Vec::new();
    for id in games.enum_keys().flatten() {
        let Ok(k) = games.open_subkey(&id) else { continue };
        let name: String = k.get_value("gameName").unwrap_or_default();
        let exe: String = k.get_value("exe").unwrap_or_default();
        let folder: String = k.get_value("path").unwrap_or_default();
        if name.is_empty() || exe.is_empty() || !Path::new(&exe).exists() {
            continue;
        }
        out.push(Installed { name, real_exe: crate::config::file_name_lower(&exe), launch: exe, folder, store: "GOG" });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_valve_text_lines() {
        assert_eq!(vdf_value("\t\"name\"\t\t\"Portal 2\"", "name").as_deref(), Some("Portal 2"));
        assert_eq!(vdf_value("\"path\"\t\t\"D:\\\\SteamLibrary\"", "path").as_deref(), Some("D:\\SteamLibrary"));
        assert_eq!(vdf_value("\"appid\"\t\"620\"", "name"), None);
    }
}
