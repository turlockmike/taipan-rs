//! Pirate combat — original Taipan! depth.
//!
//! A `Battle` is a small state machine. The UI drives it by repeatedly reading
//! the current `prompt`, asking the player for a `Decision`, and calling
//! `step`. Every random outcome is drawn from an injected `Rng`, so a seed plus
//! a fixed decision sequence produces a fully deterministic, unit-testable
//! battle — no I/O involved.
//!
//! Mechanics, faithful to the original feel:
//!   * You face N enemy ships. Each carries some hull points.
//!   * FIGHT: each of your guns gets a chance to hit; enough hits sink a ship.
//!     Surviving enemies then fire back, damaging your hull.
//!   * RUN: a chance to escape that improves as enemies are sunk; failure means
//!     the enemies get a free volley at you.
//!   * THROW CARGO: jettison goods to distract them; a chance they take the
//!     bait and leave.
//!
//! The battle ends when all enemies are sunk (Victory), you escape (Escaped),
//! they leave with your cargo (Bribed), or your hull hits 0 (Sunk).

use crate::game::Game;
use crate::market::Good;
use crate::rng::Rng;

/// Hull points each enemy ship starts with.
const ENEMY_HP: u32 = 2;
/// Chance (out of `GUN_DENOM`) that a single gun scores a hit per round.
const GUN_HIT_NUM: u32 = 1;
const GUN_DENOM: u32 = 2;
/// Damage each surviving enemy inflicts on your hull per return volley.
const ENEMY_DAMAGE: u32 = 3;

/// What the player chose to do this round.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Decision {
    Fight,
    Run,
    /// Throw `qty` units of `good` overboard to placate the pirates.
    Throw {
        good: Good,
        qty: u32,
    },
}

/// Terminal result of a battle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BattleResult {
    /// All enemy ships sunk.
    Victory { sunk: u32 },
    /// Player escaped with the ship intact.
    Escaped,
    /// Pirates took thrown cargo and left.
    Bribed,
    /// Player's ship was destroyed.
    Sunk,
}

/// What just happened in the round we stepped, for the UI to narrate.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoundReport {
    /// We fired: `hits` guns connected, sinking `sunk_now` ships; then took
    /// `damage_taken` from the survivors' return fire.
    Fought {
        hits: u32,
        sunk_now: u32,
        damage_taken: u32,
    },
    /// Tried to run: `escaped` says whether we got away; if not, `damage_taken`
    /// is the free volley we ate.
    Ran { escaped: bool, damage_taken: u32 },
    /// Threw cargo: `accepted` says whether they took the bait and left.
    Threw { accepted: bool, jettisoned: u32 },
}

/// One pirate engagement.
#[derive(Debug, Clone)]
pub struct Battle {
    /// Remaining hull points for each still-floating enemy ship.
    enemies: Vec<u32>,
    /// Running count of enemies sunk this battle.
    sunk: u32,
}

impl Battle {
    /// Start a battle against `ships` pirate vessels.
    pub fn new(ships: u32) -> Self {
        Battle {
            enemies: vec![ENEMY_HP; ships as usize],
            sunk: 0,
        }
    }

    /// Roll the number of attacking ships from the RNG, scaled loosely by the
    /// number of guns you carry (more guns => bolder pirates), capped sane.
    pub fn roll_encounter(rng: &mut Rng, guns: u32) -> Battle {
        let base = rng.range(1, 4);
        let bonus = (guns / 4).min(4);
        Battle::new((base + bonus).min(8))
    }

    /// Enemy ships still afloat.
    pub fn enemies_remaining(&self) -> u32 {
        self.enemies.len() as u32
    }

    /// Ships sunk so far.
    pub fn sunk(&self) -> u32 {
        self.sunk
    }

    /// Per-ship remaining hull points — for serializing a battle in progress.
    /// Pair with [`Battle::from_parts`] to restore it.
    pub fn enemy_hps(&self) -> &[u32] {
        &self.enemies
    }

    /// Reconstruct a battle from saved parts (enemy hull points + sunk count).
    pub fn from_parts(enemies: Vec<u32>, sunk: u32) -> Self {
        Battle { enemies, sunk }
    }

