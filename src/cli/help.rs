//! The text `catalyx help` prints.

pub(super) const HELP: &str = "\
CatalyX - a lean game booster. Run without arguments to open the window.

USAGE
  catalyx <command> [arguments] [--json]

PLAY
  play <game> [--preset P] [--dry-run]   Boost, start the game, restore when it closes
  attach <program|pid> [--preset P]      Boost a game that is already running
  boost [--preset P] [--dry-run]         Gaming mode without a game, until Enter or Ctrl+C
  (play, boost and attach also take --for <seconds>: put everything back after that long)
  doctor [--gpu] [--json]                Prove on this PC that each boost feature really works
  restore                                Undo a boost left over from a crash
  desktop [status|restore|test [s]]      The quiet desktop (taskbar off while a game runs):
                                         restore brings it back now; test tries it for s seconds
  status                                 Live CPU, memory, GPU and the busiest apps

LIBRARY
  games                                  List your games
  add <path> [--name N] [--args A] [--preset P]
                                         Add a game (.exe, .bat, .cmd, .lnk, .url, store link)
  remove <game>                          Remove a game
  rename <game> <new name>
  exe <game> <program.exe>               Set the real game program behind a launcher

PROFILES
  presets                                List global presets
  preset <game> <preset>                 Give a game a preset
  default-preset <preset>                The preset for new games and gaming mode
  show <game|preset>                     The settings a game or preset uses
  set <game|preset> <key> <value>        Change one setting (see KEYS); a game gets its own settings
  own <game> | inherit <game>            Give a game its own settings / go back to its preset

CONTROL
  apps [--game G]                        App rules (keep, pause, lower, close)
  app <program.exe> <keep|pause|lower|close> [--game G]
  services [--game G] [--all]            Service rules (stop while gaming)
  service <name> <stop|keep> [--game G]

SYSTEM
  plans                                  Power plans on this PC
  plan <name|guid>                       Switch power plan now
  gpu                                    Graphics card, offsets and safe steps
  theme [focus|slate|mica] [dark|light|system] [accent] [tint-off|tint-subtle|tint-rich] [bar|gauge|ring]
  option <name> <on|off>                 restore-on-exit, reopen-apps, keep-off, auto-boost, run-as-admin, confirm-boost
  heat-limit <75|80|83|87>               GPU temperature that undoes the overclock
  elevate setup|remove|status            One-click administrator start (one Windows prompt, once)
  autostart on|off|status                Background watcher: boosts your games without a window
  watch [--quiet]                        Run the watcher now in this terminal
  config path | export <file> | import <file> | reset
  gui | version | help

KEYS for `set`
  priority normal|above|high             power keep|high|catalyx   gpu off|safe|balanced
  cpu off|boost|max                      apps services clean-ram game-mode dvr-off notifications-off transparency-off
  maintenance-off mmcss no-throttling lower-others io-boost purge-standby quiet-desktop   (on|off)

Games and presets can be given by name (any unique part), id or number from `games`.
";
