//! The PC itself: `plans`, `plan`, `gpu`, `theme`, `option`.

use super::*;

impl Cli {
    pub(super) fn plans(&self) -> Result<(), String> {
        let plans = power::list();
        if self.json {
            let v: Vec<_> = plans.iter().map(|p| json!({"name": p.name, "guid": p.guid, "active": p.active})).collect();
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            return Ok(());
        }
        for p in &plans {
            println!("{} {:<40} {}", if p.active { "*" } else { " " }, trunc(&p.name, 40), p.guid);
        }
        Ok(())
    }

    pub(super) fn plan(&self) -> Result<(), String> {
        let q = self.pos[1..].join(" ").to_lowercase();
        if q.is_empty() {
            return Err("missing plan name".into());
        }
        if (q == "catalyx" || q == "pureboost") && !power::list().iter().any(|p| p.guid.eq_ignore_ascii_case(power::BOOST_PLAN)) {
            power::ensure_boost_plan(CpuLevel::Boost, &power::active().unwrap_or_default());
        }
        let plans = power::list();
        let hit: Vec<_> = plans.iter().filter(|p| p.guid.to_lowercase() == q || p.name.to_lowercase().contains(&q)).collect();
        match hit.as_slice() {
            [p] => {
                if power::set_active(&p.guid) {
                    println!("Power plan: {}", p.name);
                    Ok(())
                } else {
                    Err("Windows refused the change".into())
                }
            }
            [] => Err(format!("no power plan matches '{q}'")),
            _ => Err(format!("'{q}' matches {} plans; be more specific", hit.len())),
        }
    }

    /// `gpu test [safe|balanced]`: applies the step for a few seconds, reads it back from the
    /// driver, then puts the old offsets back. Proves the overclock path works on this card.
    pub(super) fn gpu_test(&self) -> Result<(), String> {
        let level = match self.pos.get(2).map(|s| s.to_lowercase()).as_deref() {
            Some("balanced") => GpuLevel::Balanced,
            _ => GpuLevel::Safe,
        };
        let before = gpu::probe();
        if !before.supported {
            return Err(format!("overclocking is not available: {}", before.note));
        }
        let (c, m) = gpu::planned(&before, level);
        let temp = gpu::temperature().ok_or("the temperature cannot be read, so no clock change is made")?;
        let limit = Config::load().gpu_temp_limit_c;
        if temp + 5 >= limit {
            return Err(format!("the card is at {temp} °C, too close to the {limit} °C limit"));
        }
        println!("Trying core +{} MHz, memory +{} MHz on {} ({temp} °C)", c / 1000, m / 1000, before.name);
        let prev = gpu::apply(c, m)?;
        std::thread::sleep(std::time::Duration::from_secs(3));
        let during = gpu::probe();
        let t2 = gpu::temperature();
        let restored = gpu::restore(prev.0, prev.1);
        let after = gpu::probe();
        println!(
            "Driver reports core {:+} MHz, memory {:+} MHz while applied ({})",
            during.core.current_khz / 1000,
            during.mem.current_khz / 1000,
            t2.map(|t| format!("{t} °C")).unwrap_or("temperature n/a".into())
        );
        println!("After restoring: core {:+} MHz, memory {:+} MHz", after.core.current_khz / 1000, after.mem.current_khz / 1000);
        restored?;
        let ok = (c <= 0 || during.core.current_khz >= c) && (m <= 0 || during.mem.current_khz >= m);
        if ok {
            println!("PASS: the offsets were accepted by the driver and undone");
            Ok(())
        } else {
            Err("the driver accepted the call but did not report the new offsets".into())
        }
    }

