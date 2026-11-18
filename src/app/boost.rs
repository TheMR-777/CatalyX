//! The boost, described once for any skin: what it will change, the stages it goes through, how far
//! along it is, and what needs attention. Pure functions of (config, engine status, cached service
//! facts); `State` recomputes this only when one of those inputs changes, never per frame.
//!
//! Wording that is only the classic skin's (a dial's "click to restore", the launch bar's labels)
//! stays in the skin. The facts and the plan's final text live here so a completely different skin can
//! show the same boost without re-deriving any of it.

use crate::config::{Config, CpuLevel, Game, GpuLevel, PowerChoice, ProcAction, Settings};
use crate::engine::{Phase, Status};
use crate::services::ServiceInfo;

/// What a boost will change, in plain English: the lines for what is switched on and how many of the
/// options are off. The Boost page and the summary before a boost show exactly this.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub lines: Vec<String>,
    pub off: usize,
}

/// One group of work a boost does, and whether it is done yet. Labels are plain English and final.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stage {
    pub label: &'static str,
    pub done: bool,
}

/// What needs the person's attention. The skin chooses how to show it and which action to offer;
/// the text is final.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoticeKind {
    /// A previous boost left changes behind (a crashed or interrupted session).
    Interrupted,
    /// Windows is not letting CatalyX stop services or start elevated games.
    NotAdmin,
    /// The profile has no apps or services chosen for this scope.
    NothingChosen,
    /// The library is empty.
    NoGame,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Notice {
    pub kind: NoticeKind,
    pub text: &'static str,
}

/// Everything a skin needs to show the selected boost without asking Windows or walking the config
/// itself. `progress` is an estimate for a dial or a bar (0 idle, 1 boosted); the stages are the facts.
#[derive(Clone, Debug, Default)]
pub struct BoostView {
    /// The selected game's name, or "Gaming mode".
    pub name: String,
    /// The profile label: "Own settings", a preset's name, or the default preset's name.
    pub profile: String,
    pub has_game: bool,
    pub stages: Vec<Stage>,
    pub progress: f32,
    pub plan: Plan,
    pub notices: Vec<Notice>,
}

/// Builds the view for the selected game. Called when the config, the engine status, the service list
/// or the administrator state changed; the caller caches the result.
pub fn build(cfg: &Config, status: &Status, services: &[ServiceInfo], admin: bool) -> BoostView {
    let game = cfg.selected();
    let settings = cfg.settings_for(game);
    let rules = cfg.apps_for(game);
    let n_rules = rules.values().filter(|a| **a != ProcAction::Keep).count();
    let n_svc = cfg.services_for(game).values().filter(|v| **v).count();
    let k = &status.counts;
    let phase = status.phase;
    let applied = matches!(phase, Phase::Finding | Phase::Running | Phase::Boosted | Phase::Restoring);
    let finished = matches!(phase, Phase::Running | Phase::Boosted);

    let mut stages: Vec<Stage> = Vec::new();
    if settings.power != PowerChoice::Keep || settings.cpu != CpuLevel::Off {
        stages.push(Stage { label: "Power plan", done: applied || !k.power_plan.is_empty() });
    }
    if settings.windows_game_mode
        || settings.game_dvr_off
        || settings.notifications_off
        || settings.transparency_off
        || settings.pause_maintenance
        || settings.mmcss_games
    {
        stages.push(Stage { label: "Windows", done: applied || k.tweaks > 0 });
    }
    if (settings.apps && n_rules > 0) || (settings.services && n_svc > 0) || settings.lower_others {
        let done = applied || k.apps_closed + k.apps_paused + k.apps_lowered + k.services_stopped > 0;
        stages.push(Stage { label: "Apps and services", done });
    }
    if settings.clean_ram || settings.purge_standby {
        stages.push(Stage { label: "Memory", done: applied || k.ram_freed > 0 || k.standby_purged });
    }
    if settings.gpu != GpuLevel::Off {
        stages.push(Stage { label: "Graphics card", done: applied || k.gpu_core_mhz > 0 });
    }
    if game.is_some() {
        stages.push(Stage { label: "Game", done: finished || status.game_pid != 0 });
    }
    let done = stages.iter().filter(|s| s.done).count();

    BoostView {
        name: game.map(|g| g.name.clone()).unwrap_or_else(|| "Gaming mode".into()),
        profile: cfg.profile_label(game),
        has_game: game.is_some(),
        progress: progress_for(phase, done, stages.len()),
        stages,
        plan: plan(cfg, game, services, admin),
        notices: notices(cfg, status, admin),
    }
}

/// How far along a boost is. Pure, so it can be tested: applying fills 8-80% as stages finish, waiting
/// for the game is 85%, a running boost is 100%, and restoring falls back to 40%. An estimate for a
/// dial or a bar; the stages are the facts a skin may show instead.
pub(crate) fn progress_for(phase: Phase, done: usize, total: usize) -> f32 {
    match phase {
        Phase::Idle => 0.0,
        Phase::Boosting => 0.08 + 0.72 * done as f32 / total.max(1) as f32,
        Phase::Finding => 0.85,
        Phase::Running | Phase::Boosted => 1.0,
        Phase::Restoring => 0.4,
    }
}

