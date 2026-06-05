//! Core game state and the rules that tie the modules together.
//!
//! `Game` owns everything mutable: cash, the bank balance, Elder Brother Wu's
//! debt, the ship's hold and health, the Hong Kong warehouse, the current port,
//! and the live market. Higher layers (events, combat, the run loop) operate on
//! a `&mut Game`.

use crate::economy::{biases_for, EconomyMode};
use crate::market::{buy, sell, Good, Hold, Market, TradeError};
use crate::rng::Rng;
use crate::travel::Port;

/// Starting cash handed to you by Elder Brother Wu.
pub const START_CASH: u32 = 400;
/// Starting debt you owe him.
pub const START_DEBT: u32 = 5_000;
/// Initial hold capacity.
pub const START_HOLD: u32 = 60;
/// Starting cannon count.
pub const START_GUNS: u32 = 5;
/// Full ship health.
pub const MAX_HEALTH: u32 = 100;
/// Net worth needed to retire a winner.
pub const WIN_TARGET: u64 = 1_000_000;
/// Debt interest rate applied per port arrival, in percent.
pub const DEBT_INTEREST_PERCENT: u32 = 10;
/// Bank interest rate applied per port arrival, in percent.
pub const BANK_INTEREST_PERCENT: u32 = 1;
/// Cost of one cannon, in cash.
pub const GUN_PRICE: u32 = 1_000;
/// Hold units one cannon occupies (the classic cargo-vs-firepower tradeoff).
pub const GUN_HOLD_COST: u32 = 10;
/// Cost to repair one point of hull, in cash (McHenry's shipyard, Hong Kong).
pub const REPAIR_PRICE_PER_POINT: u32 = 50;
/// Cost to expand the hold by one unit, in cash (the shipyard, Hong Kong).
pub const HOLD_EXPANSION_PRICE: u32 = 500;

/// Why the game ended, if it has.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    /// Reached the net-worth target and chose to retire (or auto-win).
    Won,
    /// Ship was sunk.
    ShipDestroyed,
}

/// The whole mutable game world.
#[derive(Debug, Clone)]
pub struct Game {
    pub cash: u32,
    pub bank: u64,
    pub debt: u64,
    pub location: Port,
    pub hold: Hold,
    pub warehouse: [u32; 4],
    pub guns: u32,
    pub health: u32,
    pub market: Market,
    pub outcome: Option<Outcome>,
    /// Which economy model this game runs under. Set once at construction.
    pub mode: EconomyMode,
}

impl Game {
    /// Fresh game under the classic economy. Convenience over
    /// [`Game::new_with_mode`] — kept because most call sites (and tests) don't
    /// care about the mode.
    pub fn new(rng: &mut Rng) -> Self {
        Game::new_with_mode(EconomyMode::Classic, rng)
    }

    /// Fresh game under a chosen economy model: starting cash/debt, full health,
    /// and an opening market rolled for the home port with that model's bias (so
    /// a seed *and* a mode together fully determine the opening market).
    pub fn new_with_mode(mode: EconomyMode, rng: &mut Rng) -> Self {
        let bias = biases_for(mode, Port::HOME);
        Game {
            cash: START_CASH,
            bank: 0,
            debt: START_DEBT as u64,
            location: Port::HOME,
            hold: Hold::new(START_HOLD),
            warehouse: [0; 4],
            guns: START_GUNS,
            health: MAX_HEALTH,
            market: Market::generate_for(bias, rng),
            outcome: None,
            mode,
        }
    }

    /// Net worth = cash + bank − debt + value of cargo carried, valued at
    /// current market prices. Saturating so debt can't underflow the total.
    pub fn net_worth(&self) -> u64 {
        let cargo_value: u64 = Good::ALL
            .iter()
            .map(|&g| self.hold.quantity(g) as u64 * self.market.price(g) as u64)
            .sum();
        (self.cash as u64 + self.bank + cargo_value).saturating_sub(self.debt)
    }

    /// Has the player reached the retirement target?
    pub fn can_retire(&self) -> bool {
        self.net_worth() >= WIN_TARGET
    }

    /// True once the game is over for any reason.
    pub fn is_over(&self) -> bool {
        self.outcome.is_some()
    }

    /// Buy at the current market. Thin pass-through that keeps callers from
    /// reaching into both `market` and `hold` directly.
    pub fn buy(&mut self, good: Good, qty: u32) -> Result<u32, TradeError> {
        buy(&self.market, &mut self.hold, &mut self.cash, good, qty)
    }

    /// Sell at the current market.
    pub fn sell(&mut self, good: Good, qty: u32) -> Result<u32, TradeError> {
        sell(&self.market, &mut self.hold, &mut self.cash, good, qty)
    }

