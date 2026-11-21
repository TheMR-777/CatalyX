//! The window run's command line, parsed once. The core claims the flags every front-end shares
//! (screenshots, tours, the self-test, size); a skin's own flags are left in `rest`, so the same
//! flag is never read by two different parsers with two different meanings.
//!
//! `Origin` is what the core needs to know: whether this run may write the person's config, and
//! whether it is a check run. Everything else follows from it, so there are no combinations of
//! booleans that mean nothing.

/// How a window run was started.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum Origin {
    /// The person's own config, saved as usual.
    #[default]
    User,
    /// Sample data for pictures: a fresh default config, never saved. `session` also fakes a
    /// running boost (visual checks only; nothing on the PC changes).
    Demo { session: bool },
    /// A check run (screenshot, tour, self-test): scratch settings, never saved, per-process
    /// figures always on. `selftest` carries the report path; `demo` and `session` add the sample
    /// data a screenshot wants.
    Check { selftest: Option<String>, demo: bool, session: bool },
}

impl Origin {
    /// A check run: screenshot, tour or self-test.
    pub fn check(&self) -> bool {
        matches!(self, Origin::Check { .. })
    }
}

/// The command line of a window run, as the core understands it.
#[derive(Clone, Debug, Default)]
pub struct Options {
    /// `--screenshot <png>`: draw, photograph, exit.
    pub screenshot: Option<String>,
    /// `--tour <dir>`: photograph every page in both modes, then exit.
    pub tour: Option<String>,
    /// `--self-test <report.json>`: run the boost/restore self-test in the real window.
    pub selftest: Option<String>,
    /// `--demo`: sample data (a fresh install's library, a believable session).
    pub demo: bool,
    /// `--demo-session`: pretend a boost is running.
    pub demo_session: bool,
    /// `--no-elevate`: never relaunch for administrator rights.
    pub no_elevate: bool,
    /// `--size <WxH>`: the window's size for a check run.
    pub size: Option<(u32, u32)>,
    /// Everything the core did not claim, in order: a skin parses its own flags from here.
    pub rest: Vec<String>,
}

/// Flags that take a value. The core claims some of them; the rest travel to the skin intact.
const VALUED: [&str; 9] = ["--screenshot", "--tour", "--self-test", "--size", "--tab", "--theme", "--appearance", "--accent", "--control"];

impl Options {
    pub fn from_args(args: &[String]) -> Options {
        let mut o = Options::default();
        let mut i = 0;
        while i < args.len() {
            let a = args[i].as_str();
            match a {
                "--demo" => o.demo = true,
                "--demo-session" => o.demo_session = true,
                "--no-elevate" => o.no_elevate = true,
                _ if VALUED.contains(&a) => {
                    let v = args.get(i + 1).cloned();
                    match a {
                        "--screenshot" => o.screenshot = v,
                        "--tour" => o.tour = v,
                        "--self-test" => o.selftest = v,
                        "--size" => o.size = v.as_deref().and_then(parse_size),
                        _ => {
                            // A skin's flag: keep the flag and its value together, in order.
                            o.rest.push(a.to_string());
                            if let Some(v) = v {
                                o.rest.push(v);
                            }
                        }
                    }
                    i += 1;
                }
                // An unknown flag may or may not take a value; a skin knows its own grammar.
                _ => o.rest.push(a.to_string()),
            }
            i += 1;
        }
        o
    }

    /// A check run: screenshot, tour or self-test.
    pub fn check(&self) -> bool {
        self.screenshot.is_some() || self.tour.is_some() || self.selftest.is_some()
    }

    /// The self-test drives this run.
    pub fn selftest(&self) -> bool {
        self.selftest.is_some()
    }

    /// What this run is allowed to do with the config, and whether it is a check run.
    pub fn origin(&self) -> Origin {
        if self.check() {
            Origin::Check {
                selftest: self.selftest.clone(),
                // A picture of the window wants a library and a session to show.
                demo: self.demo || self.screenshot.is_some() || self.tour.is_some(),
                session: self.demo_session,
            }
        } else if self.demo || self.demo_session {
            Origin::Demo { session: self.demo_session }
        } else {
            Origin::User
        }
    }

    /// A flag's value among the ones the core left for the skin (`--tab settings`).
    pub fn value(&self, flag: &str) -> Option<String> {
        let i = self.rest.iter().position(|a| a == flag)?;
        self.rest.get(i + 1).cloned()
    }

    /// Whether the skin's flags contain this switch.
    pub fn has(&self, flag: &str) -> bool {
        self.rest.iter().any(|a| a == flag)
    }
}

fn parse_size(s: &str) -> Option<(u32, u32)> {
    let (w, h) = s.split_once('x')?;
    Some((w.parse().ok()?, h.parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(v: &[&str]) -> Vec<String> {
        v.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn the_core_claims_its_flags_and_leaves_a_skins_alone() {
        let o = Options::from_args(&args(&[
            "--screenshot",
            "a.png",
            "--tab",
            "settings",
            "--size",
            "760x560",
            "--demo-session",
            "--no-elevate",
        ]));
        assert_eq!(o.screenshot.as_deref(), Some("a.png"));
        assert_eq!(o.size, Some((760, 560)));
        assert!(o.demo_session && o.no_elevate);
        assert_eq!(o.rest, ["--tab", "settings"], "the skin's flags stay in order");
        assert_eq!(o.value("--tab").as_deref(), Some("settings"));
    }

    #[test]
    fn a_check_run_is_a_check_run_and_nothing_else_has_to_be_derived() {
        let shot = Options::from_args(&args(&["--screenshot", "a.png"]));
        assert_eq!(shot.origin(), Origin::Check { selftest: None, demo: true, session: false }, "a screenshot wants the sample library");
        assert!(shot.check());
        let plain = Options::from_args(&args(&["--theme", "ember"]));
        assert_eq!(plain.origin(), Origin::User, "a look flag alone is not a check run");
        let demo = Options::from_args(&args(&["--demo-session"]));
        assert_eq!(demo.origin(), Origin::Demo { session: true });
    }

    #[test]
    fn the_self_test_carries_its_report_path() {
        let o = Options::from_args(&args(&["--self-test", "r.json"]));
        assert!(o.selftest());
        assert_eq!(o.origin(), Origin::Check { selftest: Some("r.json".into()), demo: false, session: false });
    }
}
