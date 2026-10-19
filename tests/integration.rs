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
                if forever_sim::scenario::unfit(sc, &w, &b).is_some() {
                    continue;
                }
                let opts = defaults(&w, sc);
                let (v, _) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: Some(&opp), opts: &opts, seed: 7, log: false });
                for (k, x) in &v {
                    assert!(x.is_finite(), "{} level {lvl} {}: {k} = {x}", p.display(), sc.id);
                }
                assert!(v.contains_key(&sc.primary), "{} {}: no primary metric", p.display(), sc.id);
                assert_eq!(v.get("engine_spins").copied().unwrap_or(0.0), 0.0, "{} level {lvl} {}: the engine got stuck", p.display(), sc.id);
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

/// Buffs before the fight are cast for real: one global cooldown after
/// another, and their cooldowns start, so a Frost mage who puts up Ice
/// Barrier before the pull can't cast it again for 30 seconds.
#[test]
fn precombat_buffs_are_cast_for_real() {
    let w = world();
    let sc = &w.scenarios["solo_2_hard"];
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), None).unwrap();
    let opts = defaults(&w, sc);
    let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 11, log: true });
    let log = log.unwrap();
    let times = |spell: &str| -> Vec<f64> {
        log.iter().filter(|l| l.ends_with(&format!(" casts {spell}"))).map(|l| l.split_whitespace().next().unwrap().parse().unwrap()).collect()
    };
    let (armor, intellect, barrier) = (times("Ice Armor"), times("Arcane Intellect"), times("Ice Barrier"));
    assert!(armor[0] < 1e-6, "Ice Armor goes up first, at {}", armor[0]);
    assert!(intellect[0] >= armor[0] + 1.5 - 1e-6, "Arcane Intellect waits a global cooldown ({} after {})", intellect[0], armor[0]);
    assert!(barrier[0] >= intellect[0] + 1.5 - 1e-6, "Ice Barrier waits a global cooldown ({} after {})", barrier[0], intellect[0]);
    assert!(barrier[1] - barrier[0] >= 30.0 - 1e-6, "second Ice Barrier at {}, inside the cooldown from before the pull ({})", barrier[1], barrier[0]);
}

/// Rage drains out of combat, later with Lingering Rage; Anger Management
/// gives Rage only in combat.
#[test]
fn rage_decays_out_of_combat() {
    use forever_sim::engine::{Phase, Sim, Stop};
    use forever_sim::vars::Res;
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let rage_after = |lingering: u8, secs: f64| -> (f64, bool) {
        let mut b = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(30)).unwrap();
        b.talents[w.classes[b.class].talent_index["lingering_rage"] as usize] = lingering;
        let mut sim = Sim::new(&w, 1, defaults(&w, sc));
        let p = sim.spawn_player(&b, 0, 1);
        let e = sim.spawn_enemy(&w.enemies["humanoid"], 30, 1, 1000, 1.0);
        sim.actors[p].target = Some(e);
        sim.enter_combat();
        let am = w.aura_index["warrior.anger_management"];
        assert!(sim.actors[p].auras.iter().any(|i| i.def == am), "Anger Management is up in combat");
        sim.actors[e].alive = false;
        sim.leave_combat();
        sim.phase = Phase::Rest;
        let am_up = sim.actors[p].auras.iter().any(|i| i.def == am);
        sim.actors[p].res[Res::Rage as usize] = 50.0;
        sim.run(Stop::Until(sim.t + secs));
        (sim.actors[p].res[Res::Rage as usize], am_up)
    };
    let (r, am_up) = rage_after(0, 10.0);
    assert!(!am_up, "Anger Management ends with the fight");
    assert!((r - (50.0 - 12.5)).abs() <= 2.5 + 1e-6, "10 sec out of combat drains about 12.5 Rage, left {r}");
    let (r, _) = rage_after(5, 9.0);
    assert!(r >= 50.0 - 1e-6, "five ranks of Lingering Rage hold it for 10 sec, left {r}");
    let (r, _) = rage_after(5, 20.0);
    assert!(r < 50.0 - 7.5, "and then it drains, left {r}");
}

/// Low ranks of Execute aren't worth a big pool of Rage: with 60 Rage the
/// line waits unless the hit would finish the target.
#[test]
fn low_rank_execute_waits_unless_it_kills() {
    use forever_sim::engine::Sim;
    use forever_sim::vars::Res;
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(30)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    sim.precombat(p);
    let e = sim.spawn_enemy(&w.enemies["boss_plus2"], 32, 1, 1000, 1.0);
    sim.actors[p].target = Some(e);
    sim.enter_combat();
    sim.actors[p].gcd_end = 0.0;
    let rot = sim.actors[p].rotation.unwrap();
    let line = rot.actions.iter().find(|a| w.spells[a.spell as usize].id == "execute").unwrap();
    sim.actors[p].res[Res::Rage as usize] = 60.0;
    let max = sim.actors[e].max_hp;
    sim.actors[e].hp = 0.19 * max;
    assert!(0.19 * max > 300.0, "the test mob is big enough ({max})");
    assert!(sim.line_plan(p, line, false, &mut None).is_none(), "rank 1 with 60 Rage on a healthy chunk waits");
    sim.actors[e].hp = 100.0;
    assert!(sim.line_plan(p, line, false, &mut None).is_some(), "but it finishes a mob with 100 health left");
    sim.actors[e].hp = 0.19 * max;
    sim.actors[p].res[Res::Rage as usize] = 25.0;
    assert!(sim.line_plan(p, line, false, &mut None).is_some(), "and it goes with little Rage to spare");
}

/// A typical player reacts to a proc: Overpower comes at least one short
/// reaction (0.35 sec) after the dodge or Bloodthrill that opened it.
#[test]
fn typical_players_react_to_procs() {
    let w = world();
    let sc = &w.scenarios["boss"];
    let b = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(60)).unwrap();
    let opts = defaults(&w, sc);
    let mut checked = 0;
    for seed in 1..6 {
        let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed, log: true });
        let log = log.unwrap();
        let at = |l: &String| -> f64 { l.split_whitespace().next().unwrap().parse().unwrap() };
        // a dodge, or a Bloodthrill proc, opens Overpower
        let opens: Vec<f64> = log.iter().filter(|l| l.contains(" is dodged by ") || l.ends_with(" gains Bloodthrill")).map(at).collect();
        for op in log.iter().filter(|l| l.ends_with(" casts Overpower")).map(at) {
            // (a proc refreshed later isn't logged as a gain, so the window
            // can reach past the first proc's 6 sec)
            let opened = opens.iter().any(|&d| op - d >= 0.35 - 1e-6 && op - d <= 12.0 + 1e-6);
            assert!(opened, "Overpower at {op:.2} with nothing opening it 0.35 to 12 sec before");
            checked += 1;
        }
    }
    assert!(checked >= 10, "only {checked} Overpowers to check");
}

/// Movement: Charge goes from 8 to 25 yards and stuns on arrival; a Frost
/// mage gets casts off before a mob reaches it.
#[test]
fn charge_rushes_in_and_casters_cast_first() {
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let opts = defaults(&w, sc);
    let at = |l: &String| -> f64 { l.split_whitespace().next().unwrap().parse().unwrap() };
    let b = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(40)).unwrap();
    let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 5, log: true });
    let log = log.unwrap();
    let mut charges = 0;
    for (k, l) in log.iter().enumerate() {
        if let Some(rest) = l.split(" charges (").nth(1) {
            let yd: f64 = rest.split(' ').next().unwrap().parse().unwrap();
            assert!((8.0 - 0.5..=25.0 + 0.5).contains(&yd), "Charge from {yd} yards: {l}");
            let t0 = at(l);
            let stun = log[k..].iter().find(|x| x.ends_with(" gains Stunned")).map(at).expect("a stun after the charge");
            assert!(stun > t0, "the stun lands on arrival, after the charge starts ({stun} vs {t0})");
            charges += 1;
        }
    }
    assert!(charges >= 5, "only {charges} charges");
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), Some(40)).unwrap();
    let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 5, log: true });
    let log = log.unwrap();
    let first_bolt = log.iter().find(|l| l.ends_with(" casts Frostbolt")).map(at).unwrap();
    let first_swing = log.iter().find(|l| l.contains(" melee -> Frost ")).map(at).unwrap();
    assert!(first_swing > first_bolt + 2.0, "the mob reaches the mage well after its first Frostbolt ({first_swing} vs {first_bolt})");
}

/// The strongest slow counts (they don't stack), and a speed boost adds.
#[test]
fn the_strongest_slow_wins() {
    use forever_sim::engine::{Cx, Sim};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
    let base = sim.speed(e);
    assert!((base - 8.0).abs() < 1e-9, "a mob runs 8 yards a second ({base})");
    let cx = Cx::new(p, Some(e), "");
    sim.apply_aura(e, w.aura_index["mage.chilled"], p, 1, Some(5.0), None, cx);
    assert!((sim.speed(e) - 8.0 * 0.7).abs() < 1e-9, "Frost Armor's chill: 30% ({})", sim.speed(e));
    sim.apply_aura(e, w.aura_index["shaman.frost_shock"], p, 1, Some(8.0), None, cx);
    assert!((sim.speed(e) - 4.0).abs() < 1e-9, "Frost Shock's 50% wins over the chill ({})", sim.speed(e));
    sim.apply_aura(e, w.aura_index["frozen"], p, 1, Some(8.0), None, cx);
    assert_eq!(sim.speed(e), 0.0, "frozen in place");
}

/// A totem is a unit on the ground: one of each element, put down where
/// the shaman stands, not a pet and not a party member.
#[test]
fn totems_replace_only_their_own_element() {
    use forever_sim::engine::{Cx, Kind, Sim};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/shaman/enhancement.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    sim.place(p, 3.0);
    let put = |sim: &mut Sim, name: &str| {
        let sp = w.spell_index[&format!("shaman.{name}")];
        let r = sim.known(p, sp).filter(|&r| r != usize::MAX).map(|r| (sp, r));
        let cx = Cx::new(p, None, "");
        sim.apply_aura(p, w.aura_index[&format!("shaman.{name}")], p, 1, None, r, cx);
    };
    put(&mut sim, "searing_totem");
    let fire = sim.actors[p].totems[0].expect("a fire totem");
    assert_eq!(sim.actors[fire].kind, Kind::Totem);
    assert_eq!(sim.actors[fire].owner, Some(p));
    assert!((sim.pos(fire) - 3.0).abs() < 1e-9, "it stands where the shaman did");
    assert!(sim.actors[fire].is_pet(), "left out of target picking and party lists");
    assert!(!sim.allies(p, false).any(|a| a == fire));
    assert!(sim.actors[p].aura_index(w.aura_index["shaman.searing_totem"], None).is_none(), "the totem holds its pulse, not the shaman");
    put(&mut sim, "strength_of_earth_totem");
    let earth = sim.actors[p].totems[1].expect("an earth totem");
    let soe = w.aura_index["shaman.strength_of_earth_totem"];
    assert!(sim.actors[p].aura_index(soe, Some(p)).is_some(), "a buff totem buffs the shaman");
    put(&mut sim, "magma_totem");
    assert!(!sim.actors[fire].alive, "Magma Totem replaces Searing Totem");
    assert!(sim.actors[p].totems[0].map_or(false, |t| t != fire && sim.actors[t].alive));
    assert_eq!(sim.actors[p].totems[1], Some(earth), "the earth totem stays");
    put(&mut sim, "stoneclaw_totem");
    assert!(!sim.actors[earth].alive, "Stoneclaw Totem replaces Strength of Earth Totem");
    assert!(sim.actors[p].aura_index(soe, Some(p)).is_none(), "and its buff goes with it");
    let claw = sim.actors[p].totems[1].unwrap();
    assert!(sim.actors[claw].max_hp > 100.0, "Stoneclaw has the rank's health ({})", sim.actors[claw].max_hp);
}

