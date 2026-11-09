//! Power plans through Windows' own powercfg tool.

use std::process::Command;

pub const HIGH_PERFORMANCE: &str = "8c5e7fda-e8bf-4a96-9a85-a6e23a8c635c";
/// CatalyX's own plan (a copy of High performance with the values below).
pub const BOOST_PLAN: &str = "5e7b0057-9a3e-4c1f-8f00-b0057ab1e000";

#[derive(Clone, Debug)]
pub struct Plan {
    pub guid: String,
    pub name: String,
    pub active: bool,
}

pub fn run(args: &[&str]) -> Option<String> {
    let mut c = Command::new("powercfg");
    c.args(args);
    let out = crate::ffi::run_timeout(c, 20)?;
    if out.status.success() {
        Some(String::from_utf8_lossy(&out.stdout).to_string())
    } else {
        None
    }
}

pub fn list() -> Vec<Plan> {
    let mut v = Vec::new();
    if let Some(s) = run(&["/list"]) {
        for line in s.lines() {
            if let Some(i) = line.find(':') {
                let rest = line[i + 1..].trim();
                if rest.len() < 36 {
                    continue;
                }
                let guid = rest[..36].to_string();
                if guid.matches('-').count() != 4 {
                    continue;
                }
                let name = rest[36..].trim().trim_end_matches('*').trim().trim_start_matches('(').trim_end_matches(')').to_string();
                v.push(Plan { guid, name, active: line.trim_end().ends_with('*') });
            }
        }
    }
    v
}

pub fn active() -> Option<String> {
    list().into_iter().find(|p| p.active).map(|p| p.guid)
}

pub fn set_active(guid: &str) -> bool {
    run(&["/setactive", guid]).is_some()
}

/// What the game power plan sets, on both mains (AC) and battery (DC). The plan is a copy of
/// High performance (8c5e7fda-...) with these values changed. (subgroup, setting, value). Names the engine's `powercfg -aliases` does not know are
/// given by GUID; their meaning is noted beside them.
const GAME_PLAN_SETTINGS: &[(&str, &str, &str)] = &[
    ("SUB_PROCESSOR", "PROCTHROTTLEMIN", "100"),
    ("SUB_PROCESSOR", "PROCTHROTTLEMAX", "100"),
    // System cooling policy: 1 = active (setting name from Windows' documented GUID list)
    ("SUB_PROCESSOR", "94d3a615-a899-4ac5-ae2b-e4d8f634367f", "1"),
    ("SUB_DISK", "DISKIDLE", "0"),
    // Wireless adapter power saving mode: 0 = maximum performance
    ("19cbb8fa-5279-450e-9fac-8a3d5fedd0c1", "12bbebe6-58d6-4636-95bb-3217ef867c1a", "0"),
    // Sleep after / hibernate after: 0 = never
    ("SUB_SLEEP", "STANDBYIDLE", "0"),
    ("SUB_SLEEP", "HIBERNATEIDLE", "0"),
    ("SUB_PCIEXPRESS", "ASPM", "0"),
    // Display off / dim timeouts and adaptive brightness: 0. Brightness itself is left as it is.
    ("SUB_VIDEO", "VIDEOIDLE", "0"),
    ("SUB_VIDEO", "17aaa29b-8b43-4b94-aafe-35f64daaf1ee", "0"),
    ("SUB_VIDEO", "90959d22-d6a1-49b9-af93-bce885ad335b", "0"),
    ("SUB_VIDEO", "ADAPTBRIGHT", "0"),
    // Multimedia: when sharing media 1 = don't idle to sleep; when playing video 0 = best quality
    ("9596fb26-9850-41fd-ac3e-f7c3c00afd4b", "03680956-93bc-4294-bba6-4e0f09bb717f", "1"),
    ("9596fb26-9850-41fd-ac3e-f7c3c00afd4b", "34c7b99f-9a6d-4b3c-8dc7-b6693b78cef4", "0"),
    // Intel graphics power plan (only exists on PCs with Intel graphics; ignored elsewhere)
    ("f693fb01-e858-4f00-b20f-f30e12ac06d6", "191f65b5-d45c-4a4f-8aae-1ab8bfd980e6", "1"),
];

/// Left out on purpose: "require a password on wakeup" = 0
/// (0e796bdb-...) lowers security, and "lid close action" = do nothing (5ca83367-...) can cook a
/// laptop in a bag.
const POWER_SAVER: &str = "a1841308-3541-4fab-bc81-f71556f20b4a";

pub const SUB_PROCESSOR: &str = "54533251-82be-4824-96c1-47b60b740d00";
pub const PERFBOOSTMODE: &str = "be337238-0d82-4146-a960-4f3749d470c7";
const PERFBOOSTPOL: &str = "45bcc044-d885-43e2-8605-ee0ec6e96b59";
const PERFEPP: &str = "36687f9e-e3a5-4dbf-b1dc-15eb381c6863";
const PERFEPP1: &str = "36687f9e-e3a5-4dbf-b1dc-15eb381c6864";
const PERFINCPOL: &str = "465e1f50-b610-473a-ab58-00d1077dc418";
const PERFINCTHRESHOLD: &str = "06cadf0e-64ed-448a-8927-ce7bf90eb35d";
const PERFINCTIME: &str = "984cf492-3bed-4488-a8f9-4286c97bf5aa";
pub const CPMINCORES: &str = "0cc5b647-c1df-4637-891a-dec35c318583";
const CPMAXCORES: &str = "ea062031-0e34-4ff1-9b6d-eb1059334028";
pub const PROCTHROTTLEMIN: &str = "893dee8e-2bef-41e0-89c6-b55d0929964c";
pub const PROCTHROTTLEMAX: &str = "bc5038f7-23e0-4960-96da-33abaf5935ec";
pub const PERFEPP_ID: &str = PERFEPP;

