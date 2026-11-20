//! Thin hand-written bindings to the Windows functions CatalyX needs.
//! Everything here is plain Win32; nothing is hooked or injected.
#![allow(non_snake_case, clippy::upper_case_acronyms, dead_code)]

use std::ffi::c_void;

pub type HANDLE = *mut c_void;
pub type BOOL = i32;

#[link(name = "user32")]
extern "system" {
    fn GetActiveWindow() -> HANDLE;
    fn IsIconic(hwnd: HANDLE) -> BOOL;
    fn FindWindowW(class: *const u16, title: *const u16) -> HANDLE;
    fn GetWindowThreadProcessId(hwnd: HANDLE, pid: *mut u32) -> u32;
}

/// This program's own main window (found by its title among this process's windows).
pub fn our_window() -> HANDLE {
    unsafe {
        let t = wide("CatalyX");
        let h = FindWindowW(std::ptr::null(), t.as_ptr());
        if h.is_null() {
            return h;
        }
        let mut pid = 0u32;
        GetWindowThreadProcessId(h, &mut pid);
        if pid == GetCurrentProcessId() {
            h
        } else {
            std::ptr::null_mut()
        }
    }
}

/// Windows' own light/dark choice for apps (true = dark).
pub fn system_prefers_dark() -> bool {
    use winreg::enums::HKEY_CURRENT_USER;
    winreg::RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Software\\Microsoft\\Windows\\CurrentVersion\\Themes\\Personalize")
        .and_then(|k| k.get_value::<u32, _>("AppsUseLightTheme"))
        .map(|v| v == 0)
        .unwrap_or(true)
}

#[repr(C)]
struct OPENFILENAMEW {
    size: u32,
    owner: HANDLE,
    instance: HANDLE,
    filter: *const u16,
    custom_filter: *mut u16,
    max_custom: u32,
    filter_index: u32,
    file: *mut u16,
    max_file: u32,
    file_title: *mut u16,
    max_file_title: u32,
    initial_dir: *const u16,
    title: *const u16,
    flags: u32,
    file_offset: u16,
    file_ext: u16,
    def_ext: *const u16,
    cust_data: isize,
    hook: *const c_void,
    template: *const u16,
    reserved: *mut c_void,
    reserved2: u32,
    flags_ex: u32,
}

#[link(name = "comdlg32")]
extern "system" {
    fn GetOpenFileNameW(ofn: *mut OPENFILENAMEW) -> BOOL;
    fn GetSaveFileNameW(ofn: *mut OPENFILENAMEW) -> BOOL;
}

/// Windows' own Open / Save dialog. `filter` is Windows' double-NUL format
/// ("Name\0*.ext;*.ext\0...\0\0"). Shortcuts are returned as the shortcut itself, not their
/// target, so a game's own "Run as administrator" setting is kept. Blocks until the dialog closes:
/// call it from a worker thread.
pub fn pick_file(title: &str, filter: &str, default_ext: &str, save: bool) -> Option<String> {
    let filter: Vec<u16> = filter.encode_utf16().collect();
    let title = wide(title);
    let ext = wide(default_ext);
    let mut file = vec![0u16; 2048];
    let mut ofn: OPENFILENAMEW = unsafe { std::mem::zeroed() };
    ofn.size = std::mem::size_of::<OPENFILENAMEW>() as u32;
    ofn.owner = our_window();
    ofn.filter = filter.as_ptr();
    ofn.filter_index = 1;
    ofn.file = file.as_mut_ptr();
    ofn.max_file = file.len() as u32;
    ofn.title = title.as_ptr();
    if !default_ext.is_empty() {
        ofn.def_ext = ext.as_ptr();
    }
    // EXPLORER | NOCHANGEDIR | NODEREFERENCELINKS | DONTADDTORECENT, plus must-exist / overwrite prompt
    ofn.flags = 0x80000 | 0x8 | 0x100000 | 0x0200_0000 | if save { 0x2 } else { 0x1000 | 0x800 };
    let ok = unsafe {
        if save {
            GetSaveFileNameW(&mut ofn)
        } else {
            GetOpenFileNameW(&mut ofn)
        }
    };
    if ok == 0 {
        return None;
    }
    let len = file.iter().position(|c| *c == 0).unwrap_or(file.len());
    Some(String::from_utf16_lossy(&file[..len]))
}

/// True while the window is minimised: nothing needs drawing then.
pub fn is_minimized() -> bool {
    unsafe {
        let h = GetActiveWindow();
        !h.is_null() && IsIconic(h) != 0
    }
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmSetWindowAttribute(hwnd: HANDLE, attribute: u32, value: *const c_void, size: u32) -> i32;
}

