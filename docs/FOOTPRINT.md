# Footprint

CatalyX is meant to cost almost nothing while you play. This page says what it costs, how that was measured,
and where the cost comes from. All numbers are from one PC (Core i7-6700, GeForce GTX 1050 Ti, Windows 11) and
will differ on yours, mainly because the graphics driver differs.

## Results

"Private" is the private working set, the number Task Manager's *Memory* column shows.

| State | Private memory | CPU (of one core) |
|---|---|---|
| Bare GDI window, no toolkit (the floor for any window) | ~1 MB (1.7 MB committed) | 0.16% |
| Bare OpenGL window, no UI at all | 22-35 MB (75-88 MB committed) | ~0% |
| CatalyX right after start | ~67 MB (about 100 MB working set) | - |
| CatalyX open, idle (after the 30 s trim) | 4-11 MB | 0.6% |
| CatalyX minimised during a game | 1.4-2 MB | 0.08% |
| `catalyx watch` (no window) | 1.3 MB | 0.1-0.3% |
| Guard process (only while the desktop is off) | 1.9 MB | ~0% |

When the upkeep moved off the frame loop (the 0.5.0 work), it was measured beside a rebuild of the 0.4.2
code on the same machine: minimised 0.156% of a core and 1.7 MB private (0.4.2: 0.156%, 1.8 MB); open
idle 0.4-0.7% (0.4.2 on the same day 0.3-0.9%; the recorded baseline 0.469%). The one extra thread is the
upkeep. The peak between trims was 12-15 MB private (0.4.2: 12 MB); both return to 5-8 MB at the trim.

## Startup

Measured with `CATALYX_TRACE=1` (see below) on the same PC. The window appears **fully drawn about 250-285 ms after
launch**; it is never shown empty.

| Milestone | Time since launch |
|---|---|
| program start to "opening the window" | ~3 ms |
| window and OpenGL context ready | 175-215 ms |
| fonts, monitor, engine, app state built | ~2 ms |
| first frame drawn | 200-240 ms |
| second frame drawn, window shown | 245-285 ms |

Almost all of it is Windows and the graphics driver creating the OpenGL context; CatalyX's own start-up work is a
few milliseconds. What keeps it smooth: the window is created **hidden** and shown only once two frames are drawn
(no empty frame, no white flash), the hardware probes (power plans, graphics card, services) start only *after*
the window is shown so they never compete with the first frames, and `catalyx.exe` is a window program, so
Windows never opens a terminal for it (the console build, `catalyx-cli.exe`, is a separate file). `CATALYX_TRACE=1`
appends one line per milestone to `%TEMP%\catalyx-startup.txt`.

## Where the cost comes from

- **The graphics driver.** A window with an OpenGL context and nothing drawn in it costs 22-35 MB private
  here, almost all of it `nvoglv64.dll` and `nvgpucomp64.dll`. That is more than half of what CatalyX uses at
  its peak, and no UI toolkit that uses the GPU can avoid it. The only way below it is to not use the GPU at
  all (GDI or software drawing), which costs anti-aliasing and the soft look.
- **egui, fonts, shaders, buffers**: about 30 MB on top of the driver at the peak, right after start.
- **Committed memory** (~110 MB) is mostly driver reservations and does not shrink; it counts against the
  commit limit, not against RAM.

## What keeps it small

1. **Draw only when something changed, at the old visible cadence.** The core's upkeep thread samples on its
   own deadlines but wakes the window for new readings at most every 2 s (0.6 s while boosting or closing),
   so the process monitor and the graphics card do not draw twice as often. A job result or an event wakes it
   at once; a minimised window draws nothing and is not woken.
2. **Trim.** Every 30 s the working set is handed back to Windows (pages the window is not using right now:
   driver and font pages needed only while drawing). Pages return on demand, a few at a time. Trimming moves
   pages to Windows' standby list; they are still in RAM until something else needs them.
3. **Trim on minimise.** The 30 s timer is not what matters at the moment a game goes fullscreen, so CatalyX
   trims once, 1.5 s after the minimise, from the upkeep thread (this was a measured bug: before the fix a
   minimised window stayed at ~70 MB private).
4. **No polling.** The watcher sleeps for 3 s at a time on a Windows event that Ctrl+C signals (before: it woke
   7 times a second to check a flag, which was most of its CPU).
5. **Adaptive sampling.** The process monitor samples every 2.5 s normally, every 8 s while minimised, and
   every 1.5 s only while boosting or on the Apps page. Per-process detail is collected only when shown. The
   3 s auto-boost check walks the process list only once a game has a known program; with none there is
   nothing it could match.
6. **No helper programs** for readings: NVML for the graphics card, the registry for names.

## How to measure

```powershell
# window open and idle: a series of working set / private / commit, then CPU over 20 s
.\scripts\footprint.ps1 -Exe .\target\release\catalyx.exe -Arguments '--no-elevate' -Seconds 75

# minimised after 10 s (the state during a game)
.\scripts\footprint.ps1 -Exe .\target\release\catalyx.exe -Arguments '--no-elevate' -Seconds 60 -MinimizeAt 10

# the watcher
.\scripts\footprint.ps1 -Exe .\target\release\catalyx.exe -Arguments 'watch --quiet' -Seconds 60 -NoWindow
```

The script uses a scratch data folder (`CATALYX_DATA`), so it never touches your real settings.

The floors come from `tools/floor`, a 150-line native window with no toolkit:

```powershell
cargo build --release --manifest-path tools\floor\Cargo.toml
.\scripts\footprint.ps1 -Exe .\tools\floor\target\release\floor.exe -Arguments gdi -Seconds 20
.\scripts\footprint.ps1 -Exe .\tools\floor\target\release\floor.exe -Arguments gl  -Seconds 20
```

## Caveats

- One machine, one driver. An AMD or Intel GPU loads different libraries.
- CPU figures vary run to run by a factor of two at this scale (0.1-0.3% for the watcher); treat them as
  orders of magnitude.
- "Working set" overstates real memory use for any process (shared pages count); the private working set is the
  honest number.
