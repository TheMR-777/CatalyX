//! The read-only picture of the core a skin draws from. Cloning a View is a handful of Arc clones; each
//! section is rebuilt only when its input changed (see `State::view`).

use super::boost::{self, BoostView};
use crate::config::Config;
use crate::engine::{Phase, Status};
use crate::procs::Snapshot;
use crate::{gpu, library, power, services};
use std::collections::HashMap;
use std::sync::Arc;

#[derive(Clone)]
pub struct View {
    /// Moves on any change: a skin may skip work while it stays the same.
    #[allow(dead_code)] // part of the skin contract; the classic skin redraws on every frame it gets
    pub rev: u64,
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

    /// Whether Windows lets CatalyX start this service again, from the last service list. Before
    /// that list has arrived there is no reason to think it cannot: the engine still applies the
    /// same check and leaves a protected service running.
    #[allow(dead_code)] // part of the skin contract; service_stops carries the classic skin's needs
    pub fn service_can_start(&self, name: &str) -> bool {
        boost::service_can_start(&self.services, name)
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

/// Installed games found by a scan, and the real-program candidates read from each game's folder.
#[derive(Clone, Default)]
pub struct Library {
    pub installed: Option<Vec<library::Installed>>,
    pub scanning: bool,
    pub candidates: HashMap<String, Vec<(String, u64)>>,
}

#[derive(Clone, Copy, Default)]
pub struct System {
    pub admin: bool,
    /// (one-click administrator task, background watcher), once asked.
    pub elevation: Option<(bool, bool)>,
    pub elevation_busy: bool,
    /// This process's own working set, sampled while the Settings page shows it.
    pub own_working_set: u64,
}