/// Use the native Windows caption and window controls, but match their colors to our dark UI.
/// Older Windows versions may not support caption colors; immersive dark mode still works there.
pub fn titlebar(dark: bool, caption: u32, text: u32) -> bool {
    let hwnd = our_window();
    if hwnd.is_null() {
        return false;
    }
    let dark: i32 = dark as i32;
    // Windows 10 20H1+; attribute 19 covers earlier builds that supported dark captions.
    unsafe {
        let value = &dark as *const _ as *const c_void;
        if DwmSetWindowAttribute(hwnd, 20, value, 4) != 0 {
            DwmSetWindowAttribute(hwnd, 19, value, 4);
        }
        // DWM COLORREF is 0x00bbggrr. Unsupported attributes fail harmlessly.
        DwmSetWindowAttribute(hwnd, 35, &caption as *const _ as *const c_void, 4);
        DwmSetWindowAttribute(hwnd, 36, &text as *const _ as *const c_void, 4);
    }
    true
}

#[link(name = "kernel32")]
extern "system" {
    fn GetConsoleProcessList(list: *mut u32, count: u32) -> u32;
    fn FreeConsole() -> BOOL;
    fn SetConsoleCtrlHandler(handler: Option<unsafe extern "system" fn(u32) -> BOOL>, add: BOOL) -> BOOL;
}

/// When the window was opened by double-click, Windows made a console just for us: drop it.
/// When started from a terminal the console is shared and stays (so CLI output keeps working).
pub fn hide_own_console() {
    unsafe {
        let mut ids = [0u32; 4];
        if GetConsoleProcessList(ids.as_mut_ptr(), 4) == 1 {
            FreeConsole();
        }
    }
}

#[link(name = "kernel32")]
extern "system" {
    fn AttachConsole(process_id: u32) -> BOOL;
    fn SetStdHandle(which: u32, h: HANDLE) -> BOOL;
    fn CreateFileW(
        name: *const u16,
        access: u32,
        share: u32,
        security: *const c_void,
        disposition: u32,
        flags: u32,
        template: HANDLE,
    ) -> HANDLE;
}

/// The window front-end has no console of its own. When it is started from a terminal with a command
/// (`catalyx status`), join that terminal so the output appears there: attach to the parent's console and
/// point the standard handles at it (unless the caller already redirected them to a pipe or a file).
pub fn attach_parent_console() {
    const ATTACH_PARENT_PROCESS: u32 = u32::MAX;
    unsafe {
        if AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return; // no parent console (started by double-click, a task, a service): nothing to join
        }
        // STD_INPUT_HANDLE = -10, STD_OUTPUT_HANDLE = -11, STD_ERROR_HANDLE = -12
        for (which, device) in [(-10i32 as u32, "CONIN$"), (-11i32 as u32, "CONOUT$"), (-12i32 as u32, "CONOUT$")] {
            let h = GetStdHandle(which);
            if h.is_null() || h as isize == -1 {
                // GENERIC_READ | GENERIC_WRITE, FILE_SHARE_READ | FILE_SHARE_WRITE, OPEN_EXISTING
                let n = CreateFileW(wide(device).as_ptr(), 0xC000_0000, 3, std::ptr::null(), 3, 0, std::ptr::null_mut());
                if n as isize != -1 && !n.is_null() {
                    SetStdHandle(which, n);
                }
            }
        }
    }
}
#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(which: u32) -> HANDLE;
    fn SetHandleInformation(h: HANDLE, mask: u32, flags: u32) -> BOOL;
    fn CreateJobObjectW(attrs: *mut c_void, name: *const u16) -> HANDLE;
    fn SetInformationJobObject(job: HANDLE, class: i32, info: *const c_void, len: u32) -> BOOL;
    fn AssignProcessToJobObject(job: HANDLE, process: HANDLE) -> BOOL;
}

#[repr(C)]
#[derive(Default)]
struct JobExtendedLimits {
    // JOBOBJECT_BASIC_LIMIT_INFORMATION
    per_process_user_time: i64,
    per_job_user_time: i64,
    limit_flags: u32,
    min_ws: usize,
    max_ws: usize,
    active_process_limit: u32,
    affinity: usize,
    priority_class: u32,
    scheduling_class: u32,
    // IO_COUNTERS
    io: [u64; 6],
    process_memory_limit: usize,
    job_memory_limit: usize,
    peak_process_memory: usize,
    peak_job_memory: usize,
}

/// Only for `--screenshot` / `--self-test` runs: whoever started us is usually reading our
/// output and waits until every copy of that output pipe is closed. Helper programs we start
/// (powershell, powercfg, the test batch file) would otherwise inherit it and could hold the
/// checker up long after we exit. So: our output handles are not handed down, and every
/// helper is tied to us and closed by Windows the moment we exit.
pub fn isolate_check_run() {
    unsafe {
        // STD_INPUT, STD_OUTPUT, STD_ERROR; HANDLE_FLAG_INHERIT = 1
        for which in [-10i32 as u32, -11i32 as u32, -12i32 as u32] {
            let h = GetStdHandle(which);
            if !h.is_null() && h as isize != -1 {
                SetHandleInformation(h, 1, 0);
            }
        }
        let job = CreateJobObjectW(std::ptr::null_mut(), std::ptr::null());
        if job.is_null() {
            return;
        }
        // JOB_OBJECT_LIMIT_KILL_ON_JOB_CLOSE; JobObjectExtendedLimitInformation = 9
        let info = JobExtendedLimits { limit_flags: 0x2000, ..Default::default() };
        if SetInformationJobObject(job, 9, &info as *const _ as *const c_void, std::mem::size_of::<JobExtendedLimits>() as u32) != 0 {
            // The job handle is deliberately kept open for the life of the process.
            AssignProcessToJobObject(job, GetCurrentProcess());
        }
    }
}

