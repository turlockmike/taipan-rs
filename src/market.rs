//! Goods, prices, and the buy/sell mechanics.
//!
//! Four tradable goods with classic Taipan! price magnitudes. Prices are
//! regenerated every time you arrive at a port (see `Market::generate`), with
//! occasional spikes/drops layered on by the events module.

use crate::rng::Rng;
use std::fmt;

/// The four tradable goods, in canonical display order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Good {
    Opium,
    Silk,
    Arms,
    General,
}

impl Good {
    /// All goods in display order. Useful for iterating menus and holds.
    pub const ALL: [Good; 4] = [Good::Opium, Good::Silk, Good::Arms, Good::General];

    /// Index into the 4-slot arrays we use for prices and cargo.
    pub fn index(self) -> usize {
        match self {
            Good::Opium => 0,
            Good::Silk => 1,
            Good::Arms => 2,
            Good::General => 3,
        }
    }

    /// Human-readable name.
    pub fn name(self) -> &'static str {
        match self {
            Good::Opium => "Opium",
            Good::Silk => "Silk",
            Good::Arms => "Arms",
            Good::General => "General Cargo",
        }
    }

    /// Parse a good from user input (case-insensitive, prefix-friendly).
    /// "o"/"opium" -> Opium, etc. Returns None if no unambiguous match.
    pub fn parse(input: &str) -> Option<Good> {
        let s = input.trim().to_lowercase();
        if s.is_empty() {
            return None;
        }
        match s.as_str() {
            _ if "opium".starts_with(&s) => Some(Good::Opium),
            _ if "silk".starts_with(&s) => Some(Good::Silk),
            _ if "arms".starts_with(&s) => Some(Good::Arms),
            _ if "general".starts_with(&s) => Some(Good::General),
            _ => None,
        }
    }

    /// `(low, high)` base price range for this good — classic Taipan! magnitudes.
    fn price_range(self) -> (u32, u32) {
        match self {
            Good::Opium => (500, 1500),
            Good::Silk => (40, 180),
            Good::Arms => (350, 900),
            Good::General => (10, 50),
        }
    }
}

impl fmt::Display for Good {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

/// Errors a buy/sell attempt can produce. Typed so the UI can render a precise
/// message and the logic stays testable without parsing strings.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeError {
    /// Tried to buy more than `cash` allows; carries how many were affordable.
    NotEnoughCash { affordable: u32 },
    /// Tried to load more than remaining hold space; carries the free space.
    NotEnoughHold { space: u32 },
    /// Tried to sell more units than are in the hold; carries the amount held.
    NotEnoughGoods { held: u32 },
}

/// Current per-unit prices at the present port, indexed by `Good::index`.
#[derive(Debug, Clone)]
pub struct Market {
    prices: [u32; 4],
}

impl Market {
    /// Build a market with explicit prices (used in tests).
    pub fn with_prices(prices: [u32; 4]) -> Self {
        Market { prices }
    }

    /// Roll fresh prices for every good from their base ranges, then apply a
    /// per-good percent `bias` (e.g. `-40` = 40% cheaper, `+30` = 30% dearer),
    /// indexed by `Good::index`.
    ///
    /// The bias is pure arithmetic applied *after* the RNG draw, so a zero-bias
    /// call (classic economy) rolls the exact same sequence as before — seed
    /// determinism is preserved. The result is clamped back into the good's
    /// base range so a heavy bias can't push prices to absurd values.
    ///
    /// `Market` is deliberately ignorant of `Port`: the caller (the economy
    /// layer) translates a port into a bias array. That keeps the dependency
    /// direction clean and lets new economy models add bias logic without
    /// touching this type.
    pub fn generate_for(bias: [i32; 4], rng: &mut Rng) -> Self {
        let mut prices = [0u32; 4];
        for good in Good::ALL {
            let (low, high) = good.price_range();
            let rolled = rng.range(low, high) as i64;
            let skewed = rolled + rolled * bias[good.index()] as i64 / 100;
            prices[good.index()] = skewed.clamp(low as i64, high as i64) as u32;
        }
        Market { prices }
    }

