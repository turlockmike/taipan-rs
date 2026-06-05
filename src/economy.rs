//! Economy models — how a port's prices are biased.
//!
//! This module owns the *economy configuration* and the translation from a
//! port into a per-good price bias. `market.rs` stays ignorant of ports; it
//! only ever receives a `[i32; 4]` bias array. New economy models add their
//! logic here without touching `Market`.
//!
//! Bias is a percent skew applied to a freshly rolled base price:
//!   * negative  => this port is a *cheap source* for that good
//!   * positive  => this port is a *dear sink* for that good
//!
//! Values are deliberately a hand-tuned table, not arithmetic — they are game
//! design knobs, meant to be read and rebalanced, not derived.

use crate::market::Good;
use crate::travel::Port;

/// Which economy model the game runs under.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EconomyMode {
    /// Independent uniform prices at every port (the original feel). Profit
    /// comes from exploiting random cross-port variance — no fixed routes.
    Classic,
    /// Ports specialize: each is a cheap source for some goods and a dear sink
    /// for others, creating learnable buy-here / sell-there trade routes.
    Trader,
}

impl EconomyMode {
    /// Parse from a CLI value (case-insensitive). Returns None if unrecognized.
    pub fn parse(s: &str) -> Option<EconomyMode> {
        match s.trim().to_lowercase().as_str() {
            "classic" => Some(EconomyMode::Classic),
            "trader" => Some(EconomyMode::Trader),
            _ => None,
        }
    }

    pub fn name(self) -> &'static str {
        match self {
            EconomyMode::Classic => "classic",
            EconomyMode::Trader => "trader",
        }
    }
}

/// Per-good price bias (percent) for a port under the Trader model.
///
/// Read each row as a port's trade profile. Negatives are bargains to buy here;
/// positives are premiums you can sell into. Each port favors a different good
/// so that profitable routes exist between every pair. Columns are in
/// `Good::ALL` order: [Opium, Silk, Arms, General].
fn trader_bias(port: Port, good: Good) -> i32 {
    let row: [i32; 4] = match port {
        //                    Opium  Silk  Arms  General
        Port::HongKong => [40, 10, 30, -35], // home: dear opium/arms, cheap general
        Port::Shanghai => [-40, 25, 15, 10], // silk-road sink; cheap opium source
        Port::Nagasaki => [20, -40, 35, 15], // cheap silk source; dear arms
        Port::Saigon => [10, 20, -40, 25],   // arms bazaar: cheap guns to buy
        Port::Manila => [-30, 35, 20, -10],  // cheap opium; dear silk
        Port::Singapore => [25, -25, -30, 30], // cheap silk & arms; dear general
        Port::Batavia => [-35, 15, 25, -30], // cheap opium & general; dear arms
    };
    row[good.index()]
}

/// The full per-good bias array for a port under the given economy model.
/// Classic returns all-zero (no skew); Trader returns the port's trade profile.
pub fn biases_for(mode: EconomyMode, port: Port) -> [i32; 4] {
    match mode {
        EconomyMode::Classic => [0; 4],
        EconomyMode::Trader => {
            let mut b = [0i32; 4];
            for good in Good::ALL {
                b[good.index()] = trader_bias(port, good);
            }
            b
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trips() {
        assert_eq!(EconomyMode::parse("classic"), Some(EconomyMode::Classic));
        assert_eq!(EconomyMode::parse("Trader"), Some(EconomyMode::Trader));
        assert_eq!(EconomyMode::parse(" TRADER "), Some(EconomyMode::Trader));
        assert_eq!(EconomyMode::parse("nonsense"), None);
        assert_eq!(EconomyMode::parse(""), None);
    }

    #[test]
    fn classic_has_no_bias_anywhere() {
        for port in Port::ALL {
            assert_eq!(biases_for(EconomyMode::Classic, port), [0; 4]);
        }
    }

    #[test]
    fn trader_bias_is_nonzero_and_bounded() {
        for port in Port::ALL {
            let b = biases_for(EconomyMode::Trader, port);
            // Some good is biased at every port (no flat ports).
            assert!(b.iter().any(|&x| x != 0), "{port} is flat");
            // Stay within a sane band so clamping in market.rs is meaningful,
            // not a constant override.
            for &x in &b {
                assert!((-50..=50).contains(&x), "{port} bias {x} out of band");
            }
        }
    }

    #[test]
    fn every_good_is_cheap_somewhere_and_dear_somewhere() {
        // For each good there must exist a profitable route: a source port
        // (negative bias) and a sink port (positive bias). Without this, a good
        // is untradeable and the Trader economy has dead inventory.
        for good in Good::ALL {
            let mut min = i32::MAX;
            let mut max = i32::MIN;
            for port in Port::ALL {
                let v = biases_for(EconomyMode::Trader, port)[good.index()];
                min = min.min(v);
                max = max.max(v);
            }
            assert!(min < 0, "{good} is never cheap to buy");
            assert!(max > 0, "{good} is never dear to sell");
            // The spread must be wide enough to clear a 10%/hop debt drag.
            assert!(
                max - min >= 40,
                "{good} spread {} too thin to profit",
                max - min
            );
        }
    }
}