pub static CTRL_C: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);

/// Signalled together with `CTRL_C`, so a sleeping loop wakes at once instead of polling the flag.
static CTRL_EVENT: std::sync::atomic::AtomicIsize = std::sync::atomic::AtomicIsize::new(0);

#[link(name = "kernel32")]
extern "system" {
    fn CreateEventW(attrs: *const c_void, manual_reset: BOOL, initial: BOOL, name: *const u16) -> HANDLE;
    fn SetEvent(h: HANDLE) -> BOOL;
}

unsafe extern "system" fn on_ctrl(_kind: u32) -> BOOL {
    CTRL_C.store(true, std::sync::atomic::Ordering::SeqCst);
    let e = CTRL_EVENT.load(std::sync::atomic::Ordering::SeqCst);
    if e != 0 {
        SetEvent(e as HANDLE);
    }
    1
}

/// Ctrl+C / closing the terminal sets `CTRL_C` instead of killing us, so a CLI boost can restore first.
pub fn catch_ctrl_c() {
    unsafe {
        let e = CreateEventW(std::ptr::null(), 1, 0, std::ptr::null());
        CTRL_EVENT.store(e as isize, std::sync::atomic::Ordering::SeqCst);
        SetConsoleCtrlHandler(Some(on_ctrl), 1);
    }
}

/// Sleeps up to `ms`, waking at once when Ctrl+C arrives. True when Ctrl+C was pressed. No polling:
/// the thread does not run at all while it waits.
pub fn wait_ctrl_c(ms: u32) -> bool {
    use std::sync::atomic::Ordering;
    if CTRL_C.load(Ordering::SeqCst) {
        return true;
    }
    let e = CTRL_EVENT.load(Ordering::SeqCst);
    if e == 0 {
        std::thread::sleep(std::time::Duration::from_millis(ms as u64));
    } else {
        unsafe { WaitForSingleObject(e as HANDLE, ms) };
    }
    CTRL_C.load(Ordering::SeqCst)
}

pub const PROCESS_TERMINATE: u32 = 0x0001;
pub const PROCESS_SET_INFORMATION: u32 = 0x0200;
pub const PROCESS_QUERY_INFORMATION: u32 = 0x0400;
pub const PROCESS_QUERY_LIMITED_INFORMATION: u32 = 0x1000;
pub const PROCESS_SUSPEND_RESUME: u32 = 0x0800;
pub const PROCESS_SET_QUOTA: u32 = 0x0100;

pub const IDLE_PRIORITY_CLASS: u32 = 0x40;
pub const BELOW_NORMAL_PRIORITY_CLASS: u32 = 0x4000;
pub const NORMAL_PRIORITY_CLASS: u32 = 0x20;
pub const ABOVE_NORMAL_PRIORITY_CLASS: u32 = 0x8000;
pub const HIGH_PRIORITY_CLASS: u32 = 0x80;
pub const REALTIME_PRIORITY_CLASS: u32 = 0x100;

#[link(name = "kernel32")]
extern "system" {
    pub fn OpenProcess(access: u32, inherit: BOOL, pid: u32) -> HANDLE;
    pub fn CloseHandle(h: HANDLE) -> BOOL;
    pub fn TerminateProcess(h: HANDLE, code: u32) -> BOOL;
    pub fn SetPriorityClass(h: HANDLE, class: u32) -> BOOL;
    pub fn GetPriorityClass(h: HANDLE) -> u32;
    pub fn K32EmptyWorkingSet(h: HANDLE) -> BOOL;
    pub fn SetProcessInformation(h: HANDLE, class: i32, info: *const c_void, size: u32) -> BOOL;
    pub fn GetProcessInformation(h: HANDLE, class: i32, info: *mut c_void, size: u32) -> BOOL;
    pub fn LoadLibraryW(name: *const u16) -> HANDLE;
    pub fn GetProcAddress(module: HANDLE, name: *const u8) -> *const c_void;
    pub fn GetModuleHandleW(name: *const u16) -> HANDLE;
    pub fn GetLastError() -> u32;
    pub fn GetCurrentProcessId() -> u32;
    pub fn GetCurrentProcess() -> HANDLE;
}

pub const TOKEN_QUERY: u32 = 0x0008;
pub const TOKEN_USER_CLASS: i32 = 1;

