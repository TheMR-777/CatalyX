//! The individual changes a boost makes (power plan, Windows tweaks, services, apps, memory, graphics
//! card). Each one is journaled before it is made and undone by `restore`.

use super::*;

impl Engine {
    pub(super) fn apply_power(&mut self, s: &Settings) {
        let want_cpu = s.cpu != CpuLevel::Off;
        if s.power == PowerChoice::Keep && !want_cpu {
            return;
        }
        let Some(prev) = power::active() else {
            self.log("Power plan unchanged: could not read the active plan for recovery");
            return;
        };
        // A CPU level always needs CatalyX's own plan: it is where the processor settings are
        // written, so the person's own plan is never edited.
        let own_plan = s.power == PowerChoice::BoostPlan || want_cpu;
        let (label, source) = if own_plan {
            ("CatalyX plan", if s.power == PowerChoice::HighPerformance { power::HIGH_PERFORMANCE.to_string() } else { prev.clone() })
        } else {
            ("High performance", power::HIGH_PERFORMANCE.to_string())
        };
        let cpu_note = if want_cpu { format!(", CPU {}", s.cpu.label()) } else { String::new() };
        if self.dry() {
            self.log(format!("Would switch power plan to {label}{cpu_note}"));
            self.counts(|c| {
                c.power_plan = label.into();
                c.cpu_level = if want_cpu { s.cpu.label().into() } else { String::new() };
                c.planned += 1;
            });
            return;
        }
        let mut target = if own_plan { power::BOOST_PLAN.to_string() } else { power::HIGH_PERFORMANCE.to_string() };
        if own_plan && !power::ensure_boost_plan(s.cpu, &source) {
            self.log("Could not set the CatalyX processor settings; trying High performance instead (CPU tuning not verified)");
            target = power::HIGH_PERFORMANCE.to_string();
        }
        if prev.eq_ignore_ascii_case(&target) {
            self.log(format!("Power plan already {}", if target == power::BOOST_PLAN { label } else { "High performance" }));
            self.counts(|c| c.power_plan = if target == power::BOOST_PLAN { label.into() } else { "High performance".into() });
            return;
        }
        if !self.push_undo(Undo::Power { prev }) {
            return;
        }
        if power::set_active(&target) {
            // Read the processor settings back so "on" means "on".
            let verified = if want_cpu && target.eq_ignore_ascii_case(power::BOOST_PLAN) {
                power::query_ac(power::BOOST_PLAN, power::SUB_PROCESSOR, power::PERFBOOSTMODE) == Some(2)
                    && power::query_ac(power::BOOST_PLAN, power::SUB_PROCESSOR, power::CPMINCORES) == Some(100)
            } else {
                false
            };
            let applied_label = if target == power::BOOST_PLAN { label } else { "High performance" };
            self.log(format!(
                "Power plan: {applied_label}{}{}",
                if verified { cpu_note.as_str() } else { "" },
                if verified {
                    " (checked: turbo aggressive, cores unparked)"
                } else if want_cpu {
                    " (CPU tuning unavailable on this PC)"
                } else {
                    ""
                }
            ));
            self.counts(|c| {
                c.power_plan = applied_label.into();
                if verified {
                    c.cpu_level = s.cpu.label().into();
                }
            });
        } else {
            self.log("Could not change the power plan");
        }
    }
    pub(super) fn set_reg(&mut self, sets: Vec<RegSet>, label: &str) {
        if self.dry() {
            self.log(format!("Would turn on: {label}"));
            self.counts(|c| {
                c.tweaks += 1;
                c.planned += 1;
            });
            return;
        }
        let mut ok = true;
        for r in sets {
            let prev = match tweaks::read(r.hive, r.path, r.name) {
                Ok(prev) => prev,
                Err(e) => {
                    self.log(format!("Windows: {label} skipped {} (cannot save old value: {e})", r.name));
                    ok = false;
                    continue;
                }
            };
            if prev.as_ref() == Some(&r.value) {
                continue;
            }
            if !self.push_undo(Undo::Reg { hive: r.hive, path: r.path.into(), name: r.name.into(), prev }) {
                ok = false;
                continue;
            }
            if tweaks::write(r.hive, r.path, r.name, &r.value).is_err() {
                ok = false;
                self.forget_last_undo();
            }
        }
        if ok {
            self.log(format!("Windows: {label}"));
            self.counts(|c| c.tweaks += 1);
        } else {
            self.log(format!("Windows: {label} needs administrator rights"));
        }
    }

