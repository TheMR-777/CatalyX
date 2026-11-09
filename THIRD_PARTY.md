# Third-party components

CatalyX is licensed `MIT OR Apache-2.0`. It is built from the open-source crates below (and their
dependencies, 300+ packages in total; every one of them uses a permissive license: MIT, Apache-2.0, Zlib,
BSD-3-Clause, Unicode-3.0, BSL-1.0, Unlicense, or a combination). `cargo metadata` lists them all with their
licenses; `Cargo.lock` pins the exact versions.

| Crate | Used for | License |
|---|---|---|
| [eframe](https://github.com/emilk/egui) / [egui](https://github.com/emilk/egui) 0.35 | the window (OpenGL via glow) | MIT OR Apache-2.0 |
| [egui-components](https://crates.io/crates/egui-components) 0.3 | a few widgets (switch, dialogs, toasts) | Apache-2.0 |
| [serde](https://serde.rs) / serde_json | the config file and the recovery journal | MIT OR Apache-2.0 |
| [sysinfo](https://github.com/GuillaumeGomez/sysinfo) 0.30 | memory and process statistics | MIT |
| [winreg](https://github.com/gentoo90/winreg-rs) | registry access | MIT |
| [png](https://github.com/image-rs/image-png) | writing screenshots and icon images | MIT OR Apache-2.0 |

## Fonts

egui's `default_fonts` feature compiles a few open fonts into the program (for text and symbols); they carry
their own licenses (see egui's repository). On Windows the interface text uses **Segoe UI**, read from
`C:\Windows\Fonts` at run time; it is not bundled or redistributed.

## What is not here

No code, images, sounds, fonts, program files or text from any other game booster are included, and CatalyX
is not affiliated with or endorsed by their makers.
