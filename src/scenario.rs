//! Scenario runners. Each runs ONE replication with a given seed and
//! returns named metrics. The Monte Carlo driver calls these many times.

use crate::build::Build;
use crate::engine::{Kind, Outcome, Sim, Stop};
use crate::model::*;
use crate::vars::*;
use std::collections::BTreeMap;

pub type Values = BTreeMap<String, f64>;

pub struct RepInput<'a> {
    pub w: &'a World,
    pub sc: &'a ScenarioDef,
    pub build: &'a Build,
    pub opponent: Option<&'a Build>,
    pub opts: &'a [f64],
    pub seed: u64,
    pub log: bool,
}

fn put(v: &mut Values, k: &str, x: f64) {
    v.insert(k.to_string(), x);
}

fn breakdown(v: &mut Values, prefix: &str, m: &BTreeMap<&str, f64>, div: f64) {
    for (k, x) in m {
        *v.entry(format!("{prefix}{k}")).or_insert(0.0) += x / div.max(1e-9);
    }
}

pub fn run_rep(inp: &RepInput) -> (Values, Option<Vec<String>>) {
    match &inp.sc.kind {
        ScenarioKind::Leveling { pull, minutes, enemy, travel } => leveling(inp, *pull, *minutes, enemy, *travel),
        ScenarioKind::Boss { duration, enemy, execute_by_time } => boss(inp, *duration, enemy, *execute_by_time),
        ScenarioKind::Duel { max_time, .. } => duel(inp, *max_time),
        ScenarioKind::Pressure { attacker, attackers, lo, hi, steps, max_time, win_needs_kill } => {
            pressure(inp, attacker, *attackers, *lo, *hi, *steps, *max_time, *win_needs_kill)
        }
    }
}

fn new_sim<'w>(inp: &RepInput<'w>) -> Sim<'w> {
    let mut s = Sim::new(inp.w, inp.seed, inp.opts.to_vec());
    if inp.log {
        s.log = Some(vec![]);
    }
    s
}

fn rest(sim: &mut Sim, p: usize, travel: f64) -> (f64, f64, f64) {
    let w = sim.w;
    let start = sim.t;
    sim.rest_phase = true;
    sim.run(Stop::Until(sim.t + travel));
    sim.run(Stop::Rest { actor: p, until: sim.t + 90.0 });
    let rot = sim.actors[p].rotation;
    let (eat_below, drink_below) = rot.map_or((0.5, 0.4), |r| (r.eat_below, r.drink_below));
    let x = &sim.actors[p];
    let (food, drink, dur) = w.rest.at(x.level);
    let hp_def = x.max_hp - x.hp;
    let mana_max = x.max_res[Res::Mana as usize];
    let mana_def = mana_max - x.res[Res::Mana as usize];
    let need_eat = x.hp < eat_below * x.max_hp;
    let need_drink = mana_max > 0.0 && x.res[Res::Mana as usize] < drink_below * mana_max;
    let (mut eats, mut drinks) = (0.0, 0.0);
    if need_eat || need_drink {
        let t_eat = if need_eat { dur * hp_def / food } else { 0.0 };
        let t_drink = if need_drink { dur * mana_def / drink } else { 0.0 };
        let t = t_eat.max(t_drink);
        if need_eat {
            eats = (hp_def / food).ceil();
        }
        if need_drink {
            drinks = (mana_def / drink).ceil();
        }
        sim.run(Stop::Until(sim.t + t));
        let x = &mut sim.actors[p];
        if need_eat {
            x.hp = x.max_hp;
        }
        if need_drink {
            x.res[Res::Mana as usize] = mana_max;
        }
    }
    // pets recover while the owner rests
    let pets = sim.actors[p].pets.clone();
    for pt in pets {
        let x = &mut sim.actors[pt];
        x.hp = x.max_hp;
        x.res[Res::Mana as usize] = x.max_res[Res::Mana as usize];
    }
    sim.rest_phase = false;
    (sim.t - start, eats, drinks)
}