    /// Convenience for the classic economy: roll with no port bias.
    pub fn generate(rng: &mut Rng) -> Self {
        Market::generate_for([0; 4], rng)
    }

    /// Current price of a good.
    pub fn price(&self, good: Good) -> u32 {
        self.prices[good.index()]
    }

    /// All four prices in `Good::index` order — for serialization.
    pub fn prices(&self) -> [u32; 4] {
        self.prices
    }

    /// Overwrite a single good's price (used by spike/drop events).
    pub fn set_price(&mut self, good: Good, price: u32) {
        self.prices[good.index()] = price;
    }
}

/// The ship's cargo hold: fixed capacity, per-good unit counts.
#[derive(Debug, Clone)]
pub struct Hold {
    capacity: u32,
    units: [u32; 4],
}

impl Hold {
    pub fn new(capacity: u32) -> Self {
        Hold {
            capacity,
            units: [0; 4],
        }
    }

    /// Reconstruct a hold from saved parts (capacity + per-good unit counts).
    pub fn from_parts(capacity: u32, units: [u32; 4]) -> Self {
        Hold { capacity, units }
    }

    pub fn capacity(&self) -> u32 {
        self.capacity
    }

    /// Per-good unit counts in `Good::index` order — for serialization.
    pub fn units(&self) -> [u32; 4] {
        self.units
    }

    /// Total units currently loaded across all goods.
    pub fn used(&self) -> u32 {
        self.units.iter().sum()
    }

    /// Free space remaining.
    pub fn free(&self) -> u32 {
        self.capacity - self.used()
    }

    /// Units of a specific good on board.
    pub fn quantity(&self, good: Good) -> u32 {
        self.units[good.index()]
    }

    /// Increase capacity (ship hold upgrades, expand-later hook).
    pub fn expand(&mut self, extra: u32) {
        self.capacity += extra;
    }

    /// Remove up to `n` units of a good without payment (used when thrown to
    /// pirates). Returns how many were actually removed.
    pub fn jettison(&mut self, good: Good, n: u32) -> u32 {
        let removed = n.min(self.units[good.index()]);
        self.units[good.index()] -= removed;
        removed
    }
}

/// Buy `qty` units of `good` at the market price. On success mutates `cash` and
/// `hold` and returns the total cost. On failure nothing is mutated.
pub fn buy(
    market: &Market,
    hold: &mut Hold,
    cash: &mut u32,
    good: Good,
    qty: u32,
) -> Result<u32, TradeError> {
    let price = market.price(good);
    // A free good (price 0) is unlimited-affordable; otherwise how many units
    // the cash covers. `checked_div` yields None on the price==0 case.
    let affordable = cash.checked_div(price).unwrap_or(qty);
    if qty > affordable {
        return Err(TradeError::NotEnoughCash { affordable });
    }
    if qty > hold.free() {
        return Err(TradeError::NotEnoughHold { space: hold.free() });
    }
    let cost = price * qty;
    *cash -= cost;
    hold.units[good.index()] += qty;
    Ok(cost)
}

