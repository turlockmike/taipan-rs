//! Save/restore of a complete game as JSON — dependency-free.
//!
//! This is the backbone of the *stateless step interface* used by automated
//! play: each turn is a separate process invocation that loads a `Save`,
//! applies one action, and writes the `Save` back. For determinism to survive
//! a reload, we persist the RNG's live state (not just the seed) and any
//! in-progress pirate battle.
//!
//! We hand-roll a tiny JSON encoder/parser rather than pull in `serde`, keeping
//! the project zero-dependency. The shape is flat and fixed, so a minimal
//! parser is enough.

use crate::combat::Battle;
use crate::economy::EconomyMode;
use crate::game::{Game, Outcome};
use crate::market::{Good, Hold, Market};
use crate::rng::Rng;
use crate::travel::Port;

/// Save-format version. Bump when the on-disk shape changes incompatibly so
/// `from_json` can reject old saves with a clear message instead of failing on
/// a mysteriously-missing field.
pub const SAVE_VERSION: u32 = 1;

/// What the game is waiting for — determines which actions are valid next.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Pending {
    /// Awaiting a normal command (buy/sell/travel/bank/retire).
    Command,
    /// Mid-battle: awaiting a combat decision (fight/run/throw). Holds the
    /// serialized battle so it survives across step invocations.
    Combat { enemies: Vec<u32>, sunk: u32 },
}

/// A full, reloadable game snapshot: the game world, the RNG state, what the
/// game is waiting for, and a short human/machine note about the last event.
#[derive(Debug, Clone)]
pub struct Save {
    pub game: Game,
    pub rng_state: u64,
    pub pending: Pending,
    /// One-line description of what just happened (for the player to read).
    pub last_event: String,
}

impl Save {
    /// Reconstruct the live `Battle` when mid-combat, else None.
    pub fn battle(&self) -> Option<Battle> {
        match &self.pending {
            Pending::Combat { enemies, sunk } => Some(Battle::from_parts(enemies.clone(), *sunk)),
            Pending::Command => None,
        }
    }
}

// ---- Encoding --------------------------------------------------------------

fn mode_str(m: EconomyMode) -> &'static str {
    m.name()
}

fn outcome_str(o: Option<Outcome>) -> &'static str {
    match o {
        None => "playing",
        Some(Outcome::Won) => "won",
        Some(Outcome::ShipDestroyed) => "ship_destroyed",
    }
}

fn json_escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for c in s.chars() {
        match c {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\n' => out.push_str("\\n"),
            '\t' => out.push_str("\\t"),
            _ => out.push(c),
        }
    }
    out
}

/// Serialize a save to a pretty, stable JSON string.
pub fn to_json(save: &Save) -> String {
    let g = &save.game;
    let prices = g.market.prices();
    let units = g.hold.units();
    let wh = g.warehouse;

    // pending block
    let pending = match &save.pending {
        Pending::Command => "\"command\"".to_string(),
        Pending::Combat { enemies, sunk } => {
            let list = enemies
                .iter()
                .map(|h| h.to_string())
                .collect::<Vec<_>>()
                .join(", ");
            format!("{{ \"combat\": {{ \"enemy_hps\": [{list}], \"sunk\": {sunk} }} }}")
        }
    };

    format!(
        concat!(
            "{{\n",
            "  \"version\": {version},\n",
            "  \"pending\": {pending},\n",
            "  \"last_event\": \"{event}\",\n",
            "  \"outcome\": \"{outcome}\",\n",
            "  \"mode\": \"{mode}\",\n",
            "  \"location\": \"{location}\",\n",
            "  \"cash\": {cash},\n",
            "  \"bank\": {bank},\n",
            "  \"debt\": {debt},\n",
            "  \"net_worth\": {net_worth},\n",
            "  \"guns\": {guns},\n",
            "  \"health\": {health},\n",
            "  \"hold_capacity\": {cap},\n",
            "  \"hold\": {{ \"opium\": {ho}, \"silk\": {hs}, \"arms\": {ha}, \"general\": {hg} }},\n",
            "  \"warehouse\": {{ \"opium\": {wo}, \"silk\": {ws}, \"arms\": {wa}, \"general\": {wg} }},\n",
            "  \"prices\": {{ \"opium\": {po}, \"silk\": {ps}, \"arms\": {pa}, \"general\": {pg} }},\n",
            "  \"rng_state\": {rng}\n",
            "}}\n"
        ),
        version = SAVE_VERSION,
        pending = pending,
        event = json_escape(&save.last_event),
        outcome = outcome_str(g.outcome),
        mode = mode_str(g.mode),
        location = g.location.name(),
        cash = g.cash,
        bank = g.bank,
        debt = g.debt,
        net_worth = g.net_worth(),
        guns = g.guns,
        health = g.health,
        cap = g.hold.capacity(),
        ho = units[0], hs = units[1], ha = units[2], hg = units[3],
        wo = wh[0], ws = wh[1], wa = wh[2], wg = wh[3],
        po = prices[0], ps = prices[1], pa = prices[2], pg = prices[3],
        rng = save.rng_state,
    )
}

