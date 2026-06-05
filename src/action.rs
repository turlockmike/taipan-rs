//! Parse and apply a single action to a saved game — the engine behind the
//! stateless `step` interface.
//!
//! [`apply`] takes a [`Save`] and one action string, advances the game by
//! exactly one decision, and returns the new [`Save`]. It is the non-interactive
//! mirror of the `run.rs` turn loop: same rules, but driven one command at a
//! time over a save file instead of a live stdin loop.
//!
//! The valid action vocabulary depends on `save.pending`:
//!   * `Pending::Command` — buy / sell / travel / deposit / withdraw / pay /
//!     borrow / store / retire
//!   * `Pending::Combat`  — fight / run / throw
//!
//! Applying a combat action while not in combat (or vice versa) is an error,
//! never a silent no-op — that keeps the agent's mental model honest.

use crate::combat::{Battle, BattleResult, Decision, RoundReport};
use crate::events::{roll, Event};
use crate::game::Game;
use crate::market::Good;
use crate::rng::Rng;
use crate::state::{Pending, Save};
use crate::travel::Port;

/// Apply one action string to a save, returning the updated save.
/// Returns `Err(msg)` for invalid input or actions illegal in the current
/// `pending` state — the caller surfaces the message; the save is unchanged.
pub fn apply(save: &Save, action: &str) -> Result<Save, String> {
    if save.game.is_over() {
        return Err(format!(
            "game is over ({}). Start a new game.",
            crate::state::to_json(save)
                .lines()
                .find(|l| l.contains("outcome"))
                .unwrap_or("")
                .trim()
        ));
    }

    let mut game = save.game.clone();
    let mut rng = Rng::from_state(save.rng_state);

    let (pending, last_event) = match &save.pending {
        Pending::Combat { enemies, sunk } => {
            let battle = Battle::from_parts(enemies.clone(), *sunk);
            apply_combat(&mut game, &mut rng, battle, action)?
        }
        Pending::Command => apply_command(&mut game, &mut rng, action)?,
    };

    Ok(Save {
        game,
        rng_state: rng.state(),
        pending,
        last_event,
    })
}

/// Tokenize an action string into a verb and its arguments.
fn tokens(action: &str) -> Vec<String> {
    action
        .split_whitespace()
        .map(|s| s.to_lowercase())
        .collect()
}

/// Parse the `<good> <qty>` tail shared by buy/sell/store/throw.
fn good_and_qty(args: &[String]) -> Result<(Good, u32), String> {
    let good = args
        .first()
        .and_then(|s| Good::parse(s))
        .ok_or("expected a good (opium|silk|arms|general)")?;
    let qty = args
        .get(1)
        .ok_or("expected a quantity")?
        .parse::<u32>()
        .map_err(|_| "quantity must be a number".to_string())?;
    Ok((good, qty))
}

