# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

## [Unreleased]

### Added

- **Ship management at the Hong Kong shipyard**: `buy guns <n>` (each cannon
  costs cash and reserves hold space), `buy hold <n>` (enlarge the cargo hold),
  and `repair <amt>` (restore hull damage).
- **Resumable interactive play**: `taipan play` saves after every turn and
  resumes the last game automatically; `--new` starts fresh. Shares the save
  format with the step interface.
- Derived state fields `at_home` and `hold_free` for programmatic play.
- CI workflow (fmt + clippy + test) and a tag-triggered release workflow that
  publishes prebuilt macOS/Linux binaries; `install.sh` for a curl install.

### Changed

- **Economy rebalanced** so no single good dominates: arms now has the widest
  price spread, opium narrower, silk/general fill the early game.
- **Step/new/state output is now compact single-line JSON (NDJSON)** instead of
  pretty-printed, so it composes with `jq` and line-oriented tools.
- **Bank interest corrected to 0.5% per arrival** to match the original 1982
  game (`BA * 1.005`); it was previously 1%. Debt remains 10% (faithful).
- Saves carry a `version` field; incompatible saves are rejected clearly.

### Fixed

- Travel arrival events now apply to the destination market in both front-ends
  (the interactive loop previously applied them to a discarded origin market),
  aligning RNG order so saves replay identically across interfaces.
- Hardened arithmetic against overflow/underflow (debt, cash, sell proceeds,
  combat damage, repair, gun/hold purchases) and against corrupt/hand-edited
  saves (hold-capacity and health validation, JSON escape handling).

## [0.1.0] - 2026-06-04

Initial release.

### Added

- Core trading loop: four goods, seven ports, buy/sell with cash and hold
  capacity validation.
- Elder Brother Wu's debt with compounding interest, banking (deposit/withdraw),
  a Hong Kong warehouse, and the **$1,000,000** retirement win condition.
- Original-depth pirate combat as a step-able state machine: multi-ship
  encounters with fight / run / throw-cargo decisions and hull damage.
- Random arrival events: price spikes, price crashes, and Li Yuen extortion.
- Two economy models selectable via `--mode`: `classic` (independent random
  prices) and `trader` (ports specialize into learnable trade routes).
- Seeded, deterministic PRNG (`--seed`) — same seed and inputs reproduce a game
  exactly.
- Interactive front-end (`taipan play`) and a stateless JSON-driven front-end
  (`taipan new` / `step` / `state`) for scripted and automated play.
- Comprehensive unit and integration tests, including economy-winnability
  regression guards. Zero runtime dependencies.

[Unreleased]: https://github.com/turlockmike/taipan-rs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/turlockmike/taipan-rs/releases/tag/v0.1.0
