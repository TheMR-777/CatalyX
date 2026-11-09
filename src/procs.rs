//! Live view of what is running (CPU, memory, graphics card use) and the actions on one app.

use crate::ffi::*;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use sysinfo::System;

#[derive(Clone, Debug, Default)]
pub struct ProcInfo {
    pub pid: u32,
    pub parent: u32,
    pub name: String,
    pub name_lower: String,
    pub exe: String,
    pub cpu: f32,
    pub mem: u64,
    pub gpu: f32,
    pub start: u64,
}

#[derive(Clone, Debug, Default)]
pub struct Snapshot {
    pub procs: Vec<ProcInfo>,
    pub cpu_total: f32,
    pub mem_used: u64,
    pub mem_total: u64,
    pub gpu_total: f32,
    pub cpu_name: String,
    pub cores: usize,
    pub serial: u64,
    /// Clock speed the processor runs at right now, and its rated base speed (MHz; 0 = unknown).
    pub cpu_mhz: u32,
    pub cpu_base_mhz: u32,
    /// True when procs was freshly scanned for this sample. In light mode (nothing on screen
    /// needs per-process figures) the list is carried over from the last scan and may be old:
    /// nothing may act on a process id from a snapshot that is not detailed.
    pub detailed: bool,
}

impl Snapshot {
    pub fn find(&self, pid: u32) -> Option<&ProcInfo> {
        self.procs.iter().find(|p| p.pid == pid)
    }
    pub fn mem_free(&self) -> u64 {
        self.mem_total.saturating_sub(self.mem_used)
    }
    /// True when `pid` is `ancestor` or one of its children, grandchildren, ...
    pub fn descends_from(&self, pid: u32, ancestor: u32) -> bool {
        let map: HashMap<u32, u32> = self.procs.iter().map(|p| (p.pid, p.parent)).collect();
        let mut cur = pid;
        for _ in 0..32 {
            if cur == ancestor {
                return true;
            }
            match map.get(&cur) {
                Some(&p) if p != 0 && p != cur => cur = p,
                _ => return false,
            }
        }
        false
    }
}

pub type SharedSnapshot = Arc<Mutex<Snapshot>>;

static SAMPLE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1500);
/// Whether the sampler scans every process. The window turns this off on screens that only show
/// the machine totals; the command line and the engine leave it on.
static DETAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_detail(on: bool) {
    DETAIL.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// How often the background sampler refreshes (milliseconds). The window slows it when idle.
pub fn set_sample_interval(ms: u64) {
    SAMPLE_MS.store(ms, std::sync::atomic::Ordering::Relaxed);
}

/// Per-process graphics engine use from the Windows "GPU Engine" performance counters.
struct GpuCounters {
    query: isize,
    counter: isize,
    ok: bool,
}

impl GpuCounters {
    fn new() -> Self {
        let mut query = 0isize;
        let mut counter = 0isize;
        let mut ok = false;
        unsafe {
            if PdhOpenQueryW(std::ptr::null(), 0, &mut query) == 0 {
                let path = wide("\\GPU Engine(*)\\Utilization Percentage");
                if PdhAddEnglishCounterW(query, path.as_ptr(), 0, &mut counter) == 0 {
                    PdhCollectQueryData(query);
                    ok = true;
                }
            }
        }
        GpuCounters { query, counter, ok }
    }

    /// Returns (per pid use %, whole card use %).
    fn sample(&mut self) -> (HashMap<u32, f32>, f32) {
        let mut per_pid: HashMap<u32, HashMap<String, f32>> = HashMap::new();
        let mut per_engine: HashMap<String, f32> = HashMap::new();
        if !self.ok {
            return (HashMap::new(), 0.0);
        }
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return (HashMap::new(), 0.0);
            }
            let mut size = 0u32;
            let mut count = 0u32;
            let r = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, std::ptr::null_mut());
            if r != PDH_MORE_DATA || size == 0 {
                return (HashMap::new(), 0.0);
            }
            // The buffer holds the items followed by their names, so it is sized in bytes.
            let mut raw: Vec<u64> = vec![0u64; (size as usize) / 8 + 64];
            size = (raw.len() * 8) as u32;
            let items = raw.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            let r = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, items);
            if r != 0 {
                return (HashMap::new(), 0.0);
            }
            let buf = std::slice::from_raw_parts(items, count as usize);
            for item in buf {
                let name = from_wide(item.szName);
                let v = item.FmtValue.doubleValue as f32;
                if !(0.0..=1000.0).contains(&v) {
                    continue;
                }
                // pid_1234_luid_0x..._phys_0_eng_3_engtype_3D
                let pid = name.strip_prefix("pid_").and_then(|r| r.split('_').next()).and_then(|s| s.parse::<u32>().ok());
                let eng = name.rsplit("engtype_").next().unwrap_or("").to_string();
                if let Some(pid) = pid {
                    *per_pid.entry(pid).or_default().entry(eng.clone()).or_default() += v;
                }
                *per_engine.entry(eng).or_default() += v;
            }
        }
        let pids = per_pid.into_iter().map(|(pid, engs)| (pid, engs.values().cloned().fold(0.0f32, f32::max).min(100.0))).collect();
        let total = per_engine.values().cloned().fold(0.0f32, f32::max).min(100.0);
        (pids, total)
    }
}

