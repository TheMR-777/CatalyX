//! One-click administrator start and the optional background watcher.
//!
//! Boosting an elevated game, stopping services and changing machine-wide settings all need
//! administrator rights, and Windows asks for them with a prompt every time a program starts.
//! After a single approval, CatalyX can register a Task Scheduler entry that runs it with the
//! highest privileges *on demand* (no trigger): starting that task later opens CatalyX
//! elevated without any prompt. The watcher task is the same idea with a sign-in trigger and
//! no window. Both are removed with one command, and nothing is installed anywhere else.

use crate::ffi;
use std::process::Command;

pub const TASK_UI: &str = "CatalyX";
pub const TASK_WATCH: &str = "CatalyX Watcher";

fn schtasks(args: &[&str]) -> Option<std::process::Output> {
    let mut c = Command::new("schtasks");
    c.args(args);
    ffi::run_timeout(c, 20)
}

pub fn exists(name: &str) -> bool {
    schtasks(&["/Query", "/TN", name]).map(|o| o.status.success()).unwrap_or(false)
}

/// The program the task starts (read from its definition).
pub fn command_of(name: &str) -> Option<String> {
    let o = schtasks(&["/Query", "/TN", name, "/XML"])?;
    if !o.status.success() {
        return None;
    }
    // schtasks writes the definition as UTF-16 or as the console code page, depending on the build.
    let text = if o.stdout.len() > 2 && o.stdout[0] == 0xFF && o.stdout[1] == 0xFE {
        let u: Vec<u16> = o.stdout[2..].as_chunks::<2>().0.iter().map(|b| u16::from_le_bytes([b[0], b[1]])).collect();
        String::from_utf16_lossy(&u)
    } else {
        String::from_utf8_lossy(&o.stdout).to_string()
    };
    let a = text.find("<Arguments>").map(|i| i + 11)?;
    let b = text[a..].find("</Arguments>")? + a;
    Some(xml_unescape(&text[a..b]))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}

fn xml_unescape(s: &str) -> String {
    s.replace("&quot;", "\"").replace("&lt;", "<").replace("&gt;", ">").replace("&amp;", "&")
}

/// The task definition. `conhost --headless` runs the console program without a console window.
pub fn task_xml(exe: &str, args: &str, at_logon: bool, user: &str) -> String {
    let arguments = xml_escape(&format!("--headless \"{exe}\" {args}"));
    let trigger = if at_logon {
        format!("<LogonTrigger><Enabled>true</Enabled><UserId>{}</UserId></LogonTrigger>", xml_escape(user))
    } else {
        String::new()
    };
    format!(
        r#"<?xml version="1.0" encoding="UTF-16"?>
<Task version="1.4" xmlns="http://schemas.microsoft.com/windows/2004/02/mit/task">
  <RegistrationInfo><Description>Starts CatalyX with administrator rights. Created by CatalyX; remove it with: catalyx elevate remove</Description></RegistrationInfo>
  <Triggers>{trigger}</Triggers>
  <Principals><Principal id="Author"><UserId>{user}</UserId><LogonType>InteractiveToken</LogonType><RunLevel>HighestAvailable</RunLevel></Principal></Principals>
  <Settings>
    <MultipleInstancesPolicy>IgnoreNew</MultipleInstancesPolicy>
    <DisallowStartIfOnBatteries>false</DisallowStartIfOnBatteries>
    <StopIfGoingOnBatteries>false</StopIfGoingOnBatteries>
    <AllowHardTerminate>true</AllowHardTerminate>
    <StartWhenAvailable>false</StartWhenAvailable>
    <RunOnlyIfNetworkAvailable>false</RunOnlyIfNetworkAvailable>
    <AllowStartOnDemand>true</AllowStartOnDemand>
    <Enabled>true</Enabled>
    <Hidden>false</Hidden>
    <ExecutionTimeLimit>PT0S</ExecutionTimeLimit>
    <Priority>5</Priority>
  </Settings>
  <Actions Context="Author"><Exec><Command>conhost.exe</Command><Arguments>{arguments}</Arguments></Exec></Actions>
</Task>
"#,
        user = xml_escape(user)
    )
}

fn current_user() -> String {
    let d = std::env::var("USERDOMAIN").unwrap_or_default();
    let u = std::env::var("USERNAME").unwrap_or_default();
    if d.is_empty() {
        u
    } else {
        format!("{d}\\{u}")
    }
}