/// Searing Totem works on your target only within 20 yards of where it
/// stands; a buff totem reaches the party within 30 yards, and only while it
/// stands.
#[test]
fn totems_reach_from_where_they_stand() {
    use forever_sim::engine::{Cx, Sim};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/shaman/enhancement.toml"), Some(40)).unwrap();
    let wb = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let near = sim.spawn_player(&wb, 0, 2);
    let far = sim.spawn_player(&wb, 0, 3);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
    sim.place(p, 0.0);
    sim.place(near, -25.0);
    sim.place(far, -35.0);
    sim.place(e, 10.0);
    sim.actors[p].target = Some(e);
    let cx = Cx::new(p, None, "");
    for name in ["searing_totem", "strength_of_earth_totem"] {
        let sp = w.spell_index[&format!("shaman.{name}")];
        let r = sim.known(p, sp).filter(|&r| r != usize::MAX).map(|r| (sp, r));
        sim.apply_aura(p, w.aura_index[&format!("shaman.{name}")], p, 1, None, r, cx);
    }
    let fire = sim.actors[p].totems[0].unwrap();
    assert!(sim.totem_reaches(fire), "the target 10 yd away is in reach");
    sim.place(e, 25.0);
    assert!(!sim.totem_reaches(fire), "25 yd is out of Searing Totem's reach");
    let soe = w.aura_index["shaman.strength_of_earth_totem"];
    assert!(sim.actors[near].aura_index(soe, Some(p)).is_some(), "25 yd: in the buff");
    assert!(sim.actors[far].aura_index(soe, Some(p)).is_none(), "35 yd: out of it");
    let earth = sim.actors[p].totems[1].unwrap();
    sim.place(near, -40.0);
    sim.totem_buffs(earth);
    assert!(sim.actors[near].aura_index(soe, Some(p)).is_none(), "walking out takes the buff off");
    sim.place(near, -10.0);
    sim.totem_buffs(earth);
    assert!(sim.actors[near].aura_index(soe, Some(p)).is_some(), "and walking back in gives it again");
    sim.despawn_totem(earth);
    assert!(sim.actors[near].aura_index(soe, Some(p)).is_none() && sim.actors[p].aura_index(soe, Some(p)).is_none(), "gone with the totem");
}

/// Leaving a pull: the totems stay behind unless the world roll brings the
/// next fight to them, and then they stand where it starts.
#[test]
fn carried_totems_follow_you_to_the_next_fight() {
    use forever_sim::engine::{Cx, Sim};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/shaman/enhancement.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let cx = Cx::new(p, None, "");
    let put = |sim: &mut Sim| {
        let sp = w.spell_index["shaman.strength_of_earth_totem"];
        let r = sim.known(p, sp).filter(|&r| r != usize::MAX).map(|r| (sp, r));
        sim.apply_aura(p, w.aura_index["shaman.strength_of_earth_totem"], p, 1, None, r, cx);
        sim.actors[p].totems[1].unwrap()
    };
    let t = put(&mut sim);
    sim.leave_totems(p, false);
    assert!(!sim.actors[t].alive && sim.actors[p].totems[1].is_none(), "left behind");
    let t = put(&mut sim);
    sim.leave_totems(p, true);
    assert!(sim.actors[t].alive, "kept");
    sim.place(p, 22.0);
    sim.bring_totems(p);
    assert!((sim.pos(t) - 22.0).abs() < 1e-9, "it stands where the next fight starts");
    // the roll comes from the world, after the pull's other draws: the
    // carry-over can't shift which pulls a build meets
    let opts = defaults(&w, &w.scenarios["solo_2"]);
    let sc2 = &w.scenarios["solo_2"];
    let kinds = |path: &str| -> Vec<String> {
        let b = load_build(&w, Path::new(path), Some(40)).unwrap();
        let (_, _, ev) = forever_sim::scenario::run_rep_traced(&RepInput { w: &w, sc: sc2, build: &b, opponent: None, opts: &opts, seed: 3, log: true });
        ev.unwrap().iter().filter(|e| e.kind == "pull").map(|e| e.name.clone()).collect()
    };
    let (a, z) = (kinds("builds/shaman/enhancement.toml"), kinds("builds/warrior/arms.toml"));
    let n = a.len().min(z.len());
    assert!(n >= 10 && a[..n] == z[..n], "the same pulls for a shaman and a warrior");
}

/// Stoneclaw Totem draws the attacks of enemies within 8 yards to itself.
#[test]
fn stoneclaw_taunts_to_the_totem() {
    use forever_sim::engine::{Cx, Sim};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/shaman/enhancement.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let near = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
    let far = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1001, 1.0);
    sim.place(p, 0.0);
    sim.place(near, 4.0);
    sim.place(far, 12.0);
    sim.actors[near].target = Some(p);
    sim.actors[far].target = Some(p);
    let sp = w.spell_index["shaman.stoneclaw_totem"];
    let r = sim.known(p, sp).filter(|&r| r != usize::MAX).map(|r| (sp, r));
    sim.apply_aura(p, w.aura_index["shaman.stoneclaw_totem"], p, 1, None, r, Cx::new(p, None, ""));
    let claw = sim.actors[p].totems[1].unwrap();
    assert_eq!(sim.actors[near].target, Some(claw), "the mob within 8 yd turns on the totem");
    assert_eq!(sim.actors[far].target, Some(p), "the one 12 yd off doesn't");
}

/// A trap arms 2 seconds after it goes down, then catches the first enemy
/// to come within 5 yards; it needs you out of combat.
#[test]
fn traps_arm_then_catch_the_first_enemy() {
    use forever_sim::engine::{Cx, Sim, Stop};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/hunter/beast_mastery.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
    sim.place(p, 0.0);
    sim.place(e, 3.0);
    sim.actors[e].aware = false;
    let sp = w.spell_index["hunter.freezing_trap"];
    let r = sim.known(p, sp).filter(|&r| r != usize::MAX).map(|r| (sp, r));
    let mut cx = Cx::new(p, Some(p), "");
    cx.rank = r;
    cx.spell = Some(sp);
    assert!(sim.ev(w.spells[sp as usize].requires.as_ref().unwrap(), &cx) != 0.0, "out of combat: allowed");
    sim.run_effects(&w.spells[sp as usize].effects, cx);
    let trap = sim.actors[p].totems[4].expect("a trap on the ground");
    let frozen = w.aura_index["hunter.freezing_trap"];
    sim.run(Stop::Until(1.5));
    assert!(sim.actors[e].aura_index(frozen, None).is_none(), "not armed yet");
    sim.run(Stop::Until(2.5));
    assert!(sim.actors[e].aura_index(frozen, None).is_some(), "armed: it catches the mob");
    assert!(!sim.actors[trap].alive && sim.actors[p].totems[4].is_none(), "and it's used up");
    sim.actors[p].in_combat = true;
    assert_eq!(sim.ev(w.spells[sp as usize].requires.as_ref().unwrap(), &cx), 0.0, "in combat: no traps");
}

/// Duels start fair: a mirror match comes out even, two builds' scores
/// against each other add up to one, and who reacts first follows the
/// opponent's own "plays like".
#[test]
fn duels_are_fair_both_ways_round() {
    let w = world();
    let sc = &w.scenarios["duel"];
    let run = |x: &str, y: &str, opp_like: f64| -> (f64, f64) {
        let a = load_build(&w, Path::new(x), Some(40)).unwrap();
        let b = load_build(&w, Path::new(y), Some(40)).unwrap();
        let mut opts = defaults(&w, sc);
        opts[w.opt_index("opp_play_like").unwrap() as usize] = opp_like;
        let v = vec![Variant { world: 0, name: "x".into(), opts, build: a, opponent: Some(b) }];
        let r = mc::run(&[&w], "duel", &v, &McCfg { min_reps: 1500, max_reps: 1500, ..Default::default() });
        let s = mc::summarise(&r.values[0], 0.99);
        let m = |k: &str| s.iter().find(|q| q.name == k).unwrap().mean;
        (m("score"), m("first"))
    };
    let (m, first) = run("builds/warrior/arms.toml", "builds/warrior/arms.toml", 0.0);
    assert!((m - 0.5).abs() < 0.05, "a mirror match is even ({m})");
    assert!((first - 0.5).abs() < 0.05, "and each side reacts first about half the time ({first})");
    let (ab, _) = run("builds/rogue/combat_swords.toml", "builds/mage/frost.toml", 0.0);
    let (ba, _) = run("builds/mage/frost.toml", "builds/rogue/combat_swords.toml", 0.0);
    assert!((ab + ba - 1.0).abs() < 0.05, "both ways round add up to one ({ab} + {ba})");
    let (_, first) = run("builds/warrior/arms.toml", "builds/warrior/arms.toml", 2.0);
    assert!(first < 0.01, "an expert opponent always reacts first ({first})");
}

/// A caster whose target is rooted next to it steps out of its reach before
/// casting: a typical player a reaction later and less far than an expert.
#[test]
fn rooted_mobs_get_left_behind() {
    use forever_sim::engine::{Cx, Sim, Stop};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/druid/balance.toml"), Some(40)).unwrap();
    let mut seen = vec![];
    for expert in [false, true] {
        let mut opts = defaults(&w, sc);
        opts[w.opt_index("play_like").unwrap() as usize] = if expert { 1.0 } else { 0.0 };
        // a root that holds through the druid's DoTs, for the test
        opts[w.opt_index("root_break").unwrap() as usize] = 1.0;
        let mut sim = Sim::new(&w, 3, opts);
        let p = sim.spawn_player(&b, 0, 1);
        sim.precombat(p);
        sim.log = Some(vec![]);
        sim.trace = Some(vec![]);
        let e = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
        sim.place(p, 0.0);
        sim.place(e, 3.0);
        sim.actors[p].target = Some(e);
        sim.actors[e].target = Some(p);
        sim.enter_combat();
        let t0 = sim.t;
        // its DoTs are up already, so the next move is a cast
        let cx = Cx::new(p, Some(e), "");
        for dot in ["druid.moonfire", "druid.insect_swarm"] {
            sim.apply_aura(e, w.aura_index[dot], p, 1, Some(30.0), None, cx);
        }
        sim.apply_aura(e, w.aura_index["root"], p, 1, Some(20.0), None, cx);
        sim.run(Stop::Until(t0 + 4.0));
        let tr = sim.trace.take().unwrap();
        let back = tr.iter().find(|ev| ev.kind == "move" && ev.name == "backs" && ev.who_id == p).map(|ev| ev.t - t0).expect("it backs off");
        let stop = tr.iter().find(|ev| ev.kind == "move" && ev.name == "stops" && ev.who_id == p && ev.t - t0 > back).map(|ev| ev.amount).expect("and stops");
        assert!(stop > 6.0, "out of the mob's reach ({stop} yd)");
        seen.push((back, stop));
    }
    let ((tb, td), (eb, ed)) = (seen[0], seen[1]);
    assert!(tb >= 0.5 - 1e-9 && eb < tb, "a typical player backs off a reaction later ({tb} s vs {eb} s)");
    assert!(td < ed, "and less far ({td} yd vs {ed} yd)");
}

