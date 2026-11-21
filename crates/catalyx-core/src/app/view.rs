//! The read-only picture of the core a skin draws from. Cloning a View is a handful of Arc clones; each
//! section is rebuilt only when its input changed (see `State::view`).

use super::boost::{self, BoostView};
use crate::config::{Config, GpuLevel};
use crate::engine::{Phase, Status};
use crate::procs::Snapshot;
use crate::{gpu, library, power, services};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct View {
    pub config: Arc<Config>,
    /// The engine's status (phase, game, counts, activity log). Rebuilt when the engine changes it.
    pub status: Arc<Status>,
    /// The latest monitor sample. Rebuilt once per sample.
    pub snap: Arc<Snapshot>,
    pub hardware: Arc<Hardware>,
    pub services: Arc<Vec<services::ServiceInfo>>,
    pub library: Arc<Library>,
    pub system: System,
    /// The boost a skin is about to start or is showing: what it will change, the stages, progress
    /// and notices. Rebuilt only when the config, the engine status or the service list changed.
    pub boost: Arc<BoostView>,
}

impl View {
    pub fn phase(&self) -> Phase {
        self.status.phase
    }

    /// Whether a chosen service will really be paused for the session: not one Windows protects,
    /// and one it can start again. One rule for the plan, the summary and the service list.
    pub fn service_stops(&self, name: &str, chosen: bool) -> bool {
        boost::service_stops(&self.services, name, chosen)
    }
}

/// Graphics card, power plans and live card readings, filled by probes.
#[derive(Clone, Default)]
pub struct Hardware {
    pub gpu: Option<gpu::GpuState>,
    pub plans: Vec<power::Plan>,
    pub live: Option<gpu::Live>,
}

impl Hardware {
    /// The line a form shows under a graphics setting: the planned offsets when the driver allows
    /// them, the neutral hint otherwise.
    pub fn level_hint(&self, level: GpuLevel) -> String {
        gpu::level_hint(level, self.gpu.as_ref())
    }
}

/// Installed games found by a scan, and the real-program candidates read from each game's folder.
#[derive(Clone, Default)]
pub struct Library {
    pub installed: Option<Vec<library::Installed>>,
    pub scanning: bool,
    pub candidates: HashMap<String, Vec<(String, u64)>>,
}

#[derive(Clone, Copy)]
pub struct System {
    pub admin: bool,
    /// The program's version, for an About line a skin can print without its own build metadata.
    pub version: &'static str,
    /// (one-click administrator task, background watcher), once asked.
    pub elevation: Option<(bool, bool)>,
    pub elevation_busy: bool,
    /// This process's own working set, sampled while the Settings page shows it.
    pub own_working_set: u64,
}

impl Default for System {
    fn default() -> Self {
        System { admin: false, version: env!("CARGO_PKG_VERSION"), elevation: None, elevation_busy: false, own_working_set: 0 }
    }
}
