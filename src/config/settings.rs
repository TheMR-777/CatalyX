//! The choices a boost is made of: priority, power, CPU and GPU levels, what to do with an app, and the
//! `Settings` bundle a preset or a game carries.

use super::*;

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum Priority {
    Normal,
    AboveNormal,
    High,
    /// Kept only so old saved profiles still load. Never offered or applied: this class
    /// can starve input and sound even when the game itself is running normally.
    Realtime,
}

impl Priority {
    pub const ALL: [Priority; 3] = [Priority::Normal, Priority::AboveNormal, Priority::High];
    pub fn hint(self) -> &'static str {
        match self {
            Priority::Normal => "Windows default",
            Priority::AboveNormal => "Helps on busy PCs",
            Priority::High => "Game first, everything else waits",
            Priority::Realtime => "Can freeze mouse and sound",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Priority::Normal => "Normal",
            Priority::AboveNormal => "Above normal",
            Priority::High => "High",
            Priority::Realtime => "Realtime (risky)",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum PowerChoice {
    Keep,
    HighPerformance,
    BoostPlan,
}

impl PowerChoice {
    pub const ALL: [PowerChoice; 3] = [PowerChoice::Keep, PowerChoice::HighPerformance, PowerChoice::BoostPlan];
    pub fn hint(self) -> &'static str {
        match self {
            PowerChoice::Keep => "Leave the power plan alone",
            PowerChoice::HighPerformance => "The built-in Windows plan",
            PowerChoice::BoostPlan => "Turbo held high, cores never parked",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            PowerChoice::Keep => "Keep current plan",
            PowerChoice::HighPerformance => "High performance",
            PowerChoice::BoostPlan => "CatalyX plan",
        }
    }
}

#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum GpuLevel {
    Off,
    Safe,
    Balanced,
}

impl GpuLevel {
    pub const ALL: [GpuLevel; 3] = [GpuLevel::Off, GpuLevel::Safe, GpuLevel::Balanced];
    pub fn hint(self) -> &'static str {
        match self {
            GpuLevel::Off => "Clocks untouched",
            GpuLevel::Safe => "Half a step: well inside what the driver allows",
            GpuLevel::Balanced => "A full step (never more than 150 MHz)",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            GpuLevel::Off => "Off",
            GpuLevel::Safe => "Safe (half step)",
            GpuLevel::Balanced => "Balanced (full step)",
        }
    }
}

/// How hard the processor is pushed while gaming. Always inside what the CPU and its cooling
/// allow (Windows' own power controls): never voltages or multipliers.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum CpuLevel {
    Off,
    /// Turbo held on, 100% minimum state, no core parking, fast cooling.
    Boost,
    /// Boost, plus clocks that climb at once and fall slowly (best for stuttery games).
    Max,
}

impl CpuLevel {
    pub const ALL: [CpuLevel; 3] = [CpuLevel::Off, CpuLevel::Boost, CpuLevel::Max];
    pub fn label(self) -> &'static str {
        match self {
            CpuLevel::Off => "Off",
            CpuLevel::Boost => "Boost",
            CpuLevel::Max => "Max sustained",
        }
    }
    pub fn hint(self) -> &'static str {
        match self {
            CpuLevel::Off => "Leave the processor's power settings alone",
            CpuLevel::Boost => "Turbo on, 100% minimum, cores never parked",
            CpuLevel::Max => "Boost, plus clocks jump up at once and stay up",
        }
    }
}

/// What happens to an app while a game runs.
#[derive(Serialize, Deserialize, Clone, Copy, PartialEq, Eq, Debug)]
pub enum ProcAction {
    Keep,
    /// Ended at boost and ended again if it comes back while the game runs.
    Close,
    /// Frozen while the game runs, unfrozen afterwards.
    Suspend,
    /// Lowest priority and memory trimmed while the game runs.
    Lower,
}

impl ProcAction {
    pub const ALL: [ProcAction; 4] = [ProcAction::Keep, ProcAction::Suspend, ProcAction::Lower, ProcAction::Close];
    pub fn hint(self) -> &'static str {
        match self {
            ProcAction::Keep => "Leave it running as it is",
            ProcAction::Suspend => "Frozen while you play, resumed after",
            ProcAction::Lower => "Lowest priority, memory trimmed",
            ProcAction::Close => "Ended, and kept off while you play",
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            ProcAction::Keep => "Keep",
            ProcAction::Close => "Close",
            ProcAction::Suspend => "Pause",
            ProcAction::Lower => "Lower",
        }
    }
}

/// Everything one boost does. Presets and custom game profiles each hold one.
#[derive(Serialize, Deserialize, Clone, PartialEq, Debug)]
#[serde(default)]
pub struct Settings {
    pub priority: Priority,
    pub power: PowerChoice,
    pub cpu: CpuLevel,
    pub apps: bool,
    pub services: bool,
    pub clean_ram: bool,
    pub windows_game_mode: bool,
    pub game_dvr_off: bool,
    pub notifications_off: bool,
    pub transparency_off: bool,
    pub pause_maintenance: bool,
    pub mmcss_games: bool,
    pub no_power_throttling: bool,
    pub gpu: GpuLevel,
    /// Every other app of the same user goes to the lowest priority while the game runs, and is put
    /// back afterwards.
    pub lower_others: bool,
    /// Also flush Windows' standby memory list while cleaning RAM (needs administrator).
    pub purge_standby: bool,
    /// Give the game high disk (I/O) and memory page priority, not just CPU priority.
    pub io_boost: bool,
    /// Switch Windows' desktop (taskbar, icons, Start menu) off while the game runs and bring it
    /// back afterwards. See shell.rs.
    pub quiet_desktop: bool,
}

/// The default game priority is Above normal: a boost that leaves the game at Normal would change
/// nothing. (High is offered too; Realtime never is, because it can freeze Windows' own input.)
pub const DEFAULT_PRIORITY: Priority = Priority::AboveNormal;

impl Default for Settings {
    fn default() -> Self {
        Settings {
            lower_others: false,
            priority: DEFAULT_PRIORITY,
            power: PowerChoice::BoostPlan,
            cpu: CpuLevel::Boost,
            apps: true,
            services: true,
            // Trimming working sets before every game can trigger costly page faults later.
            // Leave the optional Clean RAM control to Max Squeeze or the user's own profile.
            clean_ram: false,
            windows_game_mode: true,
            game_dvr_off: true,
            notifications_off: true,
            transparency_off: false,
            pause_maintenance: true,
            mmcss_games: false,
            no_power_throttling: true,
            gpu: GpuLevel::Off,
            purge_standby: false,
            io_boost: true,
            quiet_desktop: false,
        }
    }
}