#[link(name = "advapi32")]
extern "system" {
    pub fn OpenProcessToken(process: HANDLE, access: u32, token: *mut HANDLE) -> BOOL;
    pub fn GetTokenInformation(token: HANDLE, class: i32, info: *mut c_void, len: u32, needed: *mut u32) -> BOOL;
    pub fn EqualSid(a: *const c_void, b: *const c_void) -> BOOL;
}

#[repr(C)]
pub struct SHELLEXECUTEINFOW {
    pub cbSize: u32,
    pub fMask: u32,
    pub hwnd: HANDLE,
    pub lpVerb: *const u16,
    pub lpFile: *const u16,
    pub lpParameters: *const u16,
    pub lpDirectory: *const u16,
    pub nShow: i32,
    pub hInstApp: HANDLE,
    pub lpIDList: *mut c_void,
    pub lpClass: *const u16,
    pub hkeyClass: HANDLE,
    pub dwHotKey: u32,
    pub hIconOrMonitor: HANDLE,
    pub hProcess: HANDLE,
}

#[link(name = "shell32")]
extern "system" {
    pub fn IsUserAnAdmin() -> BOOL;
    fn ShellExecuteExW(info: *mut SHELLEXECUTEINFOW) -> BOOL;
}

#[link(name = "ole32")]
extern "system" {
    fn CoInitializeEx(reserved: *mut c_void, coinit: u32) -> i32;
}

#[link(name = "kernel32")]
extern "system" {
    fn GetProcessId(h: HANDLE) -> u32;
}

/// Starts a program, batch file, shortcut or link exactly the way a double-click in Explorer
/// does: Windows applies the file's compatibility settings ("Run as administrator", old
/// Windows modes), shows the administrator prompt when one is needed, and the game gets its
/// own clean environment instead of inheriting CatalyX's console and handles.
/// Returns the started process id when Windows reports one.
///
/// When Windows refuses ("request is not supported", needs elevation, blocked file...) the start
/// is retried a different way before giving up: as administrator, with Windows' own error
/// dialog, directly, then through `cmd /c start`. A game that opens by double-click opens here.
pub fn shell_start(file: &str, params: &str, dir: &str, hidden: bool) -> Result<Option<u32>, String> {
    let first = match shell_try(file, params, dir, "open", hidden, true) {
        Ok(p) => return Ok(p),
        Err(e) => e,
    };
    if first == 1223 {
        return Err("the administrator prompt was cancelled".into());
    }
    let is_exe = file.to_lowercase().ends_with(".exe");
    // Needs elevation, or a shell quirk: ask for administrator rights explicitly.
    if matches!(first, 50 | 740 | 5) {
        match shell_try(file, params, dir, "runas", hidden, true) {
            Ok(p) => return Ok(p),
            Err(1223) => return Err("the administrator prompt was cancelled".into()),
            Err(_) => {}
        }
    }
    // Let Windows show its own question (for example "this file came from the internet").
    if let Ok(p) = shell_try(file, params, dir, "open", hidden, false) {
        return Ok(p);
    }
    use std::os::windows::process::CommandExt;
    if is_exe {
        let mut c = std::process::Command::new(file);
        if !params.trim().is_empty() {
            c.raw_arg(params);
        }
        if !dir.is_empty() {
            c.current_dir(dir);
        }
        c.stdin(std::process::Stdio::null()).stdout(std::process::Stdio::null()).stderr(std::process::Stdio::null());
        if let Ok(child) = c.spawn() {
            return Ok(Some(child.id()));
        }
    }
    let mut c = std::process::Command::new("cmd.exe");
    c.raw_arg(format!("/c start \"\" /D \"{}\" \"{}\" {}", if dir.is_empty() { "." } else { dir }, file, params))
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());
    if c.spawn().is_ok() {
        return Ok(None);
    }
    Err(format!("{} (code {first})", std::io::Error::from_raw_os_error(first as i32)))
}

fn shell_try(file: &str, params: &str, dir: &str, verb: &str, hidden: bool, no_ui: bool) -> Result<Option<u32>, u32> {
    unsafe {
        // Apartment-threaded COM, as the shell expects; harmless when already set up.
        CoInitializeEx(std::ptr::null_mut(), 0x2 | 0x4);
        let f = wide(file);
        let p = wide(params);
        let d = wide(dir);
        let verb = wide(verb);
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            // NOCLOSEPROCESS | NOASYNC | FLAG_NO_UI (errors come back to us instead of a dialog)
            fMask: 0x40 | 0x100 | if no_ui { 0x400 } else { 0 },
            hwnd: std::ptr::null_mut(),
            lpVerb: verb.as_ptr(),
            lpFile: f.as_ptr(),
            lpParameters: if params.trim().is_empty() { std::ptr::null() } else { p.as_ptr() },
            lpDirectory: if dir.is_empty() { std::ptr::null() } else { d.as_ptr() },
            nShow: if hidden { 7 } else { 1 }, // SW_SHOWMINNOACTIVE : SW_SHOWNORMAL
            hInstApp: std::ptr::null_mut(),
            lpIDList: std::ptr::null_mut(),
            lpClass: std::ptr::null(),
            hkeyClass: std::ptr::null_mut(),
            dwHotKey: 0,
            hIconOrMonitor: std::ptr::null_mut(),
            hProcess: std::ptr::null_mut(),
        };
        if ShellExecuteExW(&mut info) == 0 {
            return Err(GetLastError());
        }
        if info.hProcess.is_null() {
            return Ok(None);
        }
        let pid = GetProcessId(info.hProcess);
        CloseHandle(info.hProcess);
        Ok(if pid == 0 { None } else { Some(pid) })
    }
}