    /// Deposit cash into the bank. Caps at available cash.
    pub fn deposit(&mut self, amount: u32) -> u32 {
        let moved = amount.min(self.cash);
        self.cash -= moved;
        self.bank += moved as u64;
        moved
    }

    /// Withdraw from the bank into cash. Caps at the bank balance *and* at the
    /// headroom left in `cash` (u32), so the transfer can never silently wrap or
    /// truncate. Returns the amount actually moved.
    pub fn withdraw(&mut self, amount: u64) -> u64 {
        let headroom = (u32::MAX - self.cash) as u64;
        let moved = amount.min(self.bank).min(headroom);
        self.bank -= moved;
        self.cash += moved as u32;
        moved
    }

    /// Pay down debt from cash. Caps at whichever is smaller (cash or debt).
    /// Returns the amount actually paid.
    pub fn pay_debt(&mut self, amount: u32) -> u32 {
        let payable = (amount as u64).min(self.debt).min(self.cash as u64) as u32;
        self.cash -= payable;
        self.debt -= payable as u64;
        payable
    }

    /// Borrow more from Elder Brother Wu, increasing both cash and debt.
    /// Cash saturates at `u32::MAX` so a huge borrow can't wrap it; debt always
    /// records the full amount borrowed (it's u64).
    pub fn borrow(&mut self, amount: u32) {
        self.cash = self.cash.saturating_add(amount);
        self.debt += amount as u64;
    }

    /// Apply compounding interest to debt and bank. Called once per arrival.
    /// Integer math rounds down, which quietly favors the player slightly.
    ///
    /// Saturating throughout: a player who ignores their debt for a very long
    /// time can compound it past `u64::MAX`. That should pin the debt at the
    /// ceiling (an unwinnable hole), never panic the process.
    pub fn accrue_interest(&mut self) {
        let debt_interest = self.debt / 100 * DEBT_INTEREST_PERCENT as u64;
        self.debt = self.debt.saturating_add(debt_interest);
        let bank_interest = self.bank / 100 * BANK_INTEREST_PERCENT as u64;
        self.bank = self.bank.saturating_add(bank_interest);
    }

    /// Move the ship to a new port: accrue interest, then roll a fresh market.
    /// Returns the port arrived at. Does not itself roll combat/events — the
    /// run loop sequences those around the move so they remain independently
    /// testable.
    pub fn travel_to(&mut self, dest: Port, rng: &mut Rng) -> Port {
        self.accrue_interest();
        self.location = dest;
        self.market = Market::generate_for(biases_for(self.mode, dest), rng);
        dest
    }

    /// Apply combat/storm damage to the hull. At 0 the ship is destroyed and
    /// the game ends.
    pub fn damage(&mut self, amount: u32) {
        self.health = self.health.saturating_sub(amount);
        if self.health == 0 {
            self.outcome = Some(Outcome::ShipDestroyed);
        }
    }

    /// Repair the hull, capped at `MAX_HEALTH`.
    pub fn repair(&mut self, amount: u32) {
        self.health = (self.health + amount).min(MAX_HEALTH);
    }