fn apply_command(
    game: &mut Game,
    rng: &mut Rng,
    action: &str,
) -> Result<(Pending, String), String> {
    let t = tokens(action);
    let verb = t.first().ok_or("empty action")?.as_str();
    let args = &t[1..];

    let event = match verb {
        "buy" => {
            // `buy guns N` arms the ship, `buy hold N` enlarges the hold (HK
            // only); otherwise `buy <good> N` is cargo.
            if args.first().map(|s| s.as_str()) == Some("guns") {
                let qty = args
                    .get(1)
                    .ok_or("expected a quantity")?
                    .parse::<u32>()
                    .map_err(|_| "quantity must be a number".to_string())?;
                let cost = game.buy_guns(qty).map_err(|e| format!("cannot buy guns: {e}"))?;
                format!("Bought {qty} guns for {cost}. Now {} guns.", game.guns)
            } else if args.first().map(|s| s.as_str()) == Some("hold") {
                if game.location != Port::HOME {
                    return Err("the shipyard is only in Hong Kong".to_string());
                }
                let qty = args
                    .get(1)
                    .ok_or("expected a quantity")?
                    .parse::<u32>()
                    .map_err(|_| "quantity must be a number".to_string())?;
                let (added, spent) = game.expand_hold(qty);
                format!(
                    "Expanded hold by {added} for {spent}. Capacity now {}.",
                    game.hold.capacity()
                )
            } else {
                let (good, qty) = good_and_qty(args)?;
                let cost = game
                    .buy(good, qty)
                    .map_err(|e| format!("cannot buy: {e:?}"))?;
                format!("Bought {qty} {} for {cost}.", good.name())
            }
        }
        "repair" => {
            if game.location != Port::HOME {
                return Err("McHenry's shipyard is only in Hong Kong".to_string());
            }
            let amt = parse_amount(args)?;
            let (points, spent) = game.repair_hull(amt);
            format!("Repaired {points} hull for {spent}. Hull now {}.", game.health)
        }
        "sell" => {
            let (good, qty) = good_and_qty(args)?;
            let proceeds = game
                .sell(good, qty)
                .map_err(|e| format!("cannot sell: {e:?}"))?;
            format!("Sold {qty} {} for {proceeds}.", good.name())
        }
        "deposit" => {
            let amt = parse_amount(args)?;
            let moved = game.deposit(amt);
            format!("Deposited {moved}.")
        }
        "withdraw" => {
            let amt = parse_amount(args)?;
            let moved = game.withdraw(amt as u64);
            format!("Withdrew {moved}.")
        }
        "pay" => {
            let amt = parse_amount(args)?;
            let paid = game.pay_debt(amt);
            format!("Paid {paid} toward debt.")
        }
        "borrow" => {
            let amt = parse_amount(args)?;
            game.borrow(amt);
            format!("Borrowed {amt}.")
        }
        "store" => {
            let (good, qty) = good_and_qty(args)?;
            if game.location != Port::HOME {
                return Err("the warehouse is only in Hong Kong".to_string());
            }
            let moved = game.store(good, qty);
            format!("Stored {moved} {} in the warehouse.", good.name())
        }
        "retire" => {
            if !game.can_retire() {
                return Err(format!(
                    "net worth {} is below the {} target",
                    game.net_worth(),
                    crate::game::WIN_TARGET
                ));
            }
            game.retire();
            return Ok((Pending::Command, "You retire a legend of the China Seas!".to_string()));
        }
        "travel" => {
            let dest = parse_port(args)?;
            if dest == game.location {
                return Err("you are already there".to_string());
            }
            return Ok(do_travel(game, rng, dest));
        }
        other => {
            return Err(format!(
                "unknown command '{other}'. Valid: buy sell travel deposit withdraw pay borrow store repair retire"
            ))
        }
    };

    Ok((Pending::Command, event))
}

fn parse_amount(args: &[String]) -> Result<u32, String> {
    args.first()
        .ok_or("expected an amount")?
        .parse::<u32>()
        .map_err(|_| "amount must be a number".to_string())
}

fn parse_port(args: &[String]) -> Result<Port, String> {
    let arg = args.first().ok_or("expected a port name or number")?;
    // Accept a 1-based menu number or a name prefix.
    if let Ok(n) = arg.parse::<u32>() {
        return Port::from_menu_number(n).ok_or_else(|| format!("no port number {n}"));
    }
    // Match against the port name with spaces removed, so "hongkong" and
    // "hong kong" both resolve. Prefix match keeps "shang" -> Shanghai working.
    let needle = arg.replace(' ', "");
    Port::ALL
        .iter()
        .copied()
        .find(|p| {
            p.name()
                .to_lowercase()
                .replace(' ', "")
                .starts_with(&needle)
        })
        .ok_or_else(|| format!("unknown port '{arg}'"))
}

/// Travel: dock at the destination first (this accrues interest and rolls the
/// new port's market), *then* roll the arrival event. Docking first means the
/// destination can never be "lost" if a pirate battle interrupts — the move is
/// already committed, and any ensuing combat simply plays out in the new
/// harbor. If pirates attack, we return `Pending::Combat`; the player is now at
/// `dest` and fights from there over subsequent steps.
fn do_travel(game: &mut Game, rng: &mut Rng, dest: Port) -> (Pending, String) {
    game.travel_to(dest, rng);
    let event = roll(game, rng);
    match event {
        Event::Pirates(battle) => {
            let n = battle.enemies_remaining();
            (
                Pending::Combat {
                    enemies: battle.enemy_hps().to_vec(),
                    sunk: battle.sunk(),
                },
                format!(
                    "Arrived at {}. PIRATES! {n} ships attack. (fight/run/throw)",
                    dest.name()
                ),
            )
        }
        other => {
            let note = describe_event(&other);
            let mut msg = format!("Arrived at {}.", dest.name());
            if !note.is_empty() {
                msg = format!("{note} {msg}");
            }
            (Pending::Command, msg)
        }
    }
}

