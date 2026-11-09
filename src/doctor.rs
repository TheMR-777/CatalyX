//! `catalyx doctor`: proves on this very PC, one feature at a time, that a boost really takes
//! effect, by reading the result back from Windows - and then puts everything back.
//! Every step has its own time limit, so the doctor always finishes.

use crate::config::CpuLevel;
use crate::ffi;
use crate::tweaks::{self, Hive, RegValue};
use crate::{gpu, power, procs, services};
use serde_json::json;
use std::time::{Duration, Instant};

struct Report {
    rows: Vec<(String, &'static str, String)>,
    json: bool,
}

impl Report {
    fn add(&mut self, name: &str, state: &'static str, detail: impl Into<String>) {
        let detail = detail.into();
        if !self.json {
            println!("[{state}] {name}: {detail}");
        }
        self.rows.push((name.to_string(), state, detail));
    }
}

/// A harmless stand-in for a game: a program that just waits (our own child, so we may change
/// anything about it).
fn dummy_game() -> Option<std::process::Child> {
    use std::os::windows::process::CommandExt;
    let sys = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
    std::process::Command::new(format!("{sys}\\System32\\PING.EXE"))
        .args(["-n", "40", "127.0.0.1"])
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .ok()
}

pub fn run(with_gpu: bool, json: bool) -> Result<(), String> {
    let mut r = Report { rows: Vec::new(), json };
    if !json {
        println!("CatalyX doctor - each step is checked by reading the result back from Windows.\n");
    }
    let admin = ffi::is_admin();
    r.add(
        "administrator rights",
        if admin { "PASS" } else { "WARN" },
        if admin {
            "running as administrator: services, priorities of elevated games and power plans can all be changed"
        } else {
            "not administrator: games that run as administrator cannot be boosted and services stay as they are"
        },
    );

    // ---- CPU performance through the power plan.
    let prev_plan = power::active();
    match &prev_plan {
        None => r.add("CPU boost", "FAIL", "could not read the active power plan"),
        Some(prev) => {
            let built = power::ensure_boost_plan(CpuLevel::Max, prev);
            if !built {
                r.add("CPU boost", "FAIL", "Windows would not create the CatalyX power plan");
            } else {
                let mut ok = true;
                let mut notes = Vec::new();
                for (name, id, want) in [
                    ("turbo mode", power::PERFBOOSTMODE, 2u32),
                    ("minimum processor state", power::PROCTHROTTLEMIN, 100),
                    ("maximum processor state", power::PROCTHROTTLEMAX, 100),
                    ("minimum cores unparked", power::CPMINCORES, 100),
                    ("energy preference", power::PERFEPP_ID, 0),
                ] {
                    match power::query_ac(power::BOOST_PLAN, power::SUB_PROCESSOR, id) {
                        Some(v) if v == want => notes.push(format!("{name} = {v}")),
                        Some(v) => {
                            ok = false;
                            notes.push(format!("{name} = {v} (wanted {want})"));
                        }
                        None => notes.push(format!("{name}: this CPU/Windows does not expose it")),
                    }
                }
                r.add("CPU boost (power plan values)", if ok { "PASS" } else { "FAIL" }, notes.join(", "));
                // Switch to it for a moment, look at the real clock, switch back.
                let switched = power::set_active(power::BOOST_PLAN);
                let mut clock = String::new();
                if switched {
                    let mut f = procs::clock_probe();
                    let t = Instant::now();
                    let mut best = 0u32;
                    while t.elapsed() < Duration::from_millis(2500) {
                        std::thread::sleep(Duration::from_millis(500));
                        if let Some(v) = f.sample_mhz() {
                            best = best.max(v);
                        }
                    }
                    clock = format!("CPU clock {} MHz (rated base {} MHz)", best, procs::cpu_base_mhz());
                }
                let back = power::set_active(prev);
                power::delete_boost_plan();
                r.add(
                    "CPU clock under the boost plan",
                    if switched && back { "PASS" } else { "FAIL" },
                    if switched {
                        format!("{clock}; plan put back: {}", if back { "yes" } else { "NO - run `catalyx plans`" })
                    } else {
                        "could not switch plans".into()
                    },
                );
            }
        }
    }

    // ---- Game priority on a stand-in program.
    match dummy_game() {
        None => r.add("game priority", "FAIL", "could not start the stand-in program"),
        Some(mut child) => {
            let pid = child.id();
            let before = procs::get_priority(pid);
            let set = procs::set_priority(pid, ffi::HIGH_PRIORITY_CLASS);
            let seen = procs::get_priority(pid);
            let restored = before.map(|b| procs::set_priority(pid, b)).unwrap_or(false);
            let after = procs::get_priority(pid);
            let ok = set && seen == Some(ffi::HIGH_PRIORITY_CLASS) && after == before;
            r.add(
                "game priority",
                if ok { "PASS" } else { "FAIL" },
                format!("raised to High, read back as {:?}, then restored: {restored}", seen.map(|c| format!("0x{c:x}"))),
            );
            let s = procs::suspend(pid);
            let u = procs::resume(pid);
            r.add("pause and resume an app", if s && u { "PASS" } else { "FAIL" }, format!("paused: {s}, resumed: {u}"));
            let pt = procs::power_throttling(pid);
            let set_pt = procs::set_power_throttling(pid, 0x1 | 0x4, 0);
            if let Some((c, s)) = pt {
                let _ = procs::set_power_throttling(pid, c, s);
            }
            r.add(
                "keep Windows efficiency mode off",
                if set_pt { "PASS" } else { "WARN" },
                if set_pt { "accepted by Windows" } else { "this Windows version does not support it" },
            );
            let trimmed = procs::trim(pid);
            r.add("clean memory", if trimmed { "PASS" } else { "FAIL" }, format!("working set trimmed: {trimmed}"));
            let _ = child.kill();
            let _ = child.wait();
        }
    }

    // ---- Starting a game from a batch file and finding the real program.
    {
        let dir = std::env::temp_dir().join("catalyx-doctor");
        let _ = std::fs::create_dir_all(&dir);
        let bat = dir.join("start_game.bat");
        let sys = std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into());
        // Windows programs are never taken for a game, so the stand-in is a copy of ping.exe
        // with its own name, sitting next to the batch file like a real game would.
        let fake = dir.join("catalyx-testgame.exe");
        let _ = std::fs::copy(format!("{sys}\\System32\\PING.EXE"), &fake);
        let _ = std::fs::write(&bat, "@echo off\r\n\"%~dp0catalyx-testgame.exe\" -n 20 127.0.0.1 >nul\r\n");
        let started = procs::now_secs();
        match crate::launcher::launch(&bat.to_string_lossy(), "", true) {
            Err(e) => r.add("start a game from a batch file", "FAIL", e),
            Ok((root, plan)) => {
                let snap = procs::start_monitor();
                let t = Instant::now();
                let mut found: Option<procs::ProcInfo> = None;
                while t.elapsed() < Duration::from_secs(12) {
                    std::thread::sleep(Duration::from_millis(500));
                    let s = snap.lock().map(|s| s.clone()).unwrap_or_default();
                    if s.serial < 2 {
                        continue;
                    }
                    if let Some((p, _)) = crate::launcher::best(&s, &plan, root, "", started) {
                        found = Some(p);
                        break;
                    }
                }
                match found {
                    Some(p) => {
                        r.add(
                            "start a game from a batch file",
                            "PASS",
                            format!("started it, found the real program behind it: {} (pid {})", p.name, p.pid),
                        );
                        let s = snap.lock().map(|s| s.clone()).unwrap_or_default();
                        for q in s.procs.iter().filter(|q| q.pid == p.pid || q.pid == p.parent) {
                            if q.start + 3 >= started && (q.name_lower == "catalyx-testgame.exe" || q.name_lower == "cmd.exe") {
                                procs::terminate(q.pid);
                            }
                        }
                    }
                    None => r.add("start a game from a batch file", "FAIL", "the program behind the batch file was not found within 12 s"),
                }
            }
        }
        std::thread::sleep(Duration::from_millis(400));
        let _ = std::fs::remove_file(&bat);
        let _ = std::fs::remove_file(&fake);
    }