    pub(super) fn apply_tweaks(&mut self, s: &Settings) {
        if s.windows_game_mode {
            self.set_reg(tweaks::game_mode(), "Game Mode on (let Windows prioritise the game)");
        }
        if s.game_dvr_off {
            self.set_reg(tweaks::game_dvr_off(), "background game recording off");
        }
        if s.notifications_off {
            self.set_reg(tweaks::notifications_off(), "pop-up notifications off");
        }
        if s.transparency_off {
            self.set_reg(tweaks::transparency_off(), "transparency effects off");
        }
        if s.mmcss_games {
            self.set_reg(tweaks::mmcss_games(), "multimedia scheduler favours games");
        }
        if s.pause_maintenance {
            self.set_reg(tweaks::maintenance_off(), "automatic maintenance paused");
            if self.dry() {
                self.log("Would pause scheduled defrag");
                self.counts(|c| c.planned += 1);
            } else if tweaks::task_enabled(tweaks::DEFRAG_TASK) == Some(true)
                && self.push_undo(Undo::Task { name: tweaks::DEFRAG_TASK.into(), was_enabled: true })
            {
                if tweaks::set_task(tweaks::DEFRAG_TASK, false) {
                    self.log("Windows: scheduled defrag paused");
                } else {
                    self.forget_last_undo();
                }
            }
        }
    }
    pub(super) fn apply_services(&mut self, list: &BTreeMap<String, bool>) {
        if !self.dry() && !crate::ffi::is_admin() {
            let n = list.values().filter(|on| **on).count();
            self.log(format!("Services: {n} chosen but skipped - stopping a Windows service needs administrator rights"));
            return;
        }
        let running = services::list();
        let mut stopped = 0;
        for svc in running.iter().filter(|s| s.running) {
            let want = list.iter().any(|(n, on)| *on && n.eq_ignore_ascii_case(&svc.name));
            if !want || service_is_protected(&svc.name) || !svc.can_stop {
                continue;
            }
            let already = self.session.as_ref().map(|s| s.stopped_services.contains(&svc.name)).unwrap_or(false);
            if already {
                continue; // stopped earlier in this loop as another service's dependent
            }
            // A stop may complete even after its 3 s timeout. Record every running dependent
            // and the parent BEFORE asking Windows to stop any of them.
            let names: Vec<String> = services::running_dependents(&svc.name).into_iter().chain(std::iter::once(svc.name.clone())).collect();
            // A service that cannot be started again must never be stopped: Windows protects a
            // few of them (it lets anybody stop them, but lets nobody start them again), and
            // then the boost could never be undone. Leave those, and everything that depends on
            // them, running.
            let unstartable: Vec<&String> = names.iter().filter(|n| !services::can_start(n)).collect();
            if !unstartable.is_empty() {
                let shown = running
                    .iter()
                    .find(|r| r.name.eq_ignore_ascii_case(unstartable[0]))
                    .map(|r| r.display.clone())
                    .unwrap_or_else(|| unstartable[0].clone());
                self.log(format!("Service {shown}: Windows does not allow it to be started again, so it is left running"));
                continue;
            }
            if self.dry() {
                self.log(format!("Would stop service {}", svc.display));
                self.counts(|c| c.planned += 1);
                continue;
            }
            // Services stopped earlier in this boost keep the record they got then.
            let kept: Vec<String> =
                names.iter().filter(|n| self.session.as_ref().map(|s| s.stopped_services.contains(*n)).unwrap_or(false)).cloned().collect();
            let mut recorded = true;
            for name in &names {
                let known = kept.iter().any(|k| k.eq_ignore_ascii_case(name));
                if !known && !self.push_undo(Undo::Service { name: name.clone() }) {
                    recorded = false;
                    break;
                }
            }
            if !recorded {
                continue;
            }
            // Dependents first, 3 s per stop, and a service that timed out is skipped for the rest
            // of the day.
            match services::stop_with_dependents(&svc.name, service_is_protected) {
                Ok(done) => {
                    for name in &done {
                        stopped += 1;
                        if let Some(s) = &mut self.session {
                            s.stopped_services.insert(name.clone());
                        }
                        let shown =
                            running.iter().find(|r| r.name.eq_ignore_ascii_case(name)).map(|r| r.display.clone()).unwrap_or(name.clone());
                        if name.eq_ignore_ascii_case(&svc.name) {
                            self.log(format!("Stopped service {shown}"));
                        } else {
                            self.log(format!("Stopped service {shown} (it depends on {})", svc.display));
                        }
                    }
                    // A service that was recorded but did not stop gets its record back, so a
                    // failed stop can never leave a boost that "needs restoring" for ever.
                    self.forget_unstopped_services(&names, &done, &kept);
                }
                Err(e) => {
                    self.forget_unstopped_services(&names, &[], &kept);
                    self.log(format!("Service {}: {e}", svc.display));
                }
            }
        }
        self.counts(|c| c.services_stopped += stopped);
    }