/// One Windows performance counter that has a single value (the processor's current clock).
struct PdhOne {
    query: isize,
    counter: isize,
    ok: bool,
}

impl PdhOne {
    fn new(path: &str) -> Self {
        let (mut query, mut counter, mut ok) = (0isize, 0isize, false);
        unsafe {
            if PdhOpenQueryW(std::ptr::null(), 0, &mut query) == 0 {
                let p = wide(path);
                if PdhAddEnglishCounterW(query, p.as_ptr(), 0, &mut counter) == 0 {
                    PdhCollectQueryData(query);
                    ok = true;
                }
            }
        }
        PdhOne { query, counter, ok }
    }

    fn sample(&mut self) -> Option<f64> {
        if !self.ok {
            return None;
        }
        unsafe {
            if PdhCollectQueryData(self.query) != 0 {
                return None;
            }
            let mut size = 0u32;
            let mut count = 0u32;
            let r = PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, std::ptr::null_mut());
            if r != PDH_MORE_DATA || size == 0 {
                return None;
            }
            let mut raw: Vec<u64> = vec![0u64; (size as usize) / 8 + 64];
            size = (raw.len() * 8) as u32;
            let items = raw.as_mut_ptr() as *mut PDH_FMT_COUNTERVALUE_ITEM_W;
            if PdhGetFormattedCounterArrayW(self.counter, PDH_FMT_DOUBLE, &mut size, &mut count, items) != 0 || count == 0 {
                return None;
            }
            let v = (*items).FmtValue.doubleValue;
            (v.is_finite() && v > 0.0).then_some(v)
        }
    }
}

/// Rated (base) clock of the first processor core, from the registry, in MHz.
pub fn cpu_base_mhz() -> u32 {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("HARDWARE\\DESCRIPTION\\System\\CentralProcessor\\0")
        .and_then(|k| k.get_value::<u32, _>("~MHz"))
        .unwrap_or(0)
}

