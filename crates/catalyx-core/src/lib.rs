//! CatalyX's core: the settings, the boost engine, the Windows probes, the command line, and the loop a
//! window draws from.
//!
//! Nothing here knows about egui. The window is a skin (a separate package: `skins/classic`), so the
//! console program, the tests and any other front-end build without a GUI toolkit. A skin reads
//! `app::View`, changes things only with `app::Intent`/`Hint`, and shows what `Event`s the core hands it
//! (see docs/SKINS.md).
//!
//! The module list is the boundary: only `app`, `config`, `catalog`, `format`, `start` and `trace` are
//! reachable from a skin. The engine, the Windows operations and the process actions are private to
//! this crate, so "a skin changes nothing itself" is checked by the compiler, not by a convention.
//!
//! Start-up is two steps, in `start`: `boot` (trace, the console a window program was started from, the
//! broken-pipe hook), then either `cli::run` or a skin.

pub mod app;
pub mod catalog;
pub mod cli;
pub mod config;
pub mod format;
pub mod start;
pub mod trace;

mod doctor;
mod elevate;
mod engine;
mod ffi;
mod gpu;
mod launcher;
mod library;
mod power;
mod procs;
mod services;
mod shell;
mod tweaks;
mod watch;
