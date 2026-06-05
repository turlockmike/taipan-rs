//! Input/output abstraction and screen rendering.
//!
//! All player interaction goes through the `Io` trait, never `std::io`
//! directly. Production uses `StdIo` (real stdin/stdout). Tests use
//! `ScriptedIo`, which feeds canned input lines and captures everything
//! written — that's what lets a whole playthrough run inside `cargo test`, and
//! it's the same seam the shell smoke test pipes into.

use crate::game::Game;
use crate::market::Good;
use crate::travel::Port;
use std::io::{BufRead, Write};

/// The interaction seam between game logic and the outside world.
pub trait Io {
    /// Print a line (newline appended).
    fn writeln(&mut self, s: &str);
    /// Print without a trailing newline (for prompts).
    fn write(&mut self, s: &str);
    /// Read one line of input, trimmed. Returns None at end of input.
    fn read_line(&mut self) -> Option<String>;
}

/// Production I/O over real stdin/stdout.
pub struct StdIo<R: BufRead, W: Write> {
    reader: R,
    writer: W,
}

impl<R: BufRead, W: Write> StdIo<R, W> {
    pub fn new(reader: R, writer: W) -> Self {
        StdIo { reader, writer }
    }
}

impl<R: BufRead, W: Write> Io for StdIo<R, W> {
    fn writeln(&mut self, s: &str) {
        let _ = writeln!(self.writer, "{s}");
        let _ = self.writer.flush();
    }
    fn write(&mut self, s: &str) {
        let _ = write!(self.writer, "{s}");
        let _ = self.writer.flush();
    }
    fn read_line(&mut self) -> Option<String> {
        let mut buf = String::new();
        match self.reader.read_line(&mut buf) {
            Ok(0) => None, // EOF
            Ok(_) => Some(buf.trim().to_string()),
            Err(_) => None,
        }
    }
}

/// Test/scripted I/O: canned inputs in, captured output out.
#[derive(Default)]
pub struct ScriptedIo {
    inputs: std::collections::VecDeque<String>,
    pub output: String,
}

impl ScriptedIo {
    /// Build from a list of input lines the "player" will type, in order.
    pub fn new(inputs: &[&str]) -> Self {
        ScriptedIo {
            inputs: inputs.iter().map(|s| s.to_string()).collect(),
            output: String::new(),
        }
    }

    /// True once all scripted inputs have been consumed.
    pub fn inputs_exhausted(&self) -> bool {
        self.inputs.is_empty()
    }
}

impl Io for ScriptedIo {
    fn writeln(&mut self, s: &str) {
        self.output.push_str(s);
        self.output.push('\n');
    }
    fn write(&mut self, s: &str) {
        self.output.push_str(s);
    }
    fn read_line(&mut self) -> Option<String> {
        self.inputs.pop_front()
    }
}

// ---- Rendering -------------------------------------------------------------

/// Render the comprador's report: location, finances, hold, ship status.
pub fn render_status(io: &mut dyn Io, game: &Game) {
    io.writeln("");
    io.writeln("Comprador's Report");
    io.writeln(&format!("   Location    : {}", game.location));
    io.writeln(&format!("   Cash        : {}", game.cash));
    io.writeln(&format!("   Bank        : {}", game.bank));
    io.writeln(&format!("   Debt        : {}", game.debt));
    io.writeln(&format!("   Net Worth   : {}", game.net_worth()));
    io.writeln(&format!(
        "   Hold ({:>3}) : {} used, {} free",
        game.hold.capacity(),
        game.hold.used(),
        game.hold.free()
    ));
    for g in Good::ALL {
        let q = game.hold.quantity(g);
        if q > 0 {
            io.writeln(&format!("      {:<14}: {}", g.name(), q));
        }
    }
    io.writeln(&format!(
        "   Guns        : {}   Ship: {}/{}",
        game.guns,
        game.health,
        crate::game::MAX_HEALTH
    ));
}

/// Render the current port's prices.
pub fn render_prices(io: &mut dyn Io, game: &Game) {
    io.writeln("");
    io.writeln(&format!("{} prices per unit are now:", game.location));
    for g in Good::ALL {
        io.writeln(&format!("   {:<14}: {}", g.name(), game.market.price(g)));
    }
}

/// Render the travel menu (1-based port list).
pub fn render_travel_menu(io: &mut dyn Io) {
    io.writeln("");
    io.writeln("Where do you wish to travel?");
    for p in Port::ALL {
        io.writeln(&format!("  {}) {}", p.menu_number(), p.name()));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::rng::Rng;

    #[test]
    fn scripted_io_feeds_inputs_in_order() {
        let mut io = ScriptedIo::new(&["B", "Opium", "5"]);
        assert_eq!(io.read_line(), Some("B".to_string()));
        assert_eq!(io.read_line(), Some("Opium".to_string()));
        assert_eq!(io.read_line(), Some("5".to_string()));
        assert_eq!(io.read_line(), None);
        assert!(io.inputs_exhausted());
    }

    #[test]
    fn scripted_io_captures_output() {
        let mut io = ScriptedIo::new(&[]);
        io.writeln("hello");
        io.write("no-newline");
        assert_eq!(io.output, "hello\nno-newline");
    }

    #[test]
    fn render_status_includes_key_fields() {
        let mut rng = Rng::new(1);
        let game = Game::new(&mut rng);
        let mut io = ScriptedIo::new(&[]);
        render_status(&mut io, &game);
        assert!(io.output.contains("Comprador's Report"));
        assert!(io.output.contains("Hong Kong"));
        assert!(io.output.contains("Cash        : 400"));
        assert!(io.output.contains("Debt        : 5000"));
    }

    #[test]
    fn render_prices_lists_all_goods() {
        let mut rng = Rng::new(1);
        let game = Game::new(&mut rng);
        let mut io = ScriptedIo::new(&[]);
        render_prices(&mut io, &game);
        for g in Good::ALL {
            assert!(io.output.contains(g.name()), "missing {}", g.name());
        }
    }

    #[test]
    fn render_travel_menu_lists_seven_ports() {
        let mut io = ScriptedIo::new(&[]);
        render_travel_menu(&mut io);
        for p in Port::ALL {
            assert!(io.output.contains(p.name()), "missing {}", p.name());
        }
        assert!(io.output.contains("1) Hong Kong"));
        assert!(io.output.contains("7) Batavia"));
    }

    #[test]
    fn stdio_reads_and_writes() {
        // Drive StdIo with in-memory buffers to prove the production path works.
        let input = b"hello\nworld\n";
        let mut output: Vec<u8> = Vec::new();
        {
            let mut io = StdIo::new(&input[..], &mut output);
            assert_eq!(io.read_line(), Some("hello".to_string()));
            assert_eq!(io.read_line(), Some("world".to_string()));
            assert_eq!(io.read_line(), None);
            io.writeln("out");
        }
        assert_eq!(String::from_utf8(output).unwrap(), "out\n");
    }
}