    /// Takes the recovery records of services that were recorded but did not stop back out of the
    /// journal: nothing was changed, so there is nothing to undo.
    fn forget_unstopped_services(&mut self, names: &[String], done: &[String], kept: &[String]) {
        let missed: Vec<String> = names
            .iter()
            .filter(|n| !done.iter().any(|d| d.eq_ignore_ascii_case(n)) && !kept.iter().any(|k| k.eq_ignore_ascii_case(n)))
            .cloned()
            .collect();
        if missed.is_empty() {
            return;
        }
        self.forget_undos(|u| matches!(u, Undo::Service { name } if missed.iter().any(|m| m.eq_ignore_ascii_case(name))));
    }

    /// Pids that belong to the game or its launcher chain and must never be touched.
    pub(super) fn game_family(&self, snap: &Snapshot) -> HashSet<u32> {
        let mut set = HashSet::new();
        if let Some(s) = &self.session {
            // One parent table for the whole pass (descends_from would rebuild it per process).
            let parent: std::collections::HashMap<u32, u32> = snap.procs.iter().map(|p| (p.pid, p.parent)).collect();
            let walk_up = |from: u32, target: u32| -> bool {
                let mut cur = from;
                for _ in 0..32 {
                    if cur == target {
                        return true;
                    }
                    match parent.get(&cur) {
                        Some(&q) if q != 0 && q != cur => cur = q,
                        _ => return false,
                    }
                }
                false
            };
            for p in &snap.procs {
                if let Some(r) = s.root_pid {
                    // The launcher, everything it started, and what started the launcher.
                    if walk_up(p.pid, r) || (p.pid != 0 && walk_up(r, p.pid)) {
                        set.insert(p.pid);
                    }
                }
                if s.game_pid != 0 && (p.pid == s.game_pid || walk_up(p.pid, s.game_pid) || walk_up(s.game_pid, p.pid)) {
                    set.insert(p.pid);
                }
            }
            let real = s.req.game.as_ref().map(|g| g.real_exe.to_lowercase()).unwrap_or_default();
            if !real.is_empty() {
                for p in snap.procs.iter().filter(|p| p.name_lower == real) {
                    set.insert(p.pid);
                }
            }
        }
        set
    }