/// Roots break once the rooted unit has taken damage worth a share of its
/// health (setting `root_break`).
#[test]
fn roots_break_on_damage() {
    use forever_sim::engine::{Cx, Ev, Sim};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/druid/balance.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
    let root = w.aura_index["root"];
    sim.apply_aura(e, root, p, 1, Some(20.0), None, Cx::new(p, Some(e), ""));
    let hp = sim.actors[e].max_hp;
    let th = w.opts[w.opt_index("root_break").unwrap() as usize].default;
    sim.apply_damage_t(p, e, 0.6 * th * hp, Ev::default(), "test", 1.0);
    assert!(sim.actors[e].aura_index(root, None).is_some(), "60% of the threshold: still rooted");
    sim.apply_damage_t(p, e, 0.6 * th * hp, Ev::default(), "test", 1.0);
    assert!(sim.actors[e].aura_index(root, None).is_none(), "120%: the root breaks");
}

/// Open-world PvP: striking first is worth something, and whoever is struck
/// first acts no sooner than a reaction after the first hostile move.
#[test]
fn whoever_strikes_first_gets_a_head_start() {
    let w = world();
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), Some(40)).unwrap();
    let score = |id: &str| -> f64 {
        let sc = &w.scenarios[id];
        let v = vec![Variant { world: 0, name: "x".into(), opts: defaults(&w, sc), build: b.clone(), opponent: None }];
        let r = mc::run(&[&w], id, &v, &McCfg { min_reps: 120, max_reps: 120, ..Default::default() });
        let s = mc::summarise(&r.values[0], 0.99);
        assert_eq!(s.iter().find(|q| q.name == "engine_spins").map_or(0.0, |q| q.mean), 0.0);
        s.iter().find(|q| q.name == "score").unwrap().mean
    };
    let (first, second) = (score("strike_first"), score("struck_first"));
    assert!(first > second + 0.03, "striking first {first:.3} vs struck first {second:.3}");
    // the fight log: the mage notices the warrior's first move, then waits a reaction
    let sc = &w.scenarios["struck_first"];
    let opts = defaults(&w, sc);
    let (_, _, ev) = forever_sim::scenario::run_rep_traced(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 5, log: true });
    let ev = ev.unwrap();
    let notice = ev.iter().find(|e| e.kind == "notice" && e.who_id == 0).map(|e| e.t).expect("the mage notices");
    let acts = ev.iter().find(|e| e.who_id == 0 && e.t > notice && (e.kind == "cast" || e.kind == "move")).map(|e| e.t).expect("then acts");
    assert!(acts >= notice + 0.35 - 1e-9, "acts {acts} after noticing at {notice}");
    assert!(!ev.iter().any(|e| e.who_id == 0 && e.t < notice && e.t > 4.0 && (e.kind == "move")), "no moving before it");
}

/// The open-world opponent at `id` ("priest/shadow"), for the settings.
fn opponent_index(w: &World, sc: &ScenarioDef, id: &str) -> f64 {
    let ScenarioKind::Skirmish { opponents, .. } = &sc.kind else { panic!("not an open-world test") };
    let _ = w;
    opponents.iter().position(|o| o == id).expect("a standard opponent") as f64
}

/// Open-world PvP: whoever is jumped keeps only what they'd have up anyway
/// (Inner Fire, not a fresh Power Word: Shield), and a stealthed attacker
/// walks in and opens from Stealth.
#[test]
fn jumped_players_keep_long_buffs_and_stealthers_open_from_stealth() {
    let w = world();
    let sc = &w.scenarios["strike_first"];
    let b = load_build(&w, Path::new("builds/rogue/subtlety.toml"), Some(60)).unwrap();
    let mut opts = defaults(&w, sc);
    opts[w.opt_index("skirmish_show").unwrap() as usize] = opponent_index(&w, sc, "priest/shadow");
    let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 1, log: true });
    let log = log.unwrap();
    let open = log.iter().position(|l| ["Ambush", "Cheap Shot", "Garrote"].iter().any(|o| l.ends_with(&format!("Subtlety casts {o}")))).expect("the rogue opens");
    let at = |l: &String| -> f64 { l.split_whitespace().next().unwrap().parse().unwrap() };
    let unstealth = log.iter().position(|l| l.ends_with("Subtlety loses Stealth")).expect("and leaves Stealth");
    assert!((at(&log[unstealth]) - at(&log[open])).abs() < 1e-6, "Stealth ends with the opener, not before");
    assert!(log[..open].iter().any(|l| l.ends_with("Shadow loses Power Word: Shield")), "the jumped priest's fresh shield is gone");
    assert!(!log[..open].iter().any(|l| l.ends_with("Shadow loses Inner Fire")), "but Inner Fire stays");
}

/// Open-world PvP: someone waiting for the other side's first move notices
/// a melee player walking up (20 yards) and acts before it strikes.
#[test]
fn approaching_players_are_noticed() {
    let w = world();
    let sc = &w.scenarios["struck_first"];
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), Some(60)).unwrap();
    let mut opts = defaults(&w, sc);
    opts[w.opt_index("skirmish_show").unwrap() as usize] = opponent_index(&w, sc, "paladin/retribution");
    let (_, _, ev) = forever_sim::scenario::run_rep_traced(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 2, log: true });
    let ev = ev.unwrap();
    let notice = ev.iter().find(|e| e.kind == "notice" && e.who_id == 0).map(|e| e.t).expect("the mage notices");
    let struck = ev.iter().find(|e| e.who_id != 0 && e.target_id == Some(0) && matches!(e.kind, "hit" | "miss" | "dodge" | "parry" | "cast")).map(|e| e.t).expect("the paladin attacks");
    // (casts are logged when they land, so this checks the noticing itself)
    assert!(notice + 0.5 < struck, "the mage notices at {notice:.2}, in time to react before the first attack at {struck:.2}");
}

/// Bows and guns can't fire inside 8 yards, and only standing still.
#[test]
fn auto_shot_has_a_dead_zone() {
    use forever_sim::engine::Sim;
    use forever_sim::vars::Hand;
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/hunter/marksmanship.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
    sim.actors[p].target = Some(e);
    sim.actors[e].target = Some(p);
    sim.enter_combat();
    sim.place(p, 0.0);
    sim.place(e, 5.0);
    assert!(!sim.can_swing(p, Hand::Ranged), "no Auto Shot at 5 yards");
    sim.place(e, 20.0);
    assert!(sim.can_swing(p, Hand::Ranged), "Auto Shot at 20 yards");
    sim.place(e, 40.0);
    assert!(!sim.can_swing(p, Hand::Ranged), "nor beyond 35");
}

/// A mob that runs at low health sometimes comes back with a friend, who
/// joins 1.5 sec later and counts.
#[test]
fn fleeing_mobs_bring_friends() {
    use forever_sim::engine::{Goal, Sim, Stop};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    sim.spawned.clear();
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
    sim.actors[e].dice = [0.0, 0.5, 0.0, 0.0, 0.0, 0.0];
    sim.actors[p].target = Some(e);
    sim.actors[e].target = Some(p);
    sim.place(p, 0.0);
    sim.place(e, 4.0);
    sim.enter_combat();
    let max = sim.actors[e].max_hp;
    sim.actors[e].hp = 0.1 * max;
    sim.plan_motion();
    let t0 = sim.t;
    assert!(matches!(sim.actors[e].mo.goal, Goal::Away { .. }), "at 10% health it runs");
    sim.run(Stop::Until(t0 + 1.4));
    assert_eq!(sim.spawned.len(), 1, "nobody yet at 1.4 sec");
    sim.run(Stop::Until(t0 + 1.6));
    assert_eq!(sim.spawned.len(), 2, "a friend joins at 1.5 sec");
}

/// Hits land from behind only when the target is facing someone else, or
/// hasn't noticed anyone.
#[test]
fn behind_means_facing_someone_else() {
    use forever_sim::engine::Sim;
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/rogue/combat.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let q = sim.spawn_player(&b, 0, 2);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
    sim.actors[e].aware = false;
    assert!(sim.behind(p, e), "an unaware mob");
    sim.actors[e].aware = true;
    sim.actors[e].target = Some(p);
    assert!(!sim.behind(p, e), "a mob fighting you faces you");
    sim.actors[e].target = Some(q);
    assert!(sim.behind(p, e), "a mob fighting someone else");
}

/// A feared mob runs, mostly away from whoever feared it (turning now and
/// then), and comes back once it ends; a rooted one stays put.
#[test]
fn fear_runs_away_and_back() {
    use forever_sim::engine::{Cx, Sim, Stop};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/warlock/affliction.toml"), Some(40)).unwrap();
    let run = |seed: u64, rooted: bool| -> (f64, f64) {
        let mut sim = Sim::new(&w, seed, defaults(&w, sc));
        let p = sim.spawn_player(&b, 0, 1);
        let e = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
        sim.actors[p].target = Some(e);
        sim.actors[e].target = Some(p);
        sim.place(p, 0.0);
        sim.place(e, 4.0);
        sim.enter_combat();
        let cx = Cx::new(p, Some(e), "");
        if rooted {
            sim.apply_aura(e, w.aura_index["root"], p, 1, Some(5.0), None, cx);
        }
        sim.apply_aura(e, w.aura_index["fear"], p, 1, Some(3.0), None, cx);
        sim.plan_motion();
        sim.run(Stop::Until(2.9));
        let away = sim.dist(p, e);
        sim.run(Stop::Until(9.0));
        (away, sim.dist(p, e))
    };
    let n = 20;
    let mut total = 0.0;
    for seed in 1..=n {
        let (away, back) = run(seed, false);
        total += away;
        assert!(back < 6.0, "seed {seed}: came back ({back} yards)");
    }
    let mean = total / n as f64;
    assert!(mean > 6.0 && mean < 29.0, "it runs, but not in a straight line: {mean:.1} yards on average after 2.9 sec");
    let (away, _) = run(1, true);
    assert!((away - 4.0).abs() < 1e-6, "rooted, it stays put ({away} yards)");
}

