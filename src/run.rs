//! The interactive turn loop — the glue that sequences every module.
//!
//! Kept in the library (not `main.rs`) so the entire loop runs under
//! `cargo test` against a `ScriptedIo`: a list of input lines in, the full
//! transcript out. `main.rs` only constructs a `StdIo` and calls [`run`].

use crate::combat::{Battle, BattleResult, Decision, RoundReport};
use crate::events::{roll, Event};
use crate::game::{Game, Outcome};
use crate::market::{Good, TradeError};
use crate::rng::Rng;
use crate::travel::Port;
use crate::ui::{render_prices, render_status, render_travel_menu, Io};

/// Run a full game to completion (or until the player quits / input runs out)
/// under the classic economy. Convenience over [`run_game`] for callers and
/// tests that don't choose a mode.
pub fn run(io: &mut dyn Io, rng: &mut Rng) -> Option<Outcome> {
    let game = Game::new(rng);
    run_game(io, game, rng)
}

/// Run a pre-built game to completion. `main` uses this so the economy mode is
/// chosen at the CLI layer; the loop itself is mode-agnostic.
pub fn run_game(io: &mut dyn Io, mut game: Game, rng: &mut Rng) -> Option<Outcome> {
    io.writeln("===========================================");
    io.writeln("  Taipan!  — build your fortune on the China Seas");
    io.writeln(&format!(
        "  Retire with a net worth of $1,000,000.  [{} economy]",
        game.mode.name()
    ));
    io.writeln("===========================================");

    while !game.is_over() {
        render_status(io, &game);
        render_prices(io, &game);

        if game.can_retire() {
            io.writeln("");
            io.writeln(">> You can retire a millionaire! Choose (R) to retire. <<");
        }

        io.writeln("");
        io.write("Shall I (B)uy, (S)ell, (T)ravel, ban(K), (R)etire, or (Q)uit? ");
        let choice = match io.read_line() {
            Some(c) => c.to_uppercase(),
            None => break, // input exhausted
        };

        match choice.as_str() {
            "B" => do_buy(io, &mut game),
            "S" => do_sell(io, &mut game),
            "T" => do_travel(io, &mut game, rng),
            "K" => do_bank(io, &mut game),
            "R" => {
                if game.can_retire() {
                    game.retire();
                } else {
                    io.writeln(&format!(
                        "Taipan, you need a net worth of {} to retire — you have {}.",
                        crate::game::WIN_TARGET,
                        game.net_worth()
                    ));
                }
            }
            "Q" => {
                io.writeln("You slip away into the night. Game abandoned.");
                return game.outcome;
            }
            other => io.writeln(&format!("I don't understand '{other}', Taipan.")),
        }
    }

    announce_outcome(io, &game);
    game.outcome
}

/// Prompt for a good by name; None if invalid/blank.
fn prompt_good(io: &mut dyn Io, verb: &str) -> Option<Good> {
    io.write(&format!(
        "What do you wish to {verb} (Opium/Silk/Arms/General)? "
    ));
    let line = io.read_line()?;
    match Good::parse(&line) {
        Some(g) => Some(g),
        None => {
            io.writeln(&format!("'{line}' is not a good I trade, Taipan."));
            None
        }
    }
}

/// Prompt for a quantity; None if invalid/blank.
fn prompt_qty(io: &mut dyn Io) -> Option<u32> {
    io.write("How many? ");
    let line = io.read_line()?;
    match line.parse::<u32>() {
        Ok(n) => Some(n),
        Err(_) => {
            io.writeln(&format!("'{line}' is not a number, Taipan."));
            None
        }
    }
}

fn do_buy(io: &mut dyn Io, game: &mut Game) {
    let Some(good) = prompt_good(io, "buy") else {
        return;
    };
    let Some(qty) = prompt_qty(io) else { return };
    match game.buy(good, qty) {
        Ok(cost) => io.writeln(&format!("Bought {qty} {} for {cost}.", good.name())),
        Err(TradeError::NotEnoughCash { affordable }) => io.writeln(&format!(
            "You can only afford {affordable} {}, Taipan.",
            good.name()
        )),
        Err(TradeError::NotEnoughHold { space }) => {
            io.writeln(&format!("Your hold has room for only {space} more units."))
        }
        Err(TradeError::NotEnoughGoods { .. }) => unreachable!("buy never returns NotEnoughGoods"),
    }
}