    pub(super) fn apply_apps(&mut self, list: &BTreeMap<String, ProcAction>, enforcing: bool) {
        let snap = self.snapshot();
        let family = self.game_family(&snap);
        let me = unsafe { crate::ffi::GetCurrentProcessId() };
        let (mut closed, mut paused, mut lowered) = (0, 0, 0);
        for p in &snap.procs {
            let Some(action) = list.get(&p.name_lower).copied() else { continue };
            if action == ProcAction::Keep || app_is_protected(&p.name_lower) || family.contains(&p.pid) || p.pid == me {
                continue;
            }
            let already = self.session.as_ref().map(|s| s.paused.contains(&p.pid)).unwrap_or(false);
            if enforcing && (action == ProcAction::Lower || already) {
                continue;
            }
            if self.dry() {
                if !enforcing {
                    self.log(format!("Would {} {}", action.label().to_lowercase(), p.name));
                    self.counts(|c| c.planned += 1);
                }
                continue;
            }
            match action {
                ProcAction::Close => {
                    let reopen = self.session.as_ref().map(|s| s.req.reopen_closed_apps).unwrap_or(false);
                    let known = self
                        .session
                        .as_ref()
                        .map(|s| s.undo.iter().any(|u| matches!(u, Undo::Reopen { exe, .. } if exe.eq_ignore_ascii_case(&p.exe))))
                        .unwrap_or(true);
                    // Closing is inherently destructive: re-open only when explicitly requested.
                    // Record it first, so a crash between ending the app and saving cannot lose it.
                    if reopen
                        && !enforcing
                        && !p.exe.is_empty()
                        && !known
                        && !self.push_undo(Undo::Reopen { exe: p.exe.clone(), pid: p.pid, start: p.start })
                    {
                        continue;
                    }
                    if procs::terminate(p.pid) {
                        closed += 1;
                        self.log(format!("{} {}", if enforcing { "Closed again" } else { "Closed" }, p.name));
                    }
                }
                ProcAction::Suspend => {
                    if !self.push_undo(Undo::Resume { pid: p.pid, name: p.name.clone(), start: p.start }) {
                        continue;
                    }
                    if procs::suspend(p.pid) {
                        paused += 1;
                        if let Some(s) = &mut self.session {
                            s.paused.insert(p.pid);
                        }
                        self.log(format!("Paused {}", p.name));
                    } else {
                        self.forget_last_undo();
                    }
                }
                ProcAction::Lower => {
                    if let Some(prev) = procs::get_priority(p.pid) {
                        if prev != crate::ffi::IDLE_PRIORITY_CLASS
                            && self.push_undo(Undo::Priority { pid: p.pid, class: prev, start: p.start })
                        {
                            if procs::set_priority(p.pid, crate::ffi::IDLE_PRIORITY_CLASS) {
                                procs::trim(p.pid);
                                lowered += 1;
                            } else {
                                self.forget_last_undo();
                            }
                        }
                    }
                }
                ProcAction::Keep => {}
            }
        }
        self.counts(|c| {
            c.apps_closed += closed;
            c.apps_paused += paused;
            c.apps_lowered += lowered;
        });
    }

    /// Lower everything else: every other app of the same Windows user goes to the lowest priority (class 0x40) and is put
    /// back afterwards. Skipped: Windows and shell parts, this app, launchers (steam.exe and
    /// everything it started), the game, and anything the person already gave an action in the list.
    pub(super) fn lower_others(&mut self, list: &BTreeMap<String, ProcAction>) {
        let snap = self.snapshot();
        let family = self.game_family(&snap);
        let me = unsafe { crate::ffi::GetCurrentProcessId() };
        let steam: Vec<u32> = snap.procs.iter().filter(|p| p.name_lower == "steam.exe").map(|p| p.pid).collect();
        let mut n = 0u32;
        // Decide first, record the whole group with a single journal write, then change.
        let mut plan: Vec<(u32, u32, u64)> = Vec::new(); // (pid, previous class, start)
        for p in &snap.procs {
            if p.pid == me
                || family.contains(&p.pid)
                || app_is_protected(&p.name_lower)
                || list.contains_key(&p.name_lower)
                || p.name_lower == "steam.exe"
                || steam.contains(&p.parent)
            {
                continue;
            }
            if self.dry() {
                n += 1;
                continue;
            }
            if p.start == 0 || !procs::owned_by_current_user(p.pid) {
                continue;
            }
            if let Some(prev) = procs::get_priority(p.pid) {
                if prev != crate::ffi::IDLE_PRIORITY_CLASS {
                    plan.push((p.pid, prev, p.start));
                }
            }
        }
        if !self.dry() && !plan.is_empty() {
            let records: Vec<Undo> = plan.iter().map(|&(pid, class, start)| Undo::Priority { pid, class, start }).collect();
            if self.push_undos(records) {
                let mut failed: Vec<u32> = Vec::new();
                for &(pid, _, _) in &plan {
                    if procs::set_priority(pid, crate::ffi::IDLE_PRIORITY_CLASS) {
                        n += 1;
                    } else {
                        failed.push(pid);
                    }
                }
                if !failed.is_empty() {
                    self.forget_undos(|u| matches!(u, Undo::Priority { pid, .. } if failed.contains(pid)));
                }
            }
        }
        if self.dry() {
            self.log(format!("Would lower {n} other apps to the lowest priority"));
            self.counts(|c| c.planned += 1);
        } else {
            self.log(format!("Lowered {n} other apps to the lowest priority"));
            self.counts(|c| c.apps_lowered += n);
        }
    }

