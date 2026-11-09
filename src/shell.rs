//! The quiet desktop: while a game runs, Windows' own shell (`explorer.exe` - taskbar, desktop
//! icons, Start menu, notification area) is switched off, which frees a little memory and CPU and
//! removes every pop-up. When the game is over it is brought back.
//!
//! Two things matter more than the saving, and both are handled here:
//!
//! 1. **Stopping is clean.** Explorer is asked to leave the way its own "Exit Explorer" menu entry
//!    does (message 0x5B4 to the taskbar), so it saves its state and Windows does not treat it as a
//!    crash. It is only ended by force if it does not leave within a few seconds.
//! 2. **Starting is as the normal user.** CatalyX is usually running as administrator. A shell
//!    started from an elevated program is itself elevated, and an elevated shell is what makes the
//!    Start menu, search and the taskbar flyouts misbehave until Explorer is restarted by hand.
//!    So the shell is always started with the same token the old shell had (or, after a crash, the
//!    user's ordinary token), the user's own clean environment and on the interactive desktop.
#![allow(non_snake_case, non_camel_case_types, clippy::upper_case_acronyms)]

use crate::ffi::{self, wide, CloseHandle, OpenProcess, TerminateProcess, BOOL, HANDLE};
use std::ffi::c_void;
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::time::{Duration, Instant};

/// The shortcut that brings the desktop back: Ctrl + Alt + End.
pub const EXIT_KEYS: &str = "Ctrl+Alt+End";

#[link(name = "user32")]
extern "system" {
    fn FindWindowW(class: *const u16, title: *const u16) -> HANDLE;
    fn GetWindowThreadProcessId(hwnd: HANDLE, pid: *mut u32) -> u32;
    fn PostMessageW(hwnd: HANDLE, msg: u32, wparam: usize, lparam: isize) -> BOOL;
    fn RegisterHotKey(hwnd: HANDLE, id: i32, modifiers: u32, vk: u32) -> BOOL;
    fn UnregisterHotKey(hwnd: HANDLE, id: i32) -> BOOL;
    fn GetMessageW(msg: *mut MSG, hwnd: HANDLE, min: u32, max: u32) -> BOOL;
    fn PeekMessageW(msg: *mut MSG, hwnd: HANDLE, min: u32, max: u32, remove: u32) -> BOOL;
    fn PostThreadMessageW(thread: u32, msg: u32, wparam: usize, lparam: isize) -> BOOL;
}

#[link(name = "kernel32")]
extern "system" {
    fn ProcessIdToSessionId(pid: u32, session: *mut u32) -> BOOL;
    fn GetCurrentThreadId() -> u32;
    fn CreateProcessW(
        app: *const u16,
        cmd: *mut u16,
        pattr: *const c_void,
        tattr: *const c_void,
        inherit: BOOL,
        flags: u32,
        env: *mut c_void,
        dir: *const u16,
        si: *const STARTUPINFOW,
        pi: *mut PROCESS_INFORMATION,
    ) -> BOOL;
}

#[link(name = "advapi32")]
extern "system" {
    fn DuplicateTokenEx(token: HANDLE, access: u32, attrs: *const c_void, level: i32, kind: i32, new: *mut HANDLE) -> BOOL;
    fn CreateProcessAsUserW(
        token: HANDLE,
        app: *const u16,
        cmd: *mut u16,
        pattr: *const c_void,
        tattr: *const c_void,
        inherit: BOOL,
        flags: u32,
        env: *mut c_void,
        dir: *const u16,
        si: *const STARTUPINFOW,
        pi: *mut PROCESS_INFORMATION,
    ) -> BOOL;
    fn CreateProcessWithTokenW(
        token: HANDLE,
        logon: u32,
        app: *const u16,
        cmd: *mut u16,
        flags: u32,
        env: *mut c_void,
        dir: *const u16,
        si: *const STARTUPINFOW,
        pi: *mut PROCESS_INFORMATION,
    ) -> BOOL;
}

#[link(name = "userenv")]
extern "system" {
    fn CreateEnvironmentBlock(env: *mut *mut c_void, token: HANDLE, inherit: BOOL) -> BOOL;
    fn DestroyEnvironmentBlock(env: *mut c_void) -> BOOL;
}

#[repr(C)]
struct STARTUPINFOW {
    cb: u32,
    reserved: *mut u16,
    desktop: *mut u16,
    title: *mut u16,
    x: u32,
    y: u32,
    xsize: u32,
    ysize: u32,
    xchars: u32,
    ychars: u32,
    fill: u32,
    flags: u32,
    show: u16,
    cb2: u16,
    reserved2: *mut u8,
    std_in: HANDLE,
    std_out: HANDLE,
    std_err: HANDLE,
}

