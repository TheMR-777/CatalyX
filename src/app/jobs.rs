//! Slow work runs on short-lived threads (one per job, as the window did before); each result is stored
//! in the State by `State::finish` and wakes the skin.

use super::intent::{Event, Tone};
use super::state::State;
use crate::config::CpuLevel;
use crate::{elevate, ffi, gpu, launcher, library, power, services};
use std::sync::Arc;
use std::time::Instant;

/// A job: runs off every lock, returns what to store (None = nothing to report, nobody is woken).
pub(super) type Job = Box<dyn FnOnce() -> Option<Done> + Send>;

/// Results coming back from jobs.
pub(super) enum Done {
    Hardware {
        gpu: Box<gpu::GpuState>,
        plans: Vec<power::Plan>,
    },
    Services(Vec<services::ServiceInfo>),
    Installed(Vec<library::Installed>),
    Predicted {
        id: String,
        exe: String,
    },
    Candidates {
        id: String,
        list: Vec<(String, u64)>,
    },
    Info(String),
    Error(String),
    Elevation {
        ui_task: bool,
        watcher: bool,
    },
    EndedApp {
        name: String,
        count: usize,
    },
    /// Whether the elevated copy could be started.
    Relaunch(bool),
    /// A power-plan action: whether Windows accepted it, and whether it built CatalyX's plan
    /// (`created`) rather than switching to an existing one.
    PowerChanged {
        ok: bool,
        created: bool,
    },
}

impl State {
    pub(super) fn refresh_hardware(&mut self) {
        if self.hw_busy {
            return;
        }
        self.hw_busy = true;
        self.spawn(|| Some(Done::Hardware { gpu: Box::new(gpu::probe()), plans: power::list() }));
    }

    /// Which Task Scheduler entries exist (asked off the UI thread; it runs a helper program).
    pub(super) fn refresh_elevation(&mut self) {
        if self.system.elevation_busy {
            return;
        }
        self.system.elevation_busy = true;
        self.rev += 1;
        self.spawn(|| Some(Done::Elevation { ui_task: elevate::ui_task_ready(), watcher: elevate::watcher_enabled() }));
    }

    pub(super) fn refresh_services(&mut self) {
        if self.svc_busy {
            return;
        }
        self.svc_busy = true;
        self.last_svc = Some(Instant::now());
        self.spawn(|| Some(Done::Services(services::list_with_start())));
    }

    pub(super) fn scan_library(&mut self) {
        if self.library.scanning {
            return;
        }
        Arc::make_mut(&mut self.library).scanning = true;
        self.rev += 1;
        self.spawn(|| Some(Done::Installed(library::scan())));
    }

    /// Candidates are read from the game's folder once per game.
    pub(super) fn find_candidates(&mut self, id: String, folder: String) {
        if self.library.candidates.contains_key(&id) {
            return;
        }
        Arc::make_mut(&mut self.library).candidates.insert(id.clone(), Vec::new());
        self.rev += 1;
        self.spawn(move || Some(Done::Candidates { list: launcher::candidates(&folder), id }));
    }

    /// Works out the real program behind a launcher or batch file while the person carries on.
    pub(super) fn predict_real_exe(&mut self, id: String, path: String) {
        self.spawn(move || Some(Done::Predicted { id, exe: launcher::predict_real_exe(&path).unwrap_or_default() }));
    }

    pub(super) fn test_graphics_card(&mut self) {
        let limit = self.cfg.gpu_temp_limit_c;
        self.spawn(move || {
            Some(match gpu::roundtrip(crate::config::GpuLevel::Safe, limit) {
                Ok(s) => Done::Info(s),
                Err(e) => Done::Error(format!("Graphics test: {e}")),
            })
        });
    }

    /// Runs a CatalyX command as administrator (one prompt) off the UI thread and reports back.
    pub(super) fn run_elevated(&mut self, args: &'static str, done: &'static str) {
        let admin = self.system.admin;
        self.toast(Tone::Info, if admin { "Working..." } else { "Approve the Windows prompt to continue" });
        self.spawn(move || {
            let ok = if admin { ffi::run_self_as_current(args) } else { ffi::run_self_elevated_and_wait(args, 120_000) };
            Some(if ok {
                Done::Info(done.to_string())
            } else {
                Done::Error("That did not complete (the prompt may have been cancelled)".into())
            })
        });
    }

    /// Switches Windows to a plan on a job thread: applying a plan can block while Windows works.
    pub(super) fn set_power_plan(&mut self, guid: String) {
        self.spawn(move || Some(Done::PowerChanged { ok: power::set_active(&guid), created: false }));
    }

    /// Builds or refreshes CatalyX's own plan, off the UI thread. It is copied from whichever plan
    /// is active at the time, so the job reads that itself instead of trusting an old View.
    pub(super) fn create_boost_plan(&mut self, level: CpuLevel) {
        self.spawn(move || {
            let level = if level == CpuLevel::Off { CpuLevel::Boost } else { level };
            let cur = power::active().unwrap_or_default();
            Some(Done::PowerChanged { ok: power::ensure_boost_plan(level, &cur), created: true })
        });
    }

    /// Stores a job's result.
    pub(super) fn finish(&mut self, d: Done) {
        self.rev += 1;
        match d {
            Done::Hardware { gpu, plans } => {
                let hw = Arc::make_mut(&mut self.hardware);
                hw.gpu = Some(*gpu);
                hw.plans = plans;
                self.hw_busy = false;
            }
            Done::Services(l) => {
                self.services = Arc::new(l);
                self.services_rev += 1;
                self.svc_busy = false;
            }
            Done::Installed(list) => {
                let lib = Arc::make_mut(&mut self.library);
                lib.installed = Some(list);
                lib.scanning = false;
            }
            Done::Predicted { id, exe } => {
                if let Some(i) = self.cfg.games.iter().position(|g| g.id == id) {
                    if self.cfg.games[i].real_exe.is_empty() && !exe.is_empty() {
                        self.cfg_mut().games[i].real_exe = exe;
                        self.mark_dirty();
                    }
                }
            }
            Done::Candidates { id, list } => {
                Arc::make_mut(&mut self.library).candidates.insert(id, list);
            }
            Done::Info(s) => {
                self.toast(Tone::Info, s);
                // A finished administrator job may have changed the tasks.
                self.system.elevation = None;
            }
            Done::Error(s) => {
                self.toast(Tone::Error, s);
                self.system.elevation = None;
            }
            Done::Elevation { ui_task, watcher } => {
                self.system.elevation = Some((ui_task, watcher));
                self.system.elevation_busy = false;
            }
            Done::EndedApp { name, count } => {
                self.toast(Tone::Info, format!("Ended {count} process(es) of {name}"));
            }
            Done::Relaunch(started) => {
                if started {
                    self.event(Event::ReadyToQuit);
                } else {
                    self.toast(Tone::Error, "The administrator prompt was cancelled");
                }
            }
            Done::PowerChanged { ok, created } => {
                if created {
                    if ok {
                        self.toast(Tone::Ok, "CatalyX plan is ready");
                    } else {
                        self.toast(Tone::Error, "Windows would not create the plan");
                    }
                    self.refresh_hardware();
                } else if ok {
                    self.refresh_hardware();
                } else {
                    self.toast(Tone::Error, "Windows refused the change");
                }
            }
        }
    }
}