// ---- Parsing ---------------------------------------------------------------
//
// A minimal extractor: the JSON we emit is flat and machine-generated, so we
// pull each field by key rather than building a general parser. Strict enough
// to catch a corrupt/missing field, lenient about whitespace.

fn find_number(json: &str, key: &str) -> Result<u64, String> {
    let pat = format!("\"{key}\":");
    let start = json
        .find(&pat)
        .ok_or_else(|| format!("missing field '{key}'"))?
        + pat.len();
    let rest = &json[start..];
    let digits: String = rest
        .chars()
        .skip_while(|c| c.is_whitespace())
        .take_while(|c| c.is_ascii_digit())
        .collect();
    digits
        .parse::<u64>()
        .map_err(|_| format!("field '{key}' is not a number"))
}

fn find_string(json: &str, key: &str) -> Result<String, String> {
    let pat = format!("\"{key}\":");
    let start = json
        .find(&pat)
        .ok_or_else(|| format!("missing field '{key}'"))?
        + pat.len();
    let rest = &json[start..];
    let open = rest
        .find('"')
        .ok_or_else(|| format!("field '{key}' is not a string"))?;
    // Walk the value honoring backslash escapes, so a `\"` inside the string
    // doesn't end it prematurely. This inverts `json_escape`.
    let mut out = String::new();
    let mut chars = rest[open + 1..].chars();
    loop {
        match chars.next() {
            None => return Err(format!("field '{key}' string not terminated")),
            Some('"') => break, // unescaped closing quote
            Some('\\') => match chars.next() {
                Some('"') => out.push('"'),
                Some('\\') => out.push('\\'),
                Some('n') => out.push('\n'),
                Some('t') => out.push('\t'),
                Some(other) => out.push(other), // unknown escape: keep literal
                None => return Err(format!("field '{key}' ends in a dangling escape")),
            },
            Some(c) => out.push(c),
        }
    }
    Ok(out)
}

/// Extract a nested numeric field like `"hold": { "opium": 3, ... }`.
fn find_nested(json: &str, outer: &str, inner: &str) -> Result<u64, String> {
    let pat = format!("\"{outer}\":");
    let start = json
        .find(&pat)
        .ok_or_else(|| format!("missing field '{outer}'"))?
        + pat.len();
    // Search for the inner key only within this object's braces.
    let rest = &json[start..];
    let obj_end = rest
        .find('}')
        .ok_or_else(|| format!("field '{outer}' object not terminated"))?;
    find_number(&rest[..obj_end], inner).map_err(|e| format!("{outer}.{e}"))
}

fn parse_mode(s: &str) -> Result<EconomyMode, String> {
    EconomyMode::parse(s).ok_or_else(|| format!("unknown mode '{s}'"))
}

fn parse_port(s: &str) -> Result<Port, String> {
    Port::ALL
        .iter()
        .copied()
        .find(|p| p.name() == s)
        .ok_or_else(|| format!("unknown port '{s}'"))
}

fn parse_outcome(s: &str) -> Result<Option<Outcome>, String> {
    match s {
        "playing" => Ok(None),
        "won" => Ok(Some(Outcome::Won)),
        "ship_destroyed" => Ok(Some(Outcome::ShipDestroyed)),
        other => Err(format!("unknown outcome '{other}'")),
    }
}

