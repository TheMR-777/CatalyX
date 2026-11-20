//! The settings catalogue: one table describes every profile setting and app option once, so the CLI
//! (`show`, `set`, `option`, the help text) and the classic forms read the same keys, labels and hints.
//! Adding a setting means adding its field to `Settings` (or `Config`) and one row here; nothing else
//! lists it again.
//!
//! The CLI tokens are part of the command-line contract: scripts use them, so they keep their old
//! spelling and order even where the window's form shows a different one.

use crate::config::{Config, CpuLevel, GpuLevel, PowerChoice, Priority, Settings};
use crate::gpu::GpuState;

/// A preset or a game with its own settings, by id. An index goes stale as soon as something is
/// removed; a skin holding one would silently edit the wrong profile.
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum Target {
    Preset(crate::app::PresetId),
    Game(crate::app::GameId),
}

/// What a setting affects. Neutral: a skin may group these however it likes.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Group {
    Performance,
    AppsServicesMemory,
    Windows,
}

impl Group {
    pub const ALL: [Group; 3] = [Group::Performance, Group::AppsServicesMemory, Group::Windows];

    pub fn label(self) -> &'static str {
        match self {
            Group::Performance => "Performance",
            Group::AppsServicesMemory => "Apps, services and memory",
            Group::Windows => "Windows while you play",
        }
    }
}

/// A setting's value, as the catalogue reads it: a switch, or the key of the chosen option.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Value {
    Switch(bool),
    Choice(&'static str),
}

/// One value of a choice setting. `aliases` are other words the CLI has always accepted.
#[derive(Clone, Copy)]
pub struct Choice {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub aliases: &'static [&'static str],
}

