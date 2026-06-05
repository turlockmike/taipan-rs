//! Taipan! — a faithful Rust recreation of the classic 1982 trading game.
//!
//! All game logic lives in this library crate so it can be unit-tested with
//! `cargo test`. The binary (`src/main.rs`) is a thin shell: parse `--seed`,
//! wire up real stdin/stdout I/O, and run the turn loop.

pub mod action;
pub mod combat;
pub mod economy;
pub mod events;
pub mod game;
pub mod market;
pub mod rng;
pub mod run;
pub mod state;
pub mod travel;
pub mod ui;