/// Interrupts lock out the school they catch; Improved Counterspell
/// silences; Counterspell has a range; Intimidating Shout's target cowers;
/// the ambush's Kidney Shot keeps its own diminishing returns.
#[test]
fn interrupts_and_crowd_control() {
    use forever_sim::engine::{Cx, Sim};
    use forever_sim::vars::Mechanic;
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/mage/arcane.toml"), Some(60)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let foe = sim.spawn_player(&load_build(&w, Path::new("builds/mage/frost.toml"), Some(60)).unwrap(), 1, 2);
    sim.place(p, 0.0);
    sim.place(foe, 20.0);
    sim.actors[p].target = Some(foe);
    sim.actors[foe].target = Some(p);
    // the frost mage starts a Frostbolt; the arcane mage counterspells it
    let fb = sim.can_cast(foe, w.spell_index["mage.frostbolt"], SpellTarget::Enemy, false).expect("a Frostbolt");
    sim.begin(foe, fb);
    // (noticed already: a reacting player sees a cast after its reaction)
    sim.actors[foe].cast.as_mut().unwrap().seen = 0.0;
    let cs = w.spell_index["mage.counterspell"];
    sim.place(foe, 40.0);
    assert!(sim.can_cast(p, cs, SpellTarget::Enemy, false).is_none(), "out of Counterspell's 30 yards");
    sim.place(foe, 20.0);
    let plan = sim.can_cast(p, cs, SpellTarget::Enemy, false).expect("Counterspell in range");
    // keep rolling until it lands (it can be resisted)
    sim.begin(p, plan);
    assert!(sim.actors[foe].cast.is_none(), "the Counterspell lands (this seed)");
    {
        assert!(sim.actors[foe].lockout[forever_sim::vars::School::Frost as usize] > sim.t, "frost is locked out");
        assert!(sim.actors[foe].lockout[forever_sim::vars::School::Fire as usize] <= sim.t, "fire isn't");
        let ic = w.classes.iter().find(|c| c.id == "mage").unwrap().talent_index["improved_counterspell"] as usize;
        if sim.actors[p].talents.get(ic).copied().unwrap_or(0) > 0 {
            assert!(sim.actors[foe].has_mech(&w, Mechanic::Silence), "Improved Counterspell silences");
        }
    }
    // Intimidating Shout: the target cowers (stunned, not running)
    let wb = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(60)).unwrap();
    let mut sim = Sim::new(&w, 3, defaults(&w, sc));
    let p = sim.spawn_player(&wb, 0, 1);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 58, 1, 1000, 1.0);
    let e2 = sim.spawn_enemy(&w.enemies["humanoid"], 58, 1, 1001, 1.0);
    for (x, d) in [(e, 3.0), (e2, 5.0)] {
        sim.place(x, d);
        sim.actors[x].target = Some(p);
    }
    sim.actors[p].target = Some(e);
    sim.enter_combat();
    sim.actors[p].res[forever_sim::vars::Res::Rage as usize] = 100.0;
    let plan = sim.can_cast(p, w.spell_index["warrior.intimidating_shout"], SpellTarget::Enemy, false).expect("a shout");
    sim.begin(p, plan);
    assert!(sim.actors[e].has_mech(&w, Mechanic::Stun), "the shout lands (this seed)");
    {
        assert!(!sim.actors[e].has_mech(&w, Mechanic::Fear), "the target cowers, it doesn't run");
    }
    assert!(!sim.actors[e2].has_mech(&w, Mechanic::Stun), "others don't cower");
    // the ambush's Kidney Shot isn't shortened by its Cheap Shot
    let mut sim = Sim::new(&w, 3, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let r = sim.spawn_enemy(&w.enemies["pressure_rogue"], 60, 1, 1000, 1.0);
    let cx = Cx::new(r, Some(p), "");
    sim.apply_aura(p, w.aura_index["stun"], r, 1, Some(4.0), None, cx);
    sim.remove_aura_at(p, sim.actors[p].auras.iter().position(|i| i.def == w.aura_index["stun"]).unwrap(), true);
    sim.apply_aura(p, w.aura_index["rogue.kidney_shot"], r, 1, Some(6.0), None, cx);
    let k = sim.actors[p].auras.iter().find(|i| i.def == w.aura_index["rogue.kidney_shot"]).expect("Kidney Shot lands");
    assert!((k.expires - sim.t - 6.0).abs() < 1e-9, "for its full 6 sec ({})", k.expires - sim.t);
}

/// A mob turns on whoever passes 110% of its target's threat from melee
/// range, or 130% from farther away; a taunt pulls it at once.
#[test]
fn aggro_turns_at_110_and_130_percent() {
    use forever_sim::engine::{Cx, Sim};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/warrior/protection.toml"), Some(40)).unwrap();
    let setup = |far: bool| {
        let mut sim = Sim::new(&w, 1, defaults(&w, sc));
        let a = sim.spawn_player(&b, 0, 1);
        let c = sim.spawn_player(&b, 0, 2);
        let e = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000, 1.0);
        sim.place(e, 0.0);
        sim.place(a, -4.0);
        sim.place(c, if far { -20.0 } else { 4.0 });
        for x in [a, c] {
            sim.actors[x].target = Some(e);
        }
        sim.enter_combat();
        sim.add_threat(e, a, 1000.0);
        sim.actors[e].target = Some(a);
        sim.retarget(e);
        (sim, a, c, e)
    };
    for (far, need) in [(false, 1.1), (true, 1.3)] {
        let (mut sim, a, c, e) = setup(far);
        sim.add_threat(e, c, 1000.0 * need - 5.0);
        sim.retarget(e);
        assert_eq!(sim.actors[e].target, Some(a), "just under {need}x: it stays");
        sim.add_threat(e, c, 10.0);
        sim.retarget(e);
        assert_eq!(sim.actors[e].target, Some(c), "just over {need}x: it turns");
    }
    let (mut sim, a, c, e) = setup(true);
    let cx = Cx::new(c, Some(e), "");
    sim.run_effects(&w.spells[w.spell_index["warrior.taunt"] as usize].effects, cx);
    sim.retarget(e);
    assert_eq!(sim.actors[e].target, Some(c), "a taunt pulls it");
    let top = sim.actors[e].threat.iter().find(|x| x.0 as usize == a).unwrap().1;
    let mine = sim.actors[e].threat.iter().find(|x| x.0 as usize == c).unwrap().1;
    assert!((mine - top).abs() < 1e-9, "the taunter's threat matches the top ({mine} vs {top})");
}

/// In the boss test the Protection warrior holds the boss for the standard
/// damage builds (typical play: a 2 sec head start and holding back near
/// the tank's threat); party buffs land.
#[test]
fn the_boss_stays_on_the_tank() {
    let w = world();
    let sc = &w.scenarios["boss"];
    for (f, level) in [("warrior/arms", 60), ("rogue/combat", 60), ("mage/frost", 60), ("hunter/beast_mastery", 60), ("warlock/affliction", 60), ("warrior/arms", 20), ("rogue/combat", 20)] {
        let b = load_build(&w, Path::new(&format!("builds/{f}.toml")), Some(level)).unwrap();
        let opts = defaults(&w, sc);
        let (mut held, mut died) = (0.0, 0.0);
        for seed in 1..5u64 {
            let (v, _) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed, log: false });
            held += v["held_share"] / 4.0;
            died += v["died"] / 4.0;
        }
        assert!(held > 0.9, "{f} at {level}: the tank held the boss {:.0}% of the time", held * 100.0);
        assert!(died < 0.2, "{f} at {level}: died in {:.0}% of fights", died * 100.0);
    }
}

/// The boss test's openers wait out the tank's head start instead of being
/// skipped (a hunter's Mark goes up), and the tank's Charge happens.
#[test]
fn boss_openers_wait_for_the_head_start() {
    let w = world();
    let sc = &w.scenarios["boss"];
    let b = load_build(&w, Path::new("builds/hunter/beast_mastery.toml"), Some(60)).unwrap();
    let opts = defaults(&w, sc);
    let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 1, log: true });
    let log = log.unwrap();
    assert!(log.iter().any(|l| l.ends_with("Beast Mastery casts Hunter's Mark")), "Hunter's Mark goes up");
    // at 20, without Vanguard, the tank charges from Battle Stance and goes back
    let b20 = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(20)).unwrap();
    let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b20, opponent: None, opts: &opts, seed: 1, log: true });
    let log = log.unwrap();
    let charge = log.iter().position(|l| l.ends_with("Protection casts Charge")).expect("the tank charges in");
    assert!(log[charge..].iter().any(|l| l.ends_with("Protection casts Defensive Stance")), "and goes back to Defensive Stance after its Charge");
}

/// Boss DPS is over the whole fight: dying early counts as no damage from
/// then on (`dps_alive` is the rate while alive).
#[test]
fn boss_dps_counts_the_whole_fight() {
    let w = world();
    let sc = &w.scenarios["boss"];
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), Some(60)).unwrap();
    let mut opts = defaults(&w, sc);
    // a boss hitting a hundred times harder kills whoever it turns on
    if let Some(i) = w.opt_index("threat_margin") {
        opts[i as usize] = 0.0;
    }
    let (v, _) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 1, log: false });
    assert!(v["dps"] <= v["dps_alive"] + 1e-9);
    if v["died"] > 0.0 {
        assert!(v["dps"] < v["dps_alive"]);
    }
}

/// The party's Battle Shout at level 25 is the client's rank: 33.9 attack power
/// (the tooltip says 34).
#[test]
fn party_battle_shout_is_the_clients_rank() {
    use forever_sim::engine::Sim;
    use forever_sim::vars::Stat;
    let w = world();
    let sc = &w.scenarios["boss"];
    let b = load_build(&w, Path::new("builds/rogue/combat.toml"), Some(25)).unwrap();
    let mut opts = defaults(&w, sc);
    // every buff, then all but Battle Shout turned off
    opts[w.opt_index("party").unwrap() as usize] = 2.0;
    for o in &w.party {
        if let Some(i) = o.option {
            opts[i as usize] = if o.id == "battle_shout" { 1.0 } else { 0.0 };
        }
    }
    let mut sim = Sim::new(&w, 1, opts.clone());
    let p = sim.spawn_player(&b, 0, 1);
    let before = sim.actors[p].st[Stat::Ap as usize];
    sim.party_buffs(p);
    let gained = sim.actors[p].st[Stat::Ap as usize] - before;
    assert!((gained - 34.0).abs() < 0.5, "Battle Shout gives {gained} (the tooltip rounds to 34)");
    let k = w.party.iter().find(|o| o.id == "battle_shout").unwrap().option.unwrap();
    opts[k as usize] = 0.0;
    let mut sim = Sim::new(&w, 1, opts);
    let p = sim.spawn_player(&b, 0, 1);
    sim.party_buffs(p);
    assert_eq!(sim.actors[p].st[Stat::Ap as usize], before, "turned off, nothing");
}

/// The healing tests' tank keeps the boss: the healer never pulls it.
#[test]
fn healers_never_pull_the_boss() {
    let w = world();
    for h in ["priest/holy", "shaman/restoration"] {
        for test in ["heal_dungeon", "heal_tank"] {
            let out = trace(h, test, 60);
            let me = out["me"].as_str().unwrap().to_string();
            let pulled = out["events"].as_array().unwrap().iter().any(|e| e["kind"] == "aggro" && e["target"] == me.as_str());
            assert!(!pulled, "{h} pulled the boss in {test}");
        }
    }
    let _ = w;
}

/// The ambushing attacker sticks to you, whatever the pet does.
#[test]
fn the_ambusher_ignores_pet_threat() {
    let w = world();
    let sc = &w.scenarios["pressure"];
    let b = load_build(&w, Path::new("builds/hunter/beast_mastery.toml"), Some(40)).unwrap();
    let opts = defaults(&w, sc);
    let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 3, log: true });
    assert!(!log.unwrap().iter().any(|l| l.contains(" turns on ")), "the attacker never switches to the pet");
}

/// The leveling mix: each pull's kind comes from the world's own draws, so
/// every build meets the same pulls; over many pulls the kinds match the
/// weights, and about one humanoid in six casts.
#[test]
fn the_mix_is_shared_and_matches_its_weights() {
    let w = world();
    let sc = &w.scenarios["solo_3"];
    let opts = defaults(&w, sc);
    let pulls = |build: &str, seed: u64| -> Vec<String> {
        let b = load_build(&w, Path::new(&format!("builds/{build}.toml")), Some(40)).unwrap();
        let (_, _, ev) = forever_sim::scenario::run_rep_traced(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed, log: true });
        ev.unwrap().iter().filter(|e| e.kind == "pull").map(|e| e.name.clone()).collect()
    };
    let a = pulls("warrior/arms", 9);
    let b = pulls("mage/frost", 9);
    let n = a.len().min(b.len());
    assert!(n >= 20 && a[..n] == b[..n], "the same pulls for every build");
    // kinds by weight, over many pulls (chi-square, 6 degrees of freedom)
    let mix = &w.mixes["overworld"];
    let mut seen = std::collections::BTreeMap::<String, f64>::new();
    let mut total = 0.0;
    for seed in 0..40 {
        for k in pulls("hunter/beast_mastery", seed) {
            *seen.entry(k).or_insert(0.0) += 1.0;
            total += 1.0;
        }
    }
    let chi: f64 = mix.kinds.iter().map(|(k, wt)| {
        let e = wt * total;
        let o = seen.get(k).copied().unwrap_or(0.0);
        (o - e).powi(2) / e
    }).sum();
    assert!(chi < 22.5, "kinds follow the weights (chi-square {chi:.1} over {total} pulls)");
}