#[derive(Clone, Copy)]
pub enum Kind {
    Switch,
    Choice(&'static [Choice]),
}

/// Where a form's hint line comes from.
#[derive(Clone, Copy)]
pub enum Hint {
    /// The same line whatever the value (switches).
    Fixed(&'static str),
    /// The chosen value's own hint (priority, power, CPU).
    Selected,
    /// The live graphics-card hint: planned offsets when the driver allows them.
    GpuLive,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum Field {
    Priority,
    Power,
    Cpu,
    Gpu,
    Apps,
    Services,
    CleanRam,
    IoBoost,
    PurgeStandby,
    WindowsGameMode,
    GameDvrOff,
    NotificationsOff,
    TransparencyOff,
    PauseMaintenance,
    MmcssGames,
    NoPowerThrottling,
    LowerOthers,
    QuietDesktop,
}

impl Field {
    fn get(self, s: &Settings) -> Value {
        match self {
            Field::Priority => Value::Choice(match s.priority {
                Priority::Normal => "normal",
                Priority::AboveNormal => "above",
                Priority::High => "high",
                Priority::Realtime => "realtime",
            }),
            Field::Power => Value::Choice(match s.power {
                PowerChoice::Keep => "keep",
                PowerChoice::HighPerformance => "high",
                PowerChoice::BoostPlan => "catalyx",
            }),
            Field::Cpu => Value::Choice(match s.cpu {
                CpuLevel::Off => "off",
                CpuLevel::Boost => "boost",
                CpuLevel::Max => "max",
            }),
            Field::Gpu => Value::Choice(match s.gpu {
                GpuLevel::Off => "off",
                GpuLevel::Safe => "safe",
                GpuLevel::Balanced => "balanced",
            }),
            Field::Apps => Value::Switch(s.apps),
            Field::Services => Value::Switch(s.services),
            Field::CleanRam => Value::Switch(s.clean_ram),
            Field::IoBoost => Value::Switch(s.io_boost),
            Field::PurgeStandby => Value::Switch(s.purge_standby),
            Field::WindowsGameMode => Value::Switch(s.windows_game_mode),
            Field::GameDvrOff => Value::Switch(s.game_dvr_off),
            Field::NotificationsOff => Value::Switch(s.notifications_off),
            Field::TransparencyOff => Value::Switch(s.transparency_off),
            Field::PauseMaintenance => Value::Switch(s.pause_maintenance),
            Field::MmcssGames => Value::Switch(s.mmcss_games),
            Field::NoPowerThrottling => Value::Switch(s.no_power_throttling),
            Field::LowerOthers => Value::Switch(s.lower_others),
            Field::QuietDesktop => Value::Switch(s.quiet_desktop),
        }
    }

    fn set(self, s: &mut Settings, v: Value) {
        match (self, v) {
            (Field::Priority, Value::Choice(k)) => {
                if let Some(p) = ["normal", "above", "high"].iter().position(|x| *x == k).map(|i| Priority::ALL[i]) {
                    s.priority = p;
                }
            }
            (Field::Power, Value::Choice(k)) => {
                if let Some(p) = ["keep", "high", "catalyx"].iter().position(|x| *x == k).map(|i| PowerChoice::ALL[i]) {
                    s.power = p;
                }
            }
            (Field::Cpu, Value::Choice(k)) => {
                if let Some(p) = ["off", "boost", "max"].iter().position(|x| *x == k).map(|i| CpuLevel::ALL[i]) {
                    s.cpu = p;
                }
            }
            (Field::Gpu, Value::Choice(k)) => {
                if let Some(p) = ["off", "safe", "balanced"].iter().position(|x| *x == k).map(|i| GpuLevel::ALL[i]) {
                    s.gpu = p;
                }
            }
            (Field::Apps, Value::Switch(b)) => s.apps = b,
            (Field::Services, Value::Switch(b)) => s.services = b,
            (Field::CleanRam, Value::Switch(b)) => s.clean_ram = b,
            (Field::IoBoost, Value::Switch(b)) => s.io_boost = b,
            (Field::PurgeStandby, Value::Switch(b)) => s.purge_standby = b,
            (Field::WindowsGameMode, Value::Switch(b)) => s.windows_game_mode = b,
            (Field::GameDvrOff, Value::Switch(b)) => s.game_dvr_off = b,
            (Field::NotificationsOff, Value::Switch(b)) => s.notifications_off = b,
            (Field::TransparencyOff, Value::Switch(b)) => s.transparency_off = b,
            (Field::PauseMaintenance, Value::Switch(b)) => s.pause_maintenance = b,
            (Field::MmcssGames, Value::Switch(b)) => s.mmcss_games = b,
            (Field::NoPowerThrottling, Value::Switch(b)) => s.no_power_throttling = b,
            (Field::LowerOthers, Value::Switch(b)) => s.lower_others = b,
            (Field::QuietDesktop, Value::Switch(b)) => s.quiet_desktop = b,
            _ => {}
        }
    }
}

/// One row of the catalogue.
#[derive(Clone, Copy)]
pub struct Spec {
    pub key: &'static str,
    pub label: &'static str,
    pub group: Group,
    pub needs_admin: bool,
    pub kind: Kind,
    pub hint: Hint,
    pub cli_order: u8,
    /// Position inside the compact KEYS help block. Choices and switches are laid out separately.
    pub help_order: u8,
    /// An old CLI-only key for this field that takes on/off (`cpu-boost`).
    pub switch_alias: Option<&'static str>,
    field: Field,
}

impl Spec {
    #[allow(clippy::too_many_arguments)]
    const fn switch(
        key: &'static str,
        label: &'static str,
        hint: &'static str,
        group: Group,
        needs_admin: bool,
        field: Field,
        cli_order: u8,
        help_order: u8,
    ) -> Spec {
        Spec {
            key,
            label,
            group,
            needs_admin,
            kind: Kind::Switch,
            hint: Hint::Fixed(hint),
            cli_order,
            help_order,
            switch_alias: None,
            field,
        }
    }

    #[allow(clippy::too_many_arguments)]
    const fn choice(
        key: &'static str,
        label: &'static str,
        hint: Hint,
        group: Group,
        needs_admin: bool,
        field: Field,
        cli_order: u8,
        help_order: u8,
        choices: &'static [Choice],
        switch_alias: Option<&'static str>,
    ) -> Spec {
        Spec { key, label, group, needs_admin, kind: Kind::Choice(choices), hint, cli_order, help_order, switch_alias, field }
    }

    pub fn get(&self, s: &Settings) -> Value {
        self.field.get(s)
    }

    pub fn set(&self, s: &mut Settings, v: Value) {
        self.field.set(s, v);
    }

    /// The line a form shows under this row.
    pub fn hint_text(&self, s: &Settings, gpu: Option<&GpuState>) -> String {
        match self.hint {
            Hint::Fixed(t) => t.to_string(),
            Hint::Selected => match self.field.get(s) {
                Value::Choice(k) => self.choice_of(k).map(|c| c.hint.to_string()).unwrap_or_else(|| match (self.field, k) {
                    // A saved Realtime priority is never offered, but its hint must still read as it did.
                    (Field::Priority, "realtime") => Priority::Realtime.hint().to_string(),
                    _ => String::new(),
                }),
                Value::Switch(_) => String::new(),
            },
            Hint::GpuLive => crate::gpu::level_hint(s.gpu, gpu),
        }
    }

    pub fn choice_of(&self, key: &str) -> Option<&'static Choice> {
        match self.kind {
            Kind::Choice(c) => c.iter().find(|c| c.key == key),
            Kind::Switch => None,
        }
    }

    /// How the CLI prints this value: a choice's label, or on/off.
    pub fn display(&self, s: &Settings) -> String {
        match self.field.get(s) {
            Value::Switch(b) => onoff(b).to_string(),
            // A loaded legacy value (Realtime priority) is not offered but must still read correctly.
            Value::Choice(k) => self
                .choice_of(k)
                .map(|c| c.label)
                .unwrap_or(if matches!((self.field, k), (Field::Priority, "realtime")) { "Realtime (risky)" } else { k })
                .to_string(),
        }
    }
}

static PRIORITY: [Choice; 3] = [
    Choice { key: "normal", label: Priority::Normal.label(), hint: Priority::Normal.hint(), aliases: &[] },
    Choice {
        key: "above",
        label: Priority::AboveNormal.label(),
        hint: Priority::AboveNormal.hint(),
        aliases: &["abovenormal", "above-normal"],
    },
    Choice { key: "high", label: Priority::High.label(), hint: Priority::High.hint(), aliases: &[] },
];

static POWER: [Choice; 3] = [
    Choice { key: "keep", label: PowerChoice::Keep.label(), hint: PowerChoice::Keep.hint(), aliases: &[] },
    Choice {
        key: "high",
        label: PowerChoice::HighPerformance.label(),
        hint: PowerChoice::HighPerformance.hint(),
        aliases: &["high-performance"],
    },
    Choice { key: "catalyx", label: PowerChoice::BoostPlan.label(), hint: PowerChoice::BoostPlan.hint(), aliases: &["boost"] },
];

static CPU: [Choice; 3] = [
    Choice { key: "off", label: CpuLevel::Off.label(), hint: CpuLevel::Off.hint(), aliases: &[] },
    Choice { key: "boost", label: CpuLevel::Boost.label(), hint: CpuLevel::Boost.hint(), aliases: &[] },
    Choice { key: "max", label: CpuLevel::Max.label(), hint: CpuLevel::Max.hint(), aliases: &["max-sustained"] },
];

static GPU: [Choice; 3] = [
    Choice { key: "off", label: GpuLevel::Off.label(), hint: GpuLevel::Off.hint(), aliases: &[] },
    Choice { key: "safe", label: GpuLevel::Safe.label(), hint: GpuLevel::Safe.hint(), aliases: &[] },
    Choice { key: "balanced", label: GpuLevel::Balanced.label(), hint: GpuLevel::Balanced.hint(), aliases: &[] },
];

/// Every profile setting, in the order the classic Profiles form shows them.
pub static SETTINGS: &[Spec] = &[
    Spec::choice("priority", "Game priority", Hint::Selected, Group::Performance, false, Field::Priority, 0, 0, &PRIORITY, None),
    Spec::choice("power", "Power plan", Hint::Selected, Group::Performance, false, Field::Power, 1, 1, &POWER, None),
    Spec::choice("cpu", "CPU performance", Hint::Selected, Group::Performance, false, Field::Cpu, 3, 3, &CPU, Some("cpu-boost")),
    Spec::choice("gpu", "Graphics card tuning", Hint::GpuLive, Group::Performance, false, Field::Gpu, 2, 2, &GPU, None),
    Spec::switch(
        "io-boost",
        "Disk and memory priority for the game",
        "The game gets the disk first: less stutter while it streams textures",
        Group::Performance,
        false,
        Field::IoBoost,
        7,
        11,
    ),
    Spec::switch(
        "apps",
        "Handle my app rules",
        "Lower, pause or close the apps you chose in Apps",
        Group::AppsServicesMemory,
        false,
        Field::Apps,
        4,
        0,
    ),
    Spec::switch(
        "services",
        "Pause my chosen services",
        "Stopped for the session, started again afterwards",
        Group::AppsServicesMemory,
        true,
        Field::Services,
        5,
        1,
    ),
    Spec::switch(
        "lower-others",
        "Lower everything else",
        "Every other app of yours runs at the lowest priority",
        Group::AppsServicesMemory,
        false,
        Field::LowerOthers,
        16,
        10,
    ),
    Spec::switch(
        "clean-ram",
        "Free memory before the game starts",
        "Trims what background apps are holding",
        Group::AppsServicesMemory,
        false,
        Field::CleanRam,
        6,
        2,
    ),
    Spec::switch(
        "purge-standby",
        "Also flush cached memory",
        "Empties Windows' standby list so the game's memory is free at once",
        Group::AppsServicesMemory,
        true,
        Field::PurgeStandby,
        8,
        12,
    ),
    Spec::switch(
        "game-mode",
        "Windows Game Mode",
        "Let Windows itself favour the game and hold back updates and drivers",
        Group::Windows,
        false,
        Field::WindowsGameMode,
        9,
        3,
    ),
    Spec::switch(
        "dvr-off",
        "Background recording off",
        "Stops Game Bar capture from recording behind the game",
        Group::Windows,
        false,
        Field::GameDvrOff,
        10,
        4,
    ),
    Spec::switch(
        "notifications-off",
        "Quiet notifications",
        "No pop-ups on top of the game",
        Group::Windows,
        false,
        Field::NotificationsOff,
        11,
        5,
    ),
    Spec::switch(
        "no-throttling",
        "Keep Windows efficiency mode off for the game",
        "Stops Windows slowing the game down to save power",
        Group::Windows,
        false,
        Field::NoPowerThrottling,
        15,
        9,
    ),
    Spec::switch(
        "maintenance-off",
        "Pause automatic maintenance",
        "No scheduled defrag or scans during the game",
        Group::Windows,
        true,
        Field::PauseMaintenance,
        13,
        7,
    ),
    Spec::switch(
        "transparency-off",
        "Transparency effects off",
        "Saves a little graphics power on weak PCs",
        Group::Windows,
        false,
        Field::TransparencyOff,
        12,
        6,
    ),
    Spec::switch(
        "mmcss",
        "Multimedia scheduler favours games",
        "Windows' own game scheduling class, tuned up",
        Group::Windows,
        true,
        Field::MmcssGames,
        14,
        8,
    ),
    Spec::switch(
        "quiet-desktop",
        "Quiet desktop",
        "Taskbar off while you play; Ctrl+Alt+End brings it back",
        Group::Windows,
        false,
        Field::QuietDesktop,
        17,
        13,
    ),
];

/// One setting by its CLI key.
pub fn setting(key: &str) -> Option<&'static Spec> {
    SETTINGS.iter().find(|sp| sp.key == key)
}

/// `catalyx show` output: the same keys and order the CLI has always printed.
pub fn describe(s: &Settings) -> Vec<(&'static str, String)> {
    let mut rows: Vec<&Spec> = SETTINGS.iter().collect();
    rows.sort_by_key(|sp| sp.cli_order);
    rows.into_iter().map(|sp| (sp.key, sp.display(s))).collect()
}

/// The compact KEYS block of `catalyx help`, laid out exactly as the old hand-written text was.
pub fn keys_help() -> String {
    let choice = |sp: &Spec| match sp.kind {
        Kind::Choice(c) => format!("{} {}", sp.key, c.iter().map(|c| c.key).collect::<Vec<_>>().join("|")),
        Kind::Switch => sp.key.to_string(),
    };
    let mut choices: Vec<&Spec> = SETTINGS.iter().filter(|sp| matches!(sp.kind, Kind::Choice(_))).collect();
    let mut switches: Vec<&Spec> = SETTINGS.iter().filter(|sp| matches!(sp.kind, Kind::Switch)).collect();
    choices.sort_by_key(|sp| sp.help_order);
    switches.sort_by_key(|sp| sp.help_order);

    let mut out = String::new();
    // Three choice segments across the first row, at the columns the old text used.
    let mut first = String::from("  ");
    first.push_str(&format!("{:<39}", choices.first().map(|sp| choice(sp)).unwrap_or_default()));
    first.push_str(&format!("{:<26}", choices.get(1).map(|sp| choice(sp)).unwrap_or_default()));
    first.push_str(choices.get(2).map(|sp| choice(sp)).unwrap_or_default().as_str());
    out.push_str(first.trim_end());
    out.push('\n');

    // The fourth choice starts the second row; the switches flow after it and wrap at the old width.
    // The choice column is fixed and already ends in the gap, so the first switch follows it directly.
    let fourth = choices.get(3).map(|sp| choice(sp)).unwrap_or_default();
    let mut line = format!("  {:<39}", fourth);
    let mut col = line.len();
    for (n, sp) in switches.into_iter().enumerate() {
        let key = sp.key;
        let sep = usize::from(n > 0);
        if col + sep + key.len() > 117 {
            out.push_str(line.trim_end());
            out.push('\n');
            line = format!("  {key}");
            col = line.len();
        } else {
            if n > 0 {
                line.push(' ');
            }
            line.push_str(key);
            col += sep + key.len();
        }
    }
    line.push_str("   (on|off)");
    out.push_str(&line);
    out.push('\n');
    out
}

/// The app options (the Settings page's Behaviour card and the CLI `option` command).
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum OptionField {
    RestoreWhenGameCloses,
    ReopenClosedApps,
    KeepAppsOff,
    AutoBoostKnownGames,
    RunAsAdmin,
    ConfirmBoost,
}

impl OptionField {
    fn get(self, c: &Config) -> bool {
        match self {
            OptionField::RestoreWhenGameCloses => c.restore_when_game_closes,
            OptionField::ReopenClosedApps => c.reopen_closed_apps,
            OptionField::KeepAppsOff => c.keep_apps_off,
            OptionField::AutoBoostKnownGames => c.auto_boost_known_games,
            OptionField::RunAsAdmin => c.run_as_admin,
            OptionField::ConfirmBoost => c.confirm_boost,
        }
    }