    pub(super) fn clean_ram(&mut self) {
        let snap = self.snapshot();
        if self.dry() {
            self.log("Would free memory held by background apps");
            self.counts(|c| c.planned += 1);
            return;
        }
        let mut sys = sysinfo::System::new();
        sys.refresh_memory();
        let before = sys.used_memory();
        let family = self.game_family(&snap);
        let mut n = 0;
        // Every process this user may touch is trimmed (working set set to "as small as possible"); there
        // is no size threshold, because a small process costs nothing to trim.
        let keep = self.session.as_ref().map(|s| &s.req.apps);
        for p in &snap.procs {
            if family.contains(&p.pid)
                || app_is_protected(&p.name_lower)
                || keep.and_then(|rules| rules.get(&p.name_lower)) == Some(&ProcAction::Keep)
            {
                continue;
            }
            if procs::trim(p.pid) {
                n += 1;
            }
        }
        std::thread::sleep(Duration::from_millis(400));
        sys.refresh_memory();
        let freed = before.saturating_sub(sys.used_memory());
        self.log(format!("Freed {} of memory from {n} apps", procs::fmt_mem(freed)));
        self.counts(|c| c.ram_freed += freed);
    }

    pub(super) fn apply_gpu(&mut self, level: GpuLevel) {
        let st = gpu::probe();
        if !st.supported {
            self.log(format!("Graphics card tuning skipped: {}", if st.note.is_empty() { "not supported" } else { &st.note }));
            return;
        }
        let (c, m) = gpu::planned(&st, level);
        if c <= 0 && m <= 0 {
            self.log("Graphics card tuning skipped: no safe offset available");
            return;
        }
        let Some(t) = gpu::temperature() else {
            self.log("Graphics card tuning skipped: temperature sensor unavailable");
            return;
        };
        let limit = self.session.as_ref().map(|s| s.req.gpu_temp_limit).unwrap_or(83);
        if t.saturating_add(5) >= limit {
            self.log(format!("Graphics card is already warm ({t} °C); tuning skipped"));
            return;
        }
        if self.dry() {
            self.log(format!("Would raise graphics clocks: core +{} MHz, memory +{} MHz", c / 1000, m / 1000));
            self.counts(|x| {
                x.gpu_core_mhz = c / 1000;
                x.gpu_mem_mhz = m / 1000;
                x.planned += 1;
            });
            return;
        }
        if !self.push_undo(Undo::Gpu { core: st.core.current_khz, mem: st.mem.current_khz }) {
            return;
        }
        match gpu::apply(c, m) {
            Ok((_pc, _pm)) => {
                if let Some(s) = &mut self.session {
                    s.gpu_applied = true;
                }
                self.log(format!("Graphics card: core +{} MHz, memory +{} MHz", c / 1000, m / 1000));
                self.counts(|x| {
                    x.gpu_core_mhz = c / 1000;
                    x.gpu_mem_mhz = m / 1000;
                });
            }
            Err(e) => {
                // Read the card back: if its offsets are exactly what they were, nothing changed
                // (for example Windows refused because CatalyX is not an administrator), so there
                // is nothing to watch or undo. If they differ, a half-applied change is kept
                // under watch and its recovery entry stays.
                let now = gpu::probe();
                if now.core.current_khz == st.core.current_khz && now.mem.current_khz == st.mem.current_khz {
                    self.forget_last_undo();
                    self.log(format!("Graphics card tuning skipped: {e}"));
                } else {
                    if let Some(s) = &mut self.session {
                        s.gpu_applied = true;
                    }
                    self.log(format!("Graphics card tuning failed: {e}; recovery is still available"));
                }
            }
        }
    }
}