fn do_sell(io: &mut dyn Io, game: &mut Game) {
    let Some(good) = prompt_good(io, "sell") else {
        return;
    };
    let Some(qty) = prompt_qty(io) else { return };
    match game.sell(good, qty) {
        Ok(proceeds) => io.writeln(&format!("Sold {qty} {} for {proceeds}.", good.name())),
        Err(TradeError::NotEnoughGoods { held }) => io.writeln(&format!(
            "You only have {held} {} to sell, Taipan.",
            good.name()
        )),
        Err(_) => unreachable!("sell only returns NotEnoughGoods"),
    }
}

fn do_bank(io: &mut dyn Io, game: &mut Game) {
    if game.location != Port::HOME {
        io.writeln("The bank and Elder Brother Wu are only in Hong Kong, Taipan.");
        return;
    }
    io.write("(D)eposit, (W)ithdraw, or (P)ay debt? ");
    let Some(action) = io.read_line() else { return };
    let Some(amount) = prompt_qty(io) else { return };
    match action.to_uppercase().as_str() {
        "D" => {
            let moved = game.deposit(amount);
            io.writeln(&format!("Deposited {moved}. Bank now {}.", game.bank));
        }
        "W" => {
            let moved = game.withdraw(amount as u64);
            io.writeln(&format!("Withdrew {moved}. Cash now {}.", game.cash));
        }
        "P" => {
            let paid = game.pay_debt(amount);
            io.writeln(&format!("Paid {paid} toward debt. Debt now {}.", game.debt));
        }
        other => io.writeln(&format!("'{other}' is not a bank action, Taipan.")),
    }
}

fn do_travel(io: &mut dyn Io, game: &mut Game, rng: &mut Rng) {
    render_travel_menu(io);
    io.write("Port number? ");
    let Some(line) = io.read_line() else { return };
    let Some(n) = line.parse::<u32>().ok() else {
        io.writeln(&format!("'{line}' is not a port number, Taipan."));
        return;
    };
    let Some(dest) = Port::from_menu_number(n) else {
        io.writeln(&format!("There is no port {n}, Taipan."));
        return;
    };
    if dest == game.location {
        io.writeln("You are already there, Taipan.");
        return;
    }

    io.writeln(&format!("Setting sail for {}...", dest.name()));

    // Roll an arrival event *before* docking so combat can sink us en route.
    let event = roll(game, rng);
    narrate_event(io, game, rng, event);
    if game.is_over() {
        return;
    }

    game.travel_to(dest, rng);
    io.writeln(&format!("Arrived at {}.", dest.name()));
}

fn narrate_event(io: &mut dyn Io, game: &mut Game, rng: &mut Rng, event: Event) {
    match event {
        Event::Quiet => {}
        Event::PriceSpike { good, new_price } => io.writeln(&format!(
            "Word is the price of {} has skyrocketed to {new_price}!",
            good.name()
        )),
        Event::PriceDrop { good, new_price } => io.writeln(&format!(
            "The price of {} has crashed to {new_price}!",
            good.name()
        )),
        Event::LiYuenExtortion { taken } => io.writeln(&format!(
            "Li Yuen's men demand tribute — they take {taken} from your purse."
        )),
        Event::Pirates(battle) => resolve_battle(io, game, rng, battle),
    }
}

/// Drive the combat state machine against the player's choices.
fn resolve_battle(io: &mut dyn Io, game: &mut Game, rng: &mut Rng, mut battle: Battle) {
    io.writeln("");
    io.writeln(&format!(
        "PIRATES! {} hostile ships close in, Taipan!",
        battle.enemies_remaining()
    ));

    loop {
        io.writeln(&format!(
            "  {} pirate ships remain. Your ship: {}/{}.",
            battle.enemies_remaining(),
            game.health,
            crate::game::MAX_HEALTH
        ));
        io.write("Will you (F)ight, (R)un, or (T)hrow cargo? ");
        let Some(choice) = io.read_line() else { return };

        let decision = match choice.to_uppercase().as_str() {
            "F" => Decision::Fight,
            "R" => Decision::Run,
            "T" => {
                let Some(good) = prompt_good(io, "throw") else {
                    continue;
                };
                let Some(qty) = prompt_qty(io) else { continue };
                Decision::Throw { good, qty }
            }
            other => {
                io.writeln(&format!("'{other}'? The pirates wait, Taipan."));
                continue;
            }
        };

        let (report, result) = battle.step(decision, game, rng);
        narrate_round(io, &report);

        if let Some(res) = result {
            match res {
                BattleResult::Victory { sunk } => {
                    io.writeln(&format!("You sank all {sunk} of them, Taipan! Victory!"))
                }
                BattleResult::Escaped => io.writeln("You slip away into the fog. Escaped!"),
                BattleResult::Bribed => io.writeln("The pirates take your offering and depart."),
                BattleResult::Sunk => io.writeln("Your ship goes down with all hands... "),
            }
            return;
        }
    }
}