    fn set(self, c: &mut Config, on: bool) {
        match self {
            OptionField::RestoreWhenGameCloses => c.restore_when_game_closes = on,
            OptionField::ReopenClosedApps => c.reopen_closed_apps = on,
            OptionField::KeepAppsOff => c.keep_apps_off = on,
            OptionField::AutoBoostKnownGames => c.auto_boost_known_games = on,
            OptionField::RunAsAdmin => c.run_as_admin = on,
            OptionField::ConfirmBoost => c.confirm_boost = on,
        }
    }
}

/// One app option, in the Settings page's order.
#[derive(Clone, Copy)]
pub struct OptionSpec {
    pub key: &'static str,
    pub label: &'static str,
    pub hint: &'static str,
    pub cli_order: u8,
    field: OptionField,
    /// The Settings page card this row belongs to.
    pub under_admin: bool,
}

impl OptionSpec {
    const fn new(
        key: &'static str,
        label: &'static str,
        hint: &'static str,
        field: OptionField,
        cli_order: u8,
        under_admin: bool,
    ) -> OptionSpec {
        OptionSpec { key, label, hint, cli_order, field, under_admin }
    }

    pub fn get(&self, c: &Config) -> bool {
        self.field.get(c)
    }

    pub fn set(&self, c: &mut Config, on: bool) {
        self.field.set(c, on);
    }
}

pub static OPTIONS: &[OptionSpec] = &[
    OptionSpec::new(
        "restore-on-exit",
        "Restore when the game closes",
        "Everything is put back automatically, after a short grace period",
        OptionField::RestoreWhenGameCloses,
        0,
        false,
    ),
    OptionSpec::new(
        "keep-off",
        "Keep chosen apps and services off",
        "If one starts again while you play, it is closed or stopped again",
        OptionField::KeepAppsOff,
        2,
        false,
    ),
    OptionSpec::new(
        "reopen-apps",
        "Reopen closed apps afterwards",
        "Closed apps start again, but unsaved work is gone either way",
        OptionField::ReopenClosedApps,
        1,
        false,
    ),
    OptionSpec::new(
        "confirm-boost",
        "Ask before boosting",
        "A short summary of what is about to change, with a way to switch it off",
        OptionField::ConfirmBoost,
        5,
        false,
    ),
    OptionSpec::new(
        "auto-boost",
        "Boost known games when they start",
        "Boosts games started outside CatalyX while this window is open",
        OptionField::AutoBoostKnownGames,
        3,
        false,
    ),
    OptionSpec::new(
        "run-as-admin",
        "Open CatalyX as administrator",
        "Needed to stop services and to boost games that run as administrator",
        OptionField::RunAsAdmin,
        4,
        true,
    ),
];

pub fn option(key: &str) -> Option<&'static OptionSpec> {
    OPTIONS.iter().find(|o| o.key == key)
}

