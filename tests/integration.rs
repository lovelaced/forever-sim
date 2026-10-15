//! End-to-end checks on the shipped data: everything loads, every build runs
//! in every scenario, results are reproducible and independent of threads,
//! and the Monte Carlo machinery behaves statistically.

use forever_sim::build::{auto_order, load_build, validate_order};
use forever_sim::mc::{self, McCfg, Variant};
use forever_sim::model::*;
use forever_sim::scenario::{run_rep, RepInput};
use forever_sim::world::{load_world, LoadOptions};
use std::path::{Path, PathBuf};

fn world() -> World {
    load_world(&LoadOptions { data_dir: PathBuf::from("data"), ..Default::default() }).expect("data loads")
}

fn builds() -> Vec<PathBuf> {
    let mut v = Vec::new();
    for d in std::fs::read_dir("builds").unwrap() {
        let d = d.unwrap().path();
        if d.is_dir() {
            for f in std::fs::read_dir(&d).unwrap() {
                let f = f.unwrap().path();
                if f.extension().map_or(false, |x| x == "toml") {
                    v.push(f);
                }
            }
        }
    }
    v.sort();
    v
}

fn defaults(w: &World, sc: &ScenarioDef) -> Vec<f64> {
    let mut v: Vec<f64> = w.opts.iter().map(|o| o.default).collect();
    for &(i, x) in &sc.opts {
        v[i as usize] = x;
    }
    v
}

#[test]
fn every_class_has_data_and_a_build() {
    let w = world();
    assert_eq!(w.classes.len(), 9, "nine classes");
    let bs = builds();
    for c in &w.classes {
        let has = bs.iter().any(|p| load_build(&w, p, None).map_or(false, |b| w.classes[b.class].id == c.id));
        assert!(has, "no build file for {}", c.id);
    }
}

#[test]
fn every_build_runs_every_scenario_at_20_40_60() {
    let w = world();
    let duel_opp = load_build(&w, Path::new("builds/warrior/vampire_v2.toml"), None).unwrap();
    for p in builds() {
        for lvl in [20u32, 40, 60] {
            let b = load_build(&w, &p, Some(lvl)).unwrap_or_else(|e| panic!("{e}"));
            let opp = load_build(&w, Path::new("builds/warrior/vampire_v2.toml"), Some(lvl)).unwrap();
            let _ = &duel_opp;
            for sc in w.scenarios.values() {
                let opts = defaults(&w, sc);
                let (v, _) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: Some(&opp), opts: &opts, seed: 7, log: false });
                for (k, x) in &v {
                    assert!(x.is_finite(), "{} level {lvl} {}: {k} = {x}", p.display(), sc.id);
                }
                assert!(v.contains_key(&sc.primary), "{} {}: no primary metric", p.display(), sc.id);
            }
        }
    }
}

#[test]
fn auto_talent_orders_are_legal() {
    let w = world();
    for p in builds() {
        let b = load_build(&w, &p, None).unwrap();
        let order = if b.order.is_empty() { auto_order(&w, b.class, &b.talents) } else { b.order.clone() };
        assert_eq!(order.len() as u32, b.points_spent(), "{}: order covers every point", p.display());
        assert!(validate_order(&w, b.class, &order).is_empty(), "{}: illegal order", p.display());
    }
}

#[test]
fn same_seed_same_result_and_thread_count_does_not_matter() {
    let w = world();
    let sc = &w.scenarios["solo_2"];
    let mk = || {
        ["builds/warrior/vampire_v2.toml", "builds/druid/warden9.toml", "builds/warlock/pact.toml"]
            .iter()
            .map(|p| {
                let b = load_build(&w, Path::new(p), Some(40)).unwrap();
                Variant { world: 0, name: b.name.clone(), opts: defaults(&w, sc), build: b, opponent: None }
            })
            .collect::<Vec<_>>()
    };
    let cfg1 = McCfg { min_reps: 40, max_reps: 40, batch: 13, threads: 1, ..Default::default() };
    let cfg4 = McCfg { threads: 4, batch: 7, ..cfg1.clone() };
    let a = mc::run(&[&w], "solo_2", &mk(), &cfg1);
    let b = mc::run(&[&w], "solo_2", &mk(), &cfg4);
    assert_eq!(a.values, b.values, "results must not depend on threads or batch size");
}

#[test]
fn different_master_seeds_give_different_but_consistent_answers() {
    let w = world();
    let sc = &w.scenarios["boss"];
    let b = load_build(&w, Path::new("builds/rogue/combat_swords.toml"), None).unwrap();
    let run = |seed| {
        let v = vec![Variant { world: 0, name: "x".into(), opts: defaults(&w, sc), build: b.clone(), opponent: None }];
        let r = mc::run(&[&w], "boss", &v, &McCfg { seed, min_reps: 200, max_reps: 200, ..Default::default() });
        mc::summarise(&r.values[0], 0.99).into_iter().find(|s| s.name == "dps").unwrap()
    };
    let x = run(1);
    let y = run(2);
    assert_ne!(x.mean, y.mean);
    // Two independent 99% intervals for the same quantity should overlap.
    assert!(x.lo <= y.hi && y.lo <= x.hi, "{:?} vs {:?}", (x.lo, x.hi), (y.lo, y.hi));
}

#[test]
fn common_random_numbers_reduce_variance_of_differences() {
    let w = world();
    let sc = &w.scenarios["boss"];
    let v: Vec<Variant> = ["builds/warrior/vampire_v2.toml", "builds/warrior/vampire_flurry5.toml"]
        .iter()
        .map(|p| {
            let b = load_build(&w, Path::new(p), None).unwrap();
            Variant { world: 0, name: b.name.clone(), opts: defaults(&w, sc), build: b, opponent: None }
        })
        .collect();
    let r = mc::run(&[&w], "boss", &v, &McCfg { min_reps: 300, max_reps: 300, ..Default::default() });
    let cmp = mc::compare(&["a".into(), "b".into()], &r, &["dps".into()], 0.95);
    assert!(cmp[0].variance_reduction > 1.0, "CRN should help: x{}", cmp[0].variance_reduction);
}

#[test]
fn a_patch_overlay_changes_only_what_it_says() {
    let base = world();
    let patched = load_world(&LoadOptions {
        data_dir: PathBuf::from("data"),
        extra_patches: vec![PathBuf::from("data/whatif/example_hotfix.toml")],
        ..Default::default()
    })
    .unwrap();
    assert_eq!(base.spells.len(), patched.spells.len());
    assert_eq!(base.classes[0].talents.len(), patched.classes[0].talents.len());
    let sc = &base.scenarios["boss"];
    // The hotfix only touches healing, so boss damage on the same seeds is identical.
    let b0 = load_build(&base, Path::new("builds/warrior/vampire_v2.toml"), None).unwrap();
    let b1 = load_build(&patched, Path::new("builds/warrior/vampire_v2.toml"), None).unwrap();
    let opts = defaults(&base, sc);
    let (v0, _) = run_rep(&RepInput { w: &base, sc, build: &b0, opponent: None, opts: &opts, seed: 3, log: false });
    let (v1, _) = run_rep(&RepInput { w: &patched, sc: &patched.scenarios["boss"], build: &b1, opponent: None, opts: &opts, seed: 3, log: false });
    assert_eq!(v0["dps"], v1["dps"]);
}
