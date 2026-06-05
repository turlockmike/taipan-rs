# Changelog

All notable changes to this project are documented here. The format is based on
[Keep a Changelog](https://keepachangelog.com/), and this project adheres to
[Semantic Versioning](https://semver.org/).

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

[0.1.0]: https://github.com/turlockmike/taipan-rs/releases/tag/v0.1.0