/// `catalyx option` keys in the order the help text and its error message list them.
pub fn option_keys() -> String {
    let mut rows: Vec<&OptionSpec> = OPTIONS.iter().collect();
    rows.sort_by_key(|o| o.cli_order);
    rows.into_iter().map(|o| o.key).collect::<Vec<_>>().join(", ")
}

/// The shared switch parser: the CLI has always accepted these spellings.
pub fn parse_on(v: &str) -> Result<bool, String> {
    match v.to_lowercase().as_str() {
        "on" | "yes" | "true" | "1" => Ok(true),
        "off" | "no" | "false" | "0" => Ok(false),
        o => Err(format!("expected on or off, got '{o}'")),
    }
}

pub fn onoff(b: bool) -> &'static str {
    if b {
        "on"
    } else {
        "off"
    }
}

/// Applies one `catalyx set` value, with the CLI's keys, aliases and error wording.
pub fn set(s: &mut Settings, key: &str, value: &str) -> Result<(), String> {
    let key = key.to_lowercase();
    let spec = SETTINGS.iter().find(|sp| sp.key == key || sp.switch_alias == Some(key.as_str()));
    let Some(spec) = spec else {
        // The old code parsed the value before it looked the key up; keep that error order.
        parse_on(value)?;
        return Err(format!("unknown setting '{key}' (see `catalyx help`)"));
    };
    if spec.switch_alias == Some(key.as_str()) {
        // `cpu-boost` was an on/off alias for the CPU choice; it only ever meant these two values.
        let on = parse_on(value)?;
        spec.set(s, Value::Choice(if on { "boost" } else { "off" }));
        return Ok(());
    }
    match spec.kind {
        Kind::Switch => {
            let on = parse_on(value)?;
            spec.set(s, Value::Switch(on));
        }
        Kind::Choice(choices) => {
            let v = value.to_lowercase();
            let Some(c) = choices.iter().find(|c| c.key == v || c.aliases.contains(&v.as_str())) else {
                let keys: Vec<&str> = choices.iter().map(|c| c.key).collect();
                let note = if spec.field == Field::Priority { "; realtime can freeze Windows input" } else { "" };
                return Err(format!("{}: {} (got '{v}'){note}", spec.key, keys_or(&keys)));
            };
            spec.set(s, Value::Choice(c.key));
        }
    }
    Ok(())
}

