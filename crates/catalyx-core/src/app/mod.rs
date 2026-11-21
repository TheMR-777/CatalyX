//! The core: everything that is not drawing (the engine link, the config, Windows probes, the boost
//! policy, the upkeep). A skin draws from `view()`, changes things only with `send(Intent)` or
//! `hint(Hint)`, and shows what `take_events()` returns. See docs/SKINS.md for the contract.
//!
//! Threads: the engine and the monitor have their own; slow work runs on short-lived job threads; the
//! upkeep runs on one more, so it keeps working while the window is minimised. A skin only calls in
//! from its own thread.
//!
//! Locks: the State mutex is taken first and may then take the engine's status or the monitor's
//! snapshot; neither of those threads ever takes the State, so there is no lock-order cycle. Jobs run
//! with no lock held and only lock the State to store their result.

mod boost;
mod intent;
mod jobs;
mod options;
mod selftest;
mod state;
mod upkeep;
mod view;
pub mod window;

/// The settings catalogue: shared by the CLI and by whatever skin is compiled in.
pub mod spec;

pub use boost::{power_summary, NoticeKind, Plan};
pub use intent::{Event, GameId, Hint, Intent, Interest, PresetId};
pub use options::{Options, Origin};
pub use selftest::{CheckPage, Drawn};
pub use spec::Target;
pub use view::View;

// The types a View exposes, re-exported so a skin can name them without reaching into the core's
// private modules. They are data a skin reads, never handles to Windows.
pub use crate::engine::{LogLine, Phase, Status, Tags};
pub use crate::gpu::{GpuState, Live};
pub use crate::library::Installed;
pub use crate::power::Plan as PowerPlan;
pub use crate::procs::{ProcInfo, Snapshot};
pub use crate::services::ServiceInfo;

use crate::config::Config;
use jobs::Job;
use state::State;
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError, Sender, TryRecvError};
use std::sync::{Arc, Mutex, MutexGuard, OnceLock, PoisonError, Weak};
use std::time::{Duration, Instant};

/// How the core starts.
pub struct CoreOptions {
    /// What may be written and what is sampled: a normal run, a demo, or a check run.
    pub origin: Origin,
    /// The skin's name. Established once, here, so `Intent::SetPref` cannot write another skin's
    /// choices; the core still never reads a preference.
    pub skin: &'static str,
    /// The skin's start-up preferences (its check-run look flags), stored verbatim and never marked
    /// for saving. The skin validates them; the core does not interpret a key or a value.
    pub prefs: Vec<(String, String)>,
}

/// A cheap-to-clone handle to the core's state and threads.
#[derive(Clone)]
pub struct Core {
    inner: Arc<Inner>,
}

struct Inner {
    state: Mutex<State>,
    /// Called when the view changed, never per frame.
    wake: OnceLock<Arc<dyn Fn() + Send + Sync>>,
    /// The upkeep thread's doorbell.
    kick: Kick,
}

/// `mpsc::Sender` is not `Sync`, so it travels behind a lock; the engine, the monitor and job threads
/// all ring it. The channel is unbounded, so ringing never blocks.
type Kick = Arc<Mutex<Sender<()>>>;

fn kick(k: &Kick) {
    if let Ok(tx) = k.lock() {
        // Sending fails only when the upkeep thread is gone, which happens as the process ends.
        let _ = tx.send(());
    }
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

    /// Starts the jobs the last change queued, with no lock held, and wakes the skin if it asked.
    fn release(self: &Arc<Self>, mut st: MutexGuard<'_, State>) {
        let jobs = std::mem::take(&mut st.jobs);
        let wake = std::mem::take(&mut st.wake);
        if wake {
            // A draw is about to happen: sampled readings wait a full cadence from here.
            st.next_live_wake = Instant::now() + st.live_every();
        }
        drop(st);
        self.spawn_jobs(jobs);
        if wake {
            self.wake();
        }
    }

    /// Each result is stored, may queue more jobs (a plan change refreshes the hardware list) and
    /// kicks the upkeep, because it can make a save or another probe due.
    fn spawn_jobs(self: &Arc<Self>, jobs: Vec<Job>) {
        for job in jobs {
            let inner = self.clone();
            std::thread::spawn(move || {
                if let Some(done) = job() {
                    let mut st = inner.lock();
                    st.finish(done);
                    st.wake_skin();
                    inner.release(st);
                    kick(&inner.kick);
                }
            });
        }
    }
}

