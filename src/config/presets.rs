//! Presets: the three built-in ones (Gentle, Balanced, Max Squeeze) and the shape of a saved preset.

use super::*;

#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
pub struct Preset {
    pub id: String,
    pub name: String,
    pub about: String,
    pub builtin: bool,
    pub settings: Settings,
}

pub const PRESET_GENTLE: &str = "gentle";
pub const PRESET_BALANCED: &str = "balanced";
pub const PRESET_MAX: &str = "max";

pub fn builtin_presets() -> Vec<Preset> {
    let gentle = Settings {
        lower_others: false,
        priority: DEFAULT_PRIORITY,
        power: PowerChoice::HighPerformance,
        cpu: CpuLevel::Off,
        apps: true,
        services: false,
        clean_ram: false,
        windows_game_mode: true,
        game_dvr_off: true,
        notifications_off: true,
        transparency_off: false,
        pause_maintenance: false,
        mmcss_games: false,
        no_power_throttling: true,
        gpu: GpuLevel::Off,
        purge_standby: false,
        io_boost: false,
        quiet_desktop: false,
    };
    let balanced = Settings::default();
    let max = Settings {
        lower_others: true,
        priority: Priority::High,
        power: PowerChoice::BoostPlan,
        cpu: CpuLevel::Max,
        apps: true,
        services: true,
        clean_ram: true,
        windows_game_mode: true,
        game_dvr_off: true,
        notifications_off: true,
        transparency_off: true,
        pause_maintenance: true,
        mmcss_games: true,
        no_power_throttling: true,
        gpu: GpuLevel::Safe,
        purge_standby: true,
        io_boost: true,
        quiet_desktop: true,
    };
    vec![
        Preset {
            id: PRESET_GENTLE.into(),
            name: "Gentle".into(),
            about: "Light touch: closes your listed apps and lets Windows Game Mode do the rest.".into(),
            builtin: true,
            settings: gentle,
        },
        Preset {
            id: PRESET_BALANCED.into(),
            name: "Balanced".into(),
            about: "Recommended: apps, services, power plan, CPU boost and quiet Windows.".into(),
            builtin: true,
            settings: balanced,
        },
        Preset {
            id: PRESET_MAX.into(),
            name: "Max Squeeze".into(),
            about: "Everything on, plus a safe graphics card overclock. Best for low-end PCs.".into(),
            builtin: true,
            settings: max,
        },
    ]
}