/// "a, b or c": the old wording of a choice error, kept word for word.
fn keys_or(keys: &[&str]) -> String {
    match keys {
        [] => String::new(),
        [one] => (*one).to_string(),
        [head @ .., last] => format!("{} or {last}", head.join(", ")),
    }
}

/// Applies one `catalyx option` value, with the CLI's keys and error wording.
pub fn set_option(c: &mut Config, key: &str, value: &str) -> Result<(), String> {
    let key = key.to_lowercase();
    // As with `set`, the old code parsed the value before it looked the name up.
    let on = parse_on(value)?;
    let Some(spec) = option(&key) else {
        return Err(format!("unknown option '{key}' ({})", option_keys()));
    };
    spec.set(c, on);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_catalogue_lists_every_setting_exactly_once() {
        assert_eq!(SETTINGS.len(), 18, "one row per Settings field");
        let mut keys: Vec<&str> = SETTINGS.iter().map(|s| s.key).collect();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), SETTINGS.len(), "keys must be unique");
        let mut fields: Vec<Field> = SETTINGS.iter().map(|s| s.field).collect();
        fields.sort_by_key(|f| format!("{f:?}"));
        fields.dedup();
        assert_eq!(fields.len(), 18, "every field has a row");
        for sp in SETTINGS {
            assert!(!sp.label.is_empty() && !sp.key.is_empty());
            if let Kind::Choice(c) = sp.kind {
                assert!(c.len() >= 2, "{} offers a real choice", sp.key);
            }
        }
        assert_eq!(OPTIONS.len(), 6, "one row per app option");
    }

    #[test]
    fn describe_keeps_the_clis_old_order_and_values() {
        let d = describe(&Settings::default());
        let keys: Vec<&str> = d.iter().map(|(k, _)| *k).collect();
        assert_eq!(
            keys,
            [
                "priority",
                "power",
                "gpu",
                "cpu",
                "apps",
                "services",
                "clean-ram",
                "io-boost",
                "purge-standby",
                "game-mode",
                "dvr-off",
                "notifications-off",
                "transparency-off",
                "maintenance-off",
                "mmcss",
                "no-throttling",
                "lower-others",
                "quiet-desktop"
            ]
        );
        assert_eq!(d[0].1, "Above normal");
        assert_eq!(d[1].1, "CatalyX plan");
        assert_eq!(d[3].1, "Boost");
        assert_eq!(d[4].1, "on");
        assert_eq!(d[8].1, "off");
    }

    #[test]
    fn set_accepts_the_old_keys_and_aliases() {
        let mut s = Settings::default();
        set(&mut s, "priority", "above-normal").unwrap();
        assert_eq!(s.priority, Priority::AboveNormal);
        set(&mut s, "power", "boost").unwrap();
        assert_eq!(s.power, PowerChoice::BoostPlan);
        set(&mut s, "cpu", "max-sustained").unwrap();
        assert_eq!(s.cpu, CpuLevel::Max);
        set(&mut s, "cpu-boost", "off").unwrap();
        assert_eq!(s.cpu, CpuLevel::Off);
        set(&mut s, "cpu-boost", "on").unwrap();
        assert_eq!(s.cpu, CpuLevel::Boost);
        set(&mut s, "gpu", "safe").unwrap();
        assert_eq!(s.gpu, GpuLevel::Safe);
        set(&mut s, "quiet-desktop", "yes").unwrap();
        assert!(s.quiet_desktop);
        set(&mut s, "quiet-desktop", "0").unwrap();
        assert!(!s.quiet_desktop);

        assert_eq!(
            set(&mut s, "priority", "realtime").unwrap_err(),
            "priority: normal, above or high (got 'realtime'); realtime can freeze Windows input"
        );
        assert_eq!(set(&mut s, "power", "wat").unwrap_err(), "power: keep, high or catalyx (got 'wat')");
        assert_eq!(set(&mut s, "cpu", "wat").unwrap_err(), "cpu: off, boost or max (got 'wat')");
        assert_eq!(set(&mut s, "gpu", "wat").unwrap_err(), "gpu: off, safe or balanced (got 'wat')");
        assert_eq!(set(&mut s, "apps", "maybe").unwrap_err(), "expected on or off, got 'maybe'");
        assert_eq!(set(&mut s, "nope", "maybe").unwrap_err(), "expected on or off, got 'maybe'", "the old error order");
        assert_eq!(set(&mut s, "nope", "on").unwrap_err(), "unknown setting 'nope' (see `catalyx help`)");
    }

    #[test]
    fn the_legacy_realtime_priority_still_reads_correctly() {
        let s = Settings { priority: Priority::Realtime, ..Default::default() };
        let d = describe(&s);
        assert_eq!(d[0].1, "Realtime (risky)");
    }

    #[test]
    fn the_keys_block_matches_the_old_help_text() {
        let text = keys_help();
        let lines: Vec<&str> = text.lines().collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0], "  priority normal|above|high             power keep|high|catalyx   gpu off|safe|balanced");
        assert_eq!(
            lines[1],
            "  cpu off|boost|max                      apps services clean-ram game-mode dvr-off notifications-off transparency-off"
        );
        assert_eq!(lines[2], "  maintenance-off mmcss no-throttling lower-others io-boost purge-standby quiet-desktop   (on|off)");
    }

    #[test]
    fn app_options_read_and_write_the_config() {
        let mut c = Config::default();
        assert!(option("confirm-boost").unwrap().get(&c));
        set_option(&mut c, "confirm-boost", "off").unwrap();
        assert!(!c.confirm_boost);
        set_option(&mut c, "run-as-admin", "no").unwrap();
        assert!(!c.run_as_admin);
        assert_eq!(option_keys(), "restore-on-exit, reopen-apps, keep-off, auto-boost, run-as-admin, confirm-boost");
        assert_eq!(
            set_option(&mut c, "nope", "on").unwrap_err(),
            "unknown option 'nope' (restore-on-exit, reopen-apps, keep-off, auto-boost, run-as-admin, confirm-boost)"
        );
        assert_eq!(set_option(&mut c, "nope", "wat").unwrap_err(), "expected on or off, got 'wat'");
    }
}