/// Starts the background sampler. It refreshes the shared snapshot about every 1.5 seconds.
pub fn start_monitor() -> SharedSnapshot {
    let shared: SharedSnapshot = Arc::new(Mutex::new(Snapshot::default()));
    let out = shared.clone();
    std::thread::spawn(move || {
        let mut sys = System::new();
        let mut gpu = GpuCounters::new();
        let mut freq = clock_probe();
        let base_mhz = cpu_base_mhz();
        let mut identity: HashMap<u32, (String, u64)> = HashMap::new();
        let mut unreadable: std::collections::HashSet<u32> = std::collections::HashSet::new();
        sys.refresh_cpu();
        sys.refresh_processes();
        let mut serial = 0u64;
        let mut last_procs: Vec<ProcInfo> = Vec::new();
        let mut last_gpu_total = 0.0f32;
        loop {
            // Nobody holds the snapshot any more (a finished CLI session or a closed window):
            // stop sampling instead of running for the life of the process.
            if Arc::strong_count(&out) == 1 {
                return;
            }
            sys.refresh_cpu_usage();
            sys.refresh_memory();
            // Light mode (nothing on screen needs per-process figures): only the machine totals
            // are refreshed - no process scan and no per-process graphics counters.
            let detailed = DETAIL.load(std::sync::atomic::Ordering::Relaxed);
            if detailed {
                sys.refresh_processes();
            }
            let (gpu_pid, gpu_total) = if detailed {
                gpu.sample()
            } else {
                (HashMap::new(), crate::gpu::live().and_then(|l| l.util_pct).map(|u| u as f32).unwrap_or(last_gpu_total))
            };
            last_gpu_total = gpu_total;
            let cores = sys.cpus().len().max(1);
            let cpu_name = sys.cpus().first().map(|c| c.brand().trim().to_string()).unwrap_or_default();
            let mhz = freq.sample_mhz().unwrap_or(0);
            let mut procs: Vec<ProcInfo> = if !detailed {
                last_procs.clone()
            } else {
                sys.processes()
                    .iter()
                    .map(|(pid, p)| {
                        let pid = pid.as_u32();
                        let name = p.name().to_string();
                        ProcInfo {
                            pid,
                            parent: p.parent().map(|x| x.as_u32()).unwrap_or(0),
                            name_lower: name.to_lowercase(),
                            name,
                            exe: p.exe().map(|e| e.to_string_lossy().to_string()).unwrap_or_default(),
                            cpu: p.cpu_usage() / cores as f32,
                            mem: p.memory(),
                            gpu: gpu_pid.get(&pid).cloned().unwrap_or(0.0),
                            start: p.start_time(),
                        }
                    })
                    .collect()
            };
            // Programs running as administrator hide their path and start time from the normal
            // list; read them through the limited-information right instead.
            let alive: std::collections::HashSet<u32> = procs.iter().map(|p| p.pid).collect();
            identity.retain(|pid, _| alive.contains(pid));
            unreadable.retain(|pid| alive.contains(pid));
            for p in procs.iter_mut().filter(|p| detailed && p.pid > 4 && (p.exe.is_empty() || p.start == 0)) {
                if unreadable.contains(&p.pid) {
                    continue;
                }
                if let std::collections::hash_map::Entry::Vacant(e) = identity.entry(p.pid) {
                    match process_identity(p.pid) {
                        Some(id) if !id.0.is_empty() || id.1 != 0 => {
                            e.insert(id);
                        }
                        _ => {
                            unreadable.insert(p.pid);
                            continue;
                        }
                    }
                }
                if let Some((exe, start)) = identity.get(&p.pid) {
                    if p.exe.is_empty() {
                        p.exe = exe.clone();
                    }
                    if p.start == 0 {
                        p.start = *start;
                    }
                }
            }
            serial += 1;
            if detailed {
                last_procs = procs.clone();
            }
            let snap = Snapshot {
                procs,
                cpu_total: sys.global_cpu_info().cpu_usage(),
                mem_used: sys.used_memory(),
                mem_total: sys.total_memory(),
                gpu_total,
                cpu_name,
                cores,
                serial,
                cpu_mhz: mhz,
                cpu_base_mhz: base_mhz,
                detailed,
            };
            if let Ok(mut s) = out.lock() {
                *s = snap;
            }
            // Sleep in short slices, so turning detail on (opening the Apps page, starting a
            // boost) is answered at once instead of after a long idle interval.
            let wait = Duration::from_millis(SAMPLE_MS.load(std::sync::atomic::Ordering::Relaxed));
            let t = std::time::Instant::now();
            while t.elapsed() < wait && DETAIL.load(std::sync::atomic::Ordering::Relaxed) == detailed && Arc::strong_count(&out) > 1 {
                std::thread::sleep(Duration::from_millis(100));
            }
        }
    });
    shared
}

pub fn now_secs() -> u64 {
    std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

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

pub fn fmt_mem(b: u64) -> String {
    let mb = b as f64 / (1024.0 * 1024.0);
    if mb >= 1024.0 {
        format!("{:.1} GB", mb / 1024.0)
    } else {
        format!("{:.0} MB", mb)
    }
}

/// The processor's real clock right now, for the status line, the doctor and the Tuning screen.
/// Windows' "Processor Frequency" counter only reports the nominal (base) speed; Task Manager's
/// speed is that number times "% Processor Performance", which goes above 100 when turbo is
/// working. Measured here: 3401 MHz nominal x 108.8 % = about 3.70 GHz under load.
pub struct ClockProbe {
    nominal: PdhOne,
    perf: PdhOne,
}

pub fn clock_probe() -> ClockProbe {
    ClockProbe {
        nominal: PdhOne::new("\\Processor Information(_Total)\\Processor Frequency"),
        perf: PdhOne::new("\\Processor Information(_Total)\\% Processor Performance"),
    }
}

impl ClockProbe {
    pub fn sample_mhz(&mut self) -> Option<u32> {
        let nominal = self.nominal.sample()?;
        let perf = self.perf.sample().filter(|p| *p > 5.0 && *p < 400.0).unwrap_or(100.0);
        Some((nominal * perf / 100.0).round() as u32)
    }
}