#[repr(C)]
struct PROCESS_INFORMATION {
    process: HANDLE,
    thread: HANDLE,
    pid: u32,
    tid: u32,
}

#[repr(C)]
struct MSG {
    hwnd: HANDLE,
    message: u32,
    wparam: usize,
    lparam: isize,
    time: u32,
    x: i32,
    y: i32,
    private: u32,
}

const TOKEN_ASSIGN_PRIMARY: u32 = 0x1;
const TOKEN_DUPLICATE: u32 = 0x2;
const TOKEN_QUERY: u32 = 0x8;
const MAXIMUM_ALLOWED: u32 = 0x0200_0000;
const TOKEN_ELEVATION: i32 = 20;
const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
const SYNCHRONIZE: u32 = 0x0010_0000;
const PROCESS_TERMINATE: u32 = 0x1;
const CREATE_UNICODE_ENVIRONMENT: u32 = 0x400;
const WM_HOTKEY: u32 = 0x312;
const WM_QUIT: u32 = 0x12;
/// What Explorer's "Exit Explorer" entry (Ctrl+Shift+right-click on the taskbar) sends it.
const WM_EXIT_EXPLORER: u32 = 0x5B4;

/// A copy of the user's ordinary token, taken from the running shell just before it is stopped.
static USER_TOKEN: AtomicIsize = AtomicIsize::new(0);
static EXIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static HOTKEY_THREAD: AtomicU32 = AtomicU32::new(0);

fn tray_window() -> HANDLE {
    unsafe { FindWindowW(wide("Shell_TrayWnd").as_ptr(), std::ptr::null()) }
}

/// True while the Windows shell (taskbar) is up on this desktop.
pub fn running() -> bool {
    !tray_window().is_null()
}

fn windows_dir() -> String {
    std::env::var("SystemRoot").unwrap_or_else(|_| "C:\\Windows".into())
}

fn wait_process_gone(pid: u32, timeout: Duration) -> bool {
    unsafe {
        let h = OpenProcess(SYNCHRONIZE, 0, pid);
        if h.is_null() {
            return true; // already gone
        }
        let r = ffi::WaitForSingleObject(h, timeout.as_millis() as u32);
        CloseHandle(h);
        r == 0
    }
}

fn is_elevated(token: HANDLE) -> bool {
    let mut e: u32 = 0;
    let mut n = 0u32;
    unsafe { ffi::GetTokenInformation(token, TOKEN_ELEVATION, &mut e as *mut u32 as *mut c_void, 4, &mut n) != 0 && e != 0 }
}

/// True when Windows' User Account Control is switched off (every program then runs elevated, and
/// there is no separate "ordinary" token).
fn uac_disabled() -> bool {
    use winreg::{enums::HKEY_LOCAL_MACHINE, RegKey};
    RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SOFTWARE\\Microsoft\\Windows\\CurrentVersion\\Policies\\System")
        .and_then(|k| k.get_value::<u32, _>("EnableLUA"))
        .map(|v| v == 0)
        .unwrap_or(false)
}

/// A duplicate (as a primary token) of the token of process `pid`. With `ordinary_only` it must be
/// the user's ordinary, non-elevated token; without, whatever token that process has (used for the
/// shell itself: giving it back exactly the token it had is what "as before" means).
fn token_of(pid: u32, ordinary_only: bool) -> Option<HANDLE> {
    unsafe {
        let p = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if p.is_null() {
            return None;
        }
        let mut t: HANDLE = std::ptr::null_mut();
        let ok = ffi::OpenProcessToken(p, TOKEN_ASSIGN_PRIMARY | TOKEN_DUPLICATE | TOKEN_QUERY, &mut t) != 0;
        CloseHandle(p);
        if !ok {
            return None;
        }
        let elevated = is_elevated(t);
        let mut dup: HANDLE = std::ptr::null_mut();
        // TokenPrimary = 1, SecurityImpersonation = 2
        let made = !(ordinary_only && elevated) && DuplicateTokenEx(t, MAXIMUM_ALLOWED, std::ptr::null(), 2, 1, &mut dup) != 0;
        CloseHandle(t);
        if made {
            Some(dup)
        } else {
            None
        }
    }
}

fn my_session() -> u32 {
    let mut s = 0u32;
    unsafe { ProcessIdToSessionId(ffi::GetCurrentProcessId(), &mut s) };
    s
}