impl Core {
    /// Starts the monitor and the engine, loads the config and starts the upkeep thread.
    pub fn start(opts: CoreOptions) -> Core {
        let snap = crate::procs::start_monitor();
        let (tx, status) = crate::engine::start(snap.clone());
        crate::trace::mark("monitor and engine started");
        let (no_save, check_run, demo, session) = match &opts.origin {
            Origin::User => (false, false, false, false),
            Origin::Demo { session } => (true, false, true, *session),
            Origin::Check { demo, session, .. } => (true, true, *demo, *session),
        };
        let mut cfg = if no_save { Config::default() } else { Config::load() };
        if demo && cfg.games.is_empty() {
            state::demo_data(&mut cfg);
        }
        for (key, value) in &opts.prefs {
            // Start-up preferences are the skin's, stored verbatim and never marked for saving:
            // a check run must not change what the person chose.
            cfg.set_pref(opts.skin, key, value);
        }
        if session {
            state::demo_session(&status);
        }
        let selftest = match &opts.origin {
            Origin::Check { selftest: Some(path), .. } => Some(selftest::SelfTest::new(path.clone())),
            _ => None,
        };
        let st = State::new(cfg, no_save, selftest, check_run, opts.skin, tx, status.clone(), snap, crate::ffi::is_admin());
        let (kick_tx, kick_rx) = channel();
        let inner = Arc::new(Inner { state: Mutex::new(st), wake: OnceLock::new(), kick: Arc::new(Mutex::new(kick_tx)) });
        // Everything that changes without the skin asking (engine stages, monitor samples, job
        // results) wakes the upkeep, which decides what is due and then whether to wake the skin.
        let k = inner.kick.clone();
        status.set_notify(move || kick(&k));
        let k = inner.kick.clone();
        crate::procs::set_on_sample(move || kick(&k));
        let weak = Arc::downgrade(&inner);
        let spawned = std::thread::Builder::new().name("catalyx-upkeep".into()).spawn(move || upkeep_loop(weak, kick_rx));
        if let Err(e) = spawned {
            // The window still works; only work that must outlive frames is lost.
            crate::trace::mark(&format!("upkeep thread not started: {e}"));
        }
        Core { inner }
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
        self.inner.release(st);
        kick(&self.inner.kick);
    }

    /// What the skin is showing, whether it draws at all, and a close request. Applied before this
    /// returns; the upkeep then runs a pass, so a new interest starts sampling at once.
    pub fn hint(&self, hint: Hint) {
        let mut st = self.inner.lock();
        st.apply_hint(hint);
        self.inner.release(st);
        kick(&self.inner.kick);
    }

    /// Things the core needs the skin to show (drained).
    pub fn take_events(&self) -> Vec<Event> {
        self.inner.lock().take_events()
    }

    /// The page a self-test needs on screen next, or None outside one (`docs/SKINS.md`, Check runs).
    pub fn selftest_page(&self) -> Option<CheckPage> {
        self.inner.lock().selftest_page()
    }

    /// Advances the self-test by one window frame; true when its report is written and the program
    /// should exit. Only a window in a check run calls this.
    pub fn selftest_step(&self, drawn: Drawn) -> bool {
        let mut st = self.inner.lock();
        let done = st.selftest_step(drawn);
        self.inner.release(st);
        done
    }

    /// The window is going away: save, and wait briefly for a running boost to be put back.
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
            // Wait for the engine rather than sleeping blind: the journal is the safety net if the
            // process ends before the restore finishes.
            let deadline = Instant::now() + Duration::from_secs(2);
            while Instant::now() < deadline {
                if !self.inner.lock().phase().active() {
                    break;
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}

/// The upkeep thread: one pass, then sleep until the pass's next deadline or a kick. It holds the
/// core weakly, so the last handle going away ends it.
fn upkeep_loop(core: Weak<Inner>, rx: Receiver<()>) {
    let mut next = Instant::now();
    loop {
        let wait = next.saturating_duration_since(Instant::now());
        let gone = if wait.is_zero() {
            matches!(rx.try_recv(), Err(TryRecvError::Disconnected))
        } else {
            matches!(rx.recv_timeout(wait), Err(RecvTimeoutError::Disconnected))
        };
        if gone {
            return;
        }
        let Some(inner) = core.upgrade() else { return };
        let now = Instant::now();
        let (due, jobs, draw, pending_at) = {
            let mut st = inner.lock();
            let due = st.upkeep(now);
            // A job result or an event is shown at once; sampled readings (the monitor, the card,
            // this window's memory) share one cadence, so two sources do not draw twice as often.
            let immediate = std::mem::take(&mut st.wake);
            let changed = st.rev != st.drawn_rev && !st.minimised;
            let draw = immediate || (changed && st.may_draw_live(now));
            if immediate {
                st.drawn(now);
            }
            let pending = st.rev != st.drawn_rev && !st.minimised;
            (due, std::mem::take(&mut st.jobs), draw, pending.then_some(st.next_live_wake))
        };
        inner.spawn_jobs(jobs);
        if draw {
            inner.wake();
        }
        // A pass that names no deadline only runs again on a kick; the minute is a safety net, not a
        // poll (every source of change kicks). A live change waiting for the cadence names its time.
        next = due.unwrap_or_else(|| now + Duration::from_secs(60));
        if let Some(t) = pending_at {
            next = next.min(t);
        }
    }
}
