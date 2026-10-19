//! The cached spell modifiers (`Sim::spell_mod_vals`) are only a shortcut:
//! every build in every test, checked against a fresh calculation at every
//! cache hit, bit for bit. Its own test binary, since the check is a global
//! switch.

use forever_sim::build::load_build;
use forever_sim::scenario::{run_rep, RepInput};
use forever_sim::world::{load_world, LoadOptions};
use std::path::{Path, PathBuf};

#[test]
fn cached_spell_modifiers_match_fresh_ones() {
    forever_sim::engine::VERIFY_SM_CACHE.store(true, std::sync::atomic::Ordering::Relaxed);
    let w = load_world(&LoadOptions { data_dir: PathBuf::from("data"), ..Default::default() }).unwrap();
    let mut paths: Vec<PathBuf> = std::fs::read_dir("builds").unwrap().flat_map(|d| std::fs::read_dir(d.unwrap().path()).unwrap()).map(|f| f.unwrap().path()).filter(|p| p.extension().map_or(false, |x| x == "toml")).collect();
    paths.sort();
    for p in paths {
        for lvl in [20u32, 40, 60] {
            let b = load_build(&w, &p, Some(lvl)).unwrap();
            let opp = load_build(&w, Path::new("builds/rogue/combat.toml"), Some(lvl)).unwrap();
            for sc in w.scenarios.values() {
                if forever_sim::scenario::unfit(sc, &w, &b).is_some() {
                    continue;
                }
                let mut opts: Vec<f64> = w.opts.iter().map(|o| o.default).collect();
                for &(i, x) in &sc.opts {
                    opts[i as usize] = x;
                }
                for seed in [3u64, 11] {
                    run_rep(&RepInput { w: &w, sc, build: &b, opponent: Some(&opp), opts: &opts, seed, log: false });
                }
            }
        }
    }
}