/// The user's ordinary token: the copy kept from the shell we stopped, or - after a crash, when
/// that copy is gone - the token of another of the user's own programs that always runs
/// un-elevated in this session.
fn user_token() -> Option<HANDLE> {
    let saved = USER_TOKEN.load(Ordering::SeqCst);
    if saved != 0 {
        return Some(saved as HANDLE);
    }
    let session = my_session();
    let list = ffi::quick_list();
    // First the user's ordinary token. Only when UAC is off (so none exists) is an elevated one the
    // user's normal token.
    let passes: &[bool] = if uac_disabled() { &[true, false] } else { &[true] };
    for &ordinary_only in passes {
        for name in ["sihost.exe", "ctfmon.exe", "taskhostw.exe", "explorer.exe", "onedrive.exe", "dllhost.exe"] {
            for (pid, _, n) in &list {
                let mut s = 0u32;
                if n != name || unsafe { ProcessIdToSessionId(*pid, &mut s) } == 0 || s != session {
                    continue;
                }
                if !crate::procs::owned_by_current_user(*pid) {
                    continue;
                }
                if let Some(t) = token_of(*pid, ordinary_only) {
                    USER_TOKEN.store(t as isize, Ordering::SeqCst);
                    return Some(t);
                }
            }
        }
    }
    None
}

/// Starts `explorer.exe` with the given primary token on the interactive desktop.
fn start_with_token(token: HANDLE) -> Result<String, String> {
    let exe = format!("{}\\explorer.exe", windows_dir());
    let dir = wide(&windows_dir());
    let mut cmd = wide(&format!("\"{exe}\""));
    let mut desktop = wide("winsta0\\default");
    unsafe {
        let mut env: *mut c_void = std::ptr::null_mut();
        let have_env = CreateEnvironmentBlock(&mut env, token, 0) != 0;
        let mut si: STARTUPINFOW = std::mem::zeroed();
        si.cb = std::mem::size_of::<STARTUPINFOW>() as u32;
        si.desktop = desktop.as_mut_ptr();
        let mut pi: PROCESS_INFORMATION = std::mem::zeroed();
        let flags = if have_env { CREATE_UNICODE_ENVIRONMENT } else { 0 };
        let envp = if have_env { env } else { std::ptr::null_mut() };
        let app = wide(&exe);
        let mut ok = CreateProcessAsUserW(
            token,
            app.as_ptr(),
            cmd.as_mut_ptr(),
            std::ptr::null(),
            std::ptr::null(),
            0,
            flags,
            envp,
            dir.as_ptr(),
            &si,
            &mut pi,
        ) != 0;
        let mut how = "as the user".to_string();
        let mut err = if ok { 0 } else { ffi::GetLastError() };
        if !ok {
            // Administrators don't hold the "replace a process token" right; this route needs only
            // the "impersonate" right, which they do.
            cmd = wide(&format!("\"{exe}\""));
            ok = CreateProcessWithTokenW(token, 0, app.as_ptr(), cmd.as_mut_ptr(), flags, envp, dir.as_ptr(), &si, &mut pi) != 0;
            if ok {
                how = "as the user (token)".into();
            } else {
                err = ffi::GetLastError();
            }
        }
        if !ok && (uac_disabled() || !ffi::is_admin()) {
            // No second token exists (UAC off) or we are not elevated: our own token is the user's.
            // Still with the user's own clean environment, not whatever this process inherited.
            cmd = wide(&format!("\"{exe}\""));
            ok = CreateProcessW(
                app.as_ptr(),
                cmd.as_mut_ptr(),
                std::ptr::null(),
                std::ptr::null(),
                0,
                flags,
                envp,
                dir.as_ptr(),
                &si,
                &mut pi,
            ) != 0;
            if ok {
                how = "as the user (this session)".into();
            } else {
                err = ffi::GetLastError();
            }
        }
        if have_env {
            DestroyEnvironmentBlock(env);
        }
        if !ok {
            return Err(format!("Windows refused to start the desktop as the user (error {err})"));
        }
        CloseHandle(pi.process);
        CloseHandle(pi.thread);
        Ok(how)
    }
}

/// Brings the Windows desktop back, as the normal user. Returns how it was started, or `Ok` with an
/// explanation when it was already running.
pub fn start() -> Result<String, String> {
    if running() {
        return Ok("already running".into());
    }
    let how = if ffi::is_admin() {
        // Elevated: use the user's own token, never this program's by accident.
        match user_token() {
            Some(t) => start_with_token(t)?,
            None => {
                return Err(
                    "could not find your ordinary user session to start the desktop in; use Task Manager > Run new task > explorer.exe"
                        .into(),
                )
            }
        }
    } else {
        // Already the ordinary user: a plain start is exactly right.
        let exe = format!("{}\\explorer.exe", windows_dir());
        use std::os::windows::process::CommandExt;
        std::process::Command::new(&exe)
            .current_dir(windows_dir())
            .creation_flags(0x0800_0000)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .map_err(|e| format!("could not start explorer.exe: {e}"))?;
        "as the user".into()
    };
    let t = Instant::now();
    while t.elapsed() < Duration::from_secs(25) {
        if running() {
            return Ok(how);
        }
        std::thread::sleep(Duration::from_millis(150));
    }
    Err("explorer.exe started but the taskbar did not appear".into())
}

