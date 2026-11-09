//! The recovery journal: every change is written here BEFORE it is made, so a crash, a power cut or a
//! killed process can still be undone. Writes replace the file atomically.

use super::types::Undo;
use std::io::Write;
use std::os::windows::ffi::OsStrExt;

#[link(name = "kernel32")]
extern "system" {
    fn MoveFileExW(from: *const u16, to: *const u16, flags: u32) -> i32;
}

pub(super) fn journal_path() -> std::path::PathBuf {
    crate::config::data_dir().join("journal.json")
}

/// Flush a complete recovery record, then replace the old one atomically. Never truncate the
/// previous journal: a crash halfway through an update must leave at least one valid copy.
pub(super) fn persist_journal(list: &[Undo]) -> std::io::Result<()> {
    let path = journal_path();
    let temp = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec_pretty(list)?;
    {
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
    }
    let w = |p: &std::path::Path| p.as_os_str().encode_wide().chain(Some(0)).collect::<Vec<u16>>();
    // MOVEFILE_REPLACE_EXISTING | MOVEFILE_WRITE_THROUGH. Unlike std::fs::rename this
    // explicitly replaces an existing destination on Windows.
    if unsafe { MoveFileExW(w(&temp).as_ptr(), w(&path).as_ptr(), 0x1 | 0x8) } == 0 {
        return Err(std::io::Error::last_os_error());
    }
    Ok(())
}

pub(super) fn read_journal() -> Vec<Undo> {
    std::fs::read_to_string(journal_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default()
}

/// True when a recovery journal says Windows' desktop was switched off and not yet brought back.
pub fn journal_has_shell() -> bool {
    read_journal().iter().any(|u| matches!(u, Undo::Shell))
}

/// Remove the "desktop is off" record (after the desktop is back), keeping everything else.
pub fn clear_shell_journal() {
    let list: Vec<Undo> = read_journal().into_iter().filter(|u| !matches!(u, Undo::Shell)).collect();
    if list.is_empty() {
        let _ = std::fs::remove_file(journal_path());
    } else {
        let _ = persist_journal(&list);
    }
}