    /// Buy `qty` cannons. Each costs `GUN_PRICE` in cash and occupies
    /// `GUN_HOLD_COST` units of hold space (the classic firepower-vs-cargo
    /// tradeoff). Returns the total cost, or an error naming what's short.
    pub fn buy_guns(&mut self, qty: u32) -> Result<u32, &'static str> {
        if qty == 0 {
            return Err("buy at least one gun");
        }
        let cost = GUN_PRICE.checked_mul(qty).ok_or("quantity too large")?;
        if cost > self.cash {
            return Err("not enough cash for that many guns");
        }
        let space = GUN_HOLD_COST * qty;
        if !self.hold.shrink(space) {
            return Err("not enough free hold space for that many guns");
        }
        self.cash -= cost;
        self.guns += qty;
        Ok(cost)
    }

    /// Pay to enlarge the cargo hold at the shipyard (Hong Kong). Each unit
    /// costs `HOLD_EXPANSION_PRICE`. Buys as many units as `cash` covers, up to
    /// the requested `units`. Returns (units_added, cash_spent).
    pub fn expand_hold(&mut self, units: u32) -> (u32, u32) {
        let affordable = self.cash / HOLD_EXPANSION_PRICE;
        let added = affordable.min(units);
        let spent = added * HOLD_EXPANSION_PRICE;
        self.cash -= spent;
        self.hold.expand(added);
        (added, spent)
    }

    /// Pay to repair the hull at McHenry's (Hong Kong). Buys as many hull points
    /// as `amount` cash covers, capped at the damage actually taken. Returns the
    /// points repaired and the cash spent.
    pub fn repair_hull(&mut self, amount: u32) -> (u32, u32) {
        let missing = MAX_HEALTH - self.health;
        let affordable = amount / REPAIR_PRICE_PER_POINT;
        let points = affordable.min(missing);
        let spent = points * REPAIR_PRICE_PER_POINT;
        self.cash -= spent;
        self.health += points;
        (points, spent)
    }

    /// Declare victory. The run loop calls this when the player retires at or
    /// above the target.
    pub fn retire(&mut self) {
        self.outcome = Some(Outcome::Won);
    }

    /// Move cargo from hold into the Hong Kong warehouse (only meaningful at
    /// home, but the rule is enforced by the caller). Caps at held quantity.
    pub fn store(&mut self, good: Good, qty: u32) -> u32 {
        let moved = self.hold.jettison(good, qty);
        self.warehouse[good.index()] += moved;
        moved
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn new_game() -> (Game, Rng) {
        let mut rng = Rng::new(1);
        let game = Game::new(&mut rng);
        (game, rng)
    }

    #[test]
    fn starts_with_classic_values() {
        let (g, _) = new_game();
        assert_eq!(g.cash, 400);
        assert_eq!(g.debt, 5_000);
        assert_eq!(g.guns, 5);
        assert_eq!(g.health, 100);
        assert_eq!(g.hold.capacity(), 60);
        assert_eq!(g.location, Port::HongKong);
        assert!(g.outcome.is_none());
    }

    #[test]
    fn net_worth_subtracts_debt_and_counts_cargo() {
        let mut rng = Rng::new(1);
        let mut g = Game::new(&mut rng);
        g.cash = 10_000;
        g.bank = 5_000;
        g.debt = 3_000;
        // No cargo yet: 10000 + 5000 - 3000 = 12000.
        assert_eq!(g.net_worth(), 12_000);
    }

    #[test]
    fn net_worth_saturates_when_debt_exceeds_assets() {
        let mut rng = Rng::new(1);
        let mut g = Game::new(&mut rng);
        g.cash = 100;
        g.bank = 0;
        g.debt = 5_000;
        assert_eq!(g.net_worth(), 0); // no underflow panic
    }

    #[test]
    fn deposit_and_withdraw_cap_correctly() {
        let (mut g, _) = new_game();
        g.cash = 1_000;
        assert_eq!(g.deposit(1_500), 1_000); // capped at cash
        assert_eq!(g.cash, 0);
        assert_eq!(g.bank, 1_000);
        assert_eq!(g.withdraw(400), 400);
        assert_eq!(g.cash, 400);
        assert_eq!(g.bank, 600);
        assert_eq!(g.withdraw(9_999), 600); // capped at bank
    }

    #[test]
    fn pay_debt_caps_at_min_of_cash_and_debt() {
        let (mut g, _) = new_game();
        g.cash = 10_000;
        g.debt = 5_000;
        assert_eq!(g.pay_debt(8_000), 5_000); // capped at debt
        assert_eq!(g.debt, 0);
        assert_eq!(g.cash, 5_000);

        g.debt = 9_000;
        assert_eq!(g.pay_debt(7_000), 5_000); // capped at cash
        assert_eq!(g.cash, 0);
        assert_eq!(g.debt, 4_000);
    }

    #[test]
    fn borrow_raises_cash_and_debt_together() {
        let (mut g, _) = new_game();
        let cash0 = g.cash;
        let debt0 = g.debt;
        g.borrow(1_000);
        assert_eq!(g.cash, cash0 + 1_000);
        assert_eq!(g.debt, debt0 + 1_000);
    }

    #[test]
    fn interest_compounds_debt_and_bank() {
        let (mut g, _) = new_game();
        g.debt = 5_000;
        g.bank = 10_000;
        g.accrue_interest();
        assert_eq!(g.debt, 5_500); // +10%
        assert_eq!(g.bank, 10_100); // +1%
    }

    #[test]
    fn travel_accrues_interest_and_changes_port() {
        let (mut g, mut rng) = new_game();
        g.debt = 5_000;
        let arrived = g.travel_to(Port::Shanghai, &mut rng);
        assert_eq!(arrived, Port::Shanghai);
        assert_eq!(g.location, Port::Shanghai);
        assert_eq!(g.debt, 5_500); // interest applied on arrival
    }

    #[test]
    fn damage_can_destroy_ship() {
        let (mut g, _) = new_game();
        g.damage(40);
        assert_eq!(g.health, 60);
        assert!(g.outcome.is_none());
        g.damage(100); // saturates to 0
        assert_eq!(g.health, 0);
        assert_eq!(g.outcome, Some(Outcome::ShipDestroyed));
    }

    #[test]
    fn repair_caps_at_max() {
        let (mut g, _) = new_game();
        g.damage(50);
        g.repair(999);
        assert_eq!(g.health, MAX_HEALTH);
    }

    #[test]
    fn can_retire_at_target() {
        let mut rng = Rng::new(1);
        let mut g = Game::new(&mut rng);
        g.debt = 0;
        g.bank = WIN_TARGET;
        assert!(g.can_retire());
        g.retire();
        assert_eq!(g.outcome, Some(Outcome::Won));
        assert!(g.is_over());
    }

    #[test]
    fn store_moves_cargo_to_warehouse() {
        let mut rng = Rng::new(1);
        let mut g = Game::new(&mut rng);
        g.market = Market::with_prices([1, 1, 1, 1]);
        g.cash = 1_000;
        g.buy(Good::Silk, 10).unwrap();
        assert_eq!(g.store(Good::Silk, 7), 7);
        assert_eq!(g.hold.quantity(Good::Silk), 3);
        assert_eq!(g.warehouse[Good::Silk.index()], 7);
    }

    #[test]
    fn buy_guns_spends_cash_takes_hold_and_arms_ship() {
        let (mut g, _) = new_game();
        g.cash = 5_000;
        let start_guns = g.guns;
        let start_cap = g.hold.capacity();
        let cost = g.buy_guns(2).unwrap();
        assert_eq!(cost, 2 * GUN_PRICE);
        assert_eq!(g.cash, 5_000 - 2 * GUN_PRICE);
        assert_eq!(g.guns, start_guns + 2);
        // Two guns reserve 2 * GUN_HOLD_COST of capacity.
        assert_eq!(g.hold.capacity(), start_cap - 2 * GUN_HOLD_COST);
    }

    #[test]
    fn buy_guns_rejects_when_cash_short() {
        let (mut g, _) = new_game();
        g.cash = GUN_PRICE - 1;
        assert!(g.buy_guns(1).is_err());
        assert_eq!(g.guns, START_GUNS); // unchanged
    }

    #[test]
    fn buy_guns_rejects_when_hold_full() {
        let (mut g, _) = new_game();
        g.cash = 1_000_000;
        g.market = Market::with_prices([1, 1, 1, 1]);
        // Fill the hold so there's no room for a gun's reserved space.
        let cap = g.hold.capacity();
        g.buy(Good::General, cap).unwrap();
        assert!(g.buy_guns(1).is_err());
        assert_eq!(g.guns, START_GUNS);
    }

    #[test]
    fn repair_hull_buys_points_capped_by_cash_and_damage() {
        let (mut g, _) = new_game();
        g.cash = 10_000;
        g.damage(30); // health 70, missing 30
        let (points, spent) = g.repair_hull(1_000); // 1000/50 = 20 points affordable
        assert_eq!(points, 20);
        assert_eq!(spent, 20 * REPAIR_PRICE_PER_POINT);
        assert_eq!(g.health, 90);
        // Now repair more than the remaining damage: capped at the missing 10.
        let (points2, _) = g.repair_hull(10_000);
        assert_eq!(points2, 10);
        assert_eq!(g.health, MAX_HEALTH);
    }

    #[test]
    fn borrow_saturates_cash_without_wrapping() {
        let (mut g, _) = new_game();
        g.cash = u32::MAX - 5;
        g.borrow(1_000); // would overflow a plain add
        assert_eq!(g.cash, u32::MAX); // saturated, not wrapped
        assert_eq!(g.debt, START_DEBT as u64 + 1_000); // debt still full
    }

    #[test]
    fn expand_hold_adds_capacity_capped_by_cash() {
        let (mut g, _) = new_game();
        g.cash = 2_500; // affords 5 units at 500 each
        let cap0 = g.hold.capacity();
        let (added, spent) = g.expand_hold(10); // want 10, afford 5
        assert_eq!(added, 5);
        assert_eq!(spent, 5 * HOLD_EXPANSION_PRICE);
        assert_eq!(g.hold.capacity(), cap0 + 5);
        assert_eq!(g.cash, 0);
    }

    #[test]
    fn withdraw_does_not_overflow_cash() {
        let (mut g, _) = new_game();
        g.cash = u32::MAX - 100;
        g.bank = 10_000;
        let moved = g.withdraw(10_000); // only 100 headroom in cash
        assert_eq!(moved, 100);
        assert_eq!(g.cash, u32::MAX);
        assert_eq!(g.bank, 9_900); // rest stays in the bank
    }
}
