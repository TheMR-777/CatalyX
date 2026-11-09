//! Starting a game from whatever the person picked (.exe, .bat, .cmd, .lnk, .url, store link)
//! and finding the real game program behind launchers and batch files.

use crate::config::{file_name_lower, HELPER_HINTS, NOT_A_GAME};
use crate::procs::{ProcInfo, Snapshot};
use std::path::{Path, PathBuf};
use std::process::Command;

#[derive(Clone, Debug, Default)]
pub struct LaunchPlan {
    /// Folder the game lives in (used to recognise its programs).
    pub folder: String,
    /// Program names mentioned by a batch file or shortcut.
    pub hints: Vec<String>,
    pub kind: String,
}

fn lower_path(p: &str) -> String {
    p.replace('/', "\\").to_lowercase()
}

/// Reads a shortcut's target with Windows' own scripting host.
fn resolve_lnk(path: &str) -> Option<(String, String, String)> {
    let script = format!(
        "$s=(New-Object -ComObject WScript.Shell).CreateShortcut('{}'); $s.TargetPath; $s.Arguments; $s.WorkingDirectory",
        path.replace('\'', "''")
    );
    let mut c = Command::new("powershell");
    c.args(["-NoProfile", "-Command", &script]);
    let out = crate::ffi::run_timeout(c, 20)?;
    let s = String::from_utf8_lossy(&out.stdout).to_string();
    let mut l = s.lines();
    let t = l.next()?.trim().to_string();
    let a = l.next().unwrap_or("").trim().to_string();
    let w = l.next().unwrap_or("").trim().to_string();
    if t.is_empty() {
        None
    } else {
        Some((t, a, w))
    }
}

fn url_of(path: &str) -> Option<String> {
    let s = std::fs::read_to_string(path).ok()?;
    s.lines().find_map(|l| l.trim().strip_prefix("URL=").map(|u| u.trim().to_string()))
}

/// Every "something.exe" mentioned in a text file (batch files, configs).
pub fn exe_mentions(text: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    // Batch files often start things as %~dp0game.exe or %cd%\game.exe: those variables are
    // folders, not part of the program's name.
    let lower = text.to_lowercase().replace("%~dp0", "\\").replace("%~dp0\\", "\\").replace("%cd%", "\\");
    let bytes = lower.as_bytes();
    let mut i = 0;
    while let Some(pos) = lower[i..].find(".exe") {
        let end = i + pos + 4;
        let mut start = i + pos;
        while start > 0 {
            let c = bytes[start - 1] as char;
            if c.is_alphanumeric() || "_-. ()[]+!".contains(c) {
                start -= 1;
            } else {
                break;
            }
        }
        let name = lower[start..end].trim_start_matches([' ', '(', ')']).to_string();
        if name.len() > 4 && !out.contains(&name) {
            out.push(name);
        }
        i = end;
    }
    out
}

/// Small launcher programs (like a game's "choose DirectX 9 or 10" starter) name the real game
/// program inside themselves. Reads the names of programs that sit next to the launcher.
fn embedded_exe_names(path: &str, dir: &str) -> Vec<String> {
    let Ok(meta) = std::fs::metadata(path) else { return Vec::new() };
    if meta.len() > 8 * 1024 * 1024 {
        return Vec::new();
    }
    let Ok(bytes) = std::fs::read(path) else { return Vec::new() };
    // Plain text and UTF-16 text: keep printable ASCII, drop the zero bytes of UTF-16.
    let text: String = bytes.iter().filter(|b| **b != 0).map(|b| if (0x20..0x7f).contains(b) { *b as char } else { '\n' }).collect();
    let own = file_name_lower(path);
    let mut out = Vec::new();
    for raw in exe_mentions(&text) {
        let name = raw.rsplit(['\\', '/']).next().unwrap_or(&raw).trim().to_string();
        if name == own || out.contains(&name) {
            continue;
        }
        if Path::new(dir).join(&name).exists() {
            out.push(name);
        }
    }
    out
}