/// What a boost with these settings will do: the lines for what is switched on, and how many options
/// are off. `services` is the last service list, whose `can_start` facts are cached: no Windows call
/// is made here, and the caller recomputes when the list or the rules change.
pub fn plan(cfg: &Config, game: Option<&Game>, services: &[ServiceInfo], admin: bool) -> Plan {
    let s = cfg.settings_for(game);
    let rules = cfg.apps_for(game);
    let count = |a: ProcAction| rules.values().filter(|x| **x == a).count();
    let n_svc = cfg.services_for(game).iter().filter(|(name, on)| service_stops(services, name, **on)).count();
    let mut lines: Vec<String> = Vec::new();
    let mut off = 0;
    let mut add = |enabled: bool, text: String| {
        if enabled {
            lines.push(text)
        } else {
            off += 1
        }
    };
    add(s.power != PowerChoice::Keep || s.cpu != CpuLevel::Off, power_summary(&s));
    add(true, format!("{} game priority{}", s.priority.label(), if s.io_boost { " + disk" } else { "" }));
    let (lo, pa, cl) = (count(ProcAction::Lower), count(ProcAction::Suspend), count(ProcAction::Close));
    add(s.apps && lo + pa + cl > 0, format!("Apps: {lo} lowered, {pa} paused, {cl} closed"));
    add(s.services && n_svc > 0, format!("{n_svc} services paused{}", if admin { "" } else { " (needs admin)" }));
    add(s.lower_others, "Everything else at lowest priority".into());
    add(s.clean_ram, format!("Free memory first{}", if s.purge_standby { " + cached memory" } else { "" }));
    add(s.windows_game_mode, "Windows Game Mode on".into());
    add(s.notifications_off || s.game_dvr_off, "Quiet notifications and recording".into());
    add(s.gpu != GpuLevel::Off, format!("Graphics card: {}", s.gpu.label().to_lowercase()));
    // The desktop only goes away while a game runs: there is nothing to come back to in gaming mode.
    add(s.quiet_desktop && game.is_some(), "Taskbar off while you play (Ctrl+Alt+End brings it back)".into());
    Plan { lines, off }
}

/// The power half of a profile, in one phrase. Used by the plan and by a skin's profile summary.
pub fn power_summary(s: &Settings) -> String {
    let plan = match s.power {
        PowerChoice::Keep => "your power plan",
        PowerChoice::HighPerformance => "High performance plan",
        PowerChoice::BoostPlan => "CatalyX power plan",
    };
    match s.cpu {
        CpuLevel::Off => plan.to_string(),
        c => format!("{plan}, CPU {}", c.label().split(' ').next().unwrap_or("").to_lowercase()),
    }
}

/// Only what needs attention, in the order a person should read it.
fn notices(cfg: &Config, status: &Status, admin: bool) -> Vec<Notice> {
    let game = cfg.selected();
    let rules = cfg.apps_for(game);
    let none_apps = rules.values().all(|a| *a == ProcAction::Keep);
    let none_svcs = !cfg.services_for(game).values().any(|v| *v);
    let idle = status.phase == Phase::Idle;
    let mut rows = Vec::new();
    if status.pending_journal && idle {
        rows.push(Notice { kind: NoticeKind::Interrupted, text: "A previous boost was interrupted: some changes may still be in place" });
    }
    if !admin {
        rows.push(Notice { kind: NoticeKind::NotAdmin, text: "Not running as administrator: services and elevated games are off-limits" });
    }
    if none_apps && none_svcs && idle {
        rows.push(Notice { kind: NoticeKind::NothingChosen, text: "No apps or services chosen yet" });
    }
    if cfg.games.is_empty() {
        rows.push(Notice { kind: NoticeKind::NoGame, text: "No game added yet" });
    }
    rows
}

/// Whether Windows lets CatalyX start this service again, from the last service list. Before that
/// list has arrived there is no reason to think it cannot: the engine still checks before stopping.
pub(crate) fn service_can_start(services: &[ServiceInfo], name: &str) -> bool {
    services.iter().find(|s| s.name.eq_ignore_ascii_case(name)).map(|s| s.can_start).unwrap_or(true)
}