// ---- Services (advapi32) ----
pub const SC_MANAGER_CONNECT: u32 = 0x0001;
pub const SC_MANAGER_ENUMERATE_SERVICE: u32 = 0x0004;
pub const SERVICE_QUERY_STATUS: u32 = 0x0004;
pub const SERVICE_START: u32 = 0x0010;
pub const SERVICE_STOP: u32 = 0x0020;
pub const SERVICE_WIN32: u32 = 0x30;
pub const SERVICE_STATE_ALL: u32 = 3;
pub const SC_ENUM_PROCESS_INFO: i32 = 0;
pub const SERVICE_CONTROL_STOP: u32 = 1;
pub const SERVICE_RUNNING: u32 = 4;
pub const SERVICE_STOPPED: u32 = 1;
pub const ERROR_MORE_DATA: u32 = 234;

#[repr(C)]
#[derive(Default, Clone, Copy)]
pub struct SERVICE_STATUS {
    pub dwServiceType: u32,
    pub dwCurrentState: u32,
    pub dwControlsAccepted: u32,
    pub dwWin32ExitCode: u32,
    pub dwServiceSpecificExitCode: u32,
    pub dwCheckPoint: u32,
    pub dwWaitHint: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct SERVICE_STATUS_PROCESS {
    pub dwServiceType: u32,
    pub dwCurrentState: u32,
    pub dwControlsAccepted: u32,
    pub dwWin32ExitCode: u32,
    pub dwServiceSpecificExitCode: u32,
    pub dwCheckPoint: u32,
    pub dwWaitHint: u32,
    pub dwProcessId: u32,
    pub dwServiceFlags: u32,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ENUM_SERVICE_STATUS_PROCESSW {
    pub lpServiceName: *const u16,
    pub lpDisplayName: *const u16,
    pub ServiceStatusProcess: SERVICE_STATUS_PROCESS,
}

pub const SERVICE_ENUMERATE_DEPENDENTS: u32 = 0x0008;
pub const SERVICE_ACTIVE: u32 = 1;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct ENUM_SERVICE_STATUSW {
    pub lpServiceName: *const u16,
    pub lpDisplayName: *const u16,
    pub ServiceStatus: SERVICE_STATUS,
}

#[link(name = "advapi32")]
extern "system" {
    pub fn OpenSCManagerW(machine: *const u16, db: *const u16, access: u32) -> HANDLE;
    pub fn OpenServiceW(scm: HANDLE, name: *const u16, access: u32) -> HANDLE;
    pub fn CloseServiceHandle(h: HANDLE) -> BOOL;
    pub fn ControlService(h: HANDLE, control: u32, status: *mut SERVICE_STATUS) -> BOOL;
    pub fn StartServiceW(h: HANDLE, argc: u32, argv: *const *const u16) -> BOOL;
    pub fn QueryServiceStatus(h: HANDLE, status: *mut SERVICE_STATUS) -> BOOL;
    pub fn EnumDependentServicesW(
        h: HANDLE,
        state: u32,
        services: *mut u8,
        buf_size: u32,
        bytes_needed: *mut u32,
        returned: *mut u32,
    ) -> BOOL;
    pub fn EnumServicesStatusExW(
        scm: HANDLE,
        info_level: i32,
        service_type: u32,
        service_state: u32,
        services: *mut u8,
        buf_size: u32,
        bytes_needed: *mut u32,
        returned: *mut u32,
        resume: *mut u32,
        group: *const u16,
    ) -> BOOL;
}

// ---- Performance counters (pdh) ----
pub const PDH_FMT_DOUBLE: u32 = 0x0000_0200;
pub const PDH_MORE_DATA: u32 = 0x8000_07D2;

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PDH_FMT_COUNTERVALUE {
    pub CStatus: u32,
    pub _pad: u32,
    pub doubleValue: f64,
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct PDH_FMT_COUNTERVALUE_ITEM_W {
    pub szName: *const u16,
    pub FmtValue: PDH_FMT_COUNTERVALUE,
}

#[link(name = "pdh")]
extern "system" {
    pub fn PdhOpenQueryW(source: *const u16, user: usize, query: *mut isize) -> u32;
    pub fn PdhAddEnglishCounterW(query: isize, path: *const u16, user: usize, counter: *mut isize) -> u32;
    pub fn PdhCollectQueryData(query: isize) -> u32;
    pub fn PdhGetFormattedCounterArrayW(
        counter: isize,
        format: u32,
        buffer_size: *mut u32,
        item_count: *mut u32,
        items: *mut PDH_FMT_COUNTERVALUE_ITEM_W,
    ) -> u32;
    pub fn PdhCloseQuery(query: isize) -> u32;
}

pub fn wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// Reads a NUL-terminated UTF-16 string.
///
/// # Safety
/// `p` must be null or point to a valid NUL-terminated UTF-16 string.
pub unsafe fn from_wide(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0usize;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

#[repr(C)]
#[derive(Default, Clone, Copy)]
struct FILETIME {
    low: u32,
    high: u32,
}

#[link(name = "kernel32")]
extern "system" {
    fn QueryFullProcessImageNameW(h: HANDLE, flags: u32, name: *mut u16, size: *mut u32) -> BOOL;
    fn GetProcessTimes(h: HANDLE, create: *mut FILETIME, exit: *mut FILETIME, kernel: *mut FILETIME, user: *mut FILETIME) -> BOOL;
}

/// Program path and start time (Unix seconds) of a process through the "limited information"
/// right, which Windows grants even for programs running as administrator. The usual process
/// list cannot read these for elevated games, which is what hid them from the boost.
pub fn process_identity(pid: u32) -> Option<(String, u64)> {
    unsafe {
        let h = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, 0, pid);
        if h.is_null() {
            return None;
        }
        let mut buf = vec![0u16; 1024];
        let mut len = buf.len() as u32;
        let path = if QueryFullProcessImageNameW(h, 0, buf.as_mut_ptr(), &mut len) != 0 {
            String::from_utf16_lossy(&buf[..len as usize])
        } else {
            String::new()
        };
        let (mut c, mut e, mut k, mut u) = (FILETIME::default(), FILETIME::default(), FILETIME::default(), FILETIME::default());
        let start = if GetProcessTimes(h, &mut c, &mut e, &mut k, &mut u) != 0 {
            let t = ((c.high as u64) << 32) | c.low as u64; // 100 ns since 1601
            (t / 10_000_000).saturating_sub(11_644_473_600)
        } else {
            0
        };
        CloseHandle(h);
        Some((path, start))
    }
}

/// Looks up a function exported by an already loaded or loadable DLL.
pub fn proc_address(dll: &str, name: &str) -> Option<*const c_void> {
    unsafe {
        let w = wide(dll);
        let mut m = GetModuleHandleW(w.as_ptr());
        if m.is_null() {
            m = LoadLibraryW(w.as_ptr());
        }
        if m.is_null() {
            return None;
        }
        let mut n = name.as_bytes().to_vec();
        n.push(0);
        let p = GetProcAddress(m, n.as_ptr());
        if p.is_null() {
            None
        } else {
            Some(p)
        }
    }
}

pub fn is_admin() -> bool {
    unsafe { IsUserAnAdmin() != 0 }
}

/// Runs a helper program and returns its output, but never waits longer than `secs`: a helper
/// that hangs (a stuck driver query, a locked-up PowerShell) is killed instead of freezing the
/// caller forever. Hidden window, no input.
pub fn run_timeout(mut cmd: std::process::Command, secs: u64) -> Option<std::process::Output> {
    use std::io::Read;
    use std::os::windows::process::CommandExt;
    use std::process::Stdio;
    cmd.creation_flags(0x0800_0000).stdin(Stdio::null()).stdout(Stdio::piped()).stderr(Stdio::piped());
    let mut child = cmd.spawn().ok()?;
    let mut out = child.stdout.take()?;
    let mut err = child.stderr.take()?;
    let t_out = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = out.read_to_end(&mut v);
        v
    });
    let t_err = std::thread::spawn(move || {
        let mut v = Vec::new();
        let _ = err.read_to_end(&mut v);
        v
    });
    let start = std::time::Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(s)) => break s,
            Ok(None) => {
                if start.elapsed() > std::time::Duration::from_secs(secs) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return None;
                }
                std::thread::sleep(std::time::Duration::from_millis(25));
            }
            Err(_) => return None,
        }
    };
    Some(std::process::Output { status, stdout: t_out.join().unwrap_or_default(), stderr: t_err.join().unwrap_or_default() })
}