/// Starts the game and returns (pid of what was started if known, plan for the detector).
/// Everything is started the way Explorer starts it (see `ffi::shell_start`), so a game behaves
/// exactly as it does from a double-click. `quiet` starts it minimised (used by the test run).
pub fn launch(path: &str, args: &str, quiet: bool) -> Result<(Option<u32>, LaunchPlan), String> {
    let p = Path::new(path);
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let dir = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let mut plan = LaunchPlan { folder: dir.clone(), kind: ext.clone(), hints: Vec::new() };

    if path.contains("://") && !p.exists() {
        // steam://rungameid/..., uplay://launch/..., com.epicgames.launcher://..., etc.
        plan.kind = "link".into();
        plan.folder.clear();
        let pid = crate::ffi::shell_start(path, "", "", quiet)?;
        return Ok((pid, plan));
    }
    if !p.exists() {
        return Err("the file can't be found".into());
    }
    match ext.as_str() {
        "exe" => {
            plan.hints.push(file_name_lower(path));
            plan.hints.extend(embedded_exe_names(path, &dir));
        }
        "bat" | "cmd" => {
            if let Ok(t) = std::fs::read(path) {
                plan.hints = exe_mentions(&String::from_utf8_lossy(&t));
            }
        }
        "lnk" => {
            // Read the shortcut only to know where the game lives; the shortcut itself is
            // started so its own "Run as administrator" and start-in folder apply.
            if let Some((t, _a, w)) = resolve_lnk(path) {
                if t.contains("://") {
                    plan.kind = "link".into();
                    plan.folder.clear();
                } else {
                    let tp = PathBuf::from(&t);
                    let td = tp.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
                    plan.folder = if td.is_empty() { w } else { td.clone() };
                    let te = tp.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
                    if te == "exe" {
                        plan.hints.push(file_name_lower(&t));
                        plan.hints.extend(embedded_exe_names(&t, &td));
                    } else if te == "bat" || te == "cmd" {
                        if let Ok(b) = std::fs::read(&t) {
                            plan.hints = exe_mentions(&String::from_utf8_lossy(&b));
                        }
                    }
                }
            }
        }
        "url" => {
            let u = url_of(path).ok_or("the internet shortcut has no address")?;
            return launch(&u, args, quiet);
        }
        _ => {}
    }
    let pid = crate::ffi::shell_start(path, args, &dir, quiet)?;
    Ok((pid, plan))
}

/// True for programs that live in the Windows folder (System32, SysWOW64, WinSxS...).
pub fn in_windows_folder(exe: &str) -> bool {
    let root = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    let root = lower_path(&root);
    let e = lower_path(exe);
    !e.is_empty() && e.starts_with(&format!("{}\\", root.trim_end_matches('\\')))
}

/// How likely a running program is the game. Higher is more likely.
pub fn score(p: &ProcInfo, snap: &Snapshot, plan: &LaunchPlan, root: Option<u32>, real_exe: &str, since: u64) -> i32 {
    let n = p.name_lower.as_str();
    if NOT_A_GAME.contains(&n) || crate::config::app_is_protected(n) {
        return -1000;
    }
    // Programs from the Windows folder (sc.exe, cmd.exe, ping.exe...) are tools a launcher script
    // calls, never the game: they must not be boosted or saved as the game's real program.
    if in_windows_folder(&p.exe) {
        return -1000;
    }
    let learned = !real_exe.is_empty() && n == real_exe.to_lowercase();
    if p.start != 0 && p.start + 3 < since && !learned {
        return -1000;
    }
    let mut s = 0;
    if learned {
        s += 60;
    }
    if let Some(r) = root {
        if snap.descends_from(p.pid, r) {
            s += 30;
        }
    }
    let exe = lower_path(&p.exe);
    let folder = lower_path(&plan.folder);
    if !folder.is_empty() && folder.len() > 3 && exe.starts_with(&folder) {
        s += 25;
    }
    if exe.contains("\\steamapps\\common\\") || exe.contains("\\epic games\\") || exe.contains("\\gog galaxy\\games\\") {
        s += 15;
    }
    if plan.hints.iter().any(|h| h == n) {
        s += 20;
    }
    if HELPER_HINTS.iter().any(|h| n.contains(h)) {
        s -= 60;
    }
    let mb = p.mem as f32 / (1024.0 * 1024.0);
    if mb > 1.0 {
        s += (mb.ln() * 3.0) as i32;
    }
    s += (p.gpu * 0.6) as i32;
    s
}

/// The best guess for the game right now, if one is convincing enough.
pub fn best(snap: &Snapshot, plan: &LaunchPlan, root: Option<u32>, real_exe: &str, since: u64) -> Option<(ProcInfo, i32)> {
    let mut best: Option<(ProcInfo, i32)> = None;
    for p in &snap.procs {
        let sc = score(p, snap, plan, root, real_exe, since);
        if sc < 45 {
            continue;
        }
        let mb = p.mem / (1024 * 1024);
        let strong = mb > 150
            || p.gpu > 3.0
            || plan.hints.contains(&p.name_lower)
            || (!real_exe.is_empty() && p.name_lower == real_exe.to_lowercase());
        if !strong {
            continue;
        }
        if best.as_ref().map(|(_, b)| sc > *b).unwrap_or(true) {
            best = Some((p.clone(), sc));
        }
    }
    best
}

/// Last resort when nothing matched by name or folder: the program that came up after the
/// boost began and is drawing the graphics card hard (a game in full swing), ignoring
/// launchers, shells and helpers. Needs a steady 12% or more of the card, so a browser tab
/// or a video player doesn't qualify.
pub fn best_by_gpu(snap: &Snapshot, since: u64) -> Option<ProcInfo> {
    snap.procs
        .iter()
        .filter(|p| {
            let n = p.name_lower.as_str();
            p.gpu >= 12.0
                && p.pid > 4
                && !NOT_A_GAME.contains(&n)
                && !crate::config::app_is_protected(n)
                && !HELPER_HINTS.iter().any(|h| n.contains(h))
                && (p.start == 0 || p.start + 3 >= since)
        })
        .max_by(|a, b| a.gpu.partial_cmp(&b.gpu).unwrap_or(std::cmp::Ordering::Equal))
        .cloned()
}
// ---------------------------------------------------------------- finding programs before launch