    pub(super) fn gpu(&self) -> Result<(), String> {
        if self.pos.get(1).map(|s| s.eq_ignore_ascii_case("test")).unwrap_or(false) {
            return self.gpu_test();
        }
        let g = gpu::probe();
        let t = gpu::temperature();
        let (sc, sm) = gpu::planned(&g, GpuLevel::Safe);
        let (bc, bm) = gpu::planned(&g, GpuLevel::Balanced);
        if self.json {
            let v = json!({
                "name": g.name, "vendor": g.vendor, "supported": g.supported, "note": g.note, "temperature_c": t,
                "core_offset_mhz": g.core.current_khz / 1000, "mem_offset_mhz": g.mem.current_khz / 1000,
                "core_range_mhz": [g.core.min_khz / 1000, g.core.max_khz / 1000],
                "mem_range_mhz": [g.mem.min_khz / 1000, g.mem.max_khz / 1000],
                "safe_mhz": [sc / 1000, sm / 1000], "balanced_mhz": [bc / 1000, bm / 1000],
            });
            println!("{}", serde_json::to_string_pretty(&v).unwrap_or_default());
            return Ok(());
        }
        println!("{} ({})", g.name, g.vendor);
        println!("Temperature   {}", t.map(|t| format!("{t} °C")).unwrap_or("n/a".into()));
        if g.supported {
            println!(
                "Core offset   {:+} MHz (driver allows {} to {})",
                g.core.current_khz / 1000,
                g.core.min_khz / 1000,
                g.core.max_khz / 1000
            );
            println!(
                "Memory offset {:+} MHz (driver allows {} to {})",
                g.mem.current_khz / 1000,
                g.mem.min_khz / 1000,
                g.mem.max_khz / 1000
            );
            println!("Safe step     +{} / +{} MHz", sc / 1000, sm / 1000);
            println!("Balanced step +{} / +{} MHz", bc / 1000, bm / 1000);
        } else {
            println!("Overclocking not available: {}", g.note);
        }
        Ok(())
    }

    pub(super) fn theme(&self) -> Result<(), String> {
        let mut cfg = Config::load();
        if self.pos.len() < 2 {
            println!(
                "Theme: {}   Mode: {}   Accent: {}   Tint: {}",
                cfg.theme,
                cfg.appearance,
                cfg.accent,
                ["off", "subtle", "rich"][(cfg.tint as usize).min(2)]
            );
            return Ok(());
        }
        for a in &self.pos[1..] {
            match a.to_lowercase().as_str() {
                t @ ("focus" | "slate" | "mica") => cfg.theme = t.into(),
                m @ ("dark" | "light" | "system") => cfg.appearance = m.into(),
                a if a == "default" || config::ACCENT_KEYS.contains(&a) => cfg.accent = a.into(),
                "tint-off" => cfg.tint = 0,
                "tint-subtle" => cfg.tint = 1,
                "tint-rich" => cfg.tint = 2,
                o => return Err(format!("unknown theme, mode or accent '{o}' (looks: focus slate mica; modes: dark light system; accents: default {}; tint-off tint-subtle tint-rich)", config::ACCENT_KEYS.join(" "))),
            }
        }
        cfg.save();
        println!(
            "Theme: {}   Mode: {}   Accent: {}   Tint: {}",
            cfg.theme,
            cfg.appearance,
            cfg.accent,
            ["off", "subtle", "rich"][(cfg.tint as usize).min(2)]
        );
        Ok(())
    }

    pub(super) fn option(&self) -> Result<(), String> {
        let mut cfg = Config::load();
        let name = self.arg(1, "option name")?.to_lowercase();
        let on = parse_on(&self.arg(2, "on or off")?)?;
        match name.as_str() {
            "restore-on-exit" => cfg.restore_when_game_closes = on,
            "reopen-apps" => cfg.reopen_closed_apps = on,
            "keep-off" => cfg.keep_apps_off = on,
            "auto-boost" => cfg.auto_boost_known_games = on,
            "run-as-admin" => cfg.run_as_admin = on,
            o => return Err(format!("unknown option '{o}' (restore-on-exit, reopen-apps, keep-off, auto-boost, run-as-admin)")),
        }
        cfg.save();
        println!("{name}: {}", if on { "on" } else { "off" });
        Ok(())
    }
}
