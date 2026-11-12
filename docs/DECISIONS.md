# Decisions

The choices that shaped CatalyX, and *why*, so nobody has to rediscover them. Newest at the bottom. Each entry
says where it lives in the code. If you change one, change the entry too.

## The promise

**Every change is journaled before it is made, and undone afterwards.** A boost is only acceptable if it always
puts things back, even after a crash. So: the undo record is written (flushed, atomically replaced) *before* the
change; if the record cannot be saved the change is not made; undo runs in reverse and is identity-checked
(process id *and* start time). `src/engine/journal.rs`, `src/engine/restore.rs`, [SAFETY.md](SAFETY.md).

**Opt-in, always.** A fresh install closes and stops nothing. "Use the suggestions" is one click and only ever
lowers or pauses programs that hold data and closes updaters. Never opt anyone in by changing a default.

**Only make changes that can be undone.** Some Windows services can be stopped by anybody but started again
by nobody but the system (`WdiSystemHost`, `WdiServiceHost` are protected that way). Stopping those would
leave a change nobody can undo, so CatalyX leaves them running. And a leftover record that Windows forbids
us to undo is dropped with a clear note rather than kept for ever: a record that can never be undone must
not block every later restore. `src/services.rs`, `src/engine/apply.rs`, `src/engine/restore.rs`.

**No network, no telemetry, no updater, no tray icon, no background service.** The optional watcher is a
scheduled task the person chooses; it is a plain process that sleeps between games.

## The window

**egui (OpenGL) rather than a native toolkit.** The footprint analysis ([FOOTPRINT.md](FOOTPRINT.md)) showed the
graphics *driver* is most of any OpenGL window's memory, not the toolkit; a native toolkit (native-windows-gui, raw
`windows`/`windows-sys`) would mean rewriting everything egui gives us (custom title bar, accents, cards, animation)
for a few MB. Moving the hand-written Win32 bindings to `windows-sys` is a sensible, separate cleanup (see
[ROADMAP.md](ROADMAP.md)).

**Calm by design.** One accent, tinted surfaces, flat cards and hairlines, four text sizes, an 8-point grid. "When
everything is accented, nothing is." Reduce noise before adding anything. Frameless window with one unified title
bar. Look (Focus/Slate/Mica) x accent x tint x dark/light are four independent choices.

**Footprint rules.** Draw only on change, nothing while minimised (and trim once after minimising, because a
minimised egui window gets no more frames), adaptive monitor sampling, no polling loops. Measured, not assumed.

## The boost control

**One control, three looks, one state.** The launch bar, the gauge and the power ring share a `Ctl` state and
`Act` actions (`src/gui/control.rs`), so they cannot drift apart. It was chosen from 13 mocked ideas (an
interactive gallery was reviewed before any code was written).

**The boosted state is the accent, never a fixed hue.** A fixed green ignored the person's theme. Green is kept
only for check marks and status.

**A summary before every boost, with "Don't ask me again".** It lists exactly what will change, using the same
list as the *This boost will* card. Restore never asks: undoing must never need a confirmation. The switch lives in
Settings > Behaviour.

## Windows features

**Quiet desktop stops the shell (explorer.exe) instead of hiding other windows.** It frees real memory and never
touches another program's windows (hiding windows by handle and re-showing them is fragile and can leave Windows'
own windows half-hidden). Because stopping the shell is the riskiest thing CatalyX does, the way back is built
first: journal entry, **Ctrl+Alt+End** (and if that shortcut is taken the desktop is left on), and a guard
process that restarts the shell if CatalyX dies. Explorer is asked to exit politely (message `0x5B4`) and restarted
with the token it had before plus the user's clean environment; on a PC with UAC off there is no second token, so
"the shell's own token" is the right one. `src/shell.rs`, `src/engine/desktop.rs`.

**Graphics card offsets are fixed and modest**: Safe +50 MHz core / +20 MHz memory, Balanced +100 / +40, clamped to
what the driver allows (the driver reports +1200 MHz as "allowed", which is meaningless as a step). Applied only
while a game runs, with a thermal guard. `src/gpu.rs`.

**CPU: only Windows' own controls** (turbo mode, core parking, minimum state). No multiplier or voltage work.

## Identity

**CatalyX** (a catalyst speeds a reaction up and comes out unchanged). The mark is an X woven from two strokes:
the rising "/" in the accent over the "\" with a gap where they cross (`src/gui/mark.rs`; one geometry for the
title-bar tile, the window icon and the `.ico`). Display name `CatalyX`, command and crate `catalyx`. It was called
PureBoost first: a first start copies `%APPDATA%\PureBoost` to `%APPDATA%\CatalyX` (copy, never move) and an
administrator start renames the scheduled tasks.

## Start-up

**Two programs from one library.** `catalyx.exe` is a Windows *window* program (a console program makes Windows
open a terminal; with Windows Terminal as the default that is a visible flash), `catalyx-cli.exe` is the console
build for scripts, pipes and agents. CI checks `catalyx.exe`'s PE subsystem is 2.

**Created hidden, shown after two frames.** The window used to appear empty at ~40 ms and fill in at ~250 ms. It is
now shown fully drawn at ~250-285 ms (the OpenGL context creation, ~175-215 ms, is the floor). Hardware probes start
after it is shown so they never compete with the first frames. `CATALYX_TRACE=1` records the timeline.

## Process

**Public repo, `MIT OR Apache-2.0`.** History starts clean; nothing derived from other products is tracked, and
private working notes live outside the repository. Commits use the GitHub no-reply address.

**Scripts over habits.** Anything repetitive is a script in `scripts/` (`check`, `release`, `screenshots`,
`click-test`, `startup-profile`), and the release script verifies what was *published*, not just what was built.
Screenshots come from a script with steady demo meters so they are reproducible.

**Test with real input, not only unit tests.** The window is driven with the real mouse (`click-test.ps1`); the
quiet desktop and the crash guard were tested by actually stopping the shell and killing the process.