/// Program files inside a game's folder (up to three levels down), biggest first, leaving out
/// installers, crash reporters and the like. Capped, so a huge game folder never stalls anything.
pub fn candidates(folder: &str) -> Vec<(String, u64)> {
    fn walk(dir: &Path, depth: u32, seen: &mut u32, out: &mut Vec<(String, u64)>) {
        let Ok(rd) = std::fs::read_dir(dir) else { return };
        for e in rd.flatten() {
            *seen += 1;
            if *seen > 20_000 {
                return;
            }
            let p = e.path();
            let Ok(t) = e.file_type() else { continue };
            if t.is_dir() {
                if depth < 3 {
                    walk(&p, depth + 1, seen, out);
                }
            } else if p.extension().map(|x| x.eq_ignore_ascii_case("exe")).unwrap_or(false) {
                let name = file_name_lower(&p.to_string_lossy());
                if NOT_A_GAME.contains(&name.as_str()) || HELPER_HINTS.iter().any(|h| name.contains(h)) {
                    continue;
                }
                out.push((name, e.metadata().map(|m| m.len()).unwrap_or(0)));
            }
        }
    }
    let mut out = Vec::new();
    let mut seen = 0u32;
    if !folder.is_empty() {
        walk(Path::new(folder), 0, &mut seen, &mut out);
    }
    out.sort_by(|a, b| b.1.cmp(&a.1).then(a.0.cmp(&b.0)));
    out.dedup_by(|a, b| a.0 == b.0);
    out
}

/// Guesses the real game program before the first launch, from what a batch file, a shortcut or
/// a small starter program names. `None` when nothing convincing is named (it is then learned
/// while the game runs). Never returns a program from the Windows folder or a known launcher.
pub fn predict_real_exe(path: &str) -> Option<String> {
    let p = Path::new(path);
    let ext = p.extension().map(|e| e.to_string_lossy().to_lowercase()).unwrap_or_default();
    let dir = p.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    let usable = |name: &str| !NOT_A_GAME.contains(&name) && !HELPER_HINTS.iter().any(|h| name.contains(h));
    let in_folder = |name: &str| candidates(&dir).iter().any(|(n, _)| n == name);
    match ext.as_str() {
        "exe" => {
            let own = file_name_lower(path);
            if let Some(n) = embedded_exe_names(path, &dir).into_iter().find(|n| usable(n)) {
                return Some(n);
            }
            usable(&own).then_some(own)
        }
        "bat" | "cmd" => {
            let text = String::from_utf8_lossy(&std::fs::read(path).ok()?).to_string();
            exe_mentions(&text)
                .into_iter()
                .map(|n| n.rsplit(['\\', '/']).next().unwrap_or(&n).trim().to_string())
                .filter(|n| usable(n) && !in_windows_folder(n))
                .find(|n| in_folder(n))
        }
        "lnk" => {
            let (t, _a, w) = resolve_lnk(path)?;
            if t.contains("://") {
                return None;
            }
            let tp = PathBuf::from(&t);
            let name = file_name_lower(&t);
            let td = tp.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or(w);
            match tp.extension().map(|e| e.to_string_lossy().to_lowercase()).as_deref() {
                Some("exe") => embedded_exe_names(&t, &td).into_iter().find(|n| usable(n)).or_else(|| usable(&name).then_some(name)),
                Some("bat") | Some("cmd") => predict_real_exe(&t),
                _ => None,
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn batch_file_names_the_real_program() {
        let names =
            exe_mentions("@echo off\r\ncd /d \"%~dp0\"\r\nset EXENAME=sp26-cod.exe\r\nstart \"\" \"%~dp0game bin\\sp26-cod.exe\" -x\r\n");
        assert!(names.iter().any(|n| n.ends_with("sp26-cod.exe")), "{names:?}");
    }

    #[test]
    fn predicts_the_exe_a_batch_file_starts() {
        let dir = std::env::temp_dir().join("catalyx-test-predict");
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("bin")).unwrap();
        std::fs::write(dir.join("bin").join("realgame.exe"), vec![0u8; 2048]).unwrap();
        std::fs::write(dir.join("crashreporter.exe"), vec![0u8; 4096]).unwrap();
        std::fs::write(dir.join("start.bat"), "@echo off\r\ncrashreporter.exe\r\n\"%~dp0bin\\realgame.exe\" -windowed\r\n").unwrap();
        let got = predict_real_exe(&dir.join("start.bat").to_string_lossy());
        assert_eq!(got.as_deref(), Some("realgame.exe"));
        let c = candidates(&dir.to_string_lossy());
        assert_eq!(c.len(), 1, "the crash reporter is left out: {c:?}");
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn windows_tools_are_never_the_game() {
        assert!(in_windows_folder("C:\\Windows\\System32\\ping.exe"));
        assert!(!in_windows_folder("C:\\Games\\ping.exe"));
    }
}