fn leveling(inp: &RepInput, pull: u32, minutes: f64, enemy: &str, travel: f64) -> (Values, Option<Vec<String>>) {
    let w = inp.w;
    let e = &w.enemies[enemy];
    let mut sim = new_sim(inp);
    let p = sim.spawn_player(inp.build, 0, 1);
    sim.actors[p].behind = inp.sc.behind;
    sim.precombat(p);
    let session = minutes * 60.0;
    let (mut kills, mut deaths, mut eats, mut drinks, mut pulls, mut stuck) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut combat_t, mut rest_t, mut dead_t) = (0.0, 0.0, 0.0);
    let mut i: u64 = 0;
    while sim.t < session {
        let lvl = (inp.build.level as i32 + e.level_offset).max(1) as u32;
        let mut ids = vec![];
        for j in 0..pull as u64 {
            ids.push(sim.spawn_enemy(e, lvl, 1, 1000 + i * 16 + j, 1.0));
        }
        sim.actors[p].target = Some(ids[0]);
        sim.enter_combat();
        sim.opener(p);
        let start = sim.t;
        let out = sim.run(Stop::Fight { until: start + 300.0 });
        combat_t += sim.t - start;
        pulls += 1.0;
        let killed = ids.iter().filter(|&&k| !sim.actors[k].alive).count() as f64;
        kills += killed;
        for &k in &ids {
            sim.actors[k].alive = false;
        }
        sim.leave_combat();
        match out {
            Outcome::PlayerDied => {
                deaths += 1.0;
                // corpse run and resurrection at half health and mana
                sim.run(Stop::Until(sim.t + 120.0));
                dead_t += 120.0;
                let x = &mut sim.actors[p];
                x.alive = true;
                x.hp = 0.5 * x.max_hp;
                x.res[Res::Mana as usize] = 0.5 * x.max_res[Res::Mana as usize];
                x.auras.clear();
                x.form = None;
                x.cast = None;
                x.queued = None;
                x.cp = 0.0;
                sim.refresh_stats(p);
                sim.precombat(p);
                let x = &mut sim.actors[p];
                x.hp = 0.5 * x.max_hp;
                x.res[Res::Mana as usize] = 0.5 * x.max_res[Res::Mana as usize];
            }
            Outcome::Timeout => stuck += 1.0,
            _ => {}
        }
        let (rt, ea, dr) = rest(&mut sim, p, travel);
        rest_t += rt;
        eats += ea;
        drinks += dr;
        i += 1;
        if i > 2000 {
            break;
        }
    }
    let hrs = sim.t / 3600.0;
    let m = &sim.metrics[p];
    let mut v = Values::new();
    put(&mut v, "kills_per_hour", kills / hrs);
    put(&mut v, "deaths_per_hour", deaths / hrs);
    put(&mut v, "eats_per_hour", eats / hrs);
    put(&mut v, "drinks_per_hour", drinks / hrs);
    put(&mut v, "rest_share", (rest_t - travel * pulls).max(0.0) / sim.t);
    put(&mut v, "combat_share", combat_t / sim.t);
    put(&mut v, "dead_share", dead_t / sim.t);
    put(&mut v, "time_per_kill", combat_t / kills.max(1.0));
    put(&mut v, "dps_in_combat", m.dmg / combat_t.max(1e-9));
    put(&mut v, "taken_per_kill", m.taken / kills.max(1.0));
    put(&mut v, "healed_per_kill", m.heal / kills.max(1.0));
    put(&mut v, "stalled_pulls", stuck);
    breakdown(&mut v, "heal_per_kill:", &m.heal_by, kills.max(1.0));
    breakdown(&mut v, "dmg_share:", &m.dmg_by, m.dmg.max(1e-9));
    breakdown(&mut v, "casts_per_kill:", &m.casts, kills.max(1.0));
    (v, sim.log.take())
}

fn boss(inp: &RepInput, duration: f64, enemy: &str, by_time: bool) -> (Values, Option<Vec<String>>) {
    let w = inp.w;
    let e = &w.enemies[enemy];
    let mut sim = new_sim(inp);
    let p = sim.spawn_player(inp.build, 0, 1);
    sim.actors[p].behind = inp.sc.behind;
    sim.precombat(p);
    let lvl = (inp.build.level as i32 + e.level_offset).max(1) as u32;
    let b = sim.spawn_enemy(e, lvl, 1, 1000, 1.0);
    if by_time {
        sim.actors[b].timed_hp = Some((sim.t, duration));
    }
    sim.actors[p].target = Some(b);
    sim.enter_combat();
    sim.opener(p);
    let start = sim.t;
    sim.run(Stop::Fight { until: start + duration });
    let el = (sim.t - start).max(1e-9);
    let m = &sim.metrics[p];
    let mut v = Values::new();
    put(&mut v, "dps", m.dmg / el);
    put(&mut v, "hps", m.heal / el);
    put(&mut v, "crit_rate", m.crits / m.hits.max(1.0));
    put(&mut v, "died", if sim.actors[p].alive { 0.0 } else { 1.0 });
    breakdown(&mut v, "dps:", &m.dmg_by, el);
    breakdown(&mut v, "casts_per_min:", &m.casts, el / 60.0);
    (v, sim.log.take())
}