fn describe_event(event: &Event) -> String {
    match event {
        Event::Quiet => String::new(),
        Event::PriceSpike { good, new_price } => {
            format!("{} spiked to {new_price}!", good.name())
        }
        Event::PriceDrop { good, new_price } => {
            format!("{} crashed to {new_price}!", good.name())
        }
        Event::LiYuenExtortion { taken } => format!("Li Yuen took {taken} in tribute."),
        Event::Pirates(_) => "Pirates!".to_string(),
    }
}

fn apply_combat(
    game: &mut Game,
    rng: &mut Rng,
    mut battle: Battle,
    action: &str,
) -> Result<(Pending, String), String> {
    let t = tokens(action);
    let verb = t.first().ok_or("empty action")?.as_str();

    let decision = match verb {
        "fight" => Decision::Fight,
        "run" => Decision::Run,
        "throw" => {
            let (good, qty) = good_and_qty(&t[1..])?;
            Decision::Throw { good, qty }
        }
        other => {
            return Err(format!(
                "in combat — valid actions are fight, run, throw <good> <qty> (got '{other}')"
            ))
        }
    };

    let (report, result) = battle.step(decision, game, rng);
    let round = describe_round(&report);

    match result {
        Some(res) => {
            let outcome_msg = match res {
                BattleResult::Victory { sunk } => format!("Victory! Sank all {sunk} ships."),
                BattleResult::Escaped => "Escaped into the fog.".to_string(),
                BattleResult::Bribed => "Pirates took the cargo and left.".to_string(),
                BattleResult::Sunk => "Your ship was sunk.".to_string(),
            };
            // Travel already completed before combat began (see `do_travel`),
            // so resolving the battle just returns control to command mode at
            // the destination port. A `Sunk` result has already set the game's
            // outcome via `Battle::step` -> `Game::damage`.
            Ok((Pending::Command, format!("{round} {outcome_msg}")))
        }
        None => Ok((
            Pending::Combat {
                enemies: battle.enemy_hps().to_vec(),
                sunk: battle.sunk(),
            },
            round,
        )),
    }
}