/// About one humanoid in six is a caster, which keeps to 30 yards while it
/// can cast, then comes in.
#[test]
fn caster_mobs_hold_range_then_close_in() {
    use forever_sim::engine::{Sim, Stop};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(40)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let e = sim.spawn_enemy(&w.enemies["humanoid_caster"], 40, 1, 1000, 1.0);
    sim.actors[p].target = Some(e);
    sim.actors[e].target = Some(p);
    sim.place(p, 0.0);
    sim.place(e, 28.0);
    sim.actors[p].auto_melee = false;
    sim.actors[p].rotation = None;
    sim.enter_combat();
    sim.run(Stop::Until(5.0));
    assert!(sim.dist(p, e) > 25.0, "it casts from range ({:.0} yards)", sim.dist(p, e));
    assert!(sim.metrics[e].dmg > 0.0, "and its bolts land");
    sim.run(Stop::Until(40.0));
    assert!(sim.dist(p, e) <= 5.0 + 1e-6, "out of mana, it comes to melee ({:.0} yards)", sim.dist(p, e));
}

/// A troll's Beast Slaying counts against beasts only, whatever the stat
/// sheet showed last.
#[test]
fn creature_bonuses_follow_the_target() {
    use forever_sim::engine::Sim;
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/rogue/subtlety.toml"), Some(40)).unwrap();
    assert_eq!(w.races[b.race].id, "troll", "a troll");
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let beast = sim.spawn_enemy(&w.enemies["beast"], 40, 1, 1000, 1.0);
    let man = sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1001, 1.0);
    assert!((sim.vs_mult(p, beast) - 1.05).abs() < 1e-9, "5% more against a beast");
    assert!((sim.vs_mult(p, man) - 1.0).abs() < 1e-9, "nothing against a humanoid");
}

/// A line on another enemy never lands on your target, and doesn't change
/// it; Chain Lightning jumps to nearby enemies, 30% less each jump.
#[test]
fn other_enemy_lines_and_chains() {
    use forever_sim::engine::act::Plan;
    use forever_sim::engine::Sim;
    let w = world();
    let sc = &w.scenarios["solo_3"];
    // DoTs spread: every Corruption on a second mob while the first is the target
    let b = load_build(&w, Path::new("builds/warlock/affliction.toml"), Some(40)).unwrap();
    let opts = defaults(&w, sc);
    let (_, _, ev) = forever_sim::scenario::run_rep_traced(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 4, log: true });
    let ev = ev.unwrap();
    let me = ev.iter().find(|e| e.kind == "pull").map(|e| e.who.clone()).unwrap();
    let spread = ev.iter().filter(|e| e.kind == "cast" && e.who == me && e.name == "Corruption").count();
    assert!(spread >= 10, "Corruption goes out ({spread})");
    let mut sim = Sim::new(&w, 1, opts.clone());
    let p = sim.spawn_player(&b, 0, 1);
    let rot = sim.actors[p].rotation.unwrap();
    let line = rot.actions.iter().find(|a| matches!(a.target, SpellTarget::OtherEnemy(_)) && w.spells[a.spell as usize].id == "corruption").expect("an other-enemy Corruption line");
    let mobs: Vec<usize> = (0..3).map(|j| sim.spawn_enemy(&w.enemies["humanoid"], 40, 1, 1000 + j, 1.0)).collect();
    for (j, &m) in mobs.iter().enumerate() {
        sim.place(m, 5.0 + j as f64);
        sim.actors[m].target = Some(p);
    }
    sim.actors[p].target = Some(mobs[0]);
    sim.enter_combat();
    sim.actors[p].gcd_end = 0.0;
    let plan: Plan = sim.line_plan(p, line, false, &mut None).expect("it applies with three mobs");
    assert_ne!(plan.target, mobs[0], "never on your target");
    assert_eq!(sim.actors[p].target, Some(mobs[0]), "and your target stays");
    // Chain Lightning: three hits, each 0.7 of the one before
    let b = load_build(&w, Path::new("builds/shaman/elemental.toml"), Some(40)).unwrap();
    let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed: 4, log: true });
    let log = log.unwrap();
    let mut jumps = 0;
    for (k, l) in log.iter().enumerate() {
        if l.ends_with(" casts Chain Lightning") {
            let hits: Vec<&String> = log[k + 1..].iter().take_while(|x| x.contains(" Chain Lightning -> ")).collect();
            assert!(hits.len() <= 3, "at most three hits");
            jumps += hits.len().saturating_sub(1);
        }
    }
    assert!(jumps > 0, "it jumps when there's more than one mob");
}

/// Every spell carries the game's icon (the website's fight log shows it),
/// and a traced fight of every standard build has an icon on each move,
/// leaves bookkeeping auras out, and tells mobs apart.
#[test]
fn fight_logs_show_the_games_icons() {
    let w = world();
    let missing: Vec<&str> = w.spells.iter().filter(|s| s.icon.is_none()).map(|s| s.qid.as_str()).collect();
    assert!(missing.is_empty(), "spells without an icon: {missing:?}");
    let hidden: Vec<String> = w.auras.iter().filter(|a| a.hidden).map(|a| a.name.clone()).collect();
    let mut checked = 0;
    for path in builds() {
        let text = std::fs::read_to_string(&path).unwrap();
        if !text.contains("kind = \"standard\"") {
            continue;
        }
        let build: toml::Value = toml::from_str(&text).unwrap();
        let build = serde_json::to_value(&build).unwrap();
        for (scenario, level) in [("solo_2", 30), ("boss", 60)] {
            let req = serde_json::json!({ "op": "trace", "scenario": scenario, "level": level, "build": build });
            let out: serde_json::Value = serde_json::from_str(&forever_sim::api::handle(&req.to_string())).unwrap();
            let out = out.get("result").unwrap_or(&out);
            let events = out["events"].as_array().unwrap_or_else(|| panic!("{}: {out}", path.display()));
            for e in events {
                let kind = e["kind"].as_str().unwrap();
                let name = e["name"].as_str().unwrap();
                let ours = e["team"].as_u64() == Some(0);
                if ours && matches!(kind, "cast" | "hit" | "heal" | "gain" | "lose") {
                    assert!(!e["icon"].as_str().unwrap_or("").is_empty(), "{} {scenario}: {kind} {name} has no icon", path.display());
                }
                if matches!(kind, "gain" | "lose") {
                    assert!(!hidden.iter().any(|h| h == name), "{} {scenario}: bookkeeping aura {name} in the log", path.display());
                }
                if scenario == "solo_2" && !ours {
                    let who = e["who"].as_str().unwrap();
                    assert!(who == "Mob 1" || who == "Mob 2", "{} {scenario}: enemy named {who}", path.display());
                }
                checked += 1;
            }
        }
    }
    assert!(checked > 1000, "only {checked} events checked");
}

/// A hit soaked up by a shield says how much was absorbed and by what.
#[test]
fn absorbed_hits_report_the_shield() {
    let text = std::fs::read_to_string("builds/mage/frost.toml").unwrap();
    let build = serde_json::to_value(toml::from_str::<toml::Value>(&text).unwrap()).unwrap();
    let req = serde_json::json!({ "op": "trace", "scenario": "solo_2_hard", "level": 60, "build": build, "until": 60 });
    let out: serde_json::Value = serde_json::from_str(&forever_sim::api::handle(&req.to_string())).unwrap();
    let out = out.get("result").unwrap_or(&out);
    let soaked: Vec<&serde_json::Value> = out["events"].as_array().unwrap().iter()
        .filter(|e| e["kind"] == "hit" && e["absorbed"].as_f64().unwrap_or(0.0) > 0.0).collect();
    assert!(!soaked.is_empty(), "Ice Barrier should soak up some hits");
    assert!(soaked.iter().all(|e| e["via"] == "Ice Barrier" || e["via"] == "Mana Shield" || !e["via"].as_str().unwrap_or("").is_empty()), "absorbs name their shield");
}

/// When a pet holds the mob (a hunter's cat), the fight log keeps what the
/// mob does to it, so the enemies' side of the log isn't empty.
#[test]
fn fight_logs_keep_hits_on_your_pet() {
    let text = std::fs::read_to_string("builds/hunter/beast_mastery.toml").unwrap();
    let build = serde_json::to_value(toml::from_str::<toml::Value>(&text).unwrap()).unwrap();
    let req = serde_json::json!({ "op": "trace", "scenario": "solo_1", "level": 30, "build": build });
    let out: serde_json::Value = serde_json::from_str(&forever_sim::api::handle(&req.to_string())).unwrap();
    let out = out.get("result").unwrap_or(&out);
    let pets: Vec<&str> = out["pets"].as_array().unwrap().iter().filter_map(|p| p.as_str()).collect();
    assert!(!pets.is_empty(), "the hunter's pet is listed");
    let on_pet = out["events"].as_array().unwrap().iter()
        .filter(|e| e["team"] != 0 && e["kind"] == "hit" && pets.contains(&e["target"].as_str().unwrap_or(""))).count();
    assert!(on_pet > 0, "the mob's hits on the pet are in the log");
}

/// Spell pushback, by the `pushback` setting: the original game's (1.0, 0.8,
/// 0.6, 0.4 then 0.2 sec per hit, a cast never pushed past its full length;
/// a channel shortened by the same) or the later rules (0.5 sec for the
/// first two hits only; a quarter of a channel per hit).
#[test]
fn pushback_by_the_rules() {
    use forever_sim::engine::{Ev, Sim};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), Some(60)).unwrap();
    // how far each of six hits moves the end of a cast that has `left` to go
    let pushes = |later: bool, spell: &str, left: f64| -> (f64, Vec<f64>) {
        let mut opts = defaults(&w, sc);
        opts[w.opt_index("pushback").unwrap() as usize] = if later { 1.0 } else { 0.0 };
        let mut sim = Sim::new(&w, 1, opts);
        let p = sim.spawn_player(&b, 0, 1);
        let e = sim.spawn_enemy(&w.enemies["humanoid"], 60, 1, 1000, 1.0);
        sim.place(e, 20.0);
        sim.actors[p].target = Some(e);
        let plan = sim.can_cast(p, w.spell_index[spell], SpellTarget::Enemy, false).expect("it can cast");
        sim.begin(p, plan);
        let c = sim.actors[p].cast.as_mut().expect("casting");
        c.end = left;
        let full = c.full;
        let mut ends = vec![left];
        for _ in 0..6 {
            sim.apply_damage(e, p, 10.0, Ev::default(), "hit");
            ends.push(sim.actors[p].cast.as_ref().unwrap().end);
        }
        (full, ends.windows(2).map(|x| ((x[1] - x[0]) * 100.0).round() / 100.0).collect())
    };
    let (full, cast) = pushes(false, "mage.frostbolt", 0.5);
    let cap = ((full - 0.5 - 1.8) * 100.0).round() / 100.0;
    assert_eq!(cast, vec![1.0, 0.8, cap, 0.0, 0.0, 0.0], "a {full} sec Frostbolt with half a second left");
    assert_eq!(pushes(true, "mage.frostbolt", 0.5).1, vec![0.5, 0.5, 0.0, 0.0, 0.0, 0.0]);
    assert_eq!(pushes(false, "mage.arcane_missiles", 4.0).1, vec![-1.0, -0.8, -0.6, -0.4, -0.2, -0.2]);
    let (am, later) = pushes(true, "mage.arcane_missiles", 4.0);
    let q = -((am * 0.25) * 100.0).round() / 100.0;
    assert_eq!(later, vec![q, q, 0.0, 0.0, 0.0, 0.0]);
}

