//! The window program: the core's command line first, then the skin. A Windows GUI program, so starting
//! it never opens a terminal; commands typed in a terminal still work (it joins that terminal), and for
//! scripts and pipes use `catalyx-cli.exe`.
#![cfg_attr(windows, windows_subsystem = "windows")]

use catalyx_core::{app, cli, start, trace};

fn main() {
    let args = start::boot(true);
    // `catalyx help`, `catalyx play ...`: a command prints and the program ends without a window.
    if let Some(code) = cli::run(&args) {
        std::process::exit(code);
    }
    // A screenshot, a tour or the self-test is a check run: scratch settings and a watchdog.
    let opts = app::Options::from_args(&args);
    if !start::prepare_window(&opts) {
        std::process::exit(0);
    }
    trace::mark("startup checks done");

    // The one line that picks the skin.
    if let Err(e) = catalyx_skin_classic::run(opts) {
        eprintln!("CatalyX: the window could not start: {e}");
        std::process::exit(1);
    }
}