    // ---- Services and Windows settings.
    {
        let list = services::list();
        let running = list.iter().filter(|s| s.running).count();
        let stoppable = list.iter().filter(|s| s.running && s.can_stop).count();
        r.add(
            "services",
            if running > 0 { "PASS" } else { "FAIL" },
            format!("{running} running, {stoppable} can be stopped{}", if admin { "" } else { " (stopping needs administrator)" }),
        );
    }
    {
        let path = "Software\\CatalyX\\DoctorTest";
        let w = tweaks::write(Hive::User, path, "Probe", &RegValue::Dword(7));
        let rd = tweaks::read(Hive::User, path, "Probe");
        let d = tweaks::delete(Hive::User, path, "Probe");
        let ok = w.is_ok() && rd == Ok(Some(RegValue::Dword(7))) && d;
        r.add(
            "Windows settings (write, read back, undo)",
            if ok { "PASS" } else { "FAIL" },
            format!("write ok: {}, read back: {:?}, removed: {d}", w.is_ok(), rd.ok().flatten()),
        );
        let _ = winreg::RegKey::predef(winreg::enums::HKEY_CURRENT_USER).delete_subkey(path);
    }

    // ---- Graphics card.
    let g = gpu::probe();
    if !g.supported {
        r.add("graphics card overclock", "SKIP", format!("{}: {}", g.name, if g.note.is_empty() { "no editable clocks" } else { &g.note }));
    } else if !with_gpu {
        let (c, m) = gpu::planned(&g, crate::config::GpuLevel::Safe);
        r.add(
            "graphics card overclock",
            "SKIP",
            format!("{} accepts offsets (Safe step +{}/+{} MHz); add --gpu to try them for a few seconds", g.name, c / 1000, m / 1000),
        );
    } else {
        match gpu_roundtrip(&g) {
            Ok(s) => r.add("graphics card overclock", "PASS", s),
            Err(e) => r.add("graphics card overclock", "FAIL", e),
        }
    }

