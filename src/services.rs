//! Windows services: list, stop while gaming, start again afterwards.

use crate::ffi::*;
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct ServiceInfo {
    pub name: String,
    pub display: String,
    pub running: bool,
    pub can_stop: bool,
}

pub fn list() -> Vec<ServiceInfo> {
    let mut out = Vec::new();
    unsafe {
        let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT | SC_MANAGER_ENUMERATE_SERVICE);
        if scm.is_null() {
            return out;
        }
        let mut needed = 0u32;
        let mut returned = 0u32;
        let mut resume = 0u32;
        EnumServicesStatusExW(
            scm,
            SC_ENUM_PROCESS_INFO,
            SERVICE_WIN32,
            SERVICE_STATE_ALL,
            std::ptr::null_mut(),
            0,
            &mut needed,
            &mut returned,
            &mut resume,
            std::ptr::null(),
        );
        if needed > 0 {
            let mut buf: Vec<u64> = vec![0; needed as usize / 8 + 64];
            resume = 0;
            let ok = EnumServicesStatusExW(
                scm,
                SC_ENUM_PROCESS_INFO,
                SERVICE_WIN32,
                SERVICE_STATE_ALL,
                buf.as_mut_ptr() as *mut u8,
                (buf.len() * 8) as u32,
                &mut needed,
                &mut returned,
                &mut resume,
                std::ptr::null(),
            );
            if ok != 0 {
                let items = std::slice::from_raw_parts(buf.as_ptr() as *const ENUM_SERVICE_STATUS_PROCESSW, returned as usize);
                for it in items {
                    let st = it.ServiceStatusProcess;
                    out.push(ServiceInfo {
                        name: from_wide(it.lpServiceName),
                        display: from_wide(it.lpDisplayName),
                        running: st.dwCurrentState == SERVICE_RUNNING,
                        can_stop: st.dwControlsAccepted & 1 != 0,
                    });
                }
            }
        }
        CloseServiceHandle(scm);
    }
    out.sort_by_key(|a| a.display.to_lowercase());
    out
}

fn with_service<T>(name: &str, access: u32, f: impl FnOnce(HANDLE) -> T) -> Result<T, u32> {
    unsafe {
        let scm = OpenSCManagerW(std::ptr::null(), std::ptr::null(), SC_MANAGER_CONNECT);
        if scm.is_null() {
            return Err(GetLastError());
        }
        let w = wide(name);
        let h = OpenServiceW(scm, w.as_ptr(), access);
        if h.is_null() {
            let e = GetLastError();
            CloseServiceHandle(scm);
            return Err(e);
        }
        let r = f(h);
        CloseServiceHandle(h);
        CloseServiceHandle(scm);
        Ok(r)
    }
}

/// Longest wait after a stop/start request is accepted, in milliseconds.
pub const SERVICE_WAIT_LIMIT_MS: u32 = 501;
/// Sleep step: the service's own wait hint when it is 100 ms or less, otherwise 100 ms.
pub const SERVICE_WAIT_STEP_MS: u32 = 100;

/// Waits briefly for a service to reach a state. `stop` = waiting for STOPPED (1), else RUNNING (4).
/// Stop gives up when the check point stops moving (`<= previous`); start gives up when it goes
/// back (`< previous`) or is 0.
unsafe fn wait_for_state(h: HANDLE, want: u32, stop: bool) {
    let mut st = SERVICE_STATUS::default();
    if QueryServiceStatus(h, &mut st) == 0 {
        return;
    }
    let mut waited = 0u32;
    while st.dwCurrentState != want && waited < SERVICE_WAIT_LIMIT_MS {
        let prev = st.dwCheckPoint;
        let step = if st.dwWaitHint <= SERVICE_WAIT_STEP_MS { st.dwWaitHint } else { SERVICE_WAIT_STEP_MS };
        std::thread::sleep(Duration::from_millis(step as u64));
        waited += step;
        if QueryServiceStatus(h, &mut st) == 0 {
            break;
        }
        let give_up = if stop { st.dwCheckPoint <= prev } else { st.dwCheckPoint < prev || st.dwCheckPoint == 0 };
        if give_up {
            break;
        }
    }
}

/// Asks a service to stop. Ok(true) when the request was accepted: OpenService(STOP | QUERY_STATUS),
/// ControlService(STOP), then the short wait above.
pub fn stop(name: &str) -> Result<bool, String> {
    with_service(name, SERVICE_STOP | SERVICE_QUERY_STATUS, |h| unsafe {
        let mut st = SERVICE_STATUS::default();
        if ControlService(h, SERVICE_CONTROL_STOP, &mut st) != 0 {
            wait_for_state(h, SERVICE_STOPPED, true);
            Ok(true)
        } else {
            Err(GetLastError())
        }
    })
    .map_err(err_text)?
    .map_err(err_text)
}

