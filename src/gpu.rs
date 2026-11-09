//! Safe graphics card clock offsets for NVIDIA cards, through the driver's own interface (NvAPI
//! performance states "Pstates20"). Nothing is flashed or injected; the offsets are plain driver settings
//! that are put back afterwards.

// The NVML entry points are looked up by name at run time (so no NVIDIA library is needed to start), and each
// one is cast to its documented signature right where it is declared.
#![allow(clippy::missing_transmute_annotations)]

use crate::config::GpuLevel;
use crate::ffi::proc_address;
use std::ffi::c_void;

const ID_INITIALIZE: u32 = 0x0150_E828;
const ID_ENUM_PHYSICAL_GPUS: u32 = 0xE5AC_921F;
const ID_GET_FULL_NAME: u32 = 0xCEEE_8E9F;
const ID_GET_PSTATES20: u32 = 0x6FF8_1213;
const ID_SET_PSTATES20: u32 = 0x0F4D_AE6B;

const DOMAIN_GRAPHICS: u32 = 0;
const DOMAIN_MEMORY: u32 = 4;

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct Delta {
    value: i32,
    min: i32,
    max: i32,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct ClockEntry {
    domain_id: u32,
    type_id: u32,
    editable: u32,
    freq_delta_khz: Delta,
    data: [u32; 5],
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct VoltEntry {
    domain_id: u32,
    editable: u32,
    volt_uv: u32,
    delta: Delta,
}

#[repr(C)]
#[derive(Clone, Copy, Default)]
struct PState {
    pstate_id: u32,
    editable: u32,
    clocks: [ClockEntry; 8],
    volts: [VoltEntry; 4],
}

#[repr(C)]
#[derive(Clone, Copy)]
struct PStates20 {
    version: u32,
    editable: u32,
    num_pstates: u32,
    num_clocks: u32,
    num_base_voltages: u32,
    pstates: [PState; 16],
    // room for the larger (v2) layout
    extra: [u32; 32],
}

const V1_SIZE: u32 = 0x1C94;
const V2_SIZE: u32 = 0x1CF8;

type QueryFn = unsafe extern "C" fn(u32) -> *const c_void;
type InitFn = unsafe extern "C" fn() -> i32;
type EnumFn = unsafe extern "C" fn(*mut *mut c_void, *mut u32) -> i32;
type NameFn = unsafe extern "C" fn(*mut c_void, *mut u8) -> i32;
type PstatesFn = unsafe extern "C" fn(*mut c_void, *mut PStates20) -> i32;

#[derive(Clone, Debug, Default)]
pub struct ClockInfo {
    pub current_khz: i32,
    pub min_khz: i32,
    pub max_khz: i32,
}

#[derive(Clone, Debug, Default)]
pub struct GpuState {
    pub name: String,
    pub vendor: String,
    pub supported: bool,
    pub core: ClockInfo,
    pub mem: ClockInfo,
    pub note: String,
}

struct Api {
    gpu: *mut c_void,
    get: PstatesFn,
    set: PstatesFn,
    name: Option<NameFn>,
}

fn api() -> Result<Api, String> {
    let q = proc_address("nvapi64.dll", "nvapi_QueryInterface").ok_or("no NVIDIA driver interface (nvapi64.dll)")?;
    let q: QueryFn = unsafe { std::mem::transmute(q) };
    unsafe {
        let get = |id: u32| -> Option<*const c_void> {
            let p = q(id);
            if p.is_null() {
                None
            } else {
                Some(p)
            }
        };
        let init: InitFn = std::mem::transmute(get(ID_INITIALIZE).ok_or("NvAPI_Initialize missing")?);
        if init() != 0 {
            return Err("NVIDIA driver interface did not start".into());
        }
        let enumf: EnumFn = std::mem::transmute(get(ID_ENUM_PHYSICAL_GPUS).ok_or("GPU list missing")?);
        let mut handles = [std::ptr::null_mut::<c_void>(); 64];
        let mut count = 0u32;
        if enumf(handles.as_mut_ptr(), &mut count) != 0 || count == 0 {
            return Err("no NVIDIA graphics card found".into());
        }
        let getp: PstatesFn = std::mem::transmute(get(ID_GET_PSTATES20).ok_or("clock reading missing")?);
        let setp: PstatesFn = std::mem::transmute(get(ID_SET_PSTATES20).ok_or("clock setting missing")?);
        let name = get(ID_GET_FULL_NAME).map(|p| std::mem::transmute::<*const c_void, NameFn>(p));
        Ok(Api { gpu: handles[0], get: getp, set: setp, name })
    }
}

fn blank() -> PStates20 {
    // SAFETY: plain-old-data made of integers.
    unsafe { std::mem::zeroed() }
}

fn read_states(a: &Api) -> Result<PStates20, String> {
    for size in [V1_SIZE, V2_SIZE] {
        let mut s = blank();
        let ver: u32 = if size == V1_SIZE { 1 } else { 2 };
        s.version = size | (ver << 16);
        let r = unsafe { (a.get)(a.gpu, &mut s) };
        if r == 0 {
            return Ok(s);
        }
    }
    Err("the driver did not report clock offsets".into())
}

fn clock_of(s: &PStates20, domain: u32) -> Option<ClockInfo> {
    let n = (s.num_pstates as usize).min(16);
    let p = s.pstates[..n].iter().find(|p| p.pstate_id == 0)?;
    let c = p.clocks[..(s.num_clocks as usize).min(8)].iter().find(|c| c.domain_id == domain)?;
    // Read-only domains must not be offered as safe tuning targets, even if the driver
    // reports a non-zero range for them.
    if c.editable & 1 == 0 || c.freq_delta_khz.max <= 0 || c.freq_delta_khz.min > c.freq_delta_khz.max {
        return None;
    }
    let _freq = if c.type_id == 0 { c.data[0] } else { c.data[1] };
    Some(ClockInfo { current_khz: c.freq_delta_khz.value, min_khz: c.freq_delta_khz.min, max_khz: c.freq_delta_khz.max })
}

/// The display adapters Windows knows, read straight from the registry (no PowerShell start-up).
pub fn adapter_names() -> Vec<String> {
    use winreg::enums::HKEY_LOCAL_MACHINE;
    let Ok(class) = winreg::RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Control\\Class\\{4d36e968-e325-11ce-bfc1-08002be10318}")
    else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for sub in class.enum_keys().flatten() {
        if sub.len() != 4 || !sub.chars().all(|c| c.is_ascii_digit()) {
            continue;
        }
        if let Ok(k) = class.open_subkey(&sub) {
            if let Ok(n) = k.get_value::<String, _>("DriverDesc") {
                let l = n.to_lowercase();
                if !l.contains("basic") && !l.contains("remote") && !l.contains("virtual") && !out.contains(&n) {
                    out.push(n);
                }
            }
        }
    }
    out
}

fn video_controller() -> (String, String) {
    // Prefer a discrete card over integrated graphics when both are present.
    let names = adapter_names();
    let name = names
        .iter()
        .find(|n| {
            let l = n.to_lowercase();
            l.contains("nvidia") || l.contains("radeon rx") || l.contains("geforce")
        })
        .or_else(|| names.first())
        .cloned()
        .unwrap_or_default();
    let l = name.to_lowercase();
    let vendor = if l.contains("nvidia") {
        "NVIDIA"
    } else if l.contains("amd") || l.contains("radeon") {
        "AMD"
    } else if l.contains("intel") {
        "Intel"
    } else {
        "Unknown"
    };
    (name, vendor.to_string())
}

pub fn probe() -> GpuState {
    let (wmi_name, vendor) = video_controller();
    let mut st = GpuState { name: wmi_name, vendor, ..Default::default() };
    match api() {
        Ok(a) => {
            if let Some(nf) = a.name {
                let mut buf = [0u8; 64];
                if unsafe { nf(a.gpu, buf.as_mut_ptr()) } == 0 {
                    let end = buf.iter().position(|b| *b == 0).unwrap_or(64);
                    let n = String::from_utf8_lossy(&buf[..end]).to_string();
                    if !n.is_empty() {
                        st.name = n;
                    }
                }
            }
            match read_states(&a) {
                Ok(s) => {
                    st.core = clock_of(&s, DOMAIN_GRAPHICS).unwrap_or_default();
                    st.mem = clock_of(&s, DOMAIN_MEMORY).unwrap_or_default();
                    st.supported = st.core.max_khz > 0 || st.mem.max_khz > 0;
                    if !st.supported {
                        st.note = "this card does not allow clock offsets".into();
                    }
                }
                Err(e) => st.note = e,
            }
        }
        Err(e) => st.note = if st.vendor == "AMD" { "AMD cards are not supported in this version".into() } else { e },
    }
    st
}

/// The offsets CatalyX uses, in kHz. Deliberately small: the card's own limits are far above what
/// is sensible (this driver reports +1200 MHz as "allowed"), so the step is a fixed modest amount
/// that nearly any card holds, never a share of the reported range. Backed off at once above the
/// heat limit (see `engine`), and always clamped to what the driver says is allowed.
pub const SAFE_STEP_KHZ: (i32, i32) = (50_000, 20_000);
/// "Balanced" is twice "Safe". Nothing higher is offered.
pub const BALANCED_STEP_KHZ: (i32, i32) = (100_000, 40_000);

/// The offsets CatalyX would apply for a level: (core kHz, memory kHz).
pub fn planned(st: &GpuState, level: GpuLevel) -> (i32, i32) {
    if !st.supported {
        return (0, 0);
    }
    let (c, m) = match level {
        GpuLevel::Off => (0, 0),
        GpuLevel::Safe => SAFE_STEP_KHZ,
        GpuLevel::Balanced => BALANCED_STEP_KHZ,
    };
    (c.min(st.core.max_khz.max(0)), m.min(st.mem.max_khz.max(0)))
}
fn set_domain(a: &Api, domain: u32, khz: i32) -> Result<(), String> {
    let mut s = blank();
    s.version = V1_SIZE | (1 << 16);
    s.num_pstates = 1;
    s.num_clocks = 1;
    s.pstates[0].pstate_id = 0;
    s.pstates[0].clocks[0].domain_id = domain;
    s.pstates[0].clocks[0].type_id = 0;
    s.pstates[0].clocks[0].freq_delta_khz.value = khz;
    let r = unsafe { (a.set)(a.gpu, &mut s) };
    if r == 0 {
        Ok(())
    } else {
        Err(match r {
            // NVAPI_INVALID_USER_PRIVILEGE: the driver only lets an administrator change clocks.
            -137 => "changing graphics clocks needs administrator rights (restart CatalyX as administrator)".to_string(),
            // NVAPI_NOT_SUPPORTED / NVAPI_NO_IMPLEMENTATION
            -104 | -3 => "this card or driver does not allow clock changes".to_string(),
            _ => format!("the driver refused the change (code {r})"),
        })
    }
}

/// Applies offsets (kHz). Returns the offsets that were in place before.
pub fn apply(core_khz: i32, mem_khz: i32) -> Result<(i32, i32), String> {
    let a = api()?;
    let s = read_states(&a)?;
    let core = clock_of(&s, DOMAIN_GRAPHICS);
    let mem = clock_of(&s, DOMAIN_MEMORY);
    if (core_khz > 0 && core.is_none()) || (mem_khz > 0 && mem.is_none()) {
        return Err("a clock domain is no longer editable; nothing was changed".into());
    }
    let core = core.unwrap_or_default();
    let mem = mem.unwrap_or_default();
    // Never go above what the driver allows; never lower an offset the person set themselves.
    let core_changed = core_khz > 0 && core_khz > core.current_khz;
    if core_changed {
        set_domain(&a, DOMAIN_GRAPHICS, core_khz.min(core.max_khz))?;
    }
    if mem_khz > 0 && mem_khz > mem.current_khz {
        if let Err(e) = set_domain(&a, DOMAIN_MEMORY, mem_khz.min(mem.max_khz)) {
            if core_changed {
                let _ = set_domain(&a, DOMAIN_GRAPHICS, core.current_khz);
            }
            return Err(e);
        }
    }
    Ok((core.current_khz, mem.current_khz))
}

pub fn restore(core_khz: i32, mem_khz: i32) -> Result<(), String> {
    let a = api()?;
    let s = read_states(&a)?;
    let core = clock_of(&s, DOMAIN_GRAPHICS).unwrap_or_default();
    let mem = clock_of(&s, DOMAIN_MEMORY).unwrap_or_default();
    let mut error = None;
    if core.current_khz != core_khz {
        if let Err(e) = set_domain(&a, DOMAIN_GRAPHICS, core_khz) {
            error = Some(e);
        }
    }
    if mem.current_khz != mem_khz {
        if let Err(e) = set_domain(&a, DOMAIN_MEMORY, mem_khz) {
            error = Some(e);
        }
    }
    error.map_or(Ok(()), Err)
}

// ------------------------------------------------------------------ live readings (NVML)

/// Live readings straight from the NVIDIA driver's management library (the library nvidia-smi
/// itself uses): no helper program is started, so reading every few seconds costs nothing.
#[derive(Clone, Debug, Default)]
pub struct Live {
    pub temp_c: Option<u32>,
    pub core_mhz: Option<u32>,
    pub mem_mhz: Option<u32>,
    pub util_pct: Option<u32>,
    pub power_w: Option<f32>,
    pub vram_used: Option<u64>,
    pub vram_total: Option<u64>,
}

#[repr(C)]
#[derive(Default)]
struct NvmlUtil {
    gpu: u32,
    memory: u32,
}

#[repr(C)]
#[derive(Default)]
struct NvmlMem {
    total: u64,
    free: u64,
    used: u64,
}

struct Nvml {
    dev: *mut c_void,
    temp: unsafe extern "C" fn(*mut c_void, u32, *mut u32) -> i32,
    clock: unsafe extern "C" fn(*mut c_void, u32, *mut u32) -> i32,
    util: unsafe extern "C" fn(*mut c_void, *mut NvmlUtil) -> i32,
    power: unsafe extern "C" fn(*mut c_void, *mut u32) -> i32,
    mem: unsafe extern "C" fn(*mut c_void, *mut NvmlMem) -> i32,
}

// The handle is only a driver-side identifier, valid on any thread.
unsafe impl Send for Nvml {}

fn nvml() -> Option<&'static std::sync::Mutex<Nvml>> {
    static CELL: std::sync::OnceLock<Option<std::sync::Mutex<Nvml>>> = std::sync::OnceLock::new();
    CELL.get_or_init(|| unsafe {
        let load = |n: &str| {
            crate::ffi::proc_address("nvml.dll", n)
                .or_else(|| crate::ffi::proc_address("C:\\Program Files\\NVIDIA Corporation\\NVSMI\\nvml.dll", n))
        };
        let init: unsafe extern "C" fn() -> i32 = std::mem::transmute(load("nvmlInit_v2")?);
        if init() != 0 {
            return None;
        }
        let by_index: unsafe extern "C" fn(u32, *mut *mut c_void) -> i32 = std::mem::transmute(load("nvmlDeviceGetHandleByIndex_v2")?);
        let mut dev = std::ptr::null_mut();
        if by_index(0, &mut dev) != 0 || dev.is_null() {
            return None;
        }
        Some(std::sync::Mutex::new(Nvml {
            dev,
            temp: std::mem::transmute(load("nvmlDeviceGetTemperature")?),
            clock: std::mem::transmute(load("nvmlDeviceGetClockInfo")?),
            util: std::mem::transmute(load("nvmlDeviceGetUtilizationRates")?),
            power: std::mem::transmute(load("nvmlDeviceGetPowerUsage")?),
            mem: std::mem::transmute(load("nvmlDeviceGetMemoryInfo")?),
        }))
    })
    .as_ref()
}

/// Current temperature, clocks, load, power and memory of the first NVIDIA card, if it has a
/// management library.
pub fn live() -> Option<Live> {
    let n = nvml()?.lock().ok()?;
    unsafe {
        let mut l = Live::default();
        let mut v = 0u32;
        if (n.temp)(n.dev, 0, &mut v) == 0 {
            l.temp_c = Some(v);
        }
        if (n.clock)(n.dev, 0, &mut v) == 0 {
            l.core_mhz = Some(v);
        }
        if (n.clock)(n.dev, 2, &mut v) == 0 {
            l.mem_mhz = Some(v);
        }
        let mut u = NvmlUtil::default();
        if (n.util)(n.dev, &mut u) == 0 {
            l.util_pct = Some(u.gpu);
        }
        if (n.power)(n.dev, &mut v) == 0 {
            l.power_w = Some(v as f32 / 1000.0);
        }
        let mut m = NvmlMem::default();
        if (n.mem)(n.dev, &mut m) == 0 {
            l.vram_used = Some(m.used);
            l.vram_total = Some(m.total);
        }
        Some(l)
    }
}

/// GPU temperature in °C: from the driver library, or the driver's nvidia-smi tool as a fallback.
pub fn temperature() -> Option<u32> {
    if let Some(t) = live().and_then(|l| l.temp_c) {
        return Some(t);
    }
    let mut c = std::process::Command::new("nvidia-smi");
    c.args(["--query-gpu=temperature.gpu", "--format=csv,noheader,nounits"]);
    let out = crate::ffi::run_timeout(c, 8)?;
    String::from_utf8_lossy(&out.stdout).lines().next()?.trim().parse().ok()
}

/// Applies a level's offsets for about 3 seconds, reads them back from the driver, then puts the
/// previous values back. Proves the whole path works on this card; refuses if the card is warm or
/// its temperature cannot be read.
pub fn roundtrip(level: GpuLevel, limit_c: u32) -> Result<String, String> {
    let before = probe();
    if !before.supported {
        return Err(format!("not available: {}", if before.note.is_empty() { "this card does not allow offsets" } else { &before.note }));
    }
    let (c, m) = planned(&before, level);
    let temp = temperature().ok_or("the temperature cannot be read, so no clock change is made")?;
    if temp + 5 >= limit_c {
        return Err(format!("the card is at {temp} C, too close to the {limit_c} C limit"));
    }
    let prev = apply(c, m)?;
    std::thread::sleep(std::time::Duration::from_secs(3));
    let during = probe();
    let restored = restore(prev.0, prev.1);
    let after = probe();
    restored?;
    let ok = (c <= 0 || during.core.current_khz >= c) && (m <= 0 || during.mem.current_khz >= m);
    if !ok {
        return Err("the driver accepted the call but did not report the new offsets".into());
    }
    Ok(format!(
        "core {:+} / memory {:+} MHz read back from the driver at {temp} C, then back to {:+} / {:+} MHz",
        during.core.current_khz / 1000,
        during.mem.current_khz / 1000,
        after.core.current_khz / 1000,
        after.mem.current_khz / 1000
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn card(core_max: i32, mem_max: i32) -> GpuState {
        let mut s = GpuState { supported: true, ..Default::default() };
        s.core.max_khz = core_max;
        s.mem.max_khz = mem_max;
        s
    }

    #[test]
    fn off_and_unsupported_cards_get_nothing() {
        assert_eq!(planned(&card(1_200_000, 1_000_000), GpuLevel::Off), (0, 0));
        assert_eq!(planned(&GpuState::default(), GpuLevel::Balanced), (0, 0));
    }

    #[test]
    fn safe_is_modest_and_balanced_is_twice_safe() {
        let c = card(1_200_000, 1_000_000);
        assert_eq!(planned(&c, GpuLevel::Safe), (50_000, 20_000));
        assert_eq!(planned(&c, GpuLevel::Balanced), (100_000, 40_000));
    }

    #[test]
    fn offsets_never_exceed_what_the_driver_allows() {
        let tight = card(30_000, 0);
        assert_eq!(planned(&tight, GpuLevel::Balanced), (30_000, 0));
        assert_eq!(planned(&card(-5, -5), GpuLevel::Safe), (0, 0));
    }
}