/// A cast finishing with its target out of range fails and costs nothing;
/// finishing a cast restarts the melee swing; a freeze doesn't stop a cast.
#[test]
fn casting_rules() {
    use forever_sim::engine::{Cx, Sim};
    use forever_sim::vars::Res;
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), Some(60)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 60, 1, 1000, 1.0);
    sim.place(e, 20.0);
    sim.actors[p].target = Some(e);
    let plan = sim.can_cast(p, w.spell_index["mage.frostbolt"], SpellTarget::Enemy, false).expect("it can cast");
    sim.begin(p, plan);
    // frozen mid-cast: the cast goes on
    let cx = Cx::new(e, Some(p), "");
    sim.apply_aura(p, w.aura_index["frozen"], e, 1, Some(5.0), None, cx);
    assert!(sim.actors[p].cast.is_some(), "a freeze doesn't stop the cast");
    // the target walks off: the cast fails, and costs no mana
    sim.place(e, 60.0);
    let mana = sim.actors[p].res[Res::Mana as usize];
    sim.finish_cast(p);
    assert_eq!(sim.actors[p].res[Res::Mana as usize], mana, "no mana spent");
    assert!(sim.metrics[p].casts.get("Frostbolt").is_none(), "and no Frostbolt cast");
    // a warrior's Slam restarts its swing
    let wb = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(60)).unwrap();
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&wb, 0, 1);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 60, 1, 1000, 1.0);
    sim.place(e, 3.0);
    sim.actors[p].target = Some(e);
    sim.actors[p].res[Res::Rage as usize] = 100.0;
    sim.actors[p].swing[0] = 0.0;
    let plan = sim.can_cast(p, w.spell_index["warrior.slam"], SpellTarget::Enemy, false).expect("it can Slam");
    sim.begin(p, plan);
    sim.finish_cast(p);
    assert!(sim.actors[p].swing[0] > sim.t + 1.0, "the swing starts over after the Slam ({})", sim.actors[p].swing[0]);
}

/// Debuffs roll to land: a Polymorph on a target three levels up sticks as
/// often as the spell hit table says (83% before talents and gear), once
/// per cast and target; a Charge's stun always lands.
#[test]
fn debuffs_roll_to_land() {
    use forever_sim::engine::{Cx, Sim};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), Some(60)).unwrap();
    let mut sim = Sim::new(&w, 7, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 63, 1, 1000, 1.0);
    let mut cx = Cx::new(p, Some(e), "");
    cx.spell = Some(w.spell_index["mage.polymorph"]);
    let n = 4000;
    let mut landed = 0;
    for i in 0..n {
        sim.t = i as f64;
        let first = sim.debuff_lands(&cx, e, false);
        assert_eq!(sim.debuff_lands(&cx, e, false), first, "one roll per cast and target");
        landed += first as usize;
    }
    let rate = landed as f64 / n as f64;
    let want = sim.spell_hit_chance(p, e) / 100.0;
    assert!((rate - want).abs() < 0.02, "lands {rate:.3} of the time, the hit table says {want:.3}");
    assert!(want < 0.99, "a +3 target resists some ({want})");
    let mut charge = cx;
    charge.landed = true;
    assert!((0..200).all(|i| { sim.t = (n + i) as f64; sim.debuff_lands(&charge, e, false) }), "a Charge's stun always lands");
}

/// Attack tables: glancing blows by weapon skill (95%, 95%, 85%, 65% at 0 to
/// 3 levels up), a stunned target can't dodge or parry, and Overpower is
/// never dodged, parried or blocked.
#[test]
fn attack_tables() {
    use forever_sim::engine::{combat::Roll, Cx, Sim};
    use forever_sim::vars::Hand;
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let b = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(60)).unwrap();
    let mut sim = Sim::new(&w, 3, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    for (up, want) in [(0, 0.95), (1, 0.95), (2, 0.85), (3, 0.65)] {
        let e = sim.spawn_enemy(&w.enemies["humanoid"], 60 + up, 1, 1000 + up as u64, 1.0);
        let g = sim.glance_mult(p, e);
        assert!((g - want).abs() < 1e-9, "{up} levels up: glancing blows do {g}");
    }
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 62, 1, 2000, 1.0);
    let avoids = |sim: &mut Sim, unavoidable: bool| (0..3000).filter(|_| matches!(sim.roll_melee_x(p, e, Hand::Mh, false, 0.0, unavoidable), Roll::Dodge | Roll::Parry | Roll::Block)).count();
    assert!(avoids(&mut sim, false) > 100, "a mob dodges and parries");
    assert_eq!(avoids(&mut sim, true), 0, "but never an Overpower");
    let cx = Cx::new(p, Some(e), "");
    sim.apply_aura(e, w.aura_index["stun"], p, 1, Some(100.0), None, cx);
    assert_eq!(avoids(&mut sim, false), 0, "nor anything while stunned");
}

/// The boss test's party is a typical group by default: the tank's Battle
/// Shout, a priest's Fortitude, and a paladin's one blessing for the
/// Alliance or a shaman's air and earth totems for the Horde; pets get the
/// party's buffs too.
#[test]
fn a_typical_group_by_side() {
    let w = world();
    let sc = &w.scenarios["boss"];
    let opts = defaults(&w, sc);
    let gains = |b: &forever_sim::build::Build, who: &str| -> Vec<String> {
        let (_, log) = run_rep(&RepInput { w: &w, sc, build: b, opponent: None, opts: &opts, seed: 1, log: true });
        let pre = format!(" {who} gains ");
        log.unwrap().iter().filter_map(|l| l.split_once(&pre).map(|(_, a)| a.to_string())).collect()
    };
    let mut human = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(60)).unwrap();
    human.race = w.races.iter().position(|r| r.id == "human").unwrap();
    let a = gains(&human, "Arms");
    for want in ["Battle Shout", "Power Word: Fortitude", "Blessing of Might"] {
        assert!(a.iter().any(|g| g == want), "the human Arms gets {want}: {a:?}");
    }
    assert!(!a.iter().any(|g| g.contains("Windfury") || g == "Blessing of Kings"), "one blessing, no totems: {a:?}");
    let mut orc = human.clone();
    orc.race = w.races.iter().position(|r| r.id == "orc").unwrap();
    let h = gains(&orc, "Arms");
    assert!(h.iter().any(|g| g.contains("Windfury")) && h.iter().any(|g| g.contains("Strength of Earth")), "the orc gets a shaman's totems: {h:?}");
    assert!(!h.iter().any(|g| g.starts_with("Blessing")), "and no blessings: {h:?}");
    let bm = load_build(&w, Path::new("builds/hunter/beast_mastery.toml"), Some(60)).unwrap();
    let pet = gains(&bm, "Cat");
    assert!(pet.iter().any(|g| g == "Power Word: Fortitude"), "the pet gets the party's buffs: {pet:?}");
}

/// Potions: by default only in group fights (the boss and healing tests);
/// never with "Never", and in leveling too with "Always".
#[test]
fn potions_by_setting() {
    let w = world();
    let b = load_build(&w, Path::new("builds/mage/arcane.toml"), None).unwrap();
    let k = w.opts.iter().position(|o| o.name == "potions").expect("a potions option");
    let drinks = |test: &str, setting: Option<f64>| -> usize {
        let sc = &w.scenarios[test];
        let mut opts = defaults(&w, sc);
        if let Some(x) = setting {
            opts[k] = x;
        }
        (0..6u64).map(|seed| {
            let (_, log) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed, log: true });
            log.unwrap().iter().filter(|l| l.ends_with(" casts Healing Potion") || l.ends_with(" casts Mana Potion")).count()
        }).sum()
    };
    assert_eq!(w.opts[k].default, 1.0, "in group fights by default");
    assert_eq!(drinks("solo_2_hard", None), 0, "no potions while leveling by default");
    assert!(drinks("boss", None) > 0, "but an Arcane mage drinks on the boss");
    assert_eq!(drinks("boss", Some(0.0)), 0, "never with Never");
    assert!(drinks("solo_2_hard", Some(2.0)) > 0, "and on tough pulls with Always");
}

const HEALERS: [&str; 5] = ["priest/holy", "priest/discipline", "paladin/holy", "druid/restoration", "shaman/restoration"];

fn trace(build: &str, scenario: &str, level: u32) -> serde_json::Value {
    let text = std::fs::read_to_string(format!("builds/{build}.toml")).unwrap();
    let build = serde_json::to_value(toml::from_str::<toml::Value>(&text).unwrap()).unwrap();
    let req = serde_json::json!({ "op": "trace", "scenario": scenario, "level": level, "build": build, "until": 200 });
    let out: serde_json::Value = serde_json::from_str(&forever_sim::api::handle(&req.to_string())).unwrap();
    out.get("result").cloned().unwrap_or(out)
}

/// Every healer preset heals through every healing test it can take at 20,
/// 40 and 60: the breakeven is found inside the search, never at its edges,
/// and the healer heals.
#[test]
fn healers_hold_the_healing_tests_at_20_40_60() {
    let w = world();
    for file in HEALERS {
        for lvl in [20u32, 40, 60] {
            let b = load_build(&w, Path::new(&format!("builds/{file}.toml")), Some(lvl)).unwrap();
            assert!(b.heal_rotation.is_some(), "{file}: no healing play style");
            for id in ["heal_dungeon", "heal_tank", "heal_raid"] {
                let sc = &w.scenarios[id];
                if forever_sim::scenario::unfit(sc, &w, &b).is_some() {
                    assert!(id == "heal_raid" && lvl < 60, "{file} {lvl}: can't take {id}");
                    continue;
                }
                let ScenarioKind::Healing(h) = &sc.kind else { unreachable!() };
                let opts = defaults(&w, sc);
                let mut scales: Vec<f64> = (0..8u64).map(|seed| {
                    let (v, _) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &opts, seed, log: false });
                    assert!(v["hps"] > 0.0, "{file} {lvl} {id}: no healing");
                    assert!(v["heal_breakeven"].is_finite() && v["heal_breakeven"] > 0.0, "{file} {lvl} {id}: breakeven {}", v["heal_breakeven"]);
                    v["heal_breakeven_scale"]
                }).collect();
                scales.sort_by(f64::total_cmp);
                let med = scales[4];
                assert!(med > 1.3 * h.lo && med < h.hi / 1.3, "{file} {lvl} {id}: breakeven scale {med} at the edge of {}..{}", h.lo, h.hi);
            }
        }
    }
}

/// A healer's fight log: heals land on the tank and the others, the
/// boss's attacks are labelled and have icons, and buffs on party members
/// are in it.
#[test]
fn heals_land_on_the_party() {
    let out = trace("priest/holy", "heal_dungeon", 60);
    let me = out["me"].as_str().unwrap();
    let party: Vec<&str> = out["party"].as_array().unwrap().iter().filter_map(|p| p.as_str()).collect();
    assert_eq!(out["tank"], "Tank");
    assert_eq!(party.len(), 4, "a tank and three others: {party:?}");
    let events = out["events"].as_array().unwrap();
    let healed: std::collections::BTreeSet<&str> = events.iter().filter(|e| e["kind"] == "heal" && e["who"] == me).filter_map(|e| e["target"].as_str()).collect();
    assert!(healed.contains("Tank"), "the tank is healed");
    assert!(healed.iter().filter(|t| **t != "Tank" && **t != me).count() >= 2, "others are healed too: {healed:?}");
    for e in events.iter().filter(|e| e["team"] != 0 && e["kind"] == "hit") {
        assert_eq!(e["who"], "Boss");
        if e["name"] != "melee" {
            assert!(!e["icon"].as_str().unwrap_or("").is_empty(), "{} has no icon", e["name"]);
        }
    }
    assert!(events.iter().any(|e| e["kind"] == "gain" && e["who"] == "Tank" && e["name"] == "Power Word: Shield"), "shields go on the tank");
}