/// Starts a service again: OpenService(START | QUERY_STATUS), StartServiceW with no arguments, then
/// the short wait above. The raw Windows error number is kept, so callers can tell the cases apart.
fn start_raw(name: &str) -> Result<bool, u32> {
    match with_service(name, SERVICE_START | SERVICE_QUERY_STATUS, |h| unsafe {
        if StartServiceW(h, 0, std::ptr::null()) != 0 {
            wait_for_state(h, SERVICE_RUNNING, false);
            Ok(true)
        } else {
            let e = GetLastError();
            // 1056 = already running
            if e == 1056 {
                Ok(true)
            } else {
                Err(e)
            }
        }
    }) {
        Ok(r) => r,
        Err(e) => Err(e),
    }
}

/// Whether a service is running right now. Listing needs no permission to control a service, so
/// this also works for the ones Windows protects (their state can be seen, not changed).
pub fn is_running(name: &str) -> Option<bool> {
    list().into_iter().find(|s| s.name.eq_ignore_ascii_case(name)).map(|s| s.running)
}

/// Whether Windows lets us open this service for starting it again. Some Windows services (the
/// protected ones) can be stopped but never started again by anybody but the system itself: a
/// service like that must never be stopped, because then it could not be put back.
pub fn can_start(name: &str) -> bool {
    with_service(name, SERVICE_START | SERVICE_QUERY_STATUS, |_| ()).is_ok()
}

/// What became of one service record after trying to put it back.
pub struct UndoOutcome {
    /// True when there is nothing left to restore: the service is running again, or it was never
    /// really stopped, so the record can be dropped instead of being retried for ever.
    pub done: bool,
    /// Why there was nothing to do, when that is worth telling the person.
    pub note: Option<String>,
}

/// Decides what to do with one service record. Pure, so it can be tested without touching Windows.
/// `running` is None when the service is not installed any more.
pub fn service_undo_decision(running: Option<bool>, start_error: Option<u32>, admin: bool) -> UndoOutcome {
    match running {
        Some(true) => UndoOutcome { done: true, note: None },
        None => UndoOutcome { done: true, note: Some("is not installed any more; nothing to restore".into()) },
        Some(false) => match start_error {
            None => UndoOutcome { done: true, note: None },
            // 1060 = the service is gone
            Some(1060) => UndoOutcome { done: true, note: Some("is not installed any more; nothing to restore".into()) },
            // 5 = access denied. As an administrator this is not about rights: Windows protects
            // that service and lets nobody start it, so the record can never be undone. Keeping
            // it would block every later restore, so it is dropped and the person is told.
            Some(5) if admin => UndoOutcome {
                done: true,
                note: Some(
                    "Windows does not allow this service to be started again; it is left as Windows has it, and Windows starts it again when it needs it"
                        .into(),
                ),
            },
            Some(e) => UndoOutcome { done: false, note: Some(format!("could not start again: {}", err_text_with(e, admin))) },
        },
    }
}

/// Puts a stopped service back after a boost stopped it, and says what became of it.
pub fn undo_stop(name: &str) -> UndoOutcome {
    let running = is_running(name);
    let start_error = if running == Some(true) { None } else { start_raw(name).err() };
    service_undo_decision(running, start_error, crate::ffi::is_admin())
}

/// How long one stop may take before it is given up on: the stop runs on its own thread and is
/// waited for for this long.
pub const SERVICE_STOP_TIMEOUT_MS: u64 = 3000;

fn fail_path() -> std::path::PathBuf {
    crate::config::data_dir().join("service_fail.json")
}

fn today() -> u64 {
    crate::procs::now_secs() / 86_400
}

/// A service that timed out is written down with today's date and skipped for the rest of the
/// day (kept in `service_fail.json`, checked before every stop).
pub fn failed_today(name: &str) -> bool {
    let Ok(text) = std::fs::read_to_string(fail_path()) else { return false };
    let map: std::collections::BTreeMap<String, u64> = serde_json::from_str(&text).unwrap_or_default();
    map.get(&name.to_ascii_lowercase()) == Some(&today())
}

fn mark_failed_today(name: &str) {
    let mut map: std::collections::BTreeMap<String, u64> =
        std::fs::read_to_string(fail_path()).ok().and_then(|t| serde_json::from_str(&t).ok()).unwrap_or_default();
    let d = today();
    map.retain(|_, v| *v == d);
    map.insert(name.to_ascii_lowercase(), d);
    if let Ok(j) = serde_json::to_string_pretty(&map) {
        let _ = std::fs::write(fail_path(), j);
    }
}

