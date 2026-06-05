# Taipan! (Rust)

[![CI](https://github.com/turlockmike/taipan-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/turlockmike/taipan-rs/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

A dependency-free Rust recreation of the classic 1982 trading game *Taipan!* —
build your fortune on the 19th-century China Seas. Buy low in one port, sell
high in another, fend off pirates, manage your debt to the moneylender, and
retire with a net worth of **$1,000,000**.

> **Independent reimplementation.** This is an original, from-scratch
> implementation inspired by the gameplay of the 1982 game. It contains none of
> the original's code, text, or assets, and is not affiliated with or endorsed
> by the original authors or any rights holder. Game *mechanics* are not
> copyrightable; all code here is original work, MIT-licensed (see
> [LICENSE](LICENSE)).

## Highlights

- **Zero dependencies.** Pure `std` — no crates, including a hand-rolled seeded
  PRNG and JSON serializer. Builds with `cargo` or even bare `rustc`.
- **Deterministic.** A `--seed` plus your inputs fully determine a game. Same
  seed + same moves = byte-identical playthrough.
- **Two economy models.** `classic` (independent random prices, the original
  feel) and `trader` (ports specialize, creating learnable trade routes).
- **Two front-ends over one core.** A live interactive prompt for humans, and a
  stateless turn-by-turn JSON interface for scripted/automated play.

## Install

### Quick install (macOS / Linux)

Downloads the right prebuilt binary for your platform and installs it to
`~/.local/bin`:

```sh
curl -fsSL https://raw.githubusercontent.com/turlockmike/taipan-rs/main/install.sh | sh
```

Set `TAIPAN_INSTALL_DIR` to choose a different location. Make sure the install
directory is on your `PATH`.

### With Cargo

If you have a Rust toolchain ([rustup](https://rustup.rs)):

```sh
cargo install --git https://github.com/turlockmike/taipan-rs

# or from a clone:
cargo install --path .
```

### From source, without installing

```sh
cargo run -- play
```

## Play (interactive)

```sh
taipan                              # classic economy, default seed
taipan play --mode trader           # learnable trade routes
taipan play --mode trader --seed 17 # reproducible game
```

You'll see the comprador's report (your finances, hold, ship) and the current
port's prices, then a prompt:

```
Shall I (B)uy, (S)ell, (T)ravel, ban(K), (R)etire, or (Q)uit?
```

Buy where a good is cheap, sail, sell where it's dear, pay down your debt at
Hong Kong, survive the pirates, and retire at $1M.

## Play (scripted / automated)

The interactive loop blocks on stdin, which is awkward to drive
programmatically. The `new` / `step` / `state` subcommands provide a
**stateless, one-action-at-a-time** interface over a JSON save file. Each call
applies a single action and prints the resulting game state as one line of
compact JSON (NDJSON), so it composes with `jq` and line-oriented tools.

```sh
taipan new  --seed 17 --mode trader --save g.json   # start; prints state
taipan step --save g.json --action 'buy general 26'
taipan step --save g.json --action 'travel singapore' | jq '.net_worth'
# If the returned JSON has pending == "combat", choose fight/run/throw:
taipan step --save g.json --action 'fight'
taipan step --save g.json --action 'sell general 26'
taipan state --save g.json | jq .                   # readable view for humans
```

Each call prints a single JSON line — pipe through `jq .` to pretty-print, or
`jq '.cash'` / `jq '.hold'` to pull fields. The save file carries the entire
game between invocations — including the RNG state, so determinism survives a
reload. Run `taipan --help` for the full action vocabulary and field reference.

### Actions

| Context (`pending`) | Actions |
| --- | --- |
| `command` | `buy <good> <qty>`, `sell <good> <qty>`, `travel <port>`, `deposit <amt>`, `withdraw <amt>`, `pay <amt>`, `borrow <amt>`, `store <good> <qty>` (Hong Kong only), `retire` |
| `combat` | `fight`, `run`, `throw <good> <qty>` |

Goods: `opium silk arms general`. Ports: `hongkong shanghai nagasaki saigon
manila singapore batavia`.

## Architecture

All game logic lives in the library crate (`src/lib.rs`); the binary
(`src/main.rs`) is a thin shell that parses arguments and wires up I/O. The
modules form a clean dependency layering — each knows only what it must:

| Module | Responsibility |
| --- | --- |
| `rng.rs` | Seeded xorshift PRNG. Deterministic; serializable state. |
| `market.rs` | Goods, prices, the cargo hold, buy/sell rules. Port-agnostic. |
| `travel.rs` | The seven ports and movement between them. |
| `economy.rs` | Economy models and per-port price bias (the `--mode` lever). |
| `combat.rs` | Pirate combat as a step-able state machine. |
| `events.rs` | Arrival events: price spikes/crashes, extortion, pirates. |
| `game.rs` | The mutable game world; debt, banking, the $1M win condition. |
| `ui.rs` | An `Io` trait (real stdin/stdout vs. scripted) and rendering. |
| `run.rs` | The interactive turn loop. |
| `state.rs` | JSON save/restore of a full game (dependency-free). |
| `action.rs` | Parse and apply one action — the engine behind `step`. |

Two design choices worth knowing as a contributor:

- **The `Io` trait** decouples game logic from the terminal. Production uses
  real stdin/stdout; tests inject scripted input and capture output. This is
  what lets entire playthroughs (combat and all) run inside `cargo test`.
- **`market.rs` never imports `Port`.** It receives a plain `[i32; 4]` price
  bias; the `economy` layer translates a port into that bias. Adding a new
  economy model touches only `economy.rs` — `Market` stays unchanged.

## Develop & test

```sh
cargo test            # unit + integration tests
cargo clippy --all-targets
cargo run -- play
```

The test suite covers each module in isolation plus end-to-end integration
tests that drive the public interfaces — including a regression guard proving
the economy is *winnable* (a $1M run is reachable by competent play) and a test
that fights a pirate encounter to resolution purely through the `step` engine.

See [CONTRIBUTING.md](CONTRIBUTING.md) for conventions and where to start.
