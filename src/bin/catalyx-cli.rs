//! The console front-end: the same program as `catalyx.exe`, started as a console program, so a shell waits
//! for it, pipes and redirects work, and Ctrl+C reaches it. Use it in scripts and from agents.

fn main() {
    catalyx_core::run(false);
}
