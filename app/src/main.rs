//! The window program: the core's command line first, then the skin. A Windows GUI program, so starting
//! it never opens a terminal; commands typed in a terminal still work (it joins that terminal), and for
//! scripts and pipes use `catalyx-cli.exe`.
#![cfg_attr(windows, windows_subsystem = "windows")]

use catalyx_core::{config::Config, elevate, ffi, start, trace};

fn main() {
    let args = start::boot(true);
    // `catalyx help`, `catalyx play ...`: a command prints and the program ends without a window.
    if let Some(code) = catalyx_core::cli::run(&args) {
        std::process::exit(code);
    }
    let arg_after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let has = |flag: &str| args.iter().any(|a| a == flag);

    let self_test = arg_after("--self-test");
    // A screenshot, a tour or the self-test is a check run: scratch settings and a watchdog.
    if self_test.is_some() || arg_after("--screenshot").is_some() || arg_after("--tour").is_some() {
        start::begin_check_run(self_test);
    } else if !ffi::is_admin() && !has("--no-elevate") && Config::load().run_as_admin {
        // Games that run as administrator can only be boosted by an administrator.
        // Through the one-click task there is no prompt at all; otherwise Windows asks once.
        if elevate::launch_ui_via_task() || ffi::relaunch_elevated() {
            std::process::exit(0);
        }
    }
    ffi::hide_own_console();
    trace::mark("startup checks done");

    // The one line that picks the skin.
    if let Err(e) = catalyx_skin_classic::run(&args) {
        eprintln!("CatalyX: the window could not start: {e}");
        std::process::exit(1);
    }
}
