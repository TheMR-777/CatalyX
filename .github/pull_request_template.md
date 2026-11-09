## What this changes

<!-- One or two sentences. Link the issue if there is one. -->

## If it changes Windows state

- [ ] The change is journaled **before** it is made (`push_undo`) and undone in `restore`
- [ ] The result is read back from Windows before it is reported
- [ ] It is opt-in (nothing new is closed, stopped or tuned by default)
- [ ] Tested with a stand-in game and scratch data (`CATALYX_DATA`), not a real game

## Checks

- [ ] `cargo fmt --check`, `cargo clippy --release` (no warnings), `cargo test --release`
- [ ] `--self-test` passes
- [ ] Docs updated (README / `docs/` / CLI help)
- [ ] UI change: screenshots attached (dark and light)
