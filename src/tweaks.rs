//! Windows features that get quieter while a game runs. Every change remembers the old value.

use serde::{Deserialize, Serialize};
use winreg::enums::*;
use winreg::RegKey;

#[derive(Serialize, Deserialize, Clone, Copy, Debug, PartialEq)]
pub enum Hive {
    User,
    Machine,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum RegValue {
    Dword(u32),
    Text(String),
}

fn root(h: Hive) -> RegKey {
    match h {
        Hive::User => RegKey::predef(HKEY_CURRENT_USER),
        Hive::Machine => RegKey::predef(HKEY_LOCAL_MACHINE),
    }
}

/// A missing value is different from one that cannot be read. Never replace a value whose
/// original type or contents we cannot restore exactly.
pub fn read(h: Hive, path: &str, name: &str) -> Result<Option<RegValue>, String> {
    let k = match root(h).open_subkey(path) {
        Ok(k) => k,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    let raw = match k.get_raw_value(name) {
        Ok(v) => v,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e.to_string()),
    };
    match raw.vtype {
        REG_DWORD => k.get_value::<u32, _>(name).map(RegValue::Dword).map(Some).map_err(|e| e.to_string()),
        REG_SZ => k.get_value::<String, _>(name).map(RegValue::Text).map(Some).map_err(|e| e.to_string()),
        _ => Err(format!("unsupported original registry value type: {:?}", raw.vtype)),
    }
}

pub fn write(h: Hive, path: &str, name: &str, v: &RegValue) -> Result<(), String> {
    let (k, _) = root(h).create_subkey(path).map_err(|e| e.to_string())?;
    match v {
        RegValue::Dword(d) => k.set_value(name, d),
        RegValue::Text(s) => k.set_value(name, s),
    }
    .map_err(|e| e.to_string())
}

pub fn delete(h: Hive, path: &str, name: &str) -> bool {
    match root(h).open_subkey_with_flags(path, KEY_SET_VALUE) {
        Ok(k) => k.delete_value(name).map(|_| true).unwrap_or_else(|e| e.kind() == std::io::ErrorKind::NotFound),
        Err(e) => e.kind() == std::io::ErrorKind::NotFound,
    }
}

/// One registry value a tweak sets.
pub struct RegSet {
    pub hive: Hive,
    pub path: &'static str,
    pub name: &'static str,
    pub value: RegValue,
}

fn d(hive: Hive, path: &'static str, name: &'static str, v: u32) -> RegSet {
    RegSet { hive, path, name, value: RegValue::Dword(v) }
}
fn t(hive: Hive, path: &'static str, name: &'static str, v: &str) -> RegSet {
    RegSet { hive, path, name, value: RegValue::Text(v.to_string()) }
}

pub fn game_mode() -> Vec<RegSet> {
    vec![
        d(Hive::User, r"Software\Microsoft\GameBar", "AutoGameModeEnabled", 1),
        d(Hive::User, r"Software\Microsoft\GameBar", "AllowAutoGameMode", 1),
    ]
}

pub fn game_dvr_off() -> Vec<RegSet> {
    vec![
        d(Hive::User, r"System\GameConfigStore", "GameDVR_Enabled", 0),
        d(Hive::User, r"Software\Microsoft\Windows\CurrentVersion\GameDVR", "AppCaptureEnabled", 0),
    ]
}

pub fn notifications_off() -> Vec<RegSet> {
    vec![d(Hive::User, r"Software\Microsoft\Windows\CurrentVersion\PushNotifications", "ToastEnabled", 0)]
}

pub fn transparency_off() -> Vec<RegSet> {
    vec![d(Hive::User, r"Software\Microsoft\Windows\CurrentVersion\Themes\Personalize", "EnableTransparency", 0)]
}

pub fn maintenance_off() -> Vec<RegSet> {
    vec![d(Hive::Machine, r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Schedule\Maintenance", "MaintenanceDisabled", 1)]
}

pub fn mmcss_games() -> Vec<RegSet> {
    const P: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile";
    const G: &str = r"SOFTWARE\Microsoft\Windows NT\CurrentVersion\Multimedia\SystemProfile\Tasks\Games";
    vec![
        d(Hive::Machine, P, "SystemResponsiveness", 10),
        d(Hive::Machine, G, "GPU Priority", 8),
        d(Hive::Machine, G, "Priority", 6),
        t(Hive::Machine, G, "Scheduling Category", "High"),
        t(Hive::Machine, G, "SFIO Priority", "High"),
    ]
}

pub const DEFRAG_TASK: &str = r"\Microsoft\Windows\Defrag\ScheduledDefrag";

pub fn task_enabled(name: &str) -> Option<bool> {
    use std::os::windows::process::CommandExt;
    let out = std::process::Command::new("schtasks")
        .args(["/Query", "/TN", name, "/FO", "CSV", "/NH"])
        .creation_flags(0x0800_0000)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let s = String::from_utf8_lossy(&out.stdout).to_lowercase();
    Some(!s.contains("disabled"))
}

pub fn set_task(name: &str, enabled: bool) -> bool {
    use std::os::windows::process::CommandExt;
    std::process::Command::new("schtasks")
        .args(["/Change", "/TN", name, if enabled { "/Enable" } else { "/Disable" }])
        .creation_flags(0x0800_0000)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}
