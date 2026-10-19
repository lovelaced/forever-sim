//! Data-driven Monte Carlo combat simulator for World of Warcraft: Forever.
//!
//! The game rules that change between beta builds (classes, talents,
//! spells, racials, gear kits, enemies, scenarios) live in TOML files
//! under `data/`; this crate is the engine that reads and runs them.

pub mod api;
/// Data files and preset builds, embedded at build time (see build.rs).
pub mod embedded {
    include!(concat!(env!("OUT_DIR"), "/embedded.rs"));
}
pub mod audit;
pub mod build;
pub mod engine;
pub mod explain;
pub mod expr;
pub mod ffi;
pub mod mc;
pub mod model;
pub mod rng;
pub mod scenario;
pub mod stats;
pub mod store;
pub mod validate;
pub mod vars;
pub mod world;
