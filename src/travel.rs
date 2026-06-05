//! Ports and movement between them.
//!
//! This module owns the *port domain*: the seven classic Taipan! ports, their
//! names, and menu parsing. The side effects of arriving somewhere (regenerate
//! prices, accrue debt interest, roll events) are orchestrated by `game.rs`,
//! which keeps this module pure and trivially testable.

use std::fmt;

/// The seven ports of the China Seas, in canonical menu order.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Port {
    HongKong,
    Shanghai,
    Nagasaki,
    Saigon,
    Manila,
    Singapore,
    Batavia,
}

impl Port {
    /// All ports in menu order (1-based when shown to the player).
    pub const ALL: [Port; 7] = [
        Port::HongKong,
        Port::Shanghai,
        Port::Nagasaki,
        Port::Saigon,
        Port::Manila,
        Port::Singapore,
        Port::Batavia,
    ];

    /// Home port — where you start and where Elder Brother Wu keeps the books.
    pub const HOME: Port = Port::HongKong;

    pub fn name(self) -> &'static str {
        match self {
            Port::HongKong => "Hong Kong",
            Port::Shanghai => "Shanghai",
            Port::Nagasaki => "Nagasaki",
            Port::Saigon => "Saigon",
            Port::Manila => "Manila",
            Port::Singapore => "Singapore",
            Port::Batavia => "Batavia",
        }
    }

    /// 1-based menu number, matching how ports are displayed.
    pub fn menu_number(self) -> u32 {
        Port::ALL.iter().position(|&p| p == self).unwrap() as u32 + 1
    }

    /// Parse a 1-based menu selection (e.g. user types "2" -> Shanghai).
    pub fn from_menu_number(n: u32) -> Option<Port> {
        if n >= 1 && n as usize <= Port::ALL.len() {
            Some(Port::ALL[(n - 1) as usize])
        } else {
            None
        }
    }

    /// Relative travel distance between two ports, used to scale event odds and
    /// (later) journey-length mechanics. Symmetric; 0 when same port.
    ///
    /// We derive it from menu-order separation — crude but deterministic and
    /// good enough to make far journeys riskier than near ones.
    pub fn distance(self, other: Port) -> u32 {
        let a = self.menu_number() as i32;
        let b = other.menu_number() as i32;
        (a - b).unsigned_abs()
    }
}

impl fmt::Display for Port {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn menu_numbers_are_one_based_and_unique() {
        let nums: Vec<u32> = Port::ALL.iter().map(|p| p.menu_number()).collect();
        assert_eq!(nums, vec![1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn from_menu_number_round_trips() {
        for p in Port::ALL {
            assert_eq!(Port::from_menu_number(p.menu_number()), Some(p));
        }
    }

    #[test]
    fn from_menu_number_rejects_out_of_range() {
        assert_eq!(Port::from_menu_number(0), None);
        assert_eq!(Port::from_menu_number(8), None);
        assert_eq!(Port::from_menu_number(999), None);
    }

    #[test]
    fn home_is_hong_kong() {
        assert_eq!(Port::HOME, Port::HongKong);
    }

    #[test]
    fn distance_is_symmetric_and_zero_to_self() {
        assert_eq!(Port::HongKong.distance(Port::HongKong), 0);
        assert_eq!(
            Port::HongKong.distance(Port::Batavia),
            Port::Batavia.distance(Port::HongKong)
        );
        assert!(Port::HongKong.distance(Port::Batavia) > 0);
    }
}
