//! Actions on one running program: its priority, disk and memory priority, its efficiency mode,
//! the standby list, and ending it. Each changes Windows for exactly one process (or the standby
//! list); the engine's journal puts every one of them back.

use crate::ffi::*;
use sysinfo::System;

fn with_process<T>(pid: u32, access: u32, f: impl FnOnce(HANDLE) -> T) -> Option<T> {
    unsafe {
        let h = OpenProcess(access, 0, pid);
        if h.is_null() {
            return None;
        }
        let r = f(h);
        CloseHandle(h);
        Some(r)
    }
}

pub fn terminate(pid: u32) -> bool {
    with_process(pid, PROCESS_TERMINATE, |h| unsafe { TerminateProcess(h, 1) != 0 }).unwrap_or(false)
}

/// Ends every running copy of a program by name, but only after checking that each pid still
/// belongs to the process it was listed as (Windows reuses pids). A fresh scan is made here, off
/// the window: a list drawn a moment ago may be old or not detailed. Copies whose start time
/// cannot be read are left alone: CatalyX never ends something it cannot identify.
pub fn terminate_by_name(name_lower: &str) -> usize {
    let mut sys = System::new();
    sys.refresh_processes();
    let targets: Vec<(u32, u64)> = sys
        .processes()
        .values()
        .filter(|p| p.pid().as_u32() > 4 && p.pid().as_u32() != std::process::id())
        .filter(|p| p.name().to_lowercase() == name_lower)
        .map(|p| {
            let pid = p.pid().as_u32();
            // Programs running as administrator hide their start time from an ordinary process;
            // ask Windows for the identity it does share before giving up.
            let start = match p.start_time() {
                0 => process_identity(pid).map(|(_, start)| start).unwrap_or(0),
                start => start,
            };
            (pid, start)
        })
        .collect();
    targets.into_iter().filter(|(pid, start)| same_process(*pid, *start) && terminate(*pid)).count()
}

type NtProcFn = unsafe extern "system" fn(HANDLE) -> i32;

fn nt_call(name: &str, pid: u32) -> bool {
    let Some(f) = proc_address("ntdll.dll", name) else { return false };
    let f: NtProcFn = unsafe { std::mem::transmute(f) };
    with_process(pid, PROCESS_SUSPEND_RESUME, |h| unsafe { f(h) >= 0 }).unwrap_or(false)
}

pub fn suspend(pid: u32) -> bool {
    nt_call("NtSuspendProcess", pid)
}

pub fn resume(pid: u32) -> bool {
    nt_call("NtResumeProcess", pid)
}

/// Recheck immediately before undoing a process action: Windows can reuse a PID after its
/// original process exits. A zero start time is an old journal without sufficient identity.
pub fn same_process(pid: u32, start: u64) -> bool {
    if start == 0 {
        return false;
    }
    let mut sys = System::new();
    sys.refresh_process(sysinfo::Pid::from_u32(pid));
    sys.process(sysinfo::Pid::from_u32(pid)).map(|p| p.start_time() == start).unwrap_or(false)
}

pub fn get_priority(pid: u32) -> Option<u32> {
    with_process(pid, PROCESS_QUERY_LIMITED_INFORMATION, |h| unsafe { GetPriorityClass(h) }).filter(|c| *c != 0)
}

pub fn set_priority(pid: u32, class: u32) -> bool {
    with_process(pid, PROCESS_SET_INFORMATION, |h| unsafe { SetPriorityClass(h, class) != 0 }).unwrap_or(false)
}

pub fn trim(pid: u32) -> bool {
    with_process(pid, PROCESS_SET_QUOTA | PROCESS_QUERY_LIMITED_INFORMATION, |h| unsafe { K32EmptyWorkingSet(h) != 0 }).unwrap_or(false)
}

/// Memory page priority of one process (1 = very low ... 5 = normal, the default). A game at 5
/// keeps its pages when Windows trims memory; apps lowered to 1 are trimmed first.
pub fn set_memory_priority(pid: u32, level: u32) -> bool {
    #[repr(C)]
    struct MemPrio {
        level: u32,
    }
    let v = MemPrio { level };
    with_process(pid, PROCESS_SET_INFORMATION, |h| unsafe {
        // ProcessMemoryPriority = 0
        SetProcessInformation(h, 0, &v as *const _ as *const std::ffi::c_void, 4) != 0
    })
    .unwrap_or(false)
}

/// Read the old I/O class before changing it so crash recovery can put it back.
pub fn io_priority(pid: u32) -> Option<u32> {
    type NtQueryInfo = unsafe extern "system" fn(HANDLE, u32, *mut std::ffi::c_void, u32, *mut u32) -> i32;
    let f = proc_address("ntdll.dll", "NtQueryInformationProcess")?;
    let f: NtQueryInfo = unsafe { std::mem::transmute(f) };
    with_process(pid, PROCESS_QUERY_LIMITED_INFORMATION, |h| {
        let mut level = 0u32;
        let status = unsafe { f(h, 33, &mut level as *mut _ as *mut _, 4, std::ptr::null_mut()) };
        (status >= 0 && level <= 3).then_some(level)
    })
    .flatten()
}