    /// Apply one round. Mutates both the battle and the player's `Game`
    /// (hull damage, jettisoned cargo). Returns the round report plus an
    /// optional terminal result when the battle is over.
    pub fn step(
        &mut self,
        decision: Decision,
        game: &mut Game,
        rng: &mut Rng,
    ) -> (RoundReport, Option<BattleResult>) {
        match decision {
            Decision::Fight => self.fight(game, rng),
            Decision::Run => self.run(game, rng),
            Decision::Throw { good, qty } => self.throw(good, qty, game, rng),
        }
    }

    fn fight(&mut self, game: &mut Game, rng: &mut Rng) -> (RoundReport, Option<BattleResult>) {
        // Each gun rolls to hit.
        let mut hits = 0;
        for _ in 0..game.guns {
            if rng.chance(GUN_HIT_NUM, GUN_DENOM) {
                hits += 1;
            }
        }

        // Distribute hits onto enemies front-to-back; each point of HP soaks
        // one hit. When a ship's HP reaches 0 it sinks.
        let mut remaining_hits = hits;
        let mut sunk_now = 0;
        let mut idx = 0;
        while idx < self.enemies.len() && remaining_hits > 0 {
            let absorb = remaining_hits.min(self.enemies[idx]);
            self.enemies[idx] -= absorb;
            remaining_hits -= absorb;
            if self.enemies[idx] == 0 {
                sunk_now += 1;
            }
            idx += 1;
        }
        self.enemies.retain(|&hp| hp > 0);
        self.sunk += sunk_now;

        if self.enemies.is_empty() {
            return (
                RoundReport::Fought {
                    hits,
                    sunk_now,
                    damage_taken: 0,
                },
                Some(BattleResult::Victory { sunk: self.sunk }),
            );
        }

        // Survivors return fire.
        let damage = self.enemies_remaining() * ENEMY_DAMAGE;
        game.damage(damage);
        let result = if game.health == 0 {
            Some(BattleResult::Sunk)
        } else {
            None
        };
        (
            RoundReport::Fought {
                hits,
                sunk_now,
                damage_taken: damage,
            },
            result,
        )
    }

    fn run(&mut self, game: &mut Game, rng: &mut Rng) -> (RoundReport, Option<BattleResult>) {
        // Escape odds improve as enemies thin out: fewer chasers, easier exit.
        // 1 enemy => 3/4, scaling down toward ~1/4 for a full pack.
        let remaining = self.enemies_remaining().max(1);
        let escaped = rng.chance(3, (remaining + 2).min(8));
        if escaped {
            return (
                RoundReport::Ran {
                    escaped: true,
                    damage_taken: 0,
                },
                Some(BattleResult::Escaped),
            );
        }
        // Failed escape: free volley.
        let damage = self.enemies_remaining() * ENEMY_DAMAGE;
        game.damage(damage);
        let result = if game.health == 0 {
            Some(BattleResult::Sunk)
        } else {
            None
        };
        (
            RoundReport::Ran {
                escaped: false,
                damage_taken: damage,
            },
            result,
        )
    }