/// Chain Heal jumps to the most hurt, each jump healing half as much.
#[test]
fn chain_heal_jumps_and_falls_off() {
    let out = trace("shaman/restoration", "heal_raid", 60);
    let events = out["events"].as_array().unwrap();
    let mut by_time: std::collections::BTreeMap<String, Vec<f64>> = Default::default();
    for e in events.iter().filter(|e| e["kind"] == "heal" && e["name"] == "Chain Heal") {
        let full = e["amount"].as_f64().unwrap() + e["over"].as_f64().unwrap();
        by_time.entry(format!("{:.3}", e["t"].as_f64().unwrap())).or_default().push(full);
    }
    let jumps: Vec<&Vec<f64>> = by_time.values().filter(|v| v.len() > 1).collect();
    assert!(!jumps.is_empty(), "Chain Heal never jumped");
    let mut ratios = Vec::new();
    for v in jumps {
        assert!(v.len() <= 3, "at most three targets: {v:?}");
        for k in 1..v.len() {
            // half the one before, give or take a critical heal and each
            // target's own roll within the spell's range (about 7% each way)
            assert!(v[k] <= 0.5 * 1.5 * 1.16 * v[k - 1], "jump {k} heals {} after {}", v[k], v[k - 1]);
            ratios.push(v[k] / v[k - 1]);
        }
    }
    ratios.sort_by(f64::total_cmp);
    let med = ratios[ratios.len() / 2];
    assert!((0.4..0.62).contains(&med), "a typical jump heals {med} of the one before");
}

/// `rank = "fit"` casts the smallest rank that covers what's missing.
#[test]
fn fitted_heals_downrank_for_small_gaps() {
    let w = world();
    let mut b = load_build(&w, Path::new("builds/shaman/restoration.toml"), Some(60)).unwrap();
    b.rotation = b.heal_rotation.clone().unwrap();
    let sc = &w.scenarios["heal_dungeon"];
    let mut sim = forever_sim::engine::Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&b, 0, 1);
    let tank = sim.spawn_ally(&w.allies["tank"], 60, 0, 0, 100);
    sim.tank = Some(tank);
    let mage = sim.spawn_ally(&w.allies["mage"], 60, 0, 0, 101);
    let rot = sim.actors[p].rotation.unwrap();
    let line = rot.actions.iter().find(|a| w.spells[a.spell as usize].id == "healing_wave" && a.rank == RankPick::Fit && a.target == SpellTarget::Lowest).unwrap();
    let top = sim.known(p, line.spell).unwrap();
    let rank_for = |sim: &mut forever_sim::engine::Sim, missing: f64| {
        sim.actors[mage].hp = sim.actors[mage].max_hp - missing;
        sim.line_plan(p, line, false, &mut None).expect("the line applies").rank.unwrap()
    };
    let small = rank_for(&mut sim, 0.31 * 3000.0);
    let big = rank_for(&mut sim, 0.65 * 3000.0);
    assert!(small < top, "a 930 gap takes a lower rank than the top ({small} of {top})");
    assert!(big > small, "a bigger gap takes a bigger rank ({big} vs {small})");
}

/// Alone, a heal is on yourself: the party targets change nothing solo.
#[test]
fn solo_heals_are_on_yourself() {
    let out = trace("priest/holy", "solo_2_hard", 40);
    let me = out["me"].as_str().unwrap();
    let heals: Vec<&serde_json::Value> = out["events"].as_array().unwrap().iter().filter(|e| e["kind"] == "heal" && e["who"] == me).collect();
    assert!(!heals.is_empty());
    assert!(heals.iter().all(|e| e["target"].is_null() || e["target"] == me), "solo heals land on the healer");
}

/// The healing tests give the same numbers however the fights are split.
#[test]
fn healing_results_do_not_depend_on_threads() {
    let w = world();
    let sc = &w.scenarios["heal_dungeon"];
    let mk = || {
        HEALERS.iter().take(2).map(|f| {
            let b = load_build(&w, Path::new(&format!("builds/{f}.toml")), Some(40)).unwrap();
            Variant { world: 0, name: b.name.clone(), opts: defaults(&w, sc), build: b, opponent: None }
        }).collect::<Vec<_>>()
    };
    let cfg1 = McCfg { min_reps: 12, max_reps: 12, batch: 5, threads: 1, ..Default::default() };
    let cfg4 = McCfg { threads: 4, batch: 3, ..cfg1.clone() };
    let a = mc::run(&[&w], "heal_dungeon", &mk(), &cfg1);
    let b = mc::run(&[&w], "heal_dungeon", &mk(), &cfg4);
    assert_eq!(a.values, b.values, "results must not depend on threads or batch size");
}

/// Cures take off only their own types, the newest first. Cleanse takes a
/// magic effect, a poison and a disease but never a curse or Mortal
/// Strike; Dispel Magic cleans a friend or strips an enemy's buff; Remove
/// Lesser Curse takes the curse.
#[test]
fn cleanse_removes_by_type() {
    use forever_sim::engine::{Cx, Sim};
    let w = world();
    let sc = &w.scenarios["duel"];
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let pal = sim.spawn_player(&load_build(&w, Path::new("builds/paladin/holy.toml"), Some(60)).unwrap(), 0, 1);
    let pri = sim.spawn_player(&load_build(&w, Path::new("builds/priest/holy.toml"), Some(60)).unwrap(), 0, 2);
    let mage = sim.spawn_player(&load_build(&w, Path::new("builds/mage/frost.toml"), Some(60)).unwrap(), 0, 3);
    let lock = sim.spawn_player(&load_build(&w, Path::new("builds/warlock/affliction.toml"), Some(60)).unwrap(), 1, 4);
    for (x, at) in [(pal, 0.0), (pri, 2.0), (mage, 4.0), (lock, 15.0)] {
        sim.place(x, at);
    }
    for x in [pal, pri, mage] {
        sim.actors[x].target = Some(lock);
    }
    sim.actors[lock].target = Some(pal);
    let put = |sim: &mut Sim, on: usize, spell: Option<&str>, aura: &str| {
        let mut cx = Cx::new(lock, Some(on), "");
        cx.spell = spell.map(|s| w.spell_index[s]);
        sim.apply_aura(on, w.aura_index[aura], lock, 1, Some(60.0), None, cx);
    };
    let has = |sim: &Sim, on: usize, aura: &str| sim.actors[on].auras.iter().any(|i| i.def == w.aura_index[aura]);
    put(&mut sim, pal, Some("warlock.corruption"), "warlock.corruption");
    put(&mut sim, pal, Some("warlock.bane_of_agony"), "warlock.bane_of_agony");
    put(&mut sim, pal, None, "rogue.deadly_poison");
    put(&mut sim, pal, None, "boss_mortal_wound");
    let types: Vec<u8> = sim.actors[pal].auras.iter().map(|i| i.dispel).collect();
    assert_eq!(types, vec![DISPEL_MAGIC, DISPEL_CURSE, DISPEL_POISON, 0], "dispel types from the spell or the aura");
    let plan = sim.can_cast(pal, w.spell_index["paladin.cleanse"], SpellTarget::Me, false).expect("Cleanse");
    sim.begin(pal, plan);
    assert!(!has(&sim, pal, "warlock.corruption") && !has(&sim, pal, "rogue.deadly_poison"), "Cleanse takes the magic effect and the poison");
    assert!(has(&sim, pal, "warlock.bane_of_agony") && has(&sim, pal, "boss_mortal_wound"), "but not the curse or Mortal Strike");
    // Dispel Magic on a friend (the most hurt ally)
    sim.t += 2.0;
    put(&mut sim, pal, Some("warlock.corruption"), "warlock.corruption");
    sim.actors[pal].hp = 0.5 * sim.actors[pal].max_hp;
    let plan = sim.can_cast(pri, w.spell_index["priest.dispel_magic"], SpellTarget::Lowest, false).expect("Dispel Magic");
    assert_eq!(plan.target, pal);
    sim.begin(pri, plan);
    assert!(!has(&sim, pal, "warlock.corruption"), "Dispel Magic cleans a friend");
    // and on an enemy, a magic buff
    sim.t += 2.0;
    let cx = Cx::new(pri, Some(lock), "");
    sim.apply_aura(lock, w.aura_index["priest.power_word_shield"], pri, 1, Some(30.0), None, cx);
    let plan = sim.can_cast(pri, w.spell_index["priest.dispel_magic_enemy"], SpellTarget::Enemy, false).expect("Dispel Magic on the enemy");
    sim.begin(pri, plan);
    assert!(!has(&sim, lock, "priest.power_word_shield"), "Dispel Magic strips an enemy's shield");
    // Remove Lesser Curse
    put(&mut sim, mage, Some("warlock.bane_of_agony"), "warlock.bane_of_agony");
    let plan = sim.can_cast(mage, w.spell_index["mage.remove_lesser_curse"], SpellTarget::Me, false).expect("Remove Lesser Curse");
    sim.begin(mage, plan);
    assert!(!has(&sim, mage, "warlock.bane_of_agony"), "Remove Lesser Curse takes the curse");
}

/// The healing tests' bosses: Mortal Strike halves healing on the tank for
/// 5 sec, and the dungeon boss's Shadow Word: Pain is a magic effect a
/// priest takes off.
#[test]
fn healing_bosses_wound_and_healers_dispel() {
    let out = trace("priest/holy", "heal_dungeon", 40);
    let me = out["me"].as_str().unwrap();
    let events = out["events"].as_array().unwrap();
    assert!(events.iter().any(|e| e["kind"] == "gain" && e["who"] == "Tank" && e["name"] == "Mortal Strike"), "Mortal Strike wounds the tank");
    let pains = events.iter().filter(|e| e["kind"] == "gain" && e["name"] == "Shadow Word: Pain").count();
    let dispels = events.iter().filter(|e| e["kind"] == "dispel" && e["who"] == me && e["name"] == "Shadow Word: Pain").count();
    assert!(pains > 0 && dispels > 0, "{pains} Shadow Word: Pains, {dispels} dispelled");
    // Mortal Strike's wound halves healing taken
    use forever_sim::engine::{Cx, Sim};
    use forever_sim::vars::Stat;
    let w = world();
    let sc = &w.scenarios["heal_dungeon"];
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&load_build(&w, Path::new("builds/warrior/arms.toml"), Some(40)).unwrap(), 0, 1);
    let before = sim.actors[p].st[Stat::HealTaken as usize];
    sim.apply_aura(p, w.aura_index["boss_mortal_wound"], p, 1, None, None, Cx::new(p, None, ""));
    assert!((sim.actors[p].st[Stat::HealTaken as usize] - 0.5 * before).abs() < 1e-9, "healing taken halved");
    let k = sim.actors[p].auras.iter().find(|i| i.def == w.aura_index["boss_mortal_wound"]).unwrap();
    assert!((k.expires - sim.t - 5.0).abs() < 1e-9, "for 5 sec");
}