/// Starts this program again with administrator rights (one Windows prompt), passing the same
/// arguments. True when Windows accepted; the caller should then exit. False when the prompt
/// was cancelled or is impossible: carry on without.
pub fn relaunch_elevated() -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    let args: Vec<String> = std::env::args().skip(1).map(|a| if a.contains(' ') { format!("\"{a}\"") } else { a }).collect();
    let dir = exe.parent().map(|d| d.to_string_lossy().to_string()).unwrap_or_default();
    shell_try(&exe.to_string_lossy(), &args.join(" "), &dir, "runas", false, true).is_ok()
}

// ---- Privileges, cheap process list, memory lists ----

#[repr(C)]
#[derive(Clone, Copy)]
struct LUID {
    low: u32,
    high: i32,
}

#[repr(C)]
struct TOKEN_PRIVILEGES1 {
    count: u32,
    luid: LUID,
    attributes: u32,
}

#[link(name = "advapi32")]
extern "system" {
    fn LookupPrivilegeValueW(system: *const u16, name: *const u16, luid: *mut LUID) -> BOOL;
    fn AdjustTokenPrivileges(
        token: HANDLE,
        disable_all: BOOL,
        new: *const TOKEN_PRIVILEGES1,
        len: u32,
        prev: *mut c_void,
        ret_len: *mut u32,
    ) -> BOOL;
}

