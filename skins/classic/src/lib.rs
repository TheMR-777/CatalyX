//! The classic skin: the window CatalyX has always had — seven pages, the theme, the boost control, and
//! the check runs that drive them.
//!
//! It draws from the core's `View`, changes things only with `Intent`/`Hint`, and shows the core's events.
//! It never reaches into the engine or Windows, and the core never names it (docs/SKINS.md).

pub mod gui;

use catalyx_core::app;
use gui::Options;

/// The window, with the core's flags already parsed (`app::Options`). The classic skin's own flags are
/// the ones the core left in `Options::rest`.
pub fn run(run: app::Options) -> Result<(), String> {
    // Developer helper: writes the program's icon (the build embeds app/assets/catalyx.ico).
    if let Some(path) = run.value("--write-icon") {
        let _ = std::fs::write(&path, gui::mark::ico_bytes());
        return Ok(());
    }
    let tab = run.value("--tab").and_then(|t| gui::TABS.iter().find(|(_, n, _, _)| n.eq_ignore_ascii_case(&t)).map(|(tab, ..)| *tab));
    let size = run.size;
    gui::run(Options {
        theme: run.value("--theme"),
        appearance: run.value("--appearance"),
        accent: run.value("--accent"),
        control: run.value("--control"),
        dialog: run.value("--dialog"),
        tab,
        size,
        run,
    })
}
