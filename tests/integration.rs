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

/// Build 70425: Touch of the Grave drains a fixed share of health (no Shadow
/// damage bonuses on either side) and has a 25 sec internal cooldown.
#[test]
fn touch_of_the_grave_is_fixed_and_waits_25_sec() {
    let w = world();
    let sc = &w.scenarios["boss"];
    let b = load_build(&w, Path::new("builds/priest/shadow.toml"), None).unwrap();
    assert_eq!(w.races[b.race].id, "undead");
    let opts = defaults(&w, sc);
    let mut seen = 0;
    for seed in 1..6 {
        let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed, log: true });
        let procs: Vec<(f64, String)> = log
            .unwrap()
            .iter()
            .filter(|l| l.contains("Touch of the Grave ->"))
            .map(|l| {
                let t: f64 = l.split_whitespace().next().unwrap().parse().unwrap();
                let amount = l.split(" -> ").nth(1).unwrap().split(" [").next().unwrap().rsplit(' ').next().unwrap().to_string();
                (t, amount)
            })
            .collect();
        for p in procs.windows(2) {
            assert!(p[1].0 - p[0].0 >= 25.0 - 1e-6, "procs {:.2} and {:.2} closer than 25 sec", p[0].0, p[1].0);
            assert_eq!(p[0].1, p[1].1, "the drain should not grow with Shadow Weaving or Shadowform");
        }
        seen += procs.len();
    }
    assert!(seen >= 5, "Touch of the Grave should proc in a 2 minute fight");
}

/// Wands: the first shot comes 0.5 sec after Shoot starts, then one per wand
/// speed; casting stops it, and nothing shoots during a cast.
#[test]
fn wands_shoot_on_their_own_timer_and_stop_for_casts() {
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/warlock/affliction.toml"), Some(20)).unwrap();
    let opts = defaults(&w, sc);
    let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 7, log: true });
    let me = "Affliction";
    let mut last_shot: Option<f64> = None;
    let mut last_cast: f64 = -1.0;
    let mut shots = 0;
    for l in log.unwrap() {
        let t: f64 = l.split_whitespace().next().unwrap().parse().unwrap();
        if l.contains(me) && l.contains(" casts ") {
            last_cast = t;
            last_shot = None;
        } else if l.contains(me) && l.contains(" Shoot -> ") {
            match last_shot {
                Some(p) => assert!((t - p - 1.7).abs() < 0.011, "wand shots {p:.2} and {t:.2} should be one wand speed (1.7 sec) apart"),
                None => assert!(t - last_cast >= 0.5 - 1e-6, "first shot at {t:.2}, only {:.2} sec after a cast", t - last_cast),
            }
            last_shot = Some(t);
            shots += 1;
        }
    }
    assert!(shots >= 10, "a leveling warlock should wand ({shots} shots)");
}

/// Every talent tree has a standard build (its main tree), with a leveling
/// order, so each spec can be picked and run at any level.
#[test]
fn every_talent_tree_has_a_standard_build() {
    let w = world();
    for c in &w.classes {
        let mut covered = vec![false; c.trees.len()];
        for p in builds() {
            let t: toml::Table = toml::from_str(&std::fs::read_to_string(&p).unwrap()).unwrap();
            if t.get("kind").and_then(|v| v.as_str()) != Some("standard") || t.get("class").and_then(|v| v.as_str()) != Some(c.id.as_str()) {
                continue;
            }
            assert!(t.get("order").is_some(), "{} has no leveling order", p.display());
            let b = load_build(&w, &p, None).unwrap();
            let per: Vec<u32> = c.trees.iter().map(|tr| c.talents.iter().enumerate().filter(|(_, x)| &x.tree == tr).map(|(i, _)| b.talents[i] as u32).sum()).collect();
            let main = per.iter().enumerate().max_by_key(|(_, n)| **n).unwrap().0;
            covered[main] = true;
        }
        for (i, tr) in c.trees.iter().enumerate() {
            assert!(covered[i], "{} has no standard build for {tr}", c.name);
        }
    }
}

/// Openers are real casts: Charge opens pulls but keeps its 15 sec cooldown,
/// and a rogue opens from Stealth, which the first attack ends.
#[test]
fn openers_pay_cooldowns_and_stealth_breaks() {
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let times = |file: &str, spell: &str| -> Vec<f64> {
        let b = load_build(&w, Path::new(file), Some(30)).unwrap();
        let opts = defaults(&w, sc);
        let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 11, log: true });
        log.unwrap().iter().filter(|l| l.ends_with(&format!(" casts {spell}"))).map(|l| l.split_whitespace().next().unwrap().parse().unwrap()).collect()
    };
    let charges = times("builds/warrior/arms.toml", "Charge");
    assert!(charges.len() >= 10, "Arms should open pulls with Charge ({} times)", charges.len());
    for p in charges.windows(2) {
        assert!(p[1] - p[0] >= 15.0 - 1e-6, "Charge at {:.2} and {:.2}, inside its cooldown", p[0], p[1]);
    }
    let ambush = times("builds/rogue/subtlety.toml", "Ambush");
    let stealth = times("builds/rogue/subtlety.toml", "Stealth");
    assert!(!ambush.is_empty() && ambush.len() <= stealth.len() + 1, "Ambush needs Stealth each time ({} ambushes, {} stealths)", ambush.len(), stealth.len());
}

/// Buffs before the fight start their cooldowns: a Frost mage who puts up
/// Ice Barrier before the pull can't cast it again for 30 seconds.
#[test]
fn precombat_buffs_start_their_cooldowns() {
    let w = world();
    let sc = &w.scenarios["solo_2_hard"];
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), None).unwrap();
    let opts = defaults(&w, sc);
    let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 11, log: true });
    let log = log.unwrap();
    assert!(log.iter().any(|l| l.split_whitespace().next() == Some("0.00") && l.ends_with(" gains Ice Barrier")), "Ice Barrier goes up before the pull");
    let first = log.iter().find(|l| l.ends_with(" casts Ice Barrier")).map(|l| l.split_whitespace().next().unwrap().parse::<f64>().unwrap());
    assert!(first.map_or(false, |t| t >= 30.0 - 1e-6), "first Ice Barrier in the fight at {first:?}, inside the cooldown from before the pull");
}
