//! The quiet desktop (see `crate::shell`): switching Windows' shell off once the game is running, bringing
//! it back, the crash guard, and the `desktop test` command.

use super::*;

impl Engine {
    // ------------------------------------------------------------------ quiet desktop

    /// Once the game has been running a few seconds: switch Windows' desktop off (Max Squeeze).
    /// Done only for a game CatalyX started itself, and only if the way back is guaranteed:
    /// the exit shortcut is ours, a crash guard is watching, and the desktop can be restarted as the
    /// ordinary user. Anything less and the desktop is left alone.
    pub(super) fn maybe_quiet_desktop(&mut self) {
        let Some(sess) = &self.session else { return };
        if sess.quiet_tried
            || !sess.req.settings.quiet_desktop
            || sess.req.dry_run
            || sess.req.attach_pid.is_some()
            || sess.req.game.is_none()
            || sess.game_pid == 0
            || sess.game_since.elapsed() < Duration::from_secs(6)
        {
            return;
        }
        if let Some(s) = &mut self.session {
            s.quiet_tried = true;
        }
        if !shell::running() {
            self.log("Windows' desktop was not running, so there was nothing to switch off");
            return;
        }
        if let Err(e) = shell::hotkey_start() {
            self.log(format!("Windows' desktop left on: {e}"));
            return;
        }
        // Journal first (it brings the desktop back after a crash), then the guard, then the change.
        if !self.push_undo(Undo::Shell) {
            shell::hotkey_stop();
            return;
        }
        shell::spawn_guard();
        match shell::stop() {
            Ok(()) => {
                self.log(format!("Windows' desktop switched off for the game; {} brings it back", shell::EXIT_KEYS));
                if let Ok(mut st) = self.status.lock() {
                    st.quiet_desktop = true;
                }
                self.counts(|c| c.quiet_desktop = true);
            }
            Err(e) => {
                self.log(format!("Windows' desktop left on: {e}"));
                if !shell::running() {
                    let _ = shell::start();
                }
                self.forget_last_undo();
                shell::hotkey_stop();
            }
        }
    }

    /// Bring Windows' desktop back and keep the rest of the boost running.
    pub(super) fn exit_quiet_desktop(&mut self, why: &str) {
        let has = self.session.as_ref().map(|s| s.undo.iter().any(|u| matches!(u, Undo::Shell))).unwrap_or(false);
        if !has {
            return;
        }
        match shell::start() {
            Ok(how) => {
                self.log(format!("Windows' desktop is back ({why}; started {how}); the boost keeps running"));
                self.forget_undos(|u| matches!(u, Undo::Shell));
                shell::hotkey_stop();
                if let Ok(mut st) = self.status.lock() {
                    st.quiet_desktop = false;
                }
            }
            Err(e) => self.log(format!("Windows' desktop could not be started: {e}")),
        }
    }
}

/// The crash guard (`catalyx shell-guard <pid>`): waits for the CatalyX that switched the
/// desktop off. While that program lives and the journal still says "off", it does nothing. If the
/// program ends with the desktop still off, it brings the desktop back, then leaves.
pub fn run_guard(parent: u32) {
    loop {
        std::thread::sleep(Duration::from_millis(1000));
        if !journal_has_shell() {
            return; // restored normally (or by hand)
        }
        if !shell::alive(parent) {
            // Give a clean exit a moment to finish its own restore first.
            std::thread::sleep(Duration::from_millis(2500));
            if journal_has_shell() {
                if !shell::running() {
                    let _ = shell::start();
                }
                if shell::running() {
                    clear_shell_journal();
                }
            }
            return;
        }
    }
}

/// `catalyx desktop test`: the same steps a session takes, for a few seconds, with nothing else.
pub fn desktop_test(secs: u64) -> Result<Vec<String>, String> {
    if journal_path().exists() {
        return Err("a boost is waiting to be restored; run `catalyx restore` first".into());
    }
    let mut log = Vec::new();
    if !shell::running() {
        return Err("the Windows desktop is not running".into());
    }
    shell::hotkey_start()?;
    persist_journal(&[Undo::Shell]).map_err(|e| format!("recovery journal: {e}"))?;
    shell::spawn_guard();
    let t = Instant::now();
    let stopped = shell::stop();
    log.push(match &stopped {
        Ok(()) => format!("desktop off after {:.1} s", t.elapsed().as_secs_f32()),
        Err(e) => format!("could not switch it off: {e}"),
    });
    if stopped.is_ok() {
        let until = Instant::now() + Duration::from_secs(secs);
        while Instant::now() < until {
            if shell::take_exit_request() {
                log.push(format!("{} pressed", shell::EXIT_KEYS));
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    }
    let t = Instant::now();
    let back = shell::start();
    shell::hotkey_stop();
    match &back {
        Ok(how) => {
            log.push(format!("desktop back after {:.1} s (started {how})", t.elapsed().as_secs_f32()));
            clear_shell_journal();
        }
        Err(e) => log.push(format!("COULD NOT BRING IT BACK: {e} (run `catalyx desktop restore`)")),
    }
    back?;
    stopped?;
    Ok(log)
}