fn describe_round(report: &RoundReport) -> String {
    match report {
        RoundReport::Fought {
            hits,
            sunk_now,
            damage_taken,
        } => format!("Fired: {hits} hits, {sunk_now} sunk, took {damage_taken} damage."),
        RoundReport::Ran {
            escaped,
            damage_taken,
        } => {
            if *escaped {
                "Broke for open water.".to_string()
            } else {
                format!("Failed to escape, took {damage_taken} damage.")
            }
        }
        RoundReport::Threw {
            accepted,
            jettisoned,
        } => {
            if *accepted {
                format!("Threw {jettisoned} units — they took the bait.")
            } else {
                format!("Threw {jettisoned} units — they keep coming.")
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::economy::EconomyMode;
    use crate::market::Market;
    use crate::state::{new_save, to_json};

    fn fresh() -> Save {
        new_save(EconomyMode::Classic, 1)
    }

    #[test]
    fn buy_then_sell_updates_cash_and_hold() {
        let mut save = fresh();
        // Force a known cheap market so the buy is affordable and deterministic.
        save.game.market = Market::with_prices([10, 10, 10, 10]);
        save.game.cash = 1000;

        let save = apply(&save, "buy general 10").unwrap();
        assert_eq!(save.game.hold.units()[Good::General.index()], 10);
        assert_eq!(save.game.cash, 900);
        assert!(save.last_event.contains("Bought 10"));

        let save = apply(&save, "sell general 4").unwrap();
        assert_eq!(save.game.hold.units()[Good::General.index()], 6);
        assert_eq!(save.game.cash, 940);
    }

    #[test]
    fn invalid_good_is_an_error() {
        let save = fresh();
        let err = apply(&save, "buy gold 5").unwrap_err();
        assert!(err.contains("good"));
    }

    #[test]
    fn combat_action_rejected_when_not_in_combat() {
        let save = fresh();
        let err = apply(&save, "fight").unwrap_err();
        assert!(err.contains("unknown command"));
    }

    #[test]
    fn command_action_rejected_when_in_combat() {
        let mut save = fresh();
        save.pending = Pending::Combat {
            enemies: vec![2, 2],
            sunk: 0,
        };
        let err = apply(&save, "buy general 1").unwrap_err();
        assert!(err.contains("in combat"));
    }

    #[test]
    fn retire_blocked_below_target_allowed_above() {
        let mut save = fresh();
        let err = apply(&save, "retire").unwrap_err();
        assert!(err.contains("below"));

        save.game.debt = 0;
        save.game.bank = crate::game::WIN_TARGET;
        let won = apply(&save, "retire").unwrap();
        assert!(won.game.is_over());
    }

    #[test]
    fn travel_to_same_port_errors() {
        let save = fresh(); // starts at Hong Kong
        let err = apply(&save, "travel hongkong").unwrap_err();
        assert!(err.contains("already there"));
    }

    #[test]
    fn buy_guns_arms_the_ship_through_apply() {
        let mut save = fresh();
        save.game.cash = 5_000;
        let before = save.game.guns;
        let after = apply(&save, "buy guns 2").unwrap();
        assert_eq!(after.game.guns, before + 2);
        assert!(after.last_event.contains("Bought 2 guns"));
    }

    #[test]
    fn buy_hold_expands_capacity_at_hong_kong_only() {
        let mut save = fresh(); // at Hong Kong
        save.game.cash = 5_000;
        let cap0 = save.game.hold.capacity();
        let after = apply(&save, "buy hold 4").unwrap();
        assert_eq!(after.game.hold.capacity(), cap0 + 4);
        assert!(after.last_event.contains("Expanded hold"));

        // Away from home it's rejected.
        let mut away = fresh();
        away.game.location = crate::travel::Port::Shanghai;
        away.game.cash = 5_000;
        assert!(apply(&away, "buy hold 4")
            .unwrap_err()
            .contains("Hong Kong"));
    }

    #[test]
    fn repair_only_at_hong_kong() {
        // Damaged ship away from home: repair rejected.
        let mut save = fresh();
        save.game.location = crate::travel::Port::Shanghai;
        save.game.health = 50;
        save.game.cash = 5_000;
        assert!(apply(&save, "repair 1000")
            .unwrap_err()
            .contains("Hong Kong"));

        // At home: repair works.
        save.game.location = crate::travel::Port::HongKong;
        let after = apply(&save, "repair 1000").unwrap();
        assert!(after.game.health > 50);
        assert!(after.last_event.contains("Repaired"));
    }

    #[test]
    fn travel_advances_and_is_deterministic() {
        // Same save + same action => identical resulting JSON.
        let save = fresh();
        let a = to_json(&apply(&save, "travel shanghai").unwrap());
        let b = to_json(&apply(&save, "travel shanghai").unwrap());
        assert_eq!(a, b);
    }

    #[test]
    fn a_pirate_travel_enters_combat_then_resolves() {
        // Find a seed whose first travel triggers pirates, then fight to the end
        // purely through apply() — proving the step-mode combat loop works.
        for seed in 1u64..=300 {
            let mut save = new_save(EconomyMode::Classic, seed);
            // Give plenty of guns/health so fighting converges fast.
            save.game.guns = 20;
            save.game.health = 100;
            let after = apply(&save, "travel shanghai").unwrap();
            if let Pending::Combat { .. } = after.pending {
                // Fight rounds until resolution.
                let mut cur = after;
                for _ in 0..50 {
                    if let Pending::Command = cur.pending {
                        break;
                    }
                    cur = apply(&cur, "fight").unwrap();
                }
                assert_eq!(cur.pending, Pending::Command);
                assert!(
                    cur.last_event.contains("Victory")
                        || cur.last_event.contains("sunk")
                        || cur.last_event.contains("Escaped"),
                    "unexpected resolution: {}",
                    cur.last_event
                );
                return; // proved it for one seed
            }
        }
        panic!("no pirate encounter found in seeds 1..=300");
    }
}
