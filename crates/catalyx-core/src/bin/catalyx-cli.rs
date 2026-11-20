//! The console program: the command line and nothing else, so a shell waits for it, pipes and redirects
//! work, and Ctrl+C reaches it — and it links no window toolkit at all.
//!
//! There is no window here (that is `catalyx.exe`), so a run with nothing to do says so and prints the
//! commands.

fn main() {
    let args = catalyx_core::start::boot(false);
    if let Some(code) = catalyx_core::cli::run(&args) {
        std::process::exit(code);
    }
    eprintln!("catalyx-cli has no window; start catalyx.exe for it. The commands are:");
    print!("{}", catalyx_core::cli::help_text());
}
