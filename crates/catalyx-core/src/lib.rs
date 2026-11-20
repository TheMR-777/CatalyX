//! CatalyX's core: the settings, the boost engine, the Windows probes, the command line, and the loop a
//! window draws from.
//!
//! Nothing here knows about egui. The window is a skin (a separate package: `skins/classic`), so the
//! console program, the tests and any other front-end build without a GUI toolkit. A skin reads
//! `app::View`, changes things only with `app::Intent`/`Hint`, and shows what `Event`s the core hands it
//! (see docs/SKINS.md).
//!
//! Start-up is two steps, in `start`: `boot` (trace, the console a window program was started from, the
//! broken-pipe hook), then either `cli::run` or a skin.

pub mod app;
pub mod catalog;
pub mod cli;
pub mod config;
pub mod doctor;
pub mod elevate;
pub mod engine;
pub mod ffi;
pub mod gpu;
pub mod launcher;
pub mod library;
pub mod power;
pub mod procs;
pub mod services;
pub mod shell;
pub mod start;
pub mod trace;
pub mod tweaks;
pub mod watch;
