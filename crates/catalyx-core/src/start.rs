//! What either front-end does before anything else.
//!
//! `catalyx.exe` and `catalyx-cli.exe` differ only in what happens once the command line came back with
//! nothing to run: the window program draws a window, the console program says so and prints the commands.

use crate::{cli, ffi, trace};

/// The trace, the console and the hooks, and the arguments — the first thing either front-end does.
///
/// A window program has no console of its own, so a command typed in a terminal (`catalyx help`) first
/// joins the terminal it was typed in.
pub fn boot(windowed: bool) -> Vec<String> {
    trace::start();
    let args: Vec<String> = std::env::args().skip(1).collect();
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
    args
}

/// Marks a check run (a screenshot, a tour, the self-test): it never touches the person's real settings,
/// journal or power plans' bookkeeping, and it can never hang its caller.
///
/// A watchdog independent of the window ends the process whatever the window does (minimised, a stuck
/// driver, a hidden desktop), writing a failed self-test report if one was asked for.
pub fn begin_check_run(self_test_report: Option<String>) {
    let dir = std::env::temp_dir().join("catalyx-check");
    let _ = std::fs::create_dir_all(&dir);
    // A leftover from an interrupted check must never block the next one.
    let _ = std::fs::remove_file(dir.join("journal.json"));
    std::env::set_var("CATALYX_DATA", &dir);
    // Whoever started us is usually reading our output and waits until every copy of that output pipe is
    // closed: helpers we start must not inherit it, and must end with us.
    ffi::isolate_check_run();
    std::thread::spawn(move || {
        std::thread::sleep(std::time::Duration::from_secs(110));
        if let Some(path) = self_test_report {
            let v = serde_json::json!({"version": 1, "steps": [
                {"step": "timeout", "passed": false, "detail": "the window stopped drawing; the self-test was ended after 110 seconds"}
            ]});
            let _ = std::fs::write(&path, serde_json::to_string_pretty(&v).unwrap_or_default());
        }
        eprintln!("CatalyX: check run ended by the watchdog after 110 s");
        std::process::exit(3);
    });
}