/// Parse a `Save` from JSON produced by [`to_json`].
pub fn from_json(json: &str) -> Result<Save, String> {
    // Version gate first: a save from an incompatible format gets a clear
    // message, not a confusing missing-field error deeper in parsing. Saves
    // predating the version field (v0) are also rejected explicitly.
    let version = find_number(json, "version").map_err(|_| {
        format!("save predates version {SAVE_VERSION} (no version field); please start a new game")
    })? as u32;
    if version != SAVE_VERSION {
        return Err(format!(
            "save is version {version}, but this build uses version {SAVE_VERSION}; please start a new game"
        ));
    }

    let mode = parse_mode(&find_string(json, "mode")?)?;
    let location = parse_port(&find_string(json, "location")?)?;
    let outcome = parse_outcome(&find_string(json, "outcome")?)?;

    let cash = find_number(json, "cash")? as u32;
    let bank = find_number(json, "bank")?;
    let debt = find_number(json, "debt")?;
    let guns = find_number(json, "guns")? as u32;
    let health = find_number(json, "health")? as u32;
    let cap = find_number(json, "hold_capacity")? as u32;
    let rng_state = find_number(json, "rng_state")?;

    let units = [
        find_nested(json, "hold", "opium")? as u32,
        find_nested(json, "hold", "silk")? as u32,
        find_nested(json, "hold", "arms")? as u32,
        find_nested(json, "hold", "general")? as u32,
    ];
    // Guard the hold invariant: cargo carried must not exceed capacity. A
    // corrupt or hand-edited save violating this would make `Hold::free()`
    // (capacity - used) underflow on u32, silently corrupting all later
    // capacity math. Reject it loudly instead.
    let used: u32 = units.iter().sum();
    if used > cap {
        return Err(format!(
            "corrupt save: hold carries {used} units but capacity is {cap}"
        ));
    }
    let warehouse = [
        find_nested(json, "warehouse", "opium")? as u32,
        find_nested(json, "warehouse", "silk")? as u32,
        find_nested(json, "warehouse", "arms")? as u32,
        find_nested(json, "warehouse", "general")? as u32,
    ];
    let prices = [
        find_nested(json, "prices", "opium")? as u32,
        find_nested(json, "prices", "silk")? as u32,
        find_nested(json, "prices", "arms")? as u32,
        find_nested(json, "prices", "general")? as u32,
    ];

    // pending: either the string "command" or a {"combat": {...}} object.
    let pending = if json.contains("\"combat\":") {
        let enemy_block = {
            let start =
                json.find("\"enemy_hps\":").ok_or("missing enemy_hps")? + "\"enemy_hps\":".len();
            let rest = &json[start..];
            let open = rest.find('[').ok_or("enemy_hps not an array")?;
            let close = rest.find(']').ok_or("enemy_hps not terminated")?;
            rest[open + 1..close].to_string()
        };
        let enemies: Vec<u32> = enemy_block
            .split(',')
            .filter_map(|s| s.trim().parse::<u32>().ok())
            .collect();
        let sunk = find_nested(json, "combat", "sunk")? as u32;
        Pending::Combat { enemies, sunk }
    } else {
        Pending::Command
    };

    let last_event = find_string(json, "last_event").unwrap_or_default();

    let game = Game {
        cash,
        bank,
        debt,
        location,
        hold: Hold::from_parts(cap, units),
        warehouse,
        guns,
        health,
        market: Market::with_prices(prices),
        outcome,
        mode,
    };
    let _ = Good::ALL; // keep import meaningful if fields change

    Ok(Save {
        game,
        rng_state,
        pending,
        last_event,
    })
}

/// Build the initial save for a brand-new game.
pub fn new_save(mode: EconomyMode, seed: u64) -> Save {
    let mut rng = Rng::new(seed);
    let game = Game::new_with_mode(mode, &mut rng);
    Save {
        game,
        rng_state: rng.state(),
        pending: Pending::Command,
        last_event: "New game. Welcome, Taipan.".to_string(),
    }
}