/// A bandage heals over its channel, any damage stops it, and Recently
/// Bandaged keeps the next one a minute away; between pulls a leveler
/// bandages before eating.
#[test]
fn bandages_heal_and_break_on_damage() {
    use forever_sim::engine::{Ev, Sim};
    let w = world();
    let sc = &w.scenarios["solo_1"];
    let mut sim = Sim::new(&w, 1, defaults(&w, sc));
    let p = sim.spawn_player(&load_build(&w, Path::new("builds/warrior/arms.toml"), Some(60)).unwrap(), 0, 1);
    let e = sim.spawn_enemy(&w.enemies["humanoid"], 60, 1, 1000, 1.0);
    sim.actors[p].hp = 0.3 * sim.actors[p].max_hp;
    let fa = w.spell_index["first_aid"];
    let plan = sim.can_cast(p, fa, SpellTarget::Me, false).expect("a bandage");
    sim.begin(p, plan);
    assert!(sim.actors[p].cast.as_ref().map_or(false, |c| c.channel), "bandaging is a channel");
    sim.apply_damage(e, p, 10.0, Ev::default(), "hit");
    assert!(sim.actors[p].cast.is_none(), "damage stops it");
    sim.t += 30.0;
    assert!(sim.can_cast(p, fa, SpellTarget::Me, false).is_none(), "Recently Bandaged");
    let mut opts = defaults(&w, sc);
    opts[w.opts.iter().position(|o| o.name == "first_aid").unwrap()] = 0.0;
    let mut sim = Sim::new(&w, 1, opts);
    let p = sim.spawn_player(&load_build(&w, Path::new("builds/warrior/arms.toml"), Some(60)).unwrap(), 0, 1);
    sim.actors[p].hp = 0.3 * sim.actors[p].max_hp;
    assert!(sim.can_cast(p, fa, SpellTarget::Me, false).is_none(), "not with bandages turned off");
    let sc = &w.scenarios["solo_2"];
    let b = load_build(&w, Path::new("builds/warrior/arms.toml"), Some(30)).unwrap();
    let (v, _) = run_rep(&RepInput { w: &w, sc, build: &b, opponent: None, opts: &defaults(&w, sc), seed: 1, log: false });
    assert!(v.get("casts_per_kill:First Aid").copied().unwrap_or(0.0) > 0.0, "a leveler bandages between pulls");
}

/// An emergency line goes after a reaction, never late, and stops your own
/// cast for it; an ordinary line waits for the cast to finish.
#[test]
fn emergency_lines_stop_casts() {
    use forever_sim::engine::Sim;
    let mut w = world();
    let sc = &w.scenarios["solo_1"].clone();
    let mage = w.classes.iter().position(|c| c.id == "mage").unwrap();
    let ib = w.spell_index["mage.ice_block"];
    let k = w.classes[mage].rotations["frost"].actions.iter().position(|a| a.spell == ib).expect("an Ice Block line");
    let b = load_build(&w, Path::new("builds/mage/frost.toml"), Some(60)).unwrap();
    for emergency in [false, true] {
        {
            let r = w.classes[mage].rotations.get_mut("frost").unwrap();
            r.actions[k].emergency = emergency;
            r.emergency = if emergency { vec![k as u16] } else { vec![] };
        }
        let mut sim = Sim::new(&w, 5, defaults(&w, sc));
        let p = sim.spawn_player(&b, 0, 1);
        let e = sim.spawn_enemy(&w.enemies["humanoid"], 60, 1, 1000, 1.0);
        sim.place(p, 0.0);
        sim.place(e, 25.0);
        sim.actors[p].target = Some(e);
        sim.actors[e].target = Some(p);
        sim.enter_combat();
        let fb_id = w.spell_index["mage.frostbolt"];
        let fb = sim.can_cast(p, fb_id, SpellTarget::Enemy, false).expect("a Frostbolt");
        sim.begin(p, fb);
        sim.actors[p].hp = 0.1 * sim.actors[p].max_hp;
        // past the global cooldown, the Frostbolt still going; a reaction to see it
        let mut stopped = false;
        for step in 1..=10 {
            sim.t = 1.6 + 0.1 * step as f64;
            if sim.actors[p].cast.as_ref().map_or(true, |c| c.spell != fb_id) {
                break;
            }
            sim.act(p);
            stopped |= sim.actors[p].auras.iter().any(|i| i.def == w.aura_index["mage.ice_block"]);
        }
        assert_eq!(stopped, emergency, "an emergency Ice Block stops the Frostbolt; an ordinary one waits");
    }
}

/// Your side's crowd control holds: while another enemy fights, you turn to
/// it, your pet leaves the held one, swings and single-target spells skip
/// it, and the CC can still be put back on. Alone, auto attacks still hold
/// off, and a line breaks it.
#[test]
fn own_crowd_control_holds() {
    use forever_sim::engine::{Cx, Sim};
    use forever_sim::vars::Hand;
    let w = world();
    let sc = &w.scenarios["solo_2"];
    let mut sim = Sim::new(&w, 2, defaults(&w, sc));
    let war = sim.spawn_player(&load_build(&w, Path::new("builds/warrior/arms.toml"), Some(60)).unwrap(), 0, 1);
    let mage = sim.spawn_player(&load_build(&w, Path::new("builds/mage/frost.toml"), Some(60)).unwrap(), 0, 2);
    let hunter = sim.spawn_player(&load_build(&w, Path::new("builds/hunter/beast_mastery.toml"), Some(60)).unwrap(), 0, 3);
    let e1 = sim.spawn_enemy(&w.enemies["humanoid"], 60, 1, 1000, 1.0);
    let e2 = sim.spawn_enemy(&w.enemies["humanoid"], 60, 1, 1001, 1.0);
    for (x, at) in [(war, 0.0), (mage, -20.0), (hunter, -25.0), (e1, 2.0), (e2, 3.0)] {
        sim.place(x, at);
    }
    sim.prep([hunter, hunter]);
    sim.enter_combat();
    for x in [e1, e2] {
        sim.actors[x].aware = true;
        sim.actors[x].target = Some(war);
    }
    let cx = Cx::new(mage, Some(e2), "");
    sim.apply_aura(e2, w.aura_index["mage.polymorph"], mage, 1, Some(30.0), None, cx);
    // the warrior on the sheep turns to the free one
    sim.actors[war].target = Some(e2);
    sim.retarget(war);
    assert_eq!(sim.actors[war].target, Some(e1), "you turn to the enemy that's free");
    sim.actors[war].target = Some(e2);
    assert!(!sim.can_swing(war, Hand::Mh), "no swings at the sheep");
    let ms = w.classes.iter().find(|c| c.id == "warrior").unwrap().rotations["arms_2h"].actions.iter().find(|a| a.spell == w.spell_index["warrior.mortal_strike"]).unwrap().clone();
    sim.actors[war].res[forever_sim::vars::Res::Rage as usize] = 100.0;
    assert!(sim.line_plan(war, &ms, true, &mut None).is_none(), "no Mortal Strike on the sheep");
    let mut bold = ms.clone();
    bold.breaks_cc = true;
    assert!(sim.line_plan(war, &bold, true, &mut None).is_some(), "unless the line says it breaks crowd control");
    // the hunter's pet leaves it too
    let pet = sim.actors[hunter].pets.iter().copied().find(|&p| sim.actors[p].alive).expect("a pet");
    sim.actors[hunter].target = Some(e2);
    sim.actors[pet].target = Some(e2);
    sim.retarget(pet);
    assert_eq!(sim.actors[pet].target, Some(e1), "the pet takes the free one");
    // the mage can sheep it again
    let poly = w.classes.iter().find(|c| c.id == "mage").unwrap().rotations.values().flat_map(|r| r.actions.iter()).find(|a| a.spell == w.spell_index["mage.polymorph"]).unwrap().clone();
    sim.actors[mage].target = Some(e2);
    let mut again = poly.clone();
    again.cond = None;
    assert!(sim.line_plan(mage, &again, true, &mut None).is_some(), "Polymorph can go back on");
    // with nobody else to fight, auto attacks still hold off (a breather),
    // but a line can break it
    sim.actors[e1].alive = false;
    assert!(!sim.can_swing(war, Hand::Mh), "no auto attack at your own sheep");
    assert!(sim.line_plan(war, &ms, true, &mut None).is_some(), "Mortal Strike can break it once it's the only one left");
}

/// Upkeep (reapplied a little late by a typical player) is a line watching
/// its own aura's presence, not its stacks: Rend is upkeep, Sunder isn't.
/// And the rest thresholds can read the next pull.
#[test]
fn upkeep_and_rest_by_pull() {
    let w = world();
    let war = w.classes.iter().find(|c| c.id == "warrior").unwrap();
    let line = |rot: &str, spell: &str| war.rotations[rot].actions.iter().find(|a| a.spell == w.spell_index[spell]).unwrap().clone();
    assert!(line("arms_2h", "warrior.rend").upkeep, "Rend is upkeep");
    assert!(!line("prot_shield", "warrior.sunder_armor").upkeep, "Sunder isn't");
    let dir = std::env::temp_dir().join(format!("fsim-rest-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let patch = dir.join("rest.toml");
    std::fs::write(&patch, "[classes.warrior.rotations.arms_2h]\neat_below = \"pull.size >= 2 ? 0.99 : 0.01\"\n").unwrap();
    let w2 = load_world(&LoadOptions { data_dir: PathBuf::from("data"), extra_patches: vec![patch], ..Default::default() }).unwrap();
    let b = load_build(&w2, Path::new("builds/warrior/arms.toml"), Some(30)).unwrap();
    let eats = |id: &str| {
        let sc = &w2.scenarios[id];
        let (v, _) = run_rep(&RepInput { w: &w2, sc, build: &b, opponent: None, opts: &defaults(&w2, sc), seed: 1, log: false });
        v["eats_per_hour"]
    };
    let (one, two) = (eats("solo_1"), eats("solo_2"));
    assert!(two > 5.0 * one.max(0.1), "eats before doubles ({two}/h), hardly before singles ({one}/h)");
    std::fs::remove_dir_all(&dir).ok();
}

/// The website's sessions keep results by column; for every metric that
/// keeps its values they summarise exactly as the command line does from
/// the same replications.
#[test]
fn stored_columns_summarise_like_the_command_line() {
    let w = world();
    let sc = &w.scenarios["boss"];
    let b = load_build(&w, Path::new("builds/rogue/combat.toml"), Some(60)).unwrap();
    let cfg = McCfg { min_reps: 48, max_reps: 48, batch: 16, precision: 0.0, threads: 2, ..Default::default() };
    let res = mc::run(&[&w], "boss", &[Variant { world: 0, name: b.name.clone(), build: b, opponent: None, opts: defaults(&w, sc) }], &cfg);
    let kind = |k: &str| forever_sim::scenario::metric_kind(&w, sc, &defaults(&w, sc), k);
    let cli: Vec<serde_json::Value> = mc::summarise_with(&res.values[0], 0.95, &kind).into_iter().filter(|m| !m.name.contains(':')).map(|m| serde_json::to_value(m).unwrap()).collect();
    let text = std::fs::read_to_string("builds/rogue/combat.toml").unwrap();
    let build = serde_json::to_value(toml::from_str::<toml::Value>(&text).unwrap()).unwrap();
    let call = |j: serde_json::Value| -> serde_json::Value { serde_json::from_str(&forever_sim::api::handle(&j.to_string())).unwrap() };
    let id = call(serde_json::json!({ "op": "start", "scenario": "boss", "level": 60, "builds": [build], "precision": 0.0, "min_reps": 48, "max_reps": 48 }))["id"].as_u64().unwrap();
    while !call(serde_json::json!({ "op": "step", "id": id, "reps": 16 }))["finished"].as_bool().unwrap() {}
    let r = call(serde_json::json!({ "op": "result", "id": id }));
    let web: Vec<serde_json::Value> = r["variants"][0]["metrics"].as_array().unwrap().iter().filter(|m| !m["name"].as_str().unwrap().contains(':')).cloned().collect();
    assert_eq!(web, cli);
}

/// Breakevens come off a bisection grid: it's finer than half the
/// website's Precise target (0.5%), so the grid never decides a result.
#[test]
fn breakeven_grids_are_fine() {
    let w = world();
    for id in ["pressure", "pressure_2v1", "heal_dungeon", "heal_tank", "heal_raid"] {
        let sc = &w.scenarios[id];
        let primary = sc.primary.clone();
        match forever_sim::scenario::metric_kind(&w, sc, &defaults(&w, sc), &primary) {
            mc::MetricKind::Median { rel_cell } => assert!(rel_cell > 0.0 && rel_cell < 0.0025, "{id}: a {rel_cell} grid"),
            k => panic!("{id}: {k:?}"),
        }
    }
}
