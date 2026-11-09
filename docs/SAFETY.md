# Safety model

A game booster changes things on your PC while you play. The only acceptable version of that is one that
**always puts things back**, even after a crash. This is how CatalyX does it, and what it refuses to do.

## Principles

1. **Journal first.** Before any change, the record of how to undo it is written to
   `%APPDATA%\CatalyX\journal.json` (flushed, then moved into place atomically; never truncated in place).
   If the journal cannot be saved, that change is *not made*.
2. **One session at a time.** A named mutex (`Local\CatalyX.ActiveBoost`) stops two windows or a window and
   the watcher from changing Windows at once and overwriting each other's journal.
3. **Opt-in.** A fresh install closes and stops nothing. Suggestions are one click, and only lower or pause
   programs that hold data; only updaters and Windows extras are closed.
4. **Verify, don't claim.** Power settings, priorities and clocks are read back from Windows; if the read-back
   disagrees, the step is not reported as done. `catalyx doctor` runs each step against real Windows state.
5. **Undo is reverse order and identity-checked.** A process change is only undone on the *same* process (pid
   plus start time), so a recycled process id is never touched. Anything that cannot be undone stays in the
   journal for the next restore instead of being forgotten.
6. **Small and boring.** Documented Windows controls only. No kernel drivers, no code injection, no hooks, no
   reading or writing another program's memory, no network access, no telemetry.

## What a boost changes

| Change | Undone by | Needs admin |
|---|---|---|
| Active power plan (a temporary copy of yours) | your plan made active again, copy deleted | no |
| Registry values (Game Mode, recording, notifications, transparency, multimedia scheduler) | each value restored, or deleted if it did not exist | partly (machine-wide ones) |
| Scheduled defrag task / maintenance | re-enabled | yes |
| Services you chose | started again, reverse order | yes |
| Apps: lowered / paused | priority and suspend state put back | no (admin for elevated programs) |
| Apps: closed | not reopened unless you opt in | no |
| Process working sets, standby list | nothing to undo (Windows refills them) | standby flush: yes |
| Game priority, disk priority, memory priority, efficiency mode | restored while the process lives; gone when it exits | admin if the game is elevated |
| Graphics card clock offsets | offsets read before the change and written back | yes |
| Windows' shell (quiet desktop) | shell started again as the user | no |

## Never touched

- Windows itself and its shell parts, security software, anti-cheat services and drivers' helpers, input
  methods, audio services (see `NOT_A_GAME` and the protected lists in `src/config/lists.rs`), and CatalyX.
- Realtime priority is never offered (it can freeze Windows' own input).
- "Require a password on wake" and lid-close actions are not changed in the game power plan.
- A running game's memory is never read or written.

## Crash recovery

| Situation | What happens |
|---|---|
| CatalyX is closed during a boost | it restores first (up to 20 seconds), then closes |
| Ctrl+C in a CLI session | it restores before exiting |
| CatalyX crashes or is killed | the journal remains; the next start shows "a previous boost was interrupted" with **Restore now**, or run `catalyx restore` |
| The quiet desktop was on | restored automatically at the next start, and within seconds by the guard process |
| Power cut | same as a crash; the journal is on disk |
| Some change cannot be undone | it stays in the journal and the app says so |

## The quiet desktop

Stopping Explorer is the riskiest thing CatalyX does, so the way back is built first:

1. The shortcut **Ctrl+Alt+End** is registered. If another program owns it, the desktop is *not* switched
   off.
2. The "desktop is off" record goes into the journal.
3. A guard copy of CatalyX (`catalyx shell-guard <pid>`, about 2 MB) is started. It watches the main process;
   if that ends while the journal still says "off", it restarts the shell and clears the record.
4. Only then is Explorer asked to exit, the way its own "Exit Explorer" menu entry does (message `0x5B4` to the
   taskbar), so it saves its state and Windows does not treat it as a crash. It is ended by force only if it
   has not left after 8 seconds.
5. Before stopping it, CatalyX copies the token Explorer runs with. It restarts the shell with that token
   (after a crash: the user's ordinary token) and the user's own environment block, on the interactive
   desktop, and waits for the taskbar to appear. A shell started from the wrong context is what leaves a
   misbehaving Start menu.

It is used only for a game CatalyX started itself, never for `attach`, auto-boost or "Boost only".
Open File Explorer windows close with it. `catalyx desktop restore` is the manual way back at any time.

## Graphics card safeguards

Offsets are small and fixed (see [HOW-IT-WORKS.md](HOW-IT-WORKS.md#5-graphics-card-nvidia)), clamped to what
the driver reports as allowed, applied only while a game runs, and backed off at once at your heat limit or
when the temperature reading is lost. A failed restore is retried. Only NVIDIA cards are supported; there is
no voltage control.

## Reporting a problem

If a boost ever leaves something changed, run `catalyx restore`, then open an issue with the output of
`catalyx doctor` and the activity log from the session card.
