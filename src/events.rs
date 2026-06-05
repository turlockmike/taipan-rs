//! Random events that fire when you arrive at a port.
//!
//! `roll` is a pure-ish function of `(&mut Game, &mut Rng)`: it may mutate the
//! market or cash and returns an `Event` describing what happened so the UI can
//! narrate it. A pirate encounter is special — it returns a ready `Battle` and
//! lets the run loop drive combat via the state machine in `combat.rs`.

use crate::combat::Battle;
use crate::game::Game;
use crate::market::Good;
use crate::rng::Rng;

/// Something that happened on arrival.
///
/// Not `PartialEq`/`Eq` because the `Pirates` variant carries a `Battle` (a
/// live state machine, not a value worth comparing). Tests inspect events via
/// pattern matching and `Debug` formatting instead.
#[derive(Debug, Clone)]
pub enum Event {
    /// Nothing notable.
    Quiet,
    /// A good's price spiked upward to `new_price`.
    PriceSpike { good: Good, new_price: u32 },
    /// A good's price crashed downward to `new_price`.
    PriceDrop { good: Good, new_price: u32 },
    /// Li Yuen's pirates demand tribute; `taken` was removed from cash.
    LiYuenExtortion { taken: u32 },
    /// Hostile pirates attack — the run loop must resolve the returned battle.
    Pirates(Battle),
}

/// Pick a random good using the RNG.
fn random_good(rng: &mut Rng) -> Good {
    Good::ALL[rng.range(0, 3) as usize]
}

/// Roll for an arrival event and apply its immediate effects.
///
/// Probabilities (each checked in turn, first hit wins):
///   * 1/6 pirate attack
///   * 1/8 Li Yuen extortion (only if you have cash to take)
///   * 1/5 price spike
///   * 1/5 price drop
///   * otherwise quiet
pub fn roll(game: &mut Game, rng: &mut Rng) -> Event {
    if rng.chance(1, 6) {
        return Event::Pirates(Battle::roll_encounter(rng, game.guns));
    }

    if game.cash > 0 && rng.chance(1, 8) {
        // Li Yuen takes a slice of your on-hand cash (10%–30%).
        let pct = rng.range(10, 30);
        let taken = (game.cash * pct / 100).max(1);
        game.cash -= taken;
        return Event::LiYuenExtortion { taken };
    }

    if rng.chance(1, 5) {
        let good = random_good(rng);
        // Spike: 2x–4x the current price.
        let mult = rng.range(2, 4);
        let new_price = game.market.price(good).saturating_mul(mult);
        game.market.set_price(good, new_price);
        return Event::PriceSpike { good, new_price };
    }

    if rng.chance(1, 5) {
        let good = random_good(rng);
        // Drop: down to 1/3–1/2 of current price, floor of 1.
        let div = rng.range(2, 3);
        let new_price = (game.market.price(good) / div).max(1);
        game.market.set_price(good, new_price);
        return Event::PriceDrop { good, new_price };
    }

    Event::Quiet
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::market::Market;

    fn game() -> Game {
        let mut rng = Rng::new(1);
        Game::new(&mut rng)
    }

    #[test]
    fn events_are_deterministic_per_seed() {
        // Same seed + same starting state => identical event stream.
        fn stream() -> Vec<String> {
            let mut rng = Rng::new(31337);
            let mut g = game();
            (0..50)
                .map(|_| format!("{:?}", roll(&mut g, &mut rng)))
                .collect()
        }
        assert_eq!(stream(), stream());
    }

    #[test]
    fn over_many_rolls_every_event_type_appears() {
        let mut rng = Rng::new(2024);
        let mut saw_pirates = false;
        let mut saw_extortion = false;
        let mut saw_spike = false;
        let mut saw_drop = false;
        let mut saw_quiet = false;
        for _ in 0..5000 {
            // Reset to a known-rich state so extortion always has cash to take
            // and prices are mutable.
            let mut g = game();
            g.cash = 10_000;
            g.market = Market::with_prices([1000, 100, 600, 30]);
            match roll(&mut g, &mut rng) {
                Event::Pirates(_) => saw_pirates = true,
                Event::LiYuenExtortion { taken } => {
                    assert!(taken > 0);
                    saw_extortion = true;
                }
                Event::PriceSpike { new_price, good } => {
                    assert!(new_price >= g.market.price(good) || new_price > 0);
                    saw_spike = true;
                }
                Event::PriceDrop { .. } => saw_drop = true,
                Event::Quiet => saw_quiet = true,
            }
        }
        assert!(saw_pirates, "never saw pirates");
        assert!(saw_extortion, "never saw extortion");
        assert!(saw_spike, "never saw a spike");
        assert!(saw_drop, "never saw a drop");
        assert!(saw_quiet, "never saw quiet");
    }

    #[test]
    fn extortion_never_fires_when_broke() {
        // With zero cash, the extortion branch is skipped entirely.
        let mut rng = Rng::new(9);
        for _ in 0..2000 {
            let mut g = game();
            g.cash = 0;
            if let Event::LiYuenExtortion { .. } = roll(&mut g, &mut rng) {
                panic!("extorted a broke trader");
            }
        }
    }

    #[test]
    fn spike_raises_and_drop_lowers_relative_to_base() {
        // Drive specific branches by constructing the state and checking the
        // mutation direction holds whenever that event type is produced.
        let mut rng = Rng::new(123);
        for _ in 0..3000 {
            let mut g = game();
            g.cash = 0; // disable extortion to isolate price events
            g.market = Market::with_prices([1000, 100, 600, 30]);
            let before = [
                g.market.price(Good::Opium),
                g.market.price(Good::Silk),
                g.market.price(Good::Arms),
                g.market.price(Good::General),
            ];
            match roll(&mut g, &mut rng) {
                Event::PriceSpike { good, new_price } => {
                    assert!(new_price >= before[good.index()]);
                }
                Event::PriceDrop { good, new_price } => {
                    assert!(new_price <= before[good.index()]);
                    assert!(new_price >= 1);
                }
                _ => {}
            }
        }
    }
}
