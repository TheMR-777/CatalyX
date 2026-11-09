# How a boost works

Every step below is journaled before it is made and undone afterwards (see [SAFETY.md](SAFETY.md)). The
order matters: things that can fail cleanly come first, the game starts last.

Settings come from a **preset** (Gentle, Balanced, Max Squeeze, or your own) or from a game's own settings.
Each step is a switch in Profiles; `catalyx show <preset>` lists them all.

## 1. Power plan and CPU

CatalyX makes *a copy of your own active power plan* (so your other choices stay) and writes into the copy:

- **game settings, always:** processor maximum state 100%, active cooling, no disk, sleep, hibernate or
  display timeouts, adaptive brightness off, wireless adapter at maximum performance, PCIe link power
  management off, USB selective suspend off. (Deliberately *not* touched: "require a password on wake" and
  the lid-close action, because they lower security or can overheat a laptop in a bag.)
- **the CPU level you chose** (below).

The copy is rebuilt at every boost so it never carries stale values. Afterwards your original plan is made
active again and the copy is deleted.

The CPU level is a switch in the profile:

| Level | What it changes |
|---|---|
| Off | leaves the processor's power settings alone |
| Boost | turbo mode "aggressive", 100% minimum state, cores never parked |
| Max sustained | Boost, plus clocks jump up at once and stay up |

Everything is read back from Windows (`powercfg`) and the result is shown as measured: the session card
compares the CPU clock before and after. There is no multiplier or voltage overclocking.

## 2. Windows

Registry values, each remembered and restored (a value that did not exist is deleted again):

| Switch | Value | Scope |
|---|---|---|
| Windows Game Mode | `HKCU\Software\Microsoft\GameBar` `AutoGameModeEnabled`, `AllowAutoGameMode` = 1 | user |
| Background recording off | `HKCU\...\GameDVR` `AppCaptureEnabled` = 0 | user |
| Quiet notifications | `HKCU\...\PushNotifications` `ToastEnabled` = 0 | user |
| Transparency off | `HKCU\...\Themes\Personalize` `EnableTransparency` = 0 | user |
| Maintenance paused | `HKLM\...\Schedule\Maintenance` `MaintenanceDisabled` = 1, and the scheduled defrag task disabled | machine |
| Multimedia scheduler | `HKLM\...\Multimedia\SystemProfile` and `...\Tasks\Games` (game scheduling class tuned up) | machine |

"Keep Windows efficiency mode off for the game" clears the process power-throttling flag on the game.

## 3. Services and apps

**Services** you chose are stopped with their running dependents first, each stop limited to 3 seconds. A
service that does not stop in time is skipped for the rest of the day (`service_fail.json`). Protected
services are never touched, and if a dependent is protected the service is left running. Everything that
was stopped is started again afterwards, in reverse order. Needs administrator rights.

**Apps** each have one of four actions: **Keep** (nothing), **Lower** (lowest CPU priority), **Pause**
(suspended and resumed afterwards), **Close** (ended). Nothing is selected by default. "Use the suggestions"
lowers or pauses programs that hold data and closes only updaters and Windows extras. Protected programs
(Windows itself, the shell, anti-cheat, drivers' helpers, CatalyX) are never touched. "Lower everything
else" puts every other app of the same user at the lowest priority for the session. While the game runs,
apps and services that come back are put away again ("keep off").

Closed apps are not reopened unless you turn that on.

## 4. Memory

- **Free memory first** trims the working set of the processes this user may touch (`EmptyWorkingSet`). There
  is no size threshold, since a small process costs nothing to trim. Pages return on demand.
- **Flush cached memory** (administrator) empties Windows' standby list with the same memory-list command
  RAMMap's "Empty" menu uses, so the game's memory is free at once. Windows rebuilds the cache as needed.

## 5. Graphics card (NVIDIA)

Clock offsets are set through the NVIDIA driver's own interface (NvAPI performance states) and read back.
The offsets are deliberately small and fixed: **Safe** +50 MHz core / +20 MHz memory, **Balanced** +100 / +40,
always clamped to what the driver says is allowed. Readings (temperature, clocks, utilisation) come from
NVML. A thermal guard checks the temperature every 10 seconds and puts the clocks back at once if it reaches
your heat limit (75, 80, 83 or 87 degrees; default 83) or if the reading is lost. `catalyx gpu test` applies
and removes the offsets in 3 seconds so you can see it working. AMD and Intel cards are left alone.

## 6. Starting the game

The game is started the way Explorer starts a double-click, so Windows applies the file's own settings
("Run as administrator", compatibility modes) and the game gets a clean environment. If Windows refuses, the
start is retried as administrator, then directly, then through `cmd /c start`. Accepted: `.exe`, `.bat`,
`.cmd`, `.lnk`, `.url` and store links (`steam://...`).

### Finding the real game

Launchers and batch files start a different program than the one that draws the game. CatalyX works out the
real one from, in order of strength:

1. the program learned on an earlier run (kept in the profile; editable in Games or `catalyx exe`);
2. names the launcher itself mentions: the file name, executables named inside a `.bat`/`.cmd` file or a
   shortcut's target, and program names embedded in a small launcher executable;
3. programs started after the launch from the game's own folder or by the launcher's process tree;
4. as a last resort, whichever program has been working the graphics card hard for a few checks in a row
   (after 20 seconds).

Windows programs (`cmd.exe`, `ping.exe`...) and known launchers/helpers are never taken for the game. A known
program is only replaced after a different one has stayed the active game for 25 seconds. If nothing is found
within 4 minutes, everything is put back.

### Boosting the game

Once found, the game gets the priority from the profile (Normal, Above normal or High; never Realtime), high
disk (I/O) priority, protected memory page priority, and efficiency mode off. The disk priority is read back
from Windows and only claimed when it took.

## 7. Quiet desktop

See the README and [SAFETY.md](SAFETY.md#the-quiet-desktop). About 6 seconds after the game is found, Windows'
shell is switched off; it comes back when the game closes, on **Ctrl+Alt+End**, or from the **Bring it back**
button.

## 8. While it runs, and restore

The engine checks every 1.5 seconds: it keeps looking for a better game match for the first 90 seconds,
re-stops what came back, runs the graphics thermal guard, and notices when the game closes (8 seconds after
the process is gone, so a launcher handing over is not mistaken for the end). Then every journaled change is
undone in reverse order. Anything that cannot be undone stays in the journal for `catalyx restore` rather than
being forgotten.

## The background watcher (optional)

`catalyx autostart on` registers a sign-in task running `catalyx watch --quiet`: a window-less process that
checks the process list every 3 seconds, and when a game from your library starts it boosts it (attach mode)
and restores when it closes. Between games it only sleeps.
