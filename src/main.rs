//! Taipan! binary entry point.
//!
//! Two front-ends over one game library:
//!   * **Interactive** (`taipan` / `taipan play`) — a human plays at a live
//!     stdin/stdout loop.
//!   * **Stateless step** (`taipan new` / `step` / `state`) — each invocation
//!     loads a JSON save, applies one action, prints the new state, and saves.
//!     Built for automated/agent play, where every move is a separate process
//!     call and the save file carries state between them.
//!
//! All game logic lives in the `taipan` library crate. This file is just arg
//! parsing, file I/O for saves, and dispatch.

use std::fs;
use std::io::{stdin, stdout, BufReader};
use std::process::exit;

use taipan::action::apply;
use taipan::economy::EconomyMode;
use taipan::game::Game;
use taipan::rng::Rng;
use taipan::run::run_game;
use taipan::state::{from_json, new_save, to_json};
use taipan::ui::StdIo;

const DEFAULT_SEED: u64 = 0xC0FFEE;
const DEFAULT_SAVE: &str = "taipan-save.json";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let cmd = args.first().map(|s| s.as_str()).unwrap_or("");

    let result = match cmd {
        "-h" | "--help" | "help" => {
            print!("{HELP}");
            Ok(())
        }
        "new" => cmd_new(&args[1..]),
        "step" => cmd_step(&args[1..]),
        "state" => cmd_state(&args[1..]),
        // Bare invocation or `play` (plus the legacy --seed/--mode flags) runs
        // the interactive game.
        "play" => cmd_play(&args[1..]),
        _ if cmd.starts_with("--") || cmd.is_empty() => cmd_play(&args),
        other => Err(format!(
            "error: unknown command '{other}'. Try `taipan --help`."
        )),
    };

    if let Err(msg) = result {
        eprintln!("{msg}");
        exit(2);
    }
}

// ---- Interactive front-end -------------------------------------------------

fn cmd_play(args: &[String]) -> Result<(), String> {
    let (seed, mode) = parse_seed_mode(args)?;
    let mut rng = Rng::new(seed);
    let game = Game::new_with_mode(mode, &mut rng);
    let stdin = stdin();
    let mut io = StdIo::new(BufReader::new(stdin.lock()), stdout().lock());
    run_game(&mut io, game, &mut rng);
    Ok(())
}

// ---- Stateless step front-end ----------------------------------------------

fn cmd_new(args: &[String]) -> Result<(), String> {
    let (seed, mode) = parse_seed_mode(args)?;
    let save_path = flag_value(args, "--save").unwrap_or_else(|| DEFAULT_SAVE.to_string());
    let save = new_save(mode, seed);
    let json = to_json(&save);
    fs::write(&save_path, &json).map_err(|e| format!("error: cannot write {save_path}: {e}"))?;
    print!("{json}");
    Ok(())
}

fn cmd_step(args: &[String]) -> Result<(), String> {
    let save_path = flag_value(args, "--save").unwrap_or_else(|| DEFAULT_SAVE.to_string());
    let action =
        flag_value(args, "--action").ok_or("error: step requires --action '<verb> [args]'")?;
    let json = fs::read_to_string(&save_path)
        .map_err(|e| format!("error: cannot read {save_path}: {e}"))?;
    let save = from_json(&json)?;
    let next = apply(&save, &action)?;
    let out = to_json(&next);
    fs::write(&save_path, &out).map_err(|e| format!("error: cannot write {save_path}: {e}"))?;
    print!("{out}");
    Ok(())
}

fn cmd_state(args: &[String]) -> Result<(), String> {
    let save_path = flag_value(args, "--save").unwrap_or_else(|| DEFAULT_SAVE.to_string());
    let json = fs::read_to_string(&save_path)
        .map_err(|e| format!("error: cannot read {save_path}: {e}"))?;
    // Re-emit through parse+serialize so `state` validates the save too.
    let save = from_json(&json)?;
    print!("{}", to_json(&save));
    Ok(())
}

// ---- Arg helpers -----------------------------------------------------------

/// Pull `--flag <value>` from args, if present.
fn flag_value(args: &[String], flag: &str) -> Option<String> {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
}

/// Parse `--seed <n>` and `--mode <name>` with defaults. Used by play and new.
fn parse_seed_mode(args: &[String]) -> Result<(u64, EconomyMode), String> {
    let seed = match flag_value(args, "--seed") {
        Some(v) => v
            .parse::<u64>()
            .map_err(|_| format!("error: '{v}' is not a valid seed"))?,
        None => DEFAULT_SEED,
    };
    let mode = match flag_value(args, "--mode") {
        Some(v) => EconomyMode::parse(&v)
            .ok_or_else(|| format!("error: '{v}' is not a valid mode (classic|trader)"))?,
        None => EconomyMode::Classic,
    };
    Ok((seed, mode))
}

// ---- Help ------------------------------------------------------------------

const HELP: &str = r#"Taipan! — a trading game on the China Seas. Retire with $1,000,000.

USAGE
  taipan [play] [--seed N] [--mode classic|trader]
      Play interactively at a live prompt (for a human).

  taipan new  [--seed N] [--mode classic|trader] [--save FILE]
  taipan step  --action '<verb> [args]' [--save FILE]
  taipan state [--save FILE]
      Stateless turn-by-turn play over a JSON save file. Each `step` applies
      ONE action and prints the new game state as JSON. (Default save file:
      taipan-save.json)

OPTIONS
  --seed N       Seed the RNG for a reproducible game (default 0xC0FFEE).
  --mode M       Economy model: `classic` (random prices) or `trader`
                 (ports specialize — learnable buy-here/sell-there routes).
  --save FILE    Path to the JSON save file for new/step/state.
  --action STR   The action to apply this step (see ACTIONS).

ACTIONS (pass to `step --action`)
  Command actions (when state.pending == "command"):
    buy <good> <qty>      sell <good> <qty>
    travel <port>         deposit <amt>   withdraw <amt>
    pay <amt>             borrow <amt>    store <good> <qty>   (Hong Kong only)
    retire                (only when net_worth >= 1000000)
  Combat actions (when state.pending == "combat"):
    fight                 run             throw <good> <qty>
  Goods: opium silk arms general   Ports: hongkong shanghai nagasaki
                                          saigon manila singapore batavia

STATE JSON (what `new`/`step`/`state` print)
  Read these fields to decide your next move:
    pending      "command" or {"combat":{"enemy_hps":[...],"sunk":N}}
    outcome      "playing" | "won" | "ship_destroyed"
    location, cash, bank, debt, net_worth, guns, health
    prices       {opium,silk,arms,general}  — current port's prices
    hold         {opium,silk,arms,general}  — cargo carried
    last_event   one-line note on what just happened

HOW TO PLAY AUTOMATICALLY (the step loop)
  This is how an agent (e.g. Claude) plays without an interactive terminal:
    1. taipan new --seed 17 --mode trader --save g.json
    2. Read the printed JSON. Look at `prices` and `cash`.
    3. taipan step --save g.json --action 'buy general 30'
    4. taipan step --save g.json --action 'travel shanghai'
       -> if the JSON comes back with pending=="combat", choose fight/run/throw:
          taipan step --save g.json --action 'fight'   (repeat until command)
    5. Sell where prices are high, travel, repeat. Pay debt at Hong Kong.
    6. When net_worth >= 1000000:  taipan step --save g.json --action 'retire'
  Each step is one process call; the save file carries the game between calls.
  Same seed + same actions => identical game (the RNG state is saved too).
"#;
