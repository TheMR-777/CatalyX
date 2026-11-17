//! The read-only picture of the core a skin draws from. Cloning a View is a handful of Arc clones; each
//! section is rebuilt only when its input changed (see `State::view`).

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
}

impl View {
    pub fn phase(&self) -> Phase {
        self.status.phase
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
}