/// Switches on one privilege the administrator token already holds (it is present but off by
/// default). True when Windows reports the privilege is now enabled.
pub fn enable_privilege(name: &str) -> bool {
    unsafe {
        let mut token: HANDLE = std::ptr::null_mut();
        // TOKEN_ADJUST_PRIVILEGES | TOKEN_QUERY
        if OpenProcessToken(GetCurrentProcess(), 0x20 | 0x08, &mut token) == 0 {
            return false;
        }
        let n = wide(name);
        let mut luid = LUID { low: 0, high: 0 };
        let mut ok = false;
        if LookupPrivilegeValueW(std::ptr::null(), n.as_ptr(), &mut luid) != 0 {
            let tp = TOKEN_PRIVILEGES1 { count: 1, luid, attributes: 2 }; // SE_PRIVILEGE_ENABLED
            AdjustTokenPrivileges(
                token,
                0,
                &tp,
                std::mem::size_of::<TOKEN_PRIVILEGES1>() as u32,
                std::ptr::null_mut(),
                std::ptr::null_mut(),
            );
            ok = GetLastError() == 0; // ERROR_NOT_ALL_ASSIGNED (1300) means it was not held
        }
        CloseHandle(token);
        ok
    }
}

#[repr(C)]
struct PROCESSENTRY32W {
    size: u32,
    usage: u32,
    pid: u32,
    heap: usize,
    module: u32,
    threads: u32,
    parent: u32,
    pri_class: i32,
    flags: u32,
    exe: [u16; 260],
}

#[link(name = "kernel32")]
extern "system" {
    fn CreateToolhelp32Snapshot(flags: u32, pid: u32) -> HANDLE;
    fn Process32FirstW(snap: HANDLE, entry: *mut PROCESSENTRY32W) -> BOOL;
    fn Process32NextW(snap: HANDLE, entry: *mut PROCESSENTRY32W) -> BOOL;
}

/// Every running program as (pid, parent pid, lower-case file name), in about a millisecond and
/// with no allocation beyond the result. The background watcher uses this instead of the full
/// monitor, which also reads memory, CPU and graphics use.
pub fn quick_list() -> Vec<(u32, u32, String)> {
    let mut out = Vec::new();
    unsafe {
        let snap = CreateToolhelp32Snapshot(0x2, 0); // TH32CS_SNAPPROCESS
        if snap.is_null() || snap as isize == -1 {
            return out;
        }
        let mut e: PROCESSENTRY32W = std::mem::zeroed();
        e.size = std::mem::size_of::<PROCESSENTRY32W>() as u32;
        let mut more = Process32FirstW(snap, &mut e) != 0;
        while more {
            let len = e.exe.iter().position(|c| *c == 0).unwrap_or(260);
            out.push((e.pid, e.parent, String::from_utf16_lossy(&e.exe[..len]).to_lowercase()));
            more = Process32NextW(snap, &mut e) != 0;
        }
        CloseHandle(snap);
    }
    out
}

/// Windows' own memory-list commands (what RAMMap's "Empty" menu uses). Needs administrator and
/// SeProfileSingleProcessPrivilege. 3 = modified page list, 4 = standby list.
pub fn memory_list_command(cmd: i32) -> bool {
    type NtSet = unsafe extern "system" fn(i32, *mut c_void, u32) -> i32;
    if !enable_privilege("SeProfileSingleProcessPrivilege") {
        return false;
    }
    let Some(f) = proc_address("ntdll.dll", "NtSetSystemInformation") else { return false };
    let f: NtSet = unsafe { std::mem::transmute(f) };
    let mut c = cmd;
    // SystemMemoryListInformation = 80
    unsafe { f(80, &mut c as *mut i32 as *mut c_void, 4) >= 0 }
}

#[link(name = "kernel32")]
extern "system" {
    pub fn WaitForSingleObject(h: HANDLE, ms: u32) -> u32;
    fn K32GetProcessMemoryInfo(h: HANDLE, counters: *mut [u64; 9], size: u32) -> BOOL;
}