    fn throw(
        &mut self,
        good: Good,
        qty: u32,
        game: &mut Game,
        rng: &mut Rng,
    ) -> (RoundReport, Option<BattleResult>) {
        let jettisoned = game.hold.jettison(good, qty);
        // Bigger bribes are likelier to work; no cargo thrown never works.
        let accepted = jettisoned > 0 && rng.chance(jettisoned.min(10), 12);
        let result = if accepted {
            Some(BattleResult::Bribed)
        } else {
            None
        };
        (
            RoundReport::Threw {
                accepted,
                jettisoned,
            },
            result,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::game::Game;
    use crate::market::Market;

    fn game_with(guns: u32, health: u32) -> (Game, Rng) {
        let mut rng = Rng::new(1);
        let mut g = Game::new(&mut rng);
        g.guns = guns;
        g.health = health;
        (g, rng)
    }

    #[test]
    fn battle_tracks_enemy_count() {
        let b = Battle::new(3);
        assert_eq!(b.enemies_remaining(), 3);
        assert_eq!(b.sunk(), 0);
    }

    #[test]
    fn roll_encounter_is_bounded() {
        let mut rng = Rng::new(55);
        for _ in 0..1000 {
            let b = Battle::roll_encounter(&mut rng, 5);
            assert!((1..=8).contains(&b.enemies_remaining()));
        }
    }

    #[test]
    fn overwhelming_guns_win_in_one_round() {
        // 100 guns vs 1 ship: hits effectively guaranteed, instant victory.
        let (mut g, mut rng) = game_with(100, 100);
        let mut b = Battle::new(1);
        let (report, result) = b.step(Decision::Fight, &mut g, &mut rng);
        assert_eq!(result, Some(BattleResult::Victory { sunk: 1 }));
        assert!(matches!(report, RoundReport::Fought { sunk_now: 1, .. }));
        assert_eq!(g.health, 100); // no return fire after victory
    }

    #[test]
    fn fighting_takes_damage_from_survivors() {
        // 0 guns: we can't hit anything, so the full pack fires back.
        let (mut g, mut rng) = game_with(0, 100);
        let mut b = Battle::new(2);
        let (report, result) = b.step(Decision::Fight, &mut g, &mut rng);
        assert_eq!(result, None);
        match report {
            RoundReport::Fought {
                hits,
                sunk_now,
                damage_taken,
            } => {
                assert_eq!(hits, 0);
                assert_eq!(sunk_now, 0);
                assert_eq!(damage_taken, 2 * ENEMY_DAMAGE);
            }
            other => panic!("unexpected {other:?}"),
        }
        assert_eq!(g.health, 100 - 2 * ENEMY_DAMAGE);
    }

    #[test]
    fn fighting_can_sink_the_player() {
        // 0 guns, almost dead: the return volley finishes us.
        let (mut g, mut rng) = game_with(0, 3);
        let mut b = Battle::new(2);
        let (_report, result) = b.step(Decision::Fight, &mut g, &mut rng);
        assert_eq!(result, Some(BattleResult::Sunk));
        assert_eq!(g.health, 0);
    }

    #[test]
    fn throwing_no_cargo_never_works() {
        let (mut g, mut rng) = game_with(5, 100);
        let mut b = Battle::new(3);
        // Hold is empty, so jettisoned == 0 and they never accept.
        let (report, result) = b.step(
            Decision::Throw {
                good: Good::Silk,
                qty: 10,
            },
            &mut g,
            &mut rng,
        );
        assert_eq!(result, None);
        assert_eq!(
            report,
            RoundReport::Threw {
                accepted: false,
                jettisoned: 0
            }
        );
    }

    #[test]
    fn throwing_cargo_removes_it_from_hold() {
        let mut rng = Rng::new(1);
        let mut g = Game::new(&mut rng);
        g.market = Market::with_prices([1, 1, 1, 1]);
        g.cash = 1_000;
        g.buy(Good::Silk, 10).unwrap();
        let mut b = Battle::new(3);
        let (report, _result) = b.step(
            Decision::Throw {
                good: Good::Silk,
                qty: 4,
            },
            &mut g,
            &mut rng,
        );
        // 4 units left the hold regardless of whether pirates accepted.
        assert!(matches!(report, RoundReport::Threw { jettisoned: 4, .. }));
        assert_eq!(g.hold.quantity(Good::Silk), 6);
    }

    #[test]
    fn a_full_scripted_battle_is_deterministic() {
        // Same seed + same decisions => identical outcome, every run.
        fn play() -> (BattleResult, u32) {
            let mut rng = Rng::new(777);
            let mut g = Game::new(&mut rng);
            g.guns = 6;
            g.health = 100;
            let mut b = Battle::roll_encounter(&mut rng, g.guns);
            loop {
                let (_r, result) = b.step(Decision::Fight, &mut g, &mut rng);
                if let Some(res) = result {
                    return (res, g.health);
                }
            }
        }
        let first = play();
        let second = play();
        assert_eq!(first, second);
    }
}