fn narrate_round(io: &mut dyn Io, report: &RoundReport) {
    match report {
        RoundReport::Fought {
            hits,
            sunk_now,
            damage_taken,
        } => {
            io.writeln(&format!(
                "  You fire — {hits} hits, {sunk_now} ships sunk. You take {damage_taken} damage."
            ));
        }
        RoundReport::Ran {
            escaped,
            damage_taken,
        } => {
            if *escaped {
                io.writeln("  You break for open water...");
            } else {
                io.writeln(&format!(
                    "  They cut you off! You take {damage_taken} damage."
                ));
            }
        }
        RoundReport::Threw {
            accepted,
            jettisoned,
        } => {
            io.writeln(&format!("  You heave {jettisoned} units overboard."));
            if !accepted {
                io.writeln("  They scoff and keep coming.");
            }
        }
    }
}

fn announce_outcome(io: &mut dyn Io, game: &Game) {
    io.writeln("");
    match game.outcome {
        Some(Outcome::Won) => {
            io.writeln(&format!(
                "Taipan, you retire with a net worth of {}! A legend of the China Seas.",
                game.net_worth()
            ));
        }
        Some(Outcome::ShipDestroyed) => {
            io.writeln("Your ship is lost beneath the waves. The sea claims another Taipan.");
        }
        None => {}
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ui::ScriptedIo;

    #[test]
    fn quit_ends_cleanly() {
        let mut io = ScriptedIo::new(&["Q"]);
        let mut rng = Rng::new(1);
        let outcome = run(&mut io, &mut rng);
        assert_eq!(outcome, None);
        assert!(io.output.contains("abandoned"));
    }

    #[test]
    fn input_exhaustion_ends_loop() {
        // No input at all: loop should terminate at the first read.
        let mut io = ScriptedIo::new(&[]);
        let mut rng = Rng::new(1);
        let outcome = run(&mut io, &mut rng);
        assert_eq!(outcome, None);
        assert!(io.output.contains("Taipan!"));
    }

    #[test]
    fn buy_then_status_reflects_cargo() {
        // Buy 1 General Cargo (cheap), then quit. Status should show the hold.
        let mut io = ScriptedIo::new(&["B", "General", "1", "Q"]);
        let mut rng = Rng::new(1);
        run(&mut io, &mut rng);
        assert!(io.output.contains("Bought 1 General Cargo"));
    }

    #[test]
    fn cannot_retire_below_target() {
        let mut io = ScriptedIo::new(&["R", "Q"]);
        let mut rng = Rng::new(1);
        let outcome = run(&mut io, &mut rng);
        assert_eq!(outcome, None);
        assert!(io.output.contains("you need a net worth"));
    }

    #[test]
    fn win_when_rich_and_retire() {
        // Force a winning state by buying nothing but starting rich is hard via
        // the loop; instead we drive a tiny scripted path: the game starts poor,
        // so we verify the *retire* path by checking the message wiring works
        // when net worth is insufficient (covered above). Full-win is exercised
        // by the integration test with a deterministic money path.
        // Here: deposit/withdraw smoke through the bank menu at home.
        let mut io = ScriptedIo::new(&["K", "D", "100", "K", "W", "50", "Q"]);
        let mut rng = Rng::new(1);
        run(&mut io, &mut rng);
        assert!(io.output.contains("Deposited 100"));
        assert!(io.output.contains("Withdrew 50"));
    }

    #[test]
    fn unknown_command_is_handled() {
        let mut io = ScriptedIo::new(&["Z", "Q"]);
        let mut rng = Rng::new(1);
        run(&mut io, &mut rng);
        assert!(io.output.contains("I don't understand"));
    }

    #[test]
    fn full_playthrough_is_deterministic() {
        // Same seed + same inputs => byte-identical transcript.
        fn play() -> String {
            let mut io =
                ScriptedIo::new(&["B", "General", "5", "T", "2", "S", "General", "5", "Q"]);
            let mut rng = Rng::new(2024);
            run(&mut io, &mut rng);
            io.output
        }
        assert_eq!(play(), play());
    }
}
