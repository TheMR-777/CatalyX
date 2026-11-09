# Contributing to CatalyX

Thanks for helping. CatalyX is small on purpose, and the rules below keep it that way.

## Before you start

- Open an issue first for anything bigger than a bug fix, so we can agree on the shape. Features that touch
  Windows state need a clear answer to: *how is it journaled, how is it undone, how is it verified?*
- Read [AGENTS.md](AGENTS.md) (conventions, the rules that must not be broken, pitfalls) and
  [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md).

## What we will not merge

Ads, telemetry, network access, an auto-updater, a tray icon, a background service, kernel drivers, code
injection or anything that reads or writes another program's memory, or defaults that close programs or stop
services without the person's say-so.

## Workflow

1. Fork, branch from `main`.
2. `cargo build --release`, `cargo test --release`, `cargo clippy --release` (no warnings), `cargo fmt`.
3. Run the self-test: `.\target\release\catalyx.exe --self-test shots\st.json --no-elevate`.
4. For UI changes, attach screenshots (dark and light) made with `--screenshot` (see
   [docs/DEVELOPING.md](docs/DEVELOPING.md)).
5. Update the docs that your change touches (README, `docs/`, the CLI help).
6. Open a pull request using the template. Small, focused pull requests are reviewed fastest.

## Testing safely

Use scratch data (`$env:CATALYX_DATA`), stand-in games (a renamed copy of `ping.exe`) and `--dry-run`. Never run
a real game or an anti-cheat launcher in a test. Tests must have time limits and must not touch your real
settings.

## Commit messages

Imperative, specific, one idea per commit: `Restore the shell when the guard sees the parent die`.

## Licensing

By contributing you agree that your contribution is licensed under `MIT OR Apache-2.0`, as the rest of the
project. Do not contribute code, text or data copied from other products; describe behaviour in your own
words and measure things on a real PC.

## Reporting a bug

Include your Windows version, whether UAC is on, the output of `catalyx doctor`, and the activity log from the
session card. If a boost left something changed, run `catalyx restore` first.