/// Disk (I/O) priority: 0 very low, 1 low, 2 normal, 3 high.
/// Only try High when the previous setting can be restored.
pub fn set_io_priority(pid: u32, level: u32) -> bool {
    type NtSetInfo = unsafe extern "system" fn(HANDLE, u32, *const std::ffi::c_void, u32) -> i32;
    if level >= 3 {
        enable_privilege("SeIncreaseBasePriorityPrivilege");
    }
    let Some(f) = proc_address("ntdll.dll", "NtSetInformationProcess") else { return false };
    let f: NtSetInfo = unsafe { std::mem::transmute(f) };
    with_process(pid, PROCESS_SET_INFORMATION, |h| unsafe {
        // ProcessIoPriority = 33
        f(h, 33, &level as *const u32 as *const std::ffi::c_void, 4) >= 0
    })
    .unwrap_or(false)
}

/// Empties Windows' standby memory list (cached file pages nobody is using) so a game's
/// allocations never have to wait for it to be recycled. Administrator only.
pub fn purge_standby() -> bool {
    memory_list_command(4)
}

#[repr(C)]
#[derive(Default)]
struct PowerThrottlingState {
    version: u32,
    control_mask: u32,
    state_mask: u32,
}
const PROCESS_POWER_THROTTLING: i32 = 4;

/// Read the game's Windows efficiency-mode state before changing it. If Windows does not expose
/// this state (older builds), leave it alone rather than making an untracked change.
pub fn power_throttling(pid: u32) -> Option<(u32, u32)> {
    with_process(pid, PROCESS_QUERY_LIMITED_INFORMATION, |h| unsafe {
        let mut st = PowerThrottlingState { version: 1, ..Default::default() };
        (GetProcessInformation(
            h,
            PROCESS_POWER_THROTTLING,
            &mut st as *mut _ as *mut std::ffi::c_void,
            std::mem::size_of::<PowerThrottlingState>() as u32,
        ) != 0)
            .then_some((st.control_mask, st.state_mask))
    })
    .flatten()
}

pub fn set_power_throttling(pid: u32, control_mask: u32, state_mask: u32) -> bool {
    let st = PowerThrottlingState { version: 1, control_mask, state_mask };
    with_process(pid, PROCESS_SET_INFORMATION, |h| unsafe {
        SetProcessInformation(
            h,
            PROCESS_POWER_THROTTLING,
            &st as *const _ as *const std::ffi::c_void,
            std::mem::size_of::<PowerThrottlingState>() as u32,
        ) != 0
    })
    .unwrap_or(false)
}

/// The owner (SID) of a process token, as the bytes of a TOKEN_USER.
fn token_user(process: HANDLE) -> Option<Vec<u8>> {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        if OpenProcessToken(process, TOKEN_QUERY, &mut token) == 0 {
            return None;
        }
        let mut needed = 0u32;
        GetTokenInformation(token, TOKEN_USER_CLASS, std::ptr::null_mut(), 0, &mut needed);
        let mut buf = vec![0u8; needed.max(64) as usize];
        let ok = GetTokenInformation(token, TOKEN_USER_CLASS, buf.as_mut_ptr() as *mut _, buf.len() as u32, &mut needed) != 0;
        CloseHandle(token);
        if ok {
            Some(buf)
        } else {
            None
        }
    }
}

/// True when the process runs as the same Windows user as CatalyX (compared by the SID of the
/// process token, not by name).
pub fn owned_by_current_user(pid: u32) -> bool {
    let mine = unsafe { token_user(GetCurrentProcess()) };
    let theirs = with_process(pid, PROCESS_QUERY_LIMITED_INFORMATION, token_user).flatten();
    match (mine, theirs) {
        // A TOKEN_USER starts with a pointer to the SID, which lives in the same buffer.
        (Some(a), Some(b)) => unsafe {
            let sa = *(a.as_ptr() as *const *const std::ffi::c_void);
            let sb = *(b.as_ptr() as *const *const std::ffi::c_void);
            EqualSid(sa, sb) != 0
        },
        _ => false,
    }
}

pub fn priority_class(p: crate::config::Priority) -> u32 {
    match p {
        crate::config::Priority::Normal => NORMAL_PRIORITY_CLASS,
        crate::config::Priority::AboveNormal => ABOVE_NORMAL_PRIORITY_CLASS,
        crate::config::Priority::High => HIGH_PRIORITY_CLASS,
        // Legacy saved profiles requesting Realtime are capped at High for safety.
        crate::config::Priority::Realtime => HIGH_PRIORITY_CLASS,
    }
}
