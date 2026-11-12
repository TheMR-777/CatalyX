//! Putting everything back: from the live session's undo list, or from a journal a crash left behind.

use super::*;

impl Engine {
    // ------------------------------------------------------------------ restore

    pub(super) fn undo_all(&mut self, list: Vec<Undo>) -> (u32, Vec<Undo>) {
        let mut n = 0;
        let mut remaining = Vec::new();
        for u in list.into_iter().rev() {
            let ok = match &u {
                Undo::Power { prev } => {
                    let ok = prev.is_empty() || power::set_active(prev);
                    // CatalyX's own plan is rebuilt at every boost; don't leave it lying around.
                    if ok && !prev.eq_ignore_ascii_case(power::BOOST_PLAN) {
                        power::delete_boost_plan();
                    }
                    ok
                }
                Undo::Reg { hive, path, name, prev } => match prev {
                    Some(v) => tweaks::write(*hive, path, name, v).is_ok(),
                    None => tweaks::delete(*hive, path, name),
                },
                Undo::Task { name, was_enabled } => tweaks::set_task(name, *was_enabled),
                Undo::Service { name } => {
                    // A service that is running again, or that Windows never let us control, has
                    // nothing left to restore: keeping its record would block every restore for
                    // ever, so it is dropped with a note.
                    let out = services::undo_stop(name);
                    if let Some(note) = &out.note {
                        self.log(format!("Service {name}: {note}"));
                    }
                    out.done
                }
                Undo::Resume { pid, start, .. } => !procs::same_process(*pid, *start) || procs::resume(*pid),
                Undo::Priority { pid, class, start } => !procs::same_process(*pid, *start) || procs::set_priority(*pid, *class),
                Undo::IoPriority { pid, level, start } => !procs::same_process(*pid, *start) || procs::set_io_priority(*pid, *level),
                Undo::Throttling { pid, start, control, state } => {
                    !procs::same_process(*pid, *start) || procs::set_power_throttling(*pid, *control, *state)
                }
                Undo::Gpu { core, mem } => gpu::restore(*core, *mem).is_ok(),
                Undo::Shell => {
                    shell::hotkey_stop();
                    match shell::start() {
                        Ok(how) => {
                            self.log(format!("Windows' desktop is back (started {how})"));
                            if let Ok(mut st) = self.status.lock() {
                                st.quiet_desktop = false;
                            }
                            true
                        }
                        Err(e) => {
                            self.log(format!("Windows' desktop could not be started: {e}"));
                            false
                        }
                    }
                }
                Undo::Reopen { exe, pid, start } => {
                    // If ending the app failed, it is still the same process: do not start a
                    // duplicate. Otherwise start it only when the person opted into reopening.
                    if procs::same_process(*pid, *start) {
                        true
                    } else {
                        let path = std::path::Path::new(exe);
                        let dir = path.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
                        match crate::ffi::shell_start(exe, "", &dir, false) {
                            Ok(_) => {
                                self.log(format!(
                                    "Reopened {}",
                                    path.file_name().map(|f| f.to_string_lossy().to_string()).unwrap_or(exe.clone())
                                ));
                                true
                            }
                            Err(e) => {
                                self.log(format!("Could not reopen {exe}: {e}"));
                                false
                            }
                        }
                    }
                }
            };
            if ok {
                n += 1;
            } else {
                self.log(format!("Could not undo {u:?}; kept for a later Restore"));
                remaining.push(u);
            }
        }
        remaining.reverse();
        (n, remaining)
    }

    pub(super) fn finish_restore(&mut self, list: Vec<Undo>, label: &str) {
        let (n, remaining) = self.undo_all(list);
        if remaining.is_empty() {
            if let Err(e) = std::fs::remove_file(journal_path()) {
                if e.kind() != std::io::ErrorKind::NotFound {
                    self.log(format!("Recovery journal could not be removed: {e}"));
                }
            }
        } else if let Err(e) = persist_journal(&remaining) {
            self.log(format!("Recovery journal could not be updated: {e}; the original is kept"));
        }
        let pending = journal_path().exists();
        self.log(format!("{label} ({n} changes undone{})", if pending { "; some still need restoring" } else { "" }));
        if let Ok(mut st) = self.status.lock() {
            st.phase = Phase::Idle;
            st.game_pid = 0;
            st.restored_runs += 1;
            st.pending_journal = pending;
        }
    }

    pub(super) fn restore(&mut self) {
        let Some(sess) = self.session.take() else { return };
        self.set_phase(Phase::Restoring);
        self.finish_restore(sess.undo, "Restored to normal mode");
        self.release_lock();
    }

    /// Undo a boost left over from a crash or power cut; never erase an unreadable record.
    pub(super) fn restore_journal(&mut self) {
        let Ok(text) = std::fs::read_to_string(journal_path()) else {
            self.log("Nothing to restore");
            return;
        };
        let Ok(list) = serde_json::from_str::<Vec<Undo>>(&text) else {
            self.log("Recovery journal is unreadable; kept it intact for manual recovery");
            return;
        };
        self.set_phase(Phase::Restoring);
        self.finish_restore(list, "Restored the previous boost");
    }
}
