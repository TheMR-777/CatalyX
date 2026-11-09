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

## Where the cost comes from

- **The graphics driver.** A window with an OpenGL context and nothing drawn in it costs 22-35 MB private
  here, almost all of it `nvoglv64.dll` and `nvgpucomp64.dll`. That is more than half of what CatalyX uses at
  its peak, and no UI toolkit that uses the GPU can avoid it. The only way below it is to not use the GPU at
  all (GDI or software drawing), which costs anti-aliasing and the soft look.
- **egui, fonts, shaders, buffers**: about 30 MB on top of the driver at the peak, right after start.
- **Committed memory** (~110 MB) is mostly driver reservations and does not shrink; it counts against the
  commit limit, not against RAM.

## What keeps it small

1. **Draw only when something changed.** A 2 s tick for the live meters (0.6 s while boosting) and *no*
   repaints while minimised.
2. **Trim.** Every 30 s the working set is handed back to Windows (pages the window is not using right now:
   driver and font pages needed only while drawing). Pages return on demand, a few at a time. Trimming moves
   pages to Windows' standby list; they are still in RAM until something else needs them.
3. **Trim on minimise.** A minimised window never runs its frame logic again, so the 30 s timer never fires
   for it. CatalyX trims once, 1.5 s after the minimise (this was a measured bug: before the fix a minimised
   window stayed at ~70 MB private).
4. **No polling.** The watcher sleeps for 3 s at a time on a Windows event that Ctrl+C signals (before: it woke
   7 times a second to check a flag, which was most of its CPU).
5. **Adaptive sampling.** The process monitor samples every 2.5 s normally, every 8 s while minimised, and
   every 1.5 s only while boosting or on the Apps page. Per-process detail is collected only when shown.
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