fn duel(inp: &RepInput, max_time: f64) -> (Values, Option<Vec<String>>) {
    let mut sim = new_sim(inp);
    let a = sim.spawn_player(inp.build, 0, 1);
    let opp = inp.opponent.expect("duel needs an opponent (--vs)");
    let b = sim.spawn_player(opp, 1, 2);
    sim.actors[a].behind = inp.sc.behind;
    sim.precombat(a);
    sim.precombat(b);
    sim.actors[a].target = Some(b);
    sim.actors[b].target = Some(a);
    sim.enter_combat();
    sim.opener(a);
    sim.opener(b);
    let start = sim.t;
    let out = sim.run(Stop::Fight { until: start + max_time });
    let el = sim.t - start;
    let mut v = Values::new();
    let win = match out {
        Outcome::EnemiesDead => 1.0,
        _ => 0.0,
    };
    put(&mut v, "win", win);
    put(&mut v, "loss", if out == Outcome::PlayerDied { 1.0 } else { 0.0 });
    put(&mut v, "timeout", if out == Outcome::Timeout { 1.0 } else { 0.0 });
    put(&mut v, "duration", el);
    let m = &sim.metrics[a];
    put(&mut v, "dps", m.dmg / el.max(1e-9));
    put(&mut v, "healed", m.heal);
    put(&mut v, "taken", m.taken);
    put(&mut v, "end_hp_pct", sim.actors[a].hp_pct(sim.t));
    breakdown(&mut v, "heal:", &m.heal_by, 1.0);
    (v, sim.log.take())
}

#[allow(clippy::too_many_arguments)]
fn pressure_fight(inp: &RepInput, attacker: &str, n: u32, scale: f64, max_time: f64, log: bool) -> (bool, f64, Values, Option<Vec<String>>) {
    let w = inp.w;
    let e = &w.enemies[attacker];
    let mut sim = Sim::new(w, inp.seed, inp.opts.to_vec());
    if log {
        sim.log = Some(vec![]);
    }
    let p = sim.spawn_player(inp.build, 0, 1);
    sim.actors[p].behind = inp.sc.behind;
    sim.precombat(p);
    let mut ids = vec![];
    for j in 0..n as u64 {
        let id = sim.spawn_enemy(e, inp.build.level, 1, 500 + j, scale);
        sim.actors[id].stun_sched = e.stuns.iter().map(|(s, d)| (sim.t + s, *d)).collect();
        sim.actors[id].target = Some(p);
        ids.push(id);
    }
    sim.actors[p].target = Some(ids[0]);
    sim.enter_combat();
    // The attacker opens: its opening stun lands before you can act.
    for &id in &ids {
        sim.scripted_stuns(id);
    }
    sim.opener(p);
    let start = sim.t;
    let out = sim.run(Stop::Fight { until: start + max_time });
    let kills = ids.iter().filter(|&&k| !sim.actors[k].alive).count();
    let alive = sim.actors[p].alive;
    let win = alive && kills >= 1 && out != Outcome::PlayerDied;
    let mut v = Values::new();
    let m = &sim.metrics[p];
    put(&mut v, "time_alive", sim.t - start);
    put(&mut v, "dealt", m.dmg);
    put(&mut v, "healed", m.heal);
    breakdown(&mut v, "heal:", &m.heal_by, 1.0);
    let _ = Kind::Player;
    (win, sim.t - start, v, sim.log.take())
}

#[allow(clippy::too_many_arguments)]
fn pressure(inp: &RepInput, attacker: &str, n: u32, lo: f64, hi: f64, steps: u32, max_time: f64, needs_kill: bool) -> (Values, Option<Vec<String>>) {
    let _ = needs_kill;
    let w = inp.w;
    let e = &w.enemies[attacker];
    // per-replication critical scale by bisection on a log scale (same seed every probe)
    let (mut l, mut h) = (lo, hi);
    for _ in 0..steps {
        let mid = (l * h).sqrt();
        let (win, _, _, _) = pressure_fight(inp, attacker, n, mid, max_time, false);
        if win {
            l = mid;
        } else {
            h = mid;
        }
    }
    let crit = (l * h).sqrt();
    // what one unit of scale means: the attacker's damage per second before mitigation
    let probe = Sim::new(w, 0, inp.opts.to_vec());
    let cx = crate::engine::Cx::new(0, None, "");
    let mut probe = probe;
    let id = probe.spawn_enemy(e, inp.build.level, 1, 1, 1.0);
    let cx = crate::engine::Cx { me: id, ..cx };
    let hit = probe.ev(&e.dmg, &cx);
    let unit_dps = hit * (1.0 + e.crit / 100.0) / e.speed;
    let (win_ref, t_ref, mut v, log) = pressure_fight(inp, attacker, n, 1.0, max_time, inp.log);
    put(&mut v, "breakeven", crit * unit_dps);
    put(&mut v, "breakeven_scale", crit);
    put(&mut v, "win_at_reference", if win_ref { 1.0 } else { 0.0 });
    put(&mut v, "time_alive_at_reference", t_ref);
    (v, log)
}