/// Runs this program again with administrator rights and the given arguments (one Windows
/// prompt), then waits for it to finish. True when it ran to completion.
pub fn run_self_elevated_and_wait(args: &str, wait_ms: u32) -> bool {
    let Ok(exe) = std::env::current_exe() else { return false };
    unsafe {
        CoInitializeEx(std::ptr::null_mut(), 0x2 | 0x4);
        let f = wide(&exe.to_string_lossy());
        let a = wide(args);
        let verb = wide("runas");
        let mut info = SHELLEXECUTEINFOW {
            cbSize: std::mem::size_of::<SHELLEXECUTEINFOW>() as u32,
            fMask: 0x40, // NOCLOSEPROCESS
            hwnd: std::ptr::null_mut(),
            lpVerb: verb.as_ptr(),
            lpFile: f.as_ptr(),
            lpParameters: a.as_ptr(),
            lpDirectory: std::ptr::null(),
            nShow: 0, // hidden
            hInstApp: std::ptr::null_mut(),
            lpIDList: std::ptr::null_mut(),
            lpClass: std::ptr::null(),
            hkeyClass: std::ptr::null_mut(),
            dwHotKey: 0,
            hIconOrMonitor: std::ptr::null_mut(),
            hProcess: std::ptr::null_mut(),
        };
        if ShellExecuteExW(&mut info) == 0 || info.hProcess.is_null() {
            return false;
        }
        let r = WaitForSingleObject(info.hProcess, wait_ms);
        CloseHandle(info.hProcess);
        r == 0
    }
}

/// This program's own working set (bytes in memory right now), for the About card.
pub fn own_working_set() -> u64 {
    unsafe {
        // PROCESS_MEMORY_COUNTERS: [cb:u32, PageFaultCount:u32], PeakWorkingSetSize, WorkingSetSize, ...
        let mut c = [0u64; 9];
        c[0] = std::mem::size_of_val(&c) as u64;
        if K32GetProcessMemoryInfo(GetCurrentProcess(), &mut c, std::mem::size_of_val(&c) as u32) != 0 {
            c[2]
        } else {
            0
        }
    }
}

/// Runs this program with the given arguments (hidden, no input) and waits. For jobs that must
/// run in the already-elevated copy, such as registering the one-click task.
pub fn run_self_as_current(args: &str) -> bool {
    use std::os::windows::process::CommandExt;
    let Ok(exe) = std::env::current_exe() else { return false };
    std::process::Command::new(exe)
        .raw_arg(args)
        .creation_flags(0x0800_0000)
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

#[repr(C)]
struct Margins {
    left: i32,
    right: i32,
    top: i32,
    bottom: i32,
}

#[link(name = "dwmapi")]
extern "system" {
    fn DwmExtendFrameIntoClientArea(hwnd: HANDLE, margins: *const Margins) -> i32;
}

/// Gives the borderless window what a normal one has from Windows: rounded corners (Windows 11)
/// and the soft drop shadow. Safe to call again; unsupported attributes fail harmlessly.
pub fn style_borderless_window() -> bool {
    let hwnd = our_window();
    if hwnd.is_null() {
        return false;
    }
    unsafe {
        // DWMWA_WINDOW_CORNER_PREFERENCE = 33, DWMWCP_ROUND = 2
        let round: i32 = 2;
        DwmSetWindowAttribute(hwnd, 33, &round as *const _ as *const c_void, 4);
        // A 1 px extended frame is what makes DWM draw the shadow for a window with no caption.
        let m = Margins { left: 0, right: 0, top: 1, bottom: 0 };
        DwmExtendFrameIntoClientArea(hwnd, &m);
    }
    true
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{Duration, Instant};

    /// The watcher sleeps in one long wait and must still wake the moment Ctrl+C arrives.
    /// (Everything about the flag lives in this one test: it is process-wide state.)
    #[test]
    fn ctrl_c_wakes_a_long_wait_at_once() {
        catch_ctrl_c();
        // Nothing pressed: the wait runs its full (short) time and reports no Ctrl+C.
        let t = Instant::now();
        assert!(!wait_ctrl_c(150));
        assert!(t.elapsed() >= Duration::from_millis(120));
        // Pressed 200 ms into a 10 s wait: it returns right then, not after 10 s.
        let pressed = std::thread::spawn(|| {
            std::thread::sleep(Duration::from_millis(200));
            unsafe { on_ctrl(0) };
        });
        let t = Instant::now();
        assert!(wait_ctrl_c(10_000));
        assert!(t.elapsed() < Duration::from_secs(2), "woke after {:?}", t.elapsed());
        pressed.join().unwrap();
        // Once pressed it stays pressed, and later waits return immediately.
        let t = Instant::now();
        assert!(wait_ctrl_c(10_000));
        assert!(t.elapsed() < Duration::from_millis(50));
        CTRL_C.store(false, std::sync::atomic::Ordering::SeqCst);
    }
}
