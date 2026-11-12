# Roadmap and honest gaps

What is known to be missing, what has *not* been verified, and ideas worth considering. Keep it truthful: move
things out when they are done, and say plainly when something was only tested in one place.

## Not verified yet

These work on the development PC (Windows 11, NVIDIA GTX 1050 Ti, Core i7-6700, 100% display scaling, UAC off) and
have not been checked elsewhere.

- **Windows 10.** The quiet desktop, the window's rounded corners and the title-bar styling were only exercised on
  Windows 11 (build 26300).
- **UAC on.** Everything is elevated on the development PC, so the "ordinary user token" path in `shell.rs` (a
  second, non-elevated token) is written but has not run for real. The fallback path (the shell's own token) has.
- **Display scaling above 100%.** Layouts were designed and checked at 100%.
- **AMD and Intel graphics.** Detected and left alone; nothing graphics-related was run on them.
- **Games with anti-cheat.** Boosted like any other program; never tested with a real anti-cheat title on purpose.
- **Sustained graphics-card tuning.** Offsets were verified by read-back and a few-second test, not under hours of
  real load.
- **Animations.** The needle sweep and ring fill were checked at their end states (screenshots) and with unit tests
  of the progress maths, not frame by frame.
- **`catalyx.exe` typed in a terminal.** Joining the parent console was verified in `cmd`; other terminals and
  redirects through other hosts were not.

## Known limits

- NVIDIA only for graphics tuning; no voltage control.
- Quiet desktop means no taskbar, Alt+Tab switcher or Start menu while on; open File Explorer windows close; it is
  used only for games CatalyX starts itself (not `attach`, auto-boost or "Boost only").
- The background watcher polls every 3 s (0.1-0.3% of a core); process-start events would cost less.
- Per-game real-program detection is heuristic (launcher names, folder, process tree, then GPU use); `catalyx exe`
  overrides it.

## Ideas (not decided)

- **Show what will really change.** The plan card counts the services you ticked, but a service that is not
  running right now cannot be stopped. Saying "11 of your 48 chosen services are running now" is more honest
  than a number that quietly shrinks later.
- **Boost state in the title bar** (a small pill by the meters, visible from every page), or the launch bar on
  every page rather than only on Boost. Mocked as idea 12 in the boost-control exploration.
- **`windows-sys` for the Win32 bindings** in `ffi.rs`, `shell.rs`, `procs.rs`, `gpu.rs` (type-checked signatures,
  fewer hand-written mistakes). Mechanical, no behaviour change.
- **Software-rendering option** for the window (no GPU driver, so about 20-30 MB less memory) at the cost of the
  soft look. Only worth it if footprint becomes the priority.
- **Event-driven watcher** (process-creation events instead of polling).
- **Per-game boost-control look**, and a dry-run preview in the window (the CLI has `--dry-run`).
- **Signed releases** (Authenticode) so SmartScreen does not warn on first download.
- **A `catalyx.exe` installer or winget manifest**, if people ask; today it is one file.

## How to pick something up

Read [DECISIONS.md](DECISIONS.md) first (it explains why things are the way they are), then [AGENTS.md](../AGENTS.md)
for the rules and the scripts. Run `.\scripts\check.ps1` before and after. Nothing here is a promise.
