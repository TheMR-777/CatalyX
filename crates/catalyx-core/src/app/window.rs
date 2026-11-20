//! Window helpers a skin may use, so it needs no `unsafe` or process handling of its own (docs/SKINS.md,
//! "Window helpers"). They change nothing outside the window.

use crate::ffi;

/// What a file dialog asks for.
pub struct FileDialog {
    pub title: String,
    /// Windows filter pairs, each part ending in \0 and the whole list in \0\0.
    pub filter: String,
    /// Default extension for a save dialog ("" = none).
    pub ext: String,
    pub save: bool,
}

/// Shows the native file dialog on a short-lived thread (it blocks until the person chooses) and calls
/// `done` there with the chosen path, or None when cancelled.
pub fn pick_file(d: FileDialog, done: impl FnOnce(Option<String>) + Send + 'static) {
    std::thread::spawn(move || done(ffi::pick_file(&d.title, &d.filter, &d.ext, d.save)));
}

/// Opens a folder in Explorer.
pub fn open_folder(path: impl AsRef<std::ffi::OsStr>) {
    let _ = std::process::Command::new("explorer").arg(path).spawn();
}
