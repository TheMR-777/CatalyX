//! Optional startup timing. Set `CATALYX_TRACE=1` and a line per milestone (milliseconds since the program
//! started) is appended to `%TEMP%\catalyx-startup.txt`. Costs one environment lookup when off.

use std::sync::OnceLock;
use std::time::Instant;

static START: OnceLock<Instant> = OnceLock::new();
static ON: OnceLock<bool> = OnceLock::new();

pub fn start() {
    let _ = START.set(Instant::now());
}

fn on() -> bool {
    *ON.get_or_init(|| std::env::var_os("CATALYX_TRACE").is_some())
}

/// Records that `what` just happened.
pub fn mark(what: &str) {
    if !on() {
        return;
    }
    let ms = START.get().map(|s| s.elapsed().as_secs_f64() * 1000.0).unwrap_or(0.0);
    let path = std::env::temp_dir().join("catalyx-startup.txt");
    if let Ok(mut f) = std::fs::OpenOptions::new().create(true).append(true).open(path) {
        use std::io::Write;
        let _ = writeln!(f, "{ms:8.1} ms  {what}");
    }
}

static FRAMES: std::sync::atomic::AtomicU32 = std::sync::atomic::AtomicU32::new(0);

/// Called once per drawn frame; only the first few are recorded.
pub fn frame() {
    let n = FRAMES.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    if n < 3 {
        mark(&format!("frame {}", n + 1));
    }
}
