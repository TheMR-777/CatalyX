//! `catalyx watch`: a headless watcher with no window. It checks the (very cheap) process list
//! every few seconds; when a game from your library starts it boosts it, and when the game
//! closes it puts everything back. Between games it does nothing but sleep, which is why it
//! can run all day in under 10 MB.

use crate::config::Config;
use crate::engine::{self, BoostRequest, Cmd, Phase};
use crate::{ffi, procs};
use std::collections::HashSet;
use std::time::{Duration, Instant};

/// Finds a library game that is running: (game index, pid).
pub fn find_running(cfg: &Config, ignore: &HashSet<u32>) -> Option<(usize, u32)> {
    let list = ffi::quick_list();
    for (i, g) in cfg.games.iter().enumerate() {
        let real = g.real_exe.to_lowercase();
        if real.is_empty() {
            continue;
        }
        if let Some((pid, _, _)) = list.iter().find(|(pid, _, name)| *name == real && !ignore.contains(pid)) {
            return Some((i, *pid));
        }
    }
    None
}

pub fn run(quiet: bool) -> Result<(), String> {
    ffi::catch_ctrl_c();
    let say = |s: &str| {
        if !quiet {
            println!("{s}");
        }
    };
    say("CatalyX watcher: waiting for your games (Ctrl+C to stop)");
    let mut ignore: HashSet<u32> = HashSet::new();
    let mut cfg = Config::load();
    let mut cfg_stamp = std::fs::metadata(Config::path()).and_then(|m| m.modified()).ok();
    loop {
        if ffi::wait_ctrl_c(3000) {
            say("Stopped.");
            return Ok(());
        }
        // Pick up changes made in the window or on the command line.
        let stamp = std::fs::metadata(Config::path()).and_then(|m| m.modified()).ok();
        if stamp != cfg_stamp {
            cfg = Config::load();
            cfg_stamp = stamp;
        }
        if !cfg.auto_boost_known_games {
            continue;
        }
        let Some((i, pid)) = find_running(&cfg, &ignore) else { continue };
        ignore.insert(pid);
        let game = cfg.games[i].clone();
        say(&format!("{} started: boosting", game.name));
        let snap = procs::start_monitor();
        let (tx, status) = engine::start(snap.clone());
        let req = BoostRequest {
            settings: cfg.settings_for(Some(&game)),
            apps: cfg.apps_for(Some(&game)),
            services: cfg.services_for(Some(&game)),
            keep_apps_off: cfg.keep_apps_off,
            restore_when_game_closes: true,
            reopen_closed_apps: cfg.reopen_closed_apps,
            gpu_temp_limit: cfg.gpu_temp_limit_c,
            dry_run: false,
            attach_pid: Some(pid),
            game: Some(game.clone()),
        };
        let _ = tx.send(Cmd::Boost(Box::new(req)));
        let started = Instant::now();
        let mut was_active = false;
        let mut restore_sent = false;
        loop {
            std::thread::sleep(Duration::from_millis(300));
            let st = status.lock().map(|s| s.clone()).unwrap_or_default();
            was_active |= st.phase.active();
            if ffi::CTRL_C.load(std::sync::atomic::Ordering::SeqCst) && !restore_sent {
                let _ = tx.send(Cmd::Restore);
                restore_sent = true;
            }
            if let Some((gid, exe)) = &st.learned {
                if let Some(g) = cfg.games.iter_mut().find(|g| g.id == *gid) {
                    if g.real_exe != *exe {
                        g.real_exe = exe.clone();
                        cfg.save();
                        cfg_stamp = std::fs::metadata(Config::path()).and_then(|m| m.modified()).ok();
                    }
                }
            }
            if (was_active || started.elapsed() > Duration::from_secs(10)) && st.phase == Phase::Idle {
                say(&format!("{} closed: everything restored", game.name));
                break;
            }
        }
        if ffi::CTRL_C.load(std::sync::atomic::Ordering::SeqCst) {
            return Ok(());
        }
        // Engine and monitor end when their senders and snapshot handles are dropped here.
        drop(tx);
        drop(snap);
    }
}