/// Whether a chosen service will really be paused for the session: not one Windows protects, and one
/// it can start again. One rule for the plan, the summary and a skin's service list.
pub(crate) fn service_stops(services: &[ServiceInfo], name: &str, chosen: bool) -> bool {
    chosen && !crate::config::service_is_protected(name) && service_can_start(services, name)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn info(name: &str, can_start: bool) -> ServiceInfo {
        ServiceInfo { name: name.into(), display: name.into(), running: true, can_stop: true, can_start }
    }

    fn status(phase: Phase) -> Status {
        Status { phase, pending_journal: false, ..Status::default() }
    }

    fn default_settings(cfg: &mut Config) -> Settings {
        let id = cfg.default_preset.clone();
        let s = cfg.presets.iter_mut().find(|p| p.id == id).expect("the default preset exists");
        s.settings = Settings::default();
        s.settings.clone()
    }

    #[test]
    fn progress_rises_while_applying_and_ends_full() {
        assert_eq!(progress_for(Phase::Idle, 0, 5), 0.0);
        let mut last = 0.0;
        for done in 0..=5 {
            let p = progress_for(Phase::Boosting, done, 5);
            assert!(p > last || done == 0, "progress must not go backwards");
            assert!(p <= 0.8 + 1e-6, "applying never looks finished");
            last = p;
        }
        assert!(progress_for(Phase::Finding, 5, 5) > progress_for(Phase::Boosting, 5, 5));
        assert_eq!(progress_for(Phase::Running, 5, 5), 1.0);
        assert_eq!(progress_for(Phase::Boosted, 0, 0), 1.0);
        assert!(progress_for(Phase::Restoring, 5, 5) < 1.0);
        // A profile with no stages at all must not divide by zero.
        assert!(progress_for(Phase::Boosting, 0, 0).is_finite());
    }

    #[test]
    fn the_plan_lists_only_what_is_switched_on_and_counts_the_rest() {
        let mut cfg = Config::default();
        let s = default_settings(&mut cfg);
        let p = plan(&cfg, None, &[], false);
        assert_eq!(
            p.lines,
            vec![
                "CatalyX power plan, CPU boost",
                "Above normal game priority + disk",
                "Windows Game Mode on",
                "Quiet notifications and recording",
            ]
        );
        assert_eq!(p.off, 6, "the other six switches are honestly counted");
        assert_eq!(power_summary(&s), "CatalyX power plan, CPU boost");
    }

    #[test]
    fn the_plan_counts_only_services_windows_lets_us_start_again() {
        let mut cfg = Config::default();
        default_settings(&mut cfg);
        cfg.services.insert("CatalyX Test Service".into(), true);

        let blocked = [info("CatalyX Test Service", false)];
        let p = plan(&cfg, None, &blocked, true);
        assert!(p.lines.iter().all(|l| !l.contains("services paused")), "a service that cannot come back is not promised");

        let allowed = [info("CatalyX Test Service", true)];
        let p = plan(&cfg, None, &allowed, false);
        assert!(p.lines.iter().any(|l| l == "1 services paused (needs admin)"));
    }

    #[test]
    fn the_quiet_desktop_line_needs_a_game() {
        let mut cfg = Config::default();
        let mut s = default_settings(&mut cfg);
        s.quiet_desktop = true;
        let id = cfg.default_preset.clone();
        cfg.presets.iter_mut().find(|p| p.id == id).unwrap().settings = s;

        assert!(plan(&cfg, None, &[], true).lines.iter().all(|l| !l.contains("Taskbar off")));
        cfg.games.push(Game { name: "One".into(), ..Default::default() });
        assert!(plan(&cfg, cfg.selected(), &[], true).lines.iter().any(|l| l.contains("Taskbar off")));
    }

    #[test]
    fn notices_say_what_needs_attention_in_reading_order() {
        let mut cfg = Config::default();
        default_settings(&mut cfg);
        let mut st = status(Phase::Idle);
        st.pending_journal = true;
        let rows = notices(&cfg, &st, false);
        let kinds: Vec<_> = rows.iter().map(|n| n.kind).collect();
        assert_eq!(kinds, [NoticeKind::Interrupted, NoticeKind::NotAdmin, NoticeKind::NothingChosen, NoticeKind::NoGame]);

        // The first game removes the library notice; a fresh, idle profile still has nothing chosen.
        cfg.games.push(Game { name: "One".into(), ..Default::default() });
        let rows = notices(&cfg, &status(Phase::Idle), true);
        assert!(rows.iter().all(|n| n.kind != NoticeKind::Interrupted), "no interrupted boost to restore");
        assert!(rows.iter().all(|n| n.kind != NoticeKind::NoGame));
        assert!(rows.iter().any(|n| n.kind == NoticeKind::NothingChosen));
    }

    #[test]
    fn the_view_reports_the_stages_the_profile_will_go_through() {
        let mut cfg = Config::default();
        default_settings(&mut cfg);
        cfg.games.push(Game { name: "One".into(), ..Default::default() });
        let v = build(&cfg, &status(Phase::Idle), &[], true);
        let labels: Vec<_> = v.stages.iter().map(|s| s.label).collect();
        assert_eq!(labels, ["Power plan", "Windows", "Game"]);
        assert!(v.stages.iter().all(|s| !s.done));
        assert_eq!(v.progress, 0.0);
        assert_eq!(v.name, "One");
        assert_eq!(v.profile, "Balanced");
        assert!(v.has_game);

        let v = build(&cfg, &status(Phase::Running), &[], true);
        assert!(v.stages.iter().all(|s| s.done), "a running boost has finished every stage");
        assert_eq!(v.progress, 1.0);
    }
}