/// Snapshot a live interactive game into a `Save`. The interactive loop resolves
/// combat within a single turn, so a between-turns snapshot is always in the
/// `Command` state.
pub fn save_from(game: &Game, rng: &Rng, last_event: &str) -> Save {
    Save {
        game: game.clone(),
        rng_state: rng.state(),
        pending: Pending::Command,
        last_event: last_event.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_save() -> Save {
        new_save(EconomyMode::Trader, 17)
    }

    #[test]
    fn round_trips_a_fresh_game() {
        let save = sample_save();
        let json = to_json(&save);
        let back = from_json(&json).expect("parse");
        assert_eq!(back.game.cash, save.game.cash);
        assert_eq!(back.game.debt, save.game.debt);
        assert_eq!(back.game.location, save.game.location);
        assert_eq!(back.game.mode, save.game.mode);
        assert_eq!(back.game.guns, save.game.guns);
        assert_eq!(back.game.health, save.game.health);
        assert_eq!(back.rng_state, save.rng_state);
        assert_eq!(back.pending, Pending::Command);
        assert_eq!(back.game.market.prices(), save.game.market.prices());
    }

    #[test]
    fn round_trips_hold_and_warehouse() {
        let mut save = sample_save();
        save.game.hold = Hold::from_parts(60, [1, 2, 3, 4]);
        save.game.warehouse = [5, 6, 7, 8];
        let json = to_json(&save);
        let back = from_json(&json).unwrap();
        assert_eq!(back.game.hold.units(), [1, 2, 3, 4]);
        assert_eq!(back.game.warehouse, [5, 6, 7, 8]);
    }

    #[test]
    fn round_trips_a_battle_in_progress() {
        let mut save = sample_save();
        save.pending = Pending::Combat {
            enemies: vec![2, 1, 2],
            sunk: 1,
        };
        let json = to_json(&save);
        let back = from_json(&json).unwrap();
        // It reconstructs into a live Battle.
        let battle = back.battle().unwrap();
        assert_eq!(battle.enemies_remaining(), 3);
        assert_eq!(battle.sunk(), 1);
        match &back.pending {
            Pending::Combat { enemies, sunk } => {
                assert_eq!(*enemies, vec![2, 1, 2]);
                assert_eq!(*sunk, 1);
            }
            other => panic!("expected combat, got {other:?}"),
        }
    }

    #[test]
    fn rng_state_restores_exact_future_sequence() {
        // The whole point of saving rng_state: a reloaded game must produce the
        // identical next draws, not just "a random game from the seed".
        let mut original = Rng::new(999);
        for _ in 0..37 {
            original.next_u64(); // advance to a mid-sequence point
        }
        let saved = original.state();
        let mut restored = Rng::from_state(saved);
        // Both must now yield the same next 100 values.
        for _ in 0..100 {
            assert_eq!(original.next_u64(), restored.next_u64());
        }
    }

    #[test]
    fn outcome_round_trips() {
        let mut save = sample_save();
        save.game.outcome = Some(Outcome::Won);
        let back = from_json(&to_json(&save)).unwrap();
        assert_eq!(back.game.outcome, Some(Outcome::Won));
    }

    #[test]
    fn missing_field_is_an_error_not_a_panic() {
        let err = from_json("{ \"cash\": 5 }").unwrap_err();
        assert!(!err.is_empty());
    }

    #[test]
    fn last_event_with_quotes_round_trips() {
        // A narration containing a double-quote must survive encode -> decode
        // intact (find_string honors the \" escape json_escape emits).
        let mut save = sample_save();
        save.last_event = r#"The "Sea Witch" attacks! Tabs:	end"#.to_string();
        let back = from_json(&to_json(&save)).unwrap();
        assert_eq!(back.last_event, save.last_event);
    }

    #[test]
    fn save_carries_version_and_rejects_mismatch() {
        let json = to_json(&sample_save());
        assert!(
            json.contains("\"version\": 1"),
            "save should carry a version"
        );
        // A save with a wrong version is rejected with a clear message.
        let bumped = json.replace("\"version\": 1", "\"version\": 999");
        let err = from_json(&bumped).unwrap_err();
        assert!(
            err.contains("version"),
            "expected version error, got: {err}"
        );
        // A save with no version (legacy v0) is also rejected.
        let stripped = json.replace("\"version\": 1,\n", "");
        assert!(from_json(&stripped).is_err());
    }

    #[test]
    fn corrupt_hold_over_capacity_is_rejected() {
        // Hand-build a save where the hold carries more than its capacity.
        let mut save = sample_save();
        save.game.hold = Hold::from_parts(10, [20, 0, 0, 0]); // 20 > 10
        let json = to_json(&save);
        let err = from_json(&json).unwrap_err();
        assert!(
            err.contains("capacity"),
            "expected capacity error, got: {err}"
        );
    }

    #[test]
    fn emitted_json_is_machine_parseable_shape() {
        // Sanity: the output contains the keys an agent reads to decide a move.
        let json = to_json(&sample_save());
        for key in [
            "\"pending\"",
            "\"cash\"",
            "\"prices\"",
            "\"location\"",
            "\"net_worth\"",
            "\"outcome\"",
        ] {
            assert!(json.contains(key), "output missing {key}");
        }
    }
}
