//! CatalyX - a lean game booster without ads, updaters or background nagging.
//!
//! The program is this library plus two thin front-ends that only differ in how Windows starts them:
//! `catalyx.exe` (a window program: double-clicking it never opens a terminal) and `catalyx-cli.exe` (a
//! console program, for scripts, pipes and agents). Both call `run`.

mod catalog;
mod cli;
mod config;
mod doctor;
mod elevate;
mod engine;
mod ffi;
mod gpu;
mod gui;
mod launcher;
mod library;
mod power;
mod procs;
mod selftest;
mod services;
mod shell;
mod trace;
mod tweaks;
mod watch;

use config::Config;

/// The whole program. windowed is true for the window front-end, which has no console of its own.
pub fn run(windowed: bool) {
    trace::start();
    let args: Vec<String> = std::env::args().skip(1).collect();
    // A window program has no console, so a command typed in a terminal (catalyx help) first joins the
    // terminal it was typed in.
    if windowed && cli::wants_console(&args) {
        ffi::attach_parent_console();
    }
    // `catalyx status | more` and friends: a closed pipe ends the command quietly.
    std::panic::set_hook(Box::new(|info| {
        let msg = info.to_string();
        if msg.contains("failed printing to std") {
            std::process::exit(0);
        }
        eprintln!("CatalyX: unexpected error: {msg}");
    }));
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }

    let arg_after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    // Developer helper: writes the program's icon (the build embeds assets/catalyx.ico).
    if let Some(path) = arg_after("--write-icon") {
        let _ = std::fs::write(&path, gui::mark::ico_bytes());
        return;
    }
    let screenshot = arg_after("--screenshot");
    let self_test = arg_after("--self-test");
    let tour = arg_after("--tour");
    let check_run = screenshot.is_some() || self_test.is_some() || tour.is_some();

    if check_run {
        // Check runs never touch the person's real settings, journal or power plans' bookkeeping.
        let dir = std::env::temp_dir().join("catalyx-check");
        let _ = std::fs::create_dir_all(&dir);
        // A leftover from an interrupted check must never block the next one.
        let _ = std::fs::remove_file(dir.join("journal.json"));
        std::env::set_var("CATALYX_DATA", &dir);
        // Whoever started us is usually reading our output and waits until every copy of that
        // output pipe is closed: helpers we start must not inherit it, and must end with us.
        ffi::isolate_check_run();
        // A watchdog independent of the window ends the process whatever the window does
        // (minimised, a stuck driver, a hidden desktop): a check run can never hang its caller.
        let out = self_test.clone();
        std::thread::spawn(move || {
            std::thread::sleep(std::time::Duration::from_secs(110));
            if let Some(path) = out {
                let v = serde_json::json!({"version": 1, "steps": [
                    {"step": "timeout", "passed": false, "detail": "the window stopped drawing; the self-test was ended after 110 seconds"}
                ]});
                let _ = std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap_or_default());
            }
            eprintln!("CatalyX: check run ended by the watchdog after 110 s");
            std::process::exit(3);
        });
    } else if !ffi::is_admin() && !has("--no-elevate") && Config::load().run_as_admin {
        // Games that run as administrator can only be boosted by an administrator.
        // Through the one-click task there is no prompt at all; otherwise Windows asks once.
        if elevate::launch_ui_via_task() || ffi::relaunch_elevated() {
            std::process::exit(0);
        }
    }
    // After the rename from PureBoost: move the scheduled tasks to the new names (administrator only,
    // once, off the startup path).
    if !check_run && ffi::is_admin() {
        std::thread::spawn(|| {
            elevate::migrate_legacy();
        });
    }
    ffi::hide_own_console();
    trace::mark("startup checks done");

    let tab = arg_after("--tab").and_then(|t| gui::TABS.iter().find(|(_, n, _, _)| n.eq_ignore_ascii_case(&t)).map(|(tab, ..)| *tab));
    let size = arg_after("--size").and_then(|s| {
        let (w, h) = s.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    });
    let opts = gui::Options {
        demo_session: has("--demo-session"),
        selftest: self_test,
        demo: has("--demo") || screenshot.is_some() || tour.is_some(),
        screenshot,
        tour,
        tab,
        theme: arg_after("--theme"),
        appearance: arg_after("--appearance"),
        accent: arg_after("--accent"),
        control: arg_after("--control"),
        dialog: arg_after("--dialog"),
        size,
    };
    if let Err(e) = gui::run(opts) {
        eprintln!("CatalyX: the window could not start: {e}");
        std::process::exit(1);
    }
}
