//! Live view of what is running (CPU, memory, graphics card use), and the shared sample readers
//! draw from. Actions on one process live in `act`; the sampler never changes anything.

use crate::ffi::*;

mod act;
pub(crate) use act::*;
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
}

/// Who started whom, built once for a pass over the whole list. The walk below needs this table;
/// building it inside the walk costs one scan per question, and a scoring pass asks it about every
/// process on the machine.
pub type Parents = HashMap<u32, u32>;

pub fn parents(procs: &[ProcInfo]) -> Parents {
    procs.iter().map(|p| (p.pid, p.parent)).collect()
}

/// True when `pid` is `ancestor` or one of its children, grandchildren, ... At most 32 levels:
/// a longer parent chain is a cycle, and the walk stops instead of looping.
pub fn descends_from(parents: &Parents, pid: u32, ancestor: u32) -> bool {
    let mut cur = pid;
    for _ in 0..32 {
        if cur == ancestor {
            return true;
        }
        match parents.get(&cur) {
            Some(&p) if p != 0 && p != cur => cur = p,
            _ => return false,
        }
    }
    false
}

/// The latest monitor sample.
///
/// The cell holds an `Arc`, so a reader takes one Arc clone under a short lock instead of copying
/// the whole process list, and the sampler never waits for a reader to finish copying.
pub type SharedSnapshot = Arc<Mutex<Arc<Snapshot>>>;

/// The latest sample. One lock, one Arc clone, whatever the machine is running.
///
/// A sampler that panicked while holding the lock leaves the last sample readable instead of
/// blinding every reader with an empty machine (the status cell makes the same choice, and for the
/// same reason: a poisoned lock must not hide what the program is doing).
pub fn latest(shared: &SharedSnapshot) -> Arc<Snapshot> {
    match shared.lock() {
        Ok(s) => Arc::clone(&s),
        Err(poisoned) => Arc::clone(&poisoned.into_inner()),
    }
}

/// Replaces the sample. The monitor is the only writer in the program; the tests use it too.
pub fn publish(shared: &SharedSnapshot, snap: Snapshot) {
    match shared.lock() {
        Ok(mut s) => *s = Arc::new(snap),
        Err(poisoned) => *poisoned.into_inner() = Arc::new(snap),
    }
}

static SAMPLE_MS: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1500);
/// Whether the sampler scans every process. The window turns this off on screens that only show
/// the machine totals; the command line and the engine leave it on.
static DETAIL: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);

pub fn set_detail(on: bool) {
    DETAIL.store(on, std::sync::atomic::Ordering::Relaxed);
}

/// Called by the sampler after each new snapshot, so a listener can react to fresh figures
/// instead of polling for them.
static ON_SAMPLE: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = std::sync::OnceLock::new();

/// Sets the hook the sampler calls after each new snapshot. Only the first call counts: there is
/// one listener per process (the core).
pub fn set_on_sample(f: impl Fn() + Send + Sync + 'static) {
    let _ = ON_SAMPLE.set(Box::new(f));
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
    let shared: SharedSnapshot = Arc::new(Mutex::new(Arc::default()));
    let out = shared.clone();
    std::thread::spawn(move || {
        crate::trace::mark("TEMP monitor thread up");
        let mut sys = System::new();
        let mut gpu = GpuCounters::new();
        crate::trace::mark("TEMP gpu counters");
        let mut freq = clock_probe();
        crate::trace::mark("TEMP clock probe");
        let base_mhz = cpu_base_mhz();
        crate::trace::mark("TEMP base mhz");
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
            publish(&out, snap);
            // Outside the snapshot lock: the hook may read the snapshot itself.
            if let Some(f) = ON_SAMPLE.get() {
                f();
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

#[cfg(test)]
mod tests {
    use super::*;

    fn cell(serial: u64) -> SharedSnapshot {
        Arc::new(Mutex::new(Arc::new(Snapshot { serial, ..Default::default() })))
    }

    fn chain(links: &[(u32, u32)]) -> Snapshot {
        Snapshot {
            procs: links.iter().map(|&(pid, parent)| ProcInfo { pid, parent, ..Default::default() }).collect(),
            serial: 1,
            detailed: true,
            ..Default::default()
        }
    }

    #[test]
    fn a_sample_is_handed_out_not_copied() {
        let shared = cell(7);
        let first = latest(&shared);
        let second = latest(&shared);
        assert_eq!(first.serial, 7);
        // The same allocation, not two copies of the process list.
        assert!(Arc::ptr_eq(&first, &second));
        assert_eq!(Arc::strong_count(&first), 3, "the cell plus the two readers");
    }

    #[test]
    fn a_reader_keeps_the_sample_it_took_when_the_next_one_arrives() {
        let shared = cell(7);
        let taken = latest(&shared);
        publish(&shared, Snapshot { serial: 8, ..Default::default() });
        assert_eq!(taken.serial, 7, "a sample a reader holds can never change under it");
        assert_eq!(latest(&shared).serial, 8);
    }

    #[test]
    fn an_empty_cell_reads_as_an_empty_sample() {
        let snap = latest(&cell(0));
        assert!(snap.procs.is_empty(), "the command line reads this before the first sample");
    }

    #[test]
    fn a_snapshot_finds_a_process_and_its_ancestors() {
        // 1 started 2, which started 3.
        let snap = chain(&[(1, 0), (2, 1), (3, 2)]);
        assert_eq!(snap.find(2).map(|p| p.pid), Some(2));
        assert!(snap.find(9).is_none());
        // One table, many questions: this is what a scoring pass over every process uses.
        let table = parents(&snap.procs);
        assert!(descends_from(&table, 3, 1), "a grandchild descends from its grandparent");
        assert!(descends_from(&table, 3, 3), "a process descends from itself");
        assert!(!descends_from(&table, 1, 3), "a parent does not descend from its child");
        assert!(!descends_from(&table, 9, 1), "a process that is gone descends from nothing");
        // A stale pid table can describe a cycle: the walk must end, not spin.
        let odd = chain(&[(1, 2), (2, 1)]);
        assert!(!descends_from(&parents(&odd.procs), 1, 9), "a cycle cannot reach a process outside it");
    }

    #[test]
    fn a_poisoned_cell_still_hands_out_the_last_sample() {
        let shared = cell(3);
        let other = shared.clone();
        let _ = std::thread::spawn(move || {
            let _guard = other.lock();
            panic!("poison the sampler's lock");
        })
        .join();
        assert_eq!(latest(&shared).serial, 3, "a panicked sampler must not blind the engine");
        publish(&shared, Snapshot { serial: 4, ..Default::default() });
        assert_eq!(latest(&shared).serial, 4, "and the next sample still goes in");
    }

    #[test]
    fn free_memory_never_goes_below_zero() {
        // Windows can report more used than total for a moment while it swaps.
        let snap = Snapshot { mem_used: 900, mem_total: 500, ..Default::default() };
        assert_eq!(snap.mem_free(), 0);
    }
}
