//! What either front-end does before anything else.
//!
//! `catalyx.exe` and `catalyx-cli.exe` differ only in what happens once the command line came back with
//! nothing to run: the window program draws a window, the console program says so and prints the commands.

use crate::app::{Options, Origin};
use crate::config::Config;
use crate::{cli, elevate, ffi, trace};

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

/// The last steps before a skin draws: a check run's scratch settings and watchdog, the elevation the
/// person asked for, and hiding the console a window program never owns. Returns false when an
/// elevated copy was started and this one must end.
///
/// One place reads these flags: `Options` parsed them once, so the program and the skin cannot disagree
/// about what `--self-test` means.
pub fn prepare_window(opts: &Options) -> bool {
    if let Origin::Check { selftest, .. } = opts.origin() {
        begin_check_run(selftest);
    } else if !opts.no_elevate && !ffi::is_admin() && Config::load().run_as_admin {
        // Games that run as administrator can only be boosted by an administrator. Through the
        // one-click task there is no prompt at all; otherwise Windows asks once.
        if elevate::launch_ui_via_task() || ffi::relaunch_elevated() {
            return false;
        }
    }
    ffi::hide_own_console();
    true
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
