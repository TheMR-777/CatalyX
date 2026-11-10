//! The window front-end: a Windows GUI program, so starting it never opens a terminal. Commands typed in a
//! terminal still work (it joins that terminal); for scripts and pipes use `catalyx-cli.exe`.
#![cfg_attr(windows, windows_subsystem = "windows")]

fn main() {
    catalyx_core::run(true);
}
