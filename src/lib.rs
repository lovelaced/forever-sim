//! Data-driven Monte Carlo combat simulator for World of Warcraft: Forever.
//!
//! The game rules that change between beta builds (classes, talents,
//! spells, racials, gear kits, enemies, scenarios) live in TOML files
//! under `data/`; this crate is the engine that reads and runs them.

pub mod api;
pub mod build;
pub mod engine;
pub mod expr;
pub mod ffi;
pub mod mc;
pub mod model;
pub mod rng;
pub mod scenario;
pub mod stats;
pub mod validate;
pub mod vars;
pub mod world;
