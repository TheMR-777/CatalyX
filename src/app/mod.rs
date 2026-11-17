//! The core: everything that is not drawing (the engine link, the config, Windows probes, the boost
//! policy, the upkeep). A skin draws from `view()`, changes things only with `send(Intent)`, and shows
//! what `take_events()` returns. See docs/SKINS.md for the contract this grows into.
//!
//! Locks: the State mutex is taken first and may then take the engine's status or the monitor's
//! snapshot; neither of those threads ever takes the State, so there is no lock-order cycle. Jobs run
//! with no lock held and only lock the State to store their result.

mod intent;
mod jobs;
mod state;
mod upkeep;
mod view;
pub mod window;

pub use intent::{Event, FrameFacts, Intent, Interest, Tone, Upkeep};
pub use state::apply_suggestions;
pub use view::View;

use crate::config::{Config, Game};
use state::State;
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError};
use std::time::Duration;

/// How the core starts.
pub struct CoreOptions {
    /// Never write the config (check runs and demos use a fresh default one).
    pub no_save: bool,
    /// Fill an empty library with sample games (screenshots of a fresh install).
    pub demo: bool,
    /// Pretend a boost is running (visual checks only; nothing is changed).
    pub demo_session: bool,
    /// The self-test drives this run.
    pub selftest: bool,
    /// A check run (screenshot, tour, self-test).
    pub check_run: bool,
    /// The skin's start-up adjustments to the loaded config (its check-run look flags). Applied once,
    /// without marking the config for saving.
    pub prepare: Option<ConfigEdit>,
}

/// A change to the config, made once.
pub type ConfigEdit = Box<dyn FnOnce(&mut Config) + Send>;

/// A cheap-to-clone handle to the core's state and threads.
#[derive(Clone)]
pub struct Core {
    inner: Arc<Inner>,
}

struct Inner {
    state: Mutex<State>,
    /// Called when a job finishes or a boost starts, never per frame.
    wake: OnceLock<Arc<dyn Fn() + Send + Sync>>,
}

impl Inner {
    fn lock(&self) -> MutexGuard<'_, State> {
        // A panicked job must not take the window down with it: the state stays usable.
        self.state.lock().unwrap_or_else(PoisonError::into_inner)
    }

    fn wake(&self) {
        if let Some(w) = self.wake.get() {
            w();
        }
    }
}

impl Core {
    /// Starts the monitor and the engine and loads the config.
    pub fn start(opts: CoreOptions) -> Core {
        let snap = crate::procs::start_monitor();
        let (tx, status) = crate::engine::start(snap.clone());
        crate::trace::mark("monitor and engine started");
        let mut cfg = if opts.no_save { Config::default() } else { Config::load() };
        if opts.demo && cfg.games.is_empty() {
            state::demo_data(&mut cfg);
        }
        if let Some(prepare) = opts.prepare {
            prepare(&mut cfg);
        }
        if opts.demo_session {
            state::demo_session(&status);
        }
        let st = State::new(cfg, opts.no_save, opts.selftest, opts.check_run, tx, status, snap, crate::ffi::is_admin());
        Core { inner: Arc::new(Inner { state: Mutex::new(st), wake: OnceLock::new() }) }
    }

    /// Called by the skin once its window exists. `wake` must be cheap and thread-safe.
    pub fn connect(&self, wake: impl Fn() + Send + Sync + 'static) {
        let _ = self.inner.wake.set(Arc::new(wake));
    }

    /// The current state: a handful of Arc clones; sections are rebuilt only when they changed.
    pub fn view(&self) -> View {
        self.inner.lock().view()
    }

    /// The only way to change anything. Cheap intents are applied before this returns, so the next
    /// view() shows them; slow ones run as jobs and report back through the view and events.
    pub fn send(&self, intent: Intent) {
        let mut st = self.inner.lock();
        st.apply(intent);
        self.release(st);
    }

    /// Things the core needs the skin to show (drained).
    pub fn take_events(&self) -> Vec<Event> {
        self.inner.lock().take_events()
    }

    /// TRANSITIONAL (Phase 1a T2a, gone in T2b): the per-frame upkeep, called by the window.
    pub fn tick(&self, facts: &FrameFacts) -> Upkeep {
        let mut st = self.inner.lock();
        let up = st.tick(facts);
        self.release(st);
        up
    }

    /// The window is going away: save, and put a running boost back.
    pub fn exit(&self) {
        let active = {
            let mut st = self.inner.lock();
            st.save_now();
            let active = st.phase().active();
            if active {
                let _ = st.tx.send(crate::engine::Cmd::Restore);
            }
            active
        };
        if active {
            std::thread::sleep(Duration::from_millis(1500));
        }
    }

    /// Check runs only: show the boost summary for this game whatever the "ask before boosting" option says.
    pub(crate) fn show_boost_summary(&self, game: Option<usize>) {
        let mut st = self.inner.lock();
        st.show_boost_summary(game);
        self.release(st);
    }

    /// The self-test only: adds its stand-in game (never saved) and selects it. Returns its index.
    pub(crate) fn add_selftest_game(&self, game: Game) -> usize {
        self.inner.lock().add_selftest_game(game)
    }

    /// Unlocks, then starts the jobs `apply`/`tick` queued and wakes the skin if asked to.
    fn release(&self, mut st: MutexGuard<'_, State>) {
        let jobs = std::mem::take(&mut st.jobs);
        let wake = std::mem::take(&mut st.wake);
        drop(st);
        for job in jobs {
            let inner = self.inner.clone();
            std::thread::spawn(move || {
                if let Some(done) = job() {
                    inner.lock().finish(done);
                    inner.wake();
                }
            });
        }
        if wake {
            self.inner.wake();
        }
    }
}