/// Switches the Windows desktop off. Keeps a copy of the user's token first, so the same desktop
/// can be started again exactly as it was.
pub fn stop() -> Result<(), String> {
    let tray = tray_window();
    if tray.is_null() {
        return Err("the Windows desktop is not running".into());
    }
    let mut pid = 0u32;
    unsafe { GetWindowThreadProcessId(tray, &mut pid) };
    if pid == 0 {
        return Err("could not tell which program is the Windows desktop".into());
    }
    if USER_TOKEN.load(Ordering::SeqCst) == 0 {
        // Exactly the token the shell runs with now, so it comes back exactly as it was.
        if let Some(t) = token_of(pid, false) {
            USER_TOKEN.store(t as isize, Ordering::SeqCst);
        }
    }
    // The shell must be startable again, or it must stay running.
    if ffi::is_admin() && user_token().is_none() {
        return Err("CatalyX could not get your ordinary user token, so it would not be able to bring the desktop back safely".into());
    }
    unsafe { PostMessageW(tray, WM_EXIT_EXPLORER, 0, 0) };
    if !wait_process_gone(pid, Duration::from_secs(8)) {
        unsafe {
            let h = OpenProcess(PROCESS_TERMINATE | SYNCHRONIZE, 0, pid);
            if !h.is_null() {
                TerminateProcess(h, 1);
                ffi::WaitForSingleObject(h, 4000);
                CloseHandle(h);
            }
        }
    }
    // Wait until the taskbar is really gone (and stays gone: Windows may try to restart it).
    let t = Instant::now();
    while running() && t.elapsed() < Duration::from_secs(6) {
        std::thread::sleep(Duration::from_millis(100));
    }
    if running() {
        return Err("the Windows desktop would not close".into());
    }
    Ok(())
}

// ------------------------------------------------------------------ the way back

/// Starts listening for Ctrl+Alt+End. Fails (and then the desktop must not be switched off) when
/// another program already owns the shortcut.
pub fn hotkey_start() -> Result<(), String> {
    if HOTKEY_THREAD.load(Ordering::SeqCst) != 0 {
        return Ok(());
    }
    EXIT_REQUESTED.store(false, Ordering::SeqCst);
    let (tx, rx) = std::sync::mpsc::channel::<Result<u32, String>>();
    std::thread::spawn(move || unsafe {
        let mut m: MSG = std::mem::zeroed();
        PeekMessageW(&mut m, std::ptr::null_mut(), 0, 0, 0); // creates this thread's message queue
                                                             // MOD_ALT | MOD_CONTROL | MOD_NOREPEAT, VK_END
        if RegisterHotKey(std::ptr::null_mut(), 0x7B01, 0x1 | 0x2 | 0x4000, 0x23) == 0 {
            let _ = tx.send(Err(format!("{EXIT_KEYS} is already used by another program")));
            return;
        }
        let _ = tx.send(Ok(GetCurrentThreadId()));
        loop {
            let r = GetMessageW(&mut m, std::ptr::null_mut(), 0, 0);
            if r <= 0 || m.message == WM_QUIT {
                break;
            }
            if m.message == WM_HOTKEY {
                EXIT_REQUESTED.store(true, Ordering::SeqCst);
            }
        }
        UnregisterHotKey(std::ptr::null_mut(), 0x7B01);
    });
    match rx.recv_timeout(Duration::from_secs(3)) {
        Ok(Ok(tid)) => {
            HOTKEY_THREAD.store(tid, Ordering::SeqCst);
            Ok(())
        }
        Ok(Err(e)) => Err(e),
        Err(_) => Err("the shortcut could not be registered".into()),
    }
}

pub fn hotkey_stop() {
    let tid = HOTKEY_THREAD.swap(0, Ordering::SeqCst);
    if tid != 0 {
        unsafe { PostThreadMessageW(tid, WM_QUIT, 0, 0) };
    }
}

/// True once, after the shortcut was pressed.
pub fn take_exit_request() -> bool {
    EXIT_REQUESTED.swap(false, Ordering::SeqCst)
}

/// Starts a tiny copy of this program that waits for us to end. If we ever end while the desktop is
/// still switched off (crash, killed, power button on a laptop lid...), it brings the desktop back.
pub fn spawn_guard() {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else { return };
    // DETACHED_PROCESS | CREATE_NEW_PROCESS_GROUP | CREATE_NO_WINDOW
    let _ = std::process::Command::new(exe)
        .arg("shell-guard")
        .arg(std::process::id().to_string())
        .creation_flags(0x0000_0008 | 0x0000_0200 | 0x0800_0000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn();
}

/// True while process `pid` is alive.
pub fn alive(pid: u32) -> bool {
    !wait_process_gone(pid, Duration::from_millis(0))
}
