//! The classic skin: the window CatalyX has always had — seven pages, the theme, the boost control, and
//! the self-test that drives them.
//!
//! It draws from the core's `View`, changes things only with `Intent`/`Hint`, and shows the core's events.
//! It never reaches into the engine, and the core never names it (docs/SKINS.md).

pub mod gui;
pub mod selftest;

use gui::Options;

/// The window, from the command line's flags.
pub fn run(args: &[String]) -> Result<(), String> {
    let arg_after = |flag: &str| args.iter().position(|a| a == flag).and_then(|i| args.get(i + 1)).cloned();
    let has = |flag: &str| args.iter().any(|a| a == flag);
    // Developer helper: writes the program's icon (the build embeds assets/catalyx.ico).
    if let Some(path) = arg_after("--write-icon") {
        let _ = std::fs::write(&path, gui::mark::ico_bytes());
        return Ok(());
    }
    let screenshot = arg_after("--screenshot");
    let tour = arg_after("--tour");
    let tab = arg_after("--tab").and_then(|t| gui::TABS.iter().find(|(_, n, _, _)| n.eq_ignore_ascii_case(&t)).map(|(tab, ..)| *tab));
    let size = arg_after("--size").and_then(|s| {
        let (w, h) = s.split_once('x')?;
        Some((w.parse().ok()?, h.parse().ok()?))
    });
    gui::run(Options {
        demo_session: has("--demo-session"),
        selftest: arg_after("--self-test"),
        demo: has("--demo") || screenshot.is_some() || tour.is_some(),
        screenshot,
        tour,
        tab,
        theme: arg_after("--theme"),
        appearance: arg_after("--appearance"),
        accent: arg_after("--accent"),
        control: arg_after("--control"),
        dialog: arg_after("--dialog"),
        size,
    })
}