/// Registers (or replaces) a task. Needs administrator rights.
pub fn create(name: &str, args: &str, at_logon: bool) -> Result<(), String> {
    if !ffi::is_admin() {
        return Err("creating the task needs administrator rights".into());
    }
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let xml = task_xml(&exe.to_string_lossy(), args, at_logon, &current_user());
    let path = std::env::temp_dir().join(format!("catalyx-task-{}.xml", std::process::id()));
    // Task Scheduler reads UTF-16 with a byte-order mark.
    let mut bytes = vec![0xFFu8, 0xFE];
    for u in xml.encode_utf16() {
        bytes.extend_from_slice(&u.to_le_bytes());
    }
    std::fs::write(&path, bytes).map_err(|e| e.to_string())?;
    let out = schtasks(&["/Create", "/TN", name, "/XML", &path.to_string_lossy(), "/F"]);
    let _ = std::fs::remove_file(&path);
    match out {
        Some(o) if o.status.success() => Ok(()),
        Some(o) => Err(String::from_utf8_lossy(&o.stderr).trim().to_string()),
        None => Err("Task Scheduler did not answer".into()),
    }
}

pub fn delete(name: &str) -> bool {
    schtasks(&["/Delete", "/TN", name, "/F"]).map(|o| o.status.success()).unwrap_or(false)
}

pub fn run(name: &str) -> bool {
    schtasks(&["/Run", "/TN", name]).map(|o| o.status.success()).unwrap_or(false)
}

/// True when the one-click task exists and starts *this* program (a moved or rebuilt program
/// must not be launched through a stale task).
pub fn ui_task_ready() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    command_of(TASK_UI).map(|c| c.to_lowercase().contains(&exe.to_string_lossy().to_lowercase())).unwrap_or(false)
}

/// Starts CatalyX's window elevated through the task, without a prompt. True when started.
pub fn launch_ui_via_task() -> bool {
    ui_task_ready() && run(TASK_UI)
}

pub fn watcher_enabled() -> bool {
    exists(TASK_WATCH)
}

/// The names the tasks had when the app was still called PureBoost.
const LEGACY_UI: &str = "PureBoost";
const LEGACY_WATCH: &str = "PureBoost Watcher";

/// Moves tasks registered under the old name to the new one, pointing at this program: the one-click
/// administrator start and the background watcher keep working after the rename without the person
/// having to set them up again. Needs administrator rights (does nothing without them, and tries again
/// at the next elevated start). Returns what was moved. Ends with a marker file so the check (two
/// Task Scheduler queries) is made only once.
pub fn migrate_legacy() -> Vec<String> {
    let mut moved = Vec::new();
    let marker = crate::config::data_dir().join("tasks-renamed");
    if marker.exists() || !ffi::is_admin() {
        return moved;
    }
    let mut ok = true;
    for (old, new, args, at_logon) in [(LEGACY_UI, TASK_UI, "", false), (LEGACY_WATCH, TASK_WATCH, "watch --quiet", true)] {
        if exists(old) {
            if create(new, args, at_logon).is_ok() {
                delete(old);
                moved.push(format!("{old} -> {new}"));
            } else {
                ok = false; // leave the old one in place and try again next time
            }
        }
    }
    if ok {
        let _ = std::fs::write(marker, "the scheduled tasks use the CatalyX names\n");
    }
    moved
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn task_xml_is_well_formed_and_escaped() {
        let x = task_xml("C:\\Program Files\\Pure & Boost\\catalyx.exe", "gui", false, "PC\\me");
        assert!(x.contains("&amp;"), "ampersand escaped");
        assert!(x.contains("HighestAvailable"));
        assert!(!x.contains("<LogonTrigger>"), "on demand: no trigger");
        let w = task_xml("C:\\x\\catalyx.exe", "watch --quiet", true, "PC\\me");
        assert!(w.contains("<LogonTrigger>"));
        assert!(w.contains("--headless"));
        // Opening and closing tags balance for every element we write.
        for tag in ["Task", "Triggers", "Principals", "Settings", "Actions", "Exec", "Command", "Arguments"] {
            assert_eq!(
                w.matches(&format!("<{tag}>")).count() + w.matches(&format!("<{tag} ")).count(),
                w.matches(&format!("</{tag}>")).count(),
                "{tag}"
            );
        }
    }

    #[test]
    fn xml_round_trip() {
        assert_eq!(xml_unescape(&xml_escape("a & \"b\" <c>")), "a & \"b\" <c>");
    }
}