/// Sell `qty` units of `good` at the market price. On success mutates `cash`
/// and `hold` and returns the total proceeds. On failure nothing is mutated.
pub fn sell(
    market: &Market,
    hold: &mut Hold,
    cash: &mut u32,
    good: Good,
    qty: u32,
) -> Result<u32, TradeError> {
    let held = hold.quantity(good);
    if qty > held {
        return Err(TradeError::NotEnoughGoods { held });
    }
    let proceeds = market.price(good) * qty;
    *cash += proceeds;
    hold.units[good.index()] -= qty;
    Ok(proceeds)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn good_parse_prefixes() {
        assert_eq!(Good::parse("o"), Some(Good::Opium));
        assert_eq!(Good::parse("OPIUM"), Some(Good::Opium));
        assert_eq!(Good::parse("si"), Some(Good::Silk));
        assert_eq!(Good::parse("ar"), Some(Good::Arms));
        assert_eq!(Good::parse("g"), Some(Good::General));
        assert_eq!(Good::parse(" general "), Some(Good::General));
        assert_eq!(Good::parse("x"), None);
        assert_eq!(Good::parse(""), None);
    }

    #[test]
    fn generated_prices_in_range() {
        let mut rng = Rng::new(2024);
        for _ in 0..1000 {
            let m = Market::generate(&mut rng);
            assert!((500..=1500).contains(&m.price(Good::Opium)));
            assert!((40..=180).contains(&m.price(Good::Silk)));
            assert!((350..=900).contains(&m.price(Good::Arms)));
            assert!((10..=50).contains(&m.price(Good::General)));
        }
    }

    #[test]
    fn buy_deducts_cash_and_loads_hold() {
        let m = Market::with_prices([100, 0, 0, 0]);
        let mut hold = Hold::new(60);
        let mut cash = 1000;
        let cost = buy(&m, &mut hold, &mut cash, Good::Opium, 5).unwrap();
        assert_eq!(cost, 500);
        assert_eq!(cash, 500);
        assert_eq!(hold.quantity(Good::Opium), 5);
        assert_eq!(hold.used(), 5);
    }

    #[test]
    fn buy_rejects_when_too_expensive() {
        let m = Market::with_prices([100, 0, 0, 0]);
        let mut hold = Hold::new(60);
        let mut cash = 250; // affords 2
        let err = buy(&m, &mut hold, &mut cash, Good::Opium, 3).unwrap_err();
        assert_eq!(err, TradeError::NotEnoughCash { affordable: 2 });
        // Nothing mutated on failure.
        assert_eq!(cash, 250);
        assert_eq!(hold.used(), 0);
    }

    #[test]
    fn buy_rejects_when_hold_full() {
        let m = Market::with_prices([1, 0, 0, 0]);
        let mut hold = Hold::new(10);
        let mut cash = 100_000;
        let err = buy(&m, &mut hold, &mut cash, Good::Opium, 11).unwrap_err();
        assert_eq!(err, TradeError::NotEnoughHold { space: 10 });
        assert_eq!(hold.used(), 0);
    }

    #[test]
    fn sell_adds_cash_and_unloads_hold() {
        let m = Market::with_prices([100, 0, 0, 0]);
        let mut hold = Hold::new(60);
        let mut cash = 1000;
        buy(&m, &mut hold, &mut cash, Good::Opium, 5).unwrap();
        let sell_market = Market::with_prices([200, 0, 0, 0]); // price doubled
        let proceeds = sell(&sell_market, &mut hold, &mut cash, Good::Opium, 5).unwrap();
        assert_eq!(proceeds, 1000);
        assert_eq!(cash, 1500); // 500 left after buy + 1000 from sale
        assert_eq!(hold.quantity(Good::Opium), 0);
    }

    #[test]
    fn sell_rejects_more_than_held() {
        let m = Market::with_prices([100, 0, 0, 0]);
        let mut hold = Hold::new(60);
        let mut cash = 1000;
        buy(&m, &mut hold, &mut cash, Good::Opium, 2).unwrap();
        let err = sell(&m, &mut hold, &mut cash, Good::Opium, 3).unwrap_err();
        assert_eq!(err, TradeError::NotEnoughGoods { held: 2 });
    }

    #[test]
    fn jettison_caps_at_held() {
        let mut hold = Hold::new(60);
        let m = Market::with_prices([1, 0, 0, 0]);
        let mut cash = 1000;
        buy(&m, &mut hold, &mut cash, Good::Opium, 5).unwrap();
        assert_eq!(hold.jettison(Good::Opium, 10), 5);
        assert_eq!(hold.quantity(Good::Opium), 0);
    }

    #[test]
    fn hold_free_space_tracks_usage() {
        let mut hold = Hold::new(60);
        assert_eq!(hold.free(), 60);
        let m = Market::with_prices([1, 1, 1, 1]);
        let mut cash = 1000;
        buy(&m, &mut hold, &mut cash, Good::Silk, 20).unwrap();
        assert_eq!(hold.free(), 40);
        hold.expand(40);
        assert_eq!(hold.free(), 80);
    }
}