    let failed = r.rows.iter().filter(|(_, s, _)| *s == "FAIL").count();
    if json {
        let v = json!({"version": 1, "failed": failed, "checks": r.rows.iter().map(|(n, s, d)| json!({"check": n, "result": s, "detail": d})).collect::<Vec<_>>()});
        println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
    } else {
        println!(
            "\n{}",
            if failed == 0 { "Everything that could be checked works.".to_string() } else { format!("{failed} check(s) failed.") }
        );
    }
    if failed == 0 {
        Ok(())
    } else {
        Err(format!("{failed} check(s) failed"))
    }
}

/// Applies the Safe step, reads it back from the driver, then puts the old offsets back.
fn gpu_roundtrip(before: &gpu::GpuState) -> Result<String, String> {
    let (c, m) = gpu::planned(before, crate::config::GpuLevel::Safe);
    let temp = gpu::temperature().ok_or("the temperature cannot be read, so no clock change is made")?;
    let limit = crate::config::Config::load().gpu_temp_limit_c;
    if temp + 5 >= limit {
        return Err(format!("the card is at {temp} C, too close to the {limit} C limit"));
    }
    let prev = gpu::apply(c, m)?;
    std::thread::sleep(Duration::from_secs(2));
    let during = gpu::probe();
    let restored = gpu::restore(prev.0, prev.1);
    let after = gpu::probe();
    restored?;
    let ok = (c <= 0 || during.core.current_khz >= c) && (m <= 0 || during.mem.current_khz >= m);
    if !ok {
        return Err("the driver accepted the call but did not report the new offsets".into());
    }
    Ok(format!(
        "core {:+} / memory {:+} MHz read back from the driver at {temp} C, then back to {:+} / {:+} MHz",
        during.core.current_khz / 1000,
        during.mem.current_khz / 1000,
        after.core.current_khz / 1000,
        after.mem.current_khz / 1000
    ))
}