/// The processor settings for each CPU level: (setting, value) in the "Processor power
/// management" group. All are Windows' own, ordinary controls that the CPU's firmware honours
/// within its rated limits (no voltage, no multiplier). Settings a CPU doesn't have are ignored.
pub fn cpu_settings(level: crate::config::CpuLevel) -> Vec<(&'static str, &'static str)> {
    use crate::config::CpuLevel;
    match level {
        CpuLevel::Off => Vec::new(),
        CpuLevel::Boost => vec![
            (PROCTHROTTLEMIN, "100"),
            (PROCTHROTTLEMAX, "100"),
            (PERFBOOSTMODE, "2"), // aggressive turbo
            (PERFBOOSTPOL, "100"),
            (PERFEPP, "0"), // favour performance over saving power
            (PERFEPP1, "0"),
            (CPMINCORES, "100"), // never park a core
            (CPMAXCORES, "100"),
        ],
        CpuLevel::Max => vec![
            (PROCTHROTTLEMIN, "100"),
            (PROCTHROTTLEMAX, "100"),
            (PERFBOOSTMODE, "2"),
            (PERFBOOSTPOL, "100"),
            (PERFEPP, "0"),
            (PERFEPP1, "0"),
            (CPMINCORES, "100"),
            (CPMAXCORES, "100"),
            (PERFINCPOL, "2"),        // "rocket": climb straight to the top
            (PERFINCTHRESHOLD, "10"), // at 10% load instead of 90%
            (PERFINCTIME, "1"),       // after one check
        ],
    }
}

/// Reads one AC value of a plan back (to prove a write took).
pub fn query_ac(plan: &str, sub: &str, setting: &str) -> Option<u32> {
    // Windows hides "advanced" processor settings from `powercfg /query` even though it honours
    // them, so the plan's own registry record is read first.
    let path = format!("SYSTEM\\CurrentControlSet\\Control\\Power\\User\\PowerSchemes\\{plan}\\{sub}\\{setting}");
    if let Ok(k) = winreg::RegKey::predef(winreg::enums::HKEY_LOCAL_MACHINE).open_subkey(path) {
        if let Ok(v) = k.get_value::<u32, _>("ACSettingIndex") {
            return Some(v);
        }
    }
    let text = run(&["/query", plan, sub, setting])?;
    text.lines().find_map(|l| {
        let l = l.trim();
        l.strip_prefix("Current AC Power Setting Index:").and_then(|v| u32::from_str_radix(v.trim().trim_start_matches("0x"), 16).ok())
    })
}

/// CatalyX's plan: a copy of the person's own active plan (so their other choices stay),
/// or of Windows' High performance when that is the Power saver or the plan itself, with the
/// game settings and the chosen CPU level written into it. Rebuilt every boost so it
/// never carries stale values. Returns false when Windows refuses to create it.
pub fn ensure_boost_plan(level: crate::config::CpuLevel, source: &str) -> bool {
    let src = if source.eq_ignore_ascii_case(BOOST_PLAN) || source.eq_ignore_ascii_case(POWER_SAVER) || source.is_empty() {
        HIGH_PERFORMANCE
    } else {
        source
    };
    let have = list().iter().any(|p| p.guid.eq_ignore_ascii_case(BOOST_PLAN));
    // Delete the old copy (it can't be deleted while active; then it is only refreshed).
    let rebuilt = have && run(&["/delete", BOOST_PLAN]).is_some();
    if !have || rebuilt {
        let ok = run(&["/duplicatescheme", src, BOOST_PLAN]).is_some()
            // Some PCs have no High performance plan: copy whatever is active instead.
            || active().map(|a| run(&["/duplicatescheme", &a, BOOST_PLAN]).is_some()).unwrap_or(false);
        if !ok {
            return list().iter().any(|p| p.guid.eq_ignore_ascii_case(BOOST_PLAN));
        }
        run(&["/changename", BOOST_PLAN, "CatalyX Power Plan", "Applied by CatalyX while a game runs"]);
    }
    // USB selective suspend off.
    let own: [(&str, &str, &str); 1] = [("2a737441-1930-4402-8d77-b2bebba308a3", "48e6b7a6-50f5-4782-a5d4-53bb8f07e226", "0")];
    let cpu: Vec<(&str, &str, &str)> = cpu_settings(level).into_iter().map(|(s, v)| (SUB_PROCESSOR, s, v)).collect();
    for (sub, set, val) in GAME_PLAN_SETTINGS.iter().chain(own.iter()).chain(cpu.iter()) {
        // Some settings do not exist on every PC; optional writes may fail harmlessly.
        run(&["/setacvalueindex", BOOST_PLAN, sub, set, val]);
        run(&["/setdcvalueindex", BOOST_PLAN, sub, set, val]);
    }
    if level != crate::config::CpuLevel::Off {
        // Never claim CPU tuning happened when powercfg silently rejected the key settings.
        return query_ac(BOOST_PLAN, SUB_PROCESSOR, PERFBOOSTMODE) == Some(2)
            && query_ac(BOOST_PLAN, SUB_PROCESSOR, PROCTHROTTLEMIN) == Some(100)
            && query_ac(BOOST_PLAN, SUB_PROCESSOR, CPMINCORES) == Some(100);
    }
    true
}
/// Removes CatalyX's plan (only while it is not the active one).
pub fn delete_boost_plan() {
    if active().map(|a| !a.eq_ignore_ascii_case(BOOST_PLAN)).unwrap_or(false) {
        run(&["/delete", BOOST_PLAN]);
    }
}