/// Running services that depend on `name` (they must stop first, each before the service itself).
pub fn running_dependents(name: &str) -> Vec<String> {
    with_service(name, SERVICE_ENUMERATE_DEPENDENTS, |h| unsafe {
        let mut out = Vec::new();
        let mut needed = 0u32;
        let mut returned = 0u32;
        EnumDependentServicesW(h, SERVICE_ACTIVE, std::ptr::null_mut(), 0, &mut needed, &mut returned);
        if needed == 0 {
            return out;
        }
        let mut buf: Vec<u64> = vec![0; needed as usize / 8 + 8];
        if EnumDependentServicesW(h, SERVICE_ACTIVE, buf.as_mut_ptr() as *mut u8, (buf.len() * 8) as u32, &mut needed, &mut returned) != 0 {
            let items = std::slice::from_raw_parts(buf.as_ptr() as *const ENUM_SERVICE_STATUSW, returned as usize);
            for it in items {
                out.push(from_wide(it.lpServiceName));
            }
        }
        out
    })
    .unwrap_or_default()
}

/// One stop with a 3 s limit. A stop that takes longer is left to finish on its own, and the
/// service is skipped for the rest of today.
pub fn stop_with_timeout(name: &str) -> Result<bool, String> {
    let (tx, rx) = std::sync::mpsc::channel();
    let n = name.to_string();
    std::thread::spawn(move || {
        let _ = tx.send(stop(&n));
    });
    match rx.recv_timeout(Duration::from_millis(SERVICE_STOP_TIMEOUT_MS)) {
        Ok(r) => r,
        Err(_) => {
            mark_failed_today(name);
            Err("did not stop within 3 seconds; skipped for the rest of today".into())
        }
    }
}

/// The whole stop step for one service: skip it if it timed out earlier today, stop its
/// running dependents first, then the service. `protected` services are never stopped; if a
/// dependent is protected the service is left running. Returns every service that was stopped,
/// in the order it was stopped, so each one can be started again afterwards.
pub fn stop_with_dependents(name: &str, protected: impl Fn(&str) -> bool) -> Result<Vec<String>, String> {
    if failed_today(name) {
        return Err("skipped: it did not stop in time earlier today".into());
    }
    let deps = running_dependents(name);
    if let Some(p) = deps.iter().find(|d| protected(d)) {
        return Err(format!("left running: {p} depends on it"));
    }
    let mut done = Vec::new();
    for d in &deps {
        if failed_today(d) {
            return Err(format!("left running: {d} depends on it and did not stop earlier today"));
        }
        match stop_with_timeout(d) {
            Ok(_) => done.push(d.clone()),
            Err(e) => {
                // Stopped dependents are handed back so they are still started again afterwards.
                return if done.is_empty() { Err(format!("{d} (depends on it): {e}")) } else { Ok(done) };
            }
        }
    }
    match stop_with_timeout(name) {
        Ok(_) => {
            done.push(name.to_string());
            Ok(done)
        }
        Err(e) if done.is_empty() => Err(e),
        Err(_) => Ok(done),
    }
}

fn err_text(e: u32) -> String {
    err_text_with(e, crate::ffi::is_admin())
}

/// What Windows' error number means. `admin` says whether we are already elevated: an elevated
/// "access denied" is not about rights, it means Windows protects that service from everybody.
fn err_text_with(e: u32, admin: bool) -> String {
    match e {
        5 if admin => "Windows does not allow this service to be controlled".into(),
        5 => "needs administrator rights".into(),
        1051 => "other services depend on it".into(),
        1052 => "it can't be stopped".into(),
        1060 => "not installed".into(),
        1062 => "not running".into(),
        _ => format!("Windows error {e}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_service_that_is_running_again_needs_nothing() {
        let out = service_undo_decision(Some(true), None, true);
        assert!(out.done);
        assert!(out.note.is_none());
    }

    #[test]
    fn a_service_windows_protects_is_dropped_not_kept_for_ever() {
        // Windows says "access denied" even to an administrator: the service cannot be started
        // again by anybody, so the record must not block every later restore.
        let out = service_undo_decision(Some(false), Some(5), true);
        assert!(out.done, "a record for a service we can never start again must not be kept");
        assert!(out.note.unwrap().contains("Windows"));
    }

    #[test]
    fn without_admin_rights_the_record_is_kept() {
        // Not elevated: the person can run the restore as administrator later.
        let out = service_undo_decision(Some(false), Some(5), false);
        assert!(!out.done, "a record that needs administrator rights must be kept for later");
        assert!(out.note.unwrap().contains("administrator"));
    }

    #[test]
    fn a_service_that_is_gone_needs_nothing() {
        let out = service_undo_decision(None, Some(1060), true);
        assert!(out.done);
        let out = service_undo_decision(Some(false), Some(1060), true);
        assert!(out.done);
    }

    #[test]
    fn a_real_failure_keeps_the_record() {
        let out = service_undo_decision(Some(false), Some(1051), true);
        assert!(!out.done, "a service that could not start again must stay in the journal");
        assert!(out.note.unwrap().contains("could not start again"));
    }

    #[test]
    fn access_denied_reads_differently_when_elevated() {
        assert_eq!(err_text_with(5, false), "needs administrator rights");
        assert_eq!(err_text_with(5, true), "Windows does not allow this service to be controlled");
    }
}
