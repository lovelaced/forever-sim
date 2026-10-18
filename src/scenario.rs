//! Scenario runners. Each runs ONE replication with a given seed and
//! returns named metrics. The Monte Carlo driver calls these many times.

use crate::build::Build;
use crate::engine::{Kind, Outcome, Phase, Sim, Stop, TraceEv, ENV_DUEL, ENV_PULL, ENV_SKIRMISH};
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

/// A replication's numbers, its text log, and (when logging) its events.
pub type RepOut = (Values, Option<Vec<String>>, Option<Vec<TraceEv>>);

pub fn run_rep(inp: &RepInput) -> (Values, Option<Vec<String>>) {
    let (v, log, _) = run_rep_traced(inp);
    (v, log)
}

/// One replication with its structured events (for the website's timeline);
/// `inp.log` turns both on.
pub fn run_rep_traced(inp: &RepInput) -> RepOut {
    match &inp.sc.kind {
        ScenarioKind::Leveling { pull, minutes, enemy, travel, mix, dmg_scale } => leveling(inp, *pull, *minutes, enemy, *travel, mix.as_deref(), *dmg_scale),
        ScenarioKind::Boss { duration, enemy, execute_by_time, tank } => boss(inp, *duration, enemy, *execute_by_time, tank.as_deref()),
        ScenarioKind::Duel { max_time, .. } => duel(inp, *max_time),
        ScenarioKind::Skirmish { max_time, you_first, opponents } => skirmish(inp, *max_time, *you_first, opponents),
        ScenarioKind::Pressure { attacker, attackers, lo, hi, steps, max_time, win_needs_kill } => {
            pressure(inp, attacker, *attackers, *lo, *hi, *steps, *max_time, *win_needs_kill)
        }
        ScenarioKind::Healing(h) => healing(inp, h),
    }
}

/// Why a build can't take a test, if it can't.
pub fn unfit(sc: &ScenarioDef, w: &World, b: &Build) -> Option<String> {
    let ScenarioKind::Healing(h) = &sc.kind else { return None };
    if b.heal_rotation.is_none() {
        return Some(format!("{}: {} has no healing play style", sc.id, w.classes[b.class].name));
    }
    if b.level < h.min_level {
        return Some(format!("{}: needs level {} (the build is {})", sc.id, h.min_level, b.level));
    }
    None
}

/// The highest scale a test still passes, by bisection on a log scale
/// between `lo` and `hi` (the same seed every probe). Returns the breakeven
/// and the highest probe that passed.
fn bisect(lo: f64, hi: f64, steps: u32, mut pass: impl FnMut(f64) -> bool) -> (f64, f64) {
    let (mut l, mut h) = (lo, hi);
    for _ in 0..steps {
        let mid = (l * h).sqrt();
        if pass(mid) {
            l = mid;
        } else {
            h = mid;
        }
    }
    ((l * h).sqrt(), l)
}

fn new_sim<'w>(inp: &RepInput<'w>) -> Sim<'w> {
    let mut s = Sim::new(inp.w, inp.seed, inp.opts.to_vec());
    if inp.log {
        s.log = Some(vec![]);
        s.trace = Some(vec![]);
    }
    s
}

/// How often each line of the rotation fired: per minute of fighting, and
/// the between-pulls lines per hour. Every line is reported (zeros too), so
/// averages over replications aren't biased by lines that rarely fire.
fn line_counts(v: &mut Values, sim: &Sim, p: usize, fight_secs: f64, fights: f64, hours: Option<f64>) {
    let Some(rot) = sim.actors[p].rotation else { return };
    let m = &sim.metrics[p];
    // whether each precombat line came up (1 when it did, however often)
    for k in 0..rot.precombat.len() {
        put(v, &format!("precombat_used:{k}"), m.precombat_lines.get(k).map_or(0.0, |&n| n.min(1.0)));
    }
    for k in 0..rot.opener.len() {
        put(v, &format!("opener_per_fight:{k}"), m.opener_lines.get(k).copied().unwrap_or(0.0) / fights.max(1.0));
    }
    for k in 0..rot.actions.len() {
        put(v, &format!("line_per_min:{k}"), m.lines.get(k).copied().unwrap_or(0.0) * 60.0 / fight_secs.max(1e-9));
    }
    if let Some(h) = hours {
        for k in 0..rot.rest.len() {
            put(v, &format!("rest_per_hour:{k}"), m.rest_lines.get(k).copied().unwrap_or(0.0) / h.max(1e-9));
        }
    }
}

fn rest(sim: &mut Sim, p: usize, travel: f64) -> (f64, f64, f64) {
    let w = sim.w;
    let start = sim.t;
    sim.tracef(|s| s.tev("rest", p, None, ""));
    sim.phase = Phase::Rest;
    sim.run(Stop::Until(sim.t + travel));
    sim.run(Stop::Rest { actor: p, until: sim.t + 90.0 });
    // buff up for the next pull, then eat and drink back what that cost
    sim.prep([p, p]);
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
        let what = match (need_eat, need_drink) { (true, true) => "eat and drink", (true, false) => "eat", _ => "drink" };
        sim.tracef(|s| TraceEv { amount: t, ..s.tev("eat", p, None, what) });
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
    sim.phase = Phase::Fight;
    (sim.t - start, eats, drinks)
}

fn leveling(inp: &RepInput, pull: u32, minutes: f64, enemy: &str, travel: f64, mix: Option<&str>, dmg_scale: f64) -> RepOut {
    let w = inp.w;
    let mix = mix.and_then(|m| w.mixes.get(m));
    // per kind of mob: kills, time fighting, deaths
    let mut by_kind: BTreeMap<&str, (f64, f64, f64)> = BTreeMap::new();
    let mut sim = new_sim(inp);
    let p = sim.spawn_player(inp.build, 0, 1);
    sim.actors[p].behind = inp.sc.behind;
    sim.prep_totems = false;
    sim.precombat(p);
    let session = minutes * 60.0;
    let (mut kills, mut deaths, mut eats, mut drinks, mut pulls, mut stuck) = (0.0, 0.0, 0.0, 0.0, 0.0, 0.0);
    let (mut combat_t, mut rest_t, mut dead_t) = (0.0, 0.0, 0.0);
    let mut i: u64 = 0;
    let mut approach_t = 0.0;
    let mut escapes = 0.0;
    while sim.t < session {
        // the pack stands unaware 30 to 35 yards off, spread over up to 8; it's
        // one kind of creature, and some of it may cast (the same for every
        // build: world draws, all made whether they're needed or not)
        let mut env = sim.env(ENV_PULL, i);
        let far = env.range(30.0, 35.0);
        let spreads: Vec<f64> = (0..pull).map(|_| env.range(0.0, 8.0)).collect();
        let kind_roll = env.f64();
        let caster_rolls: Vec<f64> = (0..pull).map(|_| env.f64()).collect();
        // whether the fight after this one comes to where your totems stand
        let carry_roll = env.f64();
        let kind: &str = match mix {
            Some(m) => {
                let mut acc = 0.0;
                m.kinds.iter().find(|k| {
                    acc += k.1;
                    kind_roll < acc
                }).unwrap_or(m.kinds.last().unwrap()).0.as_str()
            }
            None => enemy,
        };
        sim.spawned.clear();
        let mut ids = vec![];
        for j in 0..pull as u64 {
            let tmpl = match mix.and_then(|m| m.casters.as_ref()) {
                Some((of, share, as_)) if of == kind && caster_rolls[j as usize] < *share => as_.as_str(),
                _ => kind,
            };
            let e = &w.enemies[tmpl];
            let lvl = (inp.build.level as i32 + e.level_offset).max(1) as u32;
            let id = sim.spawn_enemy(e, lvl, 1, 1000 + i * 16 + j, dmg_scale);
            let spread = spreads[j as usize];
            sim.place(id, far + if j == 0 { 0.0 } else { spread });
            sim.actors[id].aware = false;
            sim.actors[id].pack = i as u32 + 1;
            ids.push(id);
        }
        sim.place(p, 0.0);
        for pt in sim.actors[p].pets.clone() {
            sim.place(pt, 0.0);
        }
        sim.actors[p].target = Some(ids[0]);
        sim.tracef(|s| TraceEv { stacks: pull as f64, ..s.tev("pull", p, Some(ids[0]), kind) });
        sim.enter_combat();
        sim.opener(p);
        let start = sim.t;
        let out = sim.run(Stop::Fight { until: start + 300.0 });
        // walking up to the pack is travel; the fight starts when it notices
        let woke = sim.woke_at.unwrap_or(sim.t).clamp(start, sim.t);
        approach_t += woke - start;
        combat_t += sim.t - woke;
        pulls += 1.0;
        // everything that came, fleeing mobs' friends included
        let all = sim.spawned.clone();
        let killed = all.iter().filter(|&&k| !sim.actors[k].alive).count() as f64;
        kills += killed;
        let bk = by_kind.entry(kind).or_insert((0.0, 0.0, 0.0));
        bk.0 += killed;
        bk.1 += sim.t - woke;
        if out == Outcome::PlayerDied {
            bk.2 += 1.0;
        }
        for &k in &all {
            sim.actors[k].alive = false;
        }
        sim.leave_combat();
        let carry = carry_roll < sim.opt_named(p, "totem_carry");
        sim.leave_totems(p, carry);
        match out {
            Outcome::PlayerDied => {
                deaths += 1.0;
                // a shaman's Reincarnation brings it straight back at 20% (no
                // corpse run) when it's off cooldown; otherwise the corpse run
                // and resurrection at half health and mana
                let reinc = w.spell_index.get("shaman.reincarnation").copied().filter(|&s| {
                    sim.known(p, s).is_some() && sim.actors[p].cd_ready(sim.cd_key(s)) <= sim.t + 1e-9
                });
                let (back, wait) = match reinc {
                    Some(s) => {
                        let sm = sim.spell_mod_vals(p, Some(s), None);
                        let cx = crate::engine::Cx::new(p, None, "");
                        let cd = ((sim.ev(&w.spells[s as usize].cooldown, &cx) + sm.cd_add) * sm.cd_mul).max(0.0);
                        let key = sim.cd_key(s);
                        let until = sim.t + cd;
                        match sim.actors[p].cds.iter_mut().find(|c| c.0 == key) {
                            Some(c) => c.1 = until,
                            None => sim.actors[p].cds.push((key, until)),
                        }
                        sim.tracef(|s2| s2.tev("cast", p, None, "Reincarnation"));
                        // Improved Reincarnation: +10/20% health and mana
                        let x = &sim.actors[p];
                        let imp = x.class.and_then(|c| w.classes[c].talent_index.get("improved_reincarnation").map(|&i| x.talents.get(i as usize).copied().unwrap_or(0))).unwrap_or(0);
                        (0.2 + 0.1 * imp as f64, 5.0)
                    }
                    None => (0.5, 120.0),
                };
                sim.run(Stop::Until(sim.t + wait));
                dead_t += wait;
                sim.revive(p);
                let x = &mut sim.actors[p];
                x.hp = back * x.max_hp;
                x.res[Res::Mana as usize] = back * x.max_res[Res::Mana as usize];
                x.auras.clear();
                x.mechs = 0;
                x.stealthed = false;
                x.aura_next = f64::INFINITY;
                x.form = None;
                x.cast = None;
                x.queued = None;
                x.cp = 0.0;
                sim.refresh_stats(p);
                let x = &mut sim.actors[p];
                x.hp = back * x.max_hp;
                x.res[Res::Mana as usize] = back * x.max_res[Res::Mana as usize];
            }
            Outcome::Escaped => escapes += 1.0,
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
    put(&mut v, "approach_share", approach_t / sim.t);
    put(&mut v, "interrupts_per_hour", sim.metrics[p].interrupts / hrs);
    put(&mut v, "escapes_per_hour", escapes / hrs);
    // by kind of mob (each kind's share of kills, time per kill, deaths an hour)
    for (k, (kl, ct, dd)) in &by_kind {
        put(&mut v, &format!("kills_share:{k}"), kl / kills.max(1.0));
        if *kl > 0.0 {
            put(&mut v, &format!("time_per_kill:{k}"), ct / kl);
        }
        put(&mut v, &format!("deaths_by:{k}"), dd / hrs);
    }
    put(&mut v, "engine_spins", sim.spins as f64);
    put(&mut v, "dead_share", dead_t / sim.t);
    put(&mut v, "time_per_kill", combat_t / kills.max(1.0));
    put(&mut v, "dps_in_combat", m.dmg / combat_t.max(1e-9));
    put(&mut v, "taken_per_kill", m.taken / kills.max(1.0));
    put(&mut v, "healed_per_kill", m.heal / kills.max(1.0));
    put(&mut v, "stalled_pulls", stuck);
    breakdown(&mut v, "heal_per_kill:", &m.heal_by, kills.max(1.0));
    breakdown(&mut v, "dmg_share:", &m.dmg_by, m.dmg.max(1e-9));
    breakdown(&mut v, "casts_per_kill:", &m.casts, kills.max(1.0));
    line_counts(&mut v, &sim, p, combat_t, pulls, Some(hrs));
    (v, sim.log.take(), sim.trace.take())
}

thread_local! {
    /// Preset builds made for a level, kept per world (the boss test's tank).
    static PRESETS: std::cell::RefCell<std::collections::HashMap<(u64, String, u32), Option<Build>>> = Default::default();
}

/// A preset build from builds/ (embedded), at a level.
fn preset(w: &World, id: &str, level: u32) -> Option<Build> {
    let key = (w.uid, id.to_string(), level);
    PRESETS.with(|c| {
        c.borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                let file = format!("{id}.toml");
                let (_, text) = crate::embedded::BUILD_FILES.iter().find(|(p, _)| *p == file)?;
                let t: toml::Table = toml::from_str(text).ok()?;
                crate::build::build_from_table(w, &t, &file, Some(level)).ok()
            })
            .clone()
    })
}

/// Put a player (and pets) at the distance it fights from, on the near side
/// of `foe`.
fn stand_off(sim: &mut Sim, p: usize, foe: usize) {
    let (want, _) = sim.stand(p);
    let x = sim.pos(foe) - want;
    sim.place(p, x);
    for pt in sim.actors[p].pets.clone() {
        let (pw, _) = sim.stand(pt);
        let px = sim.pos(foe) - pw;
        sim.place(pt, px);
    }
}

fn boss(inp: &RepInput, duration: f64, enemy: &str, by_time: bool, tank_build: Option<&str>) -> RepOut {
    let w = inp.w;
    let e = &w.enemies[enemy];
    let mut sim = new_sim(inp);
    let p = sim.spawn_player(inp.build, 0, 1);
    sim.actors[p].behind = inp.sc.behind;
    sim.main = Some(p);
    // a tank of your level holds the boss (it's kept alive: someone heals it)
    let tank_def = tank_build.and_then(|id| preset(w, id, inp.build.level));
    let tank = tank_def.as_ref().map(|tb| {
        let t = sim.spawn_player(tb, 0, 2);
        sim.actors[t].unkillable = true;
        t
    });
    sim.tank = tank;
    sim.prep(match tank { Some(t) => [p, t], None => [p, p] });
    // what the rest of the party brings
    if tank.is_some() {
        sim.party_buffs(p);
    }
    sim.top_up(p);
    if let Some(t) = tank {
        sim.top_up(t);
    }
    let lvl = (inp.build.level as i32 + e.level_offset).max(1) as u32;
    let b = sim.spawn_enemy(e, lvl, 1, 1000, 1.0);
    // the boss stands where the tank holds it; you fight from your own distance
    sim.place(b, 0.0);
    sim.actors[p].target = Some(b);
    if let Some(t) = tank {
        sim.actors[t].target = Some(b);
        sim.actors[b].target = Some(t);
        stand_off(&mut sim, t, b);
        sim.aggro_watch = Some((b, t, 0.0, 0));
    }
    stand_off(&mut sim, p, b);
    if by_time {
        sim.actors[b].timed_hp = Some((sim.t, duration));
    }
    sim.enter_combat();
    if let Some(t) = tank {
        // the tank pulls: the boss starts on it, and you give it a head start
        sim.add_threat(b, t, 1.0);
        sim.actors[b].target = Some(t);
        sim.opener(t);
        let lead = w.opt_index("tank_lead").map_or(0.0, |i| inp.opts[i as usize]).max(0.0);
        if lead > 0.0 {
            let go = sim.t + lead;
            let x = &mut sim.actors[p];
            x.react_until = x.react_until.max(go);
            for s in x.swing.iter_mut() {
                if *s < go {
                    *s = go;
                }
            }
            for pt in sim.actors[p].pets.clone() {
                for s in sim.actors[pt].swing.iter_mut() {
                    if *s < go {
                        *s = go;
                    }
                }
            }
        }
    }
    sim.opener(p);
    let start = sim.t;
    sim.run(Stop::Fight { until: start + duration });
    let el = (sim.t - start).max(1e-9);
    let m = &sim.metrics[p];
    let mut v = Values::new();
    if let Some((_, _, held, pulls)) = sim.aggro_watch {
        put(&mut v, "held_share", held / el);
        put(&mut v, "pulled_aggro", pulls as f64);
    }
    put(&mut v, "dps", m.dmg / el);
    put(&mut v, "hps", m.heal / el);
    put(&mut v, "crit_rate", m.crits / m.hits.max(1.0));
    put(&mut v, "died", if sim.actors[p].alive { 0.0 } else { 1.0 });
    breakdown(&mut v, "dps:", &m.dmg_by, el);
    breakdown(&mut v, "casts_per_min:", &m.casts, el / 60.0);
    line_counts(&mut v, &sim, p, el, 1.0, None);
    (v, sim.log.take(), sim.trace.take())
}

/// The duel opponent's assumptions: the run's settings without your build's
/// own overrides, plus the opponent's, and played the way "Your duel
/// opponent plays like" says.
fn opponent_opts(inp: &RepInput, opp: &Build) -> Vec<f64> {
    let w = inp.w;
    let mut v = inp.opts.to_vec();
    for &(i, _) in &inp.build.opts {
        v[i as usize] = inp.sc.opts.iter().find(|o| o.0 == i).map_or(w.opts[i as usize].default, |o| o.1);
    }
    for &(i, x) in &opp.opts {
        v[i as usize] = x;
    }
    if let (Some(pick), Some(like)) = (w.opt_index("opp_play_like"), w.opt_index("play_like")) {
        match inp.opts[pick as usize].round() as i64 {
            1 => v[like as usize] = 0.0,
            2 => v[like as usize] = 1.0,
            _ => {}
        }
    }
    v
}

fn duel(inp: &RepInput, max_time: f64) -> RepOut {
    let mut sim = new_sim(inp);
    let a = sim.spawn_player(inp.build, 0, 1);
    let opp = inp.opponent.expect("duel needs an opponent (--vs)");
    let b = sim.spawn_player(opp, 1, 2);
    sim.give_opts(b, opponent_opts(inp, opp));
    sim.actors[a].behind = inp.sc.behind;
    sim.prep([a, b]);
    sim.top_up(a);
    sim.top_up(b);
    sim.actors[a].target = Some(b);
    sim.actors[b].target = Some(a);
    // 5 to 10 yards apart, and who goes first when both act at the same
    // moment (world draws, the same for every build)
    let mut env = sim.env(ENV_DUEL, 0);
    let gap = env.range(5.0, 10.0);
    sim.flip = env.f64() < 0.5;
    sim.place(a, 0.0);
    sim.place(b, gap);
    for (o, x) in [(a, 0.0), (b, gap)] {
        for pt in sim.actors[o].pets.clone() {
            sim.place(pt, x);
        }
    }
    // both turn hostile on this tick; each moves, and swings, once it has
    // reacted (its own draw), so who strikes first is reaction time
    sim.enter_combat();
    let start = sim.t;
    let mut react = [0.0; 2];
    for (k, x) in [a, b].into_iter().enumerate() {
        let r = if sim.reacting(x) { sim.reaction(x) } else { 0.0 };
        react[k] = r;
        let y = &mut sim.actors[x];
        y.react_until = y.react_until.max(start + r);
        for s in y.swing.iter_mut() {
            if *s < f64::INFINITY {
                *s = s.max(start + r);
            }
        }
    }
    let first = react[0] < react[1] || (react[0] == react[1] && !sim.flip);
    sim.opener(a);
    sim.opener(b);
    let out = sim.run(Stop::Fight { until: start + max_time });
    let el = sim.t - start;
    let mut v = Values::new();
    // both down at once is a draw
    let (a_dead, b_dead) = (!sim.actors[a].alive, !sim.actors[b].alive);
    let (win, loss, draw) = (b_dead && !a_dead, a_dead && !b_dead, a_dead && b_dead);
    let timeout = !a_dead && !b_dead;
    let _ = out;
    if draw {
        sim.tracef(|s| s.tev("draw", a, Some(b), ""));
    }
    let b01 = |x: bool| if x { 1.0 } else { 0.0 };
    let score = b01(win) + 0.5 * b01(draw || timeout);
    put(&mut v, "score", score);
    put(&mut v, "win", b01(win));
    put(&mut v, "loss", b01(loss));
    put(&mut v, "draw", b01(draw));
    put(&mut v, "timeout", b01(timeout));
    // your score split by who reacted first (the page divides by `first`)
    put(&mut v, "first", b01(first));
    put(&mut v, "score_first", if first { score } else { 0.0 });
    put(&mut v, "score_second", if first { 0.0 } else { score });
    put(&mut v, "duration", el);
    put(&mut v, "engine_spins", sim.spins as f64);
    let m = &sim.metrics[a];
    put(&mut v, "dps", m.dmg / el.max(1e-9));
    put(&mut v, "healed", m.heal);
    put(&mut v, "taken", m.taken);
    put(&mut v, "end_hp_pct", sim.actors[a].hp_pct(sim.t));
    breakdown(&mut v, "heal:", &m.heal_by, 1.0);
    breakdown(&mut v, "dps:", &m.dmg_by, el);
    line_counts(&mut v, &sim, a, el, 1.0, None);
    (v, sim.log.take(), sim.trace.take())
}

/// The standard presets (every build file marked `kind = "standard"`), by id.
fn standard_presets() -> Vec<String> {
    let mut v: Vec<String> = crate::embedded::BUILD_FILES
        .iter()
        .filter(|(_, text)| text.lines().any(|l| l.trim() == "kind = \"standard\""))
        .map(|(p, _)| p.trim_end_matches(".toml").to_string())
        .collect();
    v.sort();
    v
}

/// One open-world fight against `opp`: both have seen each other 30 to 40
/// yards apart, and the side striking first moves at once while the other
/// waits for that first hostile move, then for its own reaction. Returns
/// (score, win, loss, draw, timeout, seconds, your damage).
fn skirmish_fight<'w>(inp: &RepInput<'w>, opp: &Build, k: usize, you_first: bool, max_time: f64, log: bool) -> ([f64; 8], Option<Vec<String>>, Option<Vec<TraceEv>>) {
    let seed = crate::rng::derive_seed(inp.seed, &[0x534b_4952, k as u64]);
    let mut sim = Sim::new(inp.w, seed, inp.opts.to_vec());
    if log {
        sim.log = Some(vec![]);
        sim.trace = Some(vec![]);
    }
    let a = sim.spawn_player(inp.build, 0, 1);
    let b = sim.spawn_player(opp, 1, 2);
    sim.give_opts(b, opponent_opts(inp, opp));
    sim.prep([a, b]);
    // you've seen each other: nobody starts in Stealth
    for x in [a, b] {
        while let Some(j) = sim.actors[x].auras.iter().position(|i| inp.w.auras[i.def as usize].stealth) {
            sim.remove_aura_at(x, j, false);
        }
        sim.top_up(x);
    }
    sim.actors[a].target = Some(b);
    sim.actors[b].target = Some(a);
    let mut env = sim.env(ENV_SKIRMISH, k as u64);
    let gap = env.range(30.0, 40.0);
    sim.flip = env.f64() < 0.5;
    sim.place(a, 0.0);
    sim.place(b, gap);
    for (o, x) in [(a, 0.0), (b, gap)] {
        for pt in sim.actors[o].pets.clone() {
            sim.place(pt, x);
        }
    }
    let defender = if you_first { b } else { a };
    sim.actors[defender].waits = true;
    sim.enter_combat();
    let start = sim.t;
    sim.opener(a);
    sim.opener(b);
    sim.run(Stop::Fight { until: start + max_time });
    let el = sim.t - start;
    let (a_dead, b_dead) = (!sim.actors[a].alive, !sim.actors[b].alive);
    let b01 = |x: bool| if x { 1.0 } else { 0.0 };
    let (win, loss, draw, timeout) = (b_dead && !a_dead, a_dead && !b_dead, a_dead && b_dead, !a_dead && !b_dead);
    if draw {
        sim.tracef(|s| s.tev("draw", a, Some(b), ""));
    }
    let score = b01(win) + 0.5 * b01(draw || timeout);
    let dmg = sim.metrics[a].dmg;
    ([score, b01(win), b01(loss), b01(draw), b01(timeout), el, dmg, sim.spins as f64], sim.log.take(), sim.trace.take())
}

/// Open-world PvP: one fight against each opponent (one build of each class,
/// or every standard build when the `skirmish_pool` setting says so). The
/// headline is the average score; each class gets its own, and with every
/// standard build each build does too.
fn skirmish(inp: &RepInput, max_time: f64, you_first: bool, main: &[String]) -> RepOut {
    let w = inp.w;
    let opt = |name: &str| w.opt_index(name).map_or(0.0, |i| inp.opts[i as usize]);
    let ids = if opt("skirmish_pool") >= 0.5 { standard_presets() } else { main.to_vec() };
    let show = opt("skirmish_show").max(0.0) as usize;
    let mut v = Values::new();
    let mut sums = [0.0; 8];
    let mut n = 0.0f64;
    let mut by_class: BTreeMap<String, (f64, f64)> = BTreeMap::new();
    let (mut log, mut trace) = (None, None);
    for (k, id) in ids.iter().enumerate() {
        let Some(opp) = preset(w, id, inp.build.level) else { continue };
        let (r, l, t) = skirmish_fight(inp, &opp, k, you_first, max_time, inp.log && k == show.min(ids.len() - 1));
        if l.is_some() {
            (log, trace) = (l, t);
        }
        for j in 0..8 {
            sums[j] += r[j];
        }
        n += 1.0;
        let cls = w.classes[opp.class].id.clone();
        let e = by_class.entry(cls).or_insert((0.0, 0.0));
        e.0 += r[0];
        e.1 += 1.0;
        if ids.len() > main.len() {
            put(&mut v, &format!("score:{id}"), r[0]);
        }
    }
    let n = n.max(1.0);
    put(&mut v, "score", sums[0] / n);
    put(&mut v, "win", sums[1] / n);
    put(&mut v, "loss", sums[2] / n);
    put(&mut v, "draw", sums[3] / n);
    put(&mut v, "timeout", sums[4] / n);
    put(&mut v, "duration", sums[5] / n);
    put(&mut v, "dps", sums[6] / sums[5].max(1e-9));
    put(&mut v, "engine_spins", sums[7]);
    for (c, (s, m)) in &by_class {
        put(&mut v, &format!("score:{c}"), s / m.max(1.0));
    }
    (v, log, trace)
}

#[allow(clippy::too_many_arguments)]
fn pressure_fight(inp: &RepInput, attacker: &str, n: u32, scale: f64, max_time: f64, log: bool) -> (bool, f64, Values, Option<Vec<String>>, Option<Vec<TraceEv>>) {
    let w = inp.w;
    let e = &w.enemies[attacker];
    let mut sim = Sim::new(w, inp.seed, inp.opts.to_vec());
    if log {
        sim.log = Some(vec![]);
        sim.trace = Some(vec![]);
    }
    let p = sim.spawn_player(inp.build, 0, 1);
    sim.actors[p].behind = inp.sc.behind;
    sim.precombat(p);
    let mut ids = vec![];
    for j in 0..n as u64 {
        let id = sim.spawn_enemy(e, inp.build.level, 1, 500 + j, scale);
        sim.actors[id].stun_sched = e.stuns.iter().map(|(s, d)| (sim.t + s, *d)).collect();
        sim.actors[id].target = Some(p);
        // an ambush: already in melee
        sim.place(id, 3.0);
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
    breakdown(&mut v, "dps:", &m.dmg_by, (sim.t - start).max(1e-9));
    line_counts(&mut v, &sim, p, sim.t - start, 1.0, None);
    let _ = Kind::Player;
    (win, sim.t - start, v, sim.log.take(), sim.trace.take())
}

#[allow(clippy::too_many_arguments)]
fn pressure(inp: &RepInput, attacker: &str, n: u32, lo: f64, hi: f64, steps: u32, max_time: f64, needs_kill: bool) -> RepOut {
    let _ = needs_kill;
    let w = inp.w;
    let e = &w.enemies[attacker];
    // per-replication critical scale by bisection on a log scale (same seed every probe)
    let (crit, _) = bisect(lo, hi, steps, |mid| pressure_fight(inp, attacker, n, mid, max_time, false).0);
    // what one unit of scale means: the attacker's damage per second before mitigation
    let probe = Sim::new(w, 0, inp.opts.to_vec());
    let cx = crate::engine::Cx::new(0, None, "");
    let mut probe = probe;
    let id = probe.spawn_enemy(e, inp.build.level, 1, 1, 1.0);
    let cx = crate::engine::Cx { me: id, ..cx };
    let hit = probe.ev(&e.dmg, &cx);
    let unit_dps = hit * (1.0 + e.crit / 100.0) / e.speed;
    let (win_ref, t_ref, mut v, log, trace) = pressure_fight(inp, attacker, n, 1.0, max_time, inp.log);
    put(&mut v, "breakeven", crit * unit_dps);
    put(&mut v, "breakeven_scale", crit);
    put(&mut v, "win_at_reference", if win_ref { 1.0 } else { 0.0 });
    put(&mut v, "time_alive_at_reference", t_ref);
    (v, log, trace)
}

/// One healing fight at a damage scale: whether the party held, and what
/// the healer did.
fn heal_fight(inp: &RepInput, h: &HealingDef, scale: f64, log: bool) -> (bool, Values, Option<Vec<String>>, Option<Vec<TraceEv>>) {
    let w = inp.w;
    let mut sim = Sim::new(w, inp.seed, inp.opts.to_vec());
    if log {
        sim.log = Some(vec![]);
        sim.trace = Some(vec![]);
    }
    let mut b = inp.build.clone();
    if let Some(r) = &inp.build.heal_rotation {
        b.rotation = r.clone();
    }
    let lvl = b.level;
    let p = sim.spawn_player(&b, 0, 1);
    let tank = sim.spawn_ally(&w.allies[&h.tank], lvl, 0, 0, 100);
    sim.tank = Some(tank);
    let members = h.party.iter().map(|a| (a, 0)).chain(h.raid.iter().map(|a| (a, 1)));
    for (j, (a, g)) in members.enumerate() {
        let id = sim.spawn_ally(&w.allies[a], lvl, 0, g, 101 + j as u64);
        // a second Mage is "Mage 2"
        let name = sim.actors[id].name.clone();
        let same = sim.actors[..id].iter().filter(|x| x.name == name || x.name.starts_with(&format!("{name} "))).count();
        if same > 0 {
            sim.actors[id].name = format!("{name} {}", same + 1);
        }
    }
    sim.precombat(p);
    let e = &w.enemies[&h.boss];
    let bl = (lvl as i32 + e.level_offset).max(1) as u32;
    let boss = sim.spawn_enemy(e, bl, 1, 1000, scale);
    sim.actors[boss].target = Some(tank);
    sim.actors[p].target = Some(boss);
    // the tank holds the boss; the rest of the party and you stand back
    sim.place(boss, 0.0);
    sim.place(tank, -4.0);
    for i in 0..sim.actors.len() {
        if sim.actors[i].kind == Kind::Ally && i != tank {
            sim.place(i, -15.0);
        }
    }
    sim.place(p, -20.0);
    for pt in sim.actors[p].pets.clone() {
        sim.place(pt, -20.0);
    }
    sim.enter_combat();
    // the tank pulled and has a few seconds of threat on the boss
    if let Some(e) = w.allies[&h.tank].threat_per_sec.as_ref() {
        let cx = crate::engine::Cx::new(tank, None, "");
        let lead = 5.0 * sim.ev(e, &cx);
        sim.add_threat(boss, tank, lead);
    }
    sim.start_script(boss);
    sim.opener(p);
    let start = sim.t;
    let out = sim.run(Stop::Party { until: start + h.duration, max_dead: h.max_dead });
    let el = (sim.t - start).max(1e-9);
    let held = out != Outcome::Failed && !sim.party_failed(h.max_dead);
    let party: Vec<usize> = (0..sim.actors.len()).filter(|&i| sim.actors[i].team == 0 && !sim.actors[i].is_pet()).collect();
    let m = &sim.metrics[p];
    let absorbed: f64 = party.iter().map(|&i| sim.metrics[i].absorbed).sum();
    let taken: f64 = party.iter().map(|&i| sim.metrics[i].taken).sum();
    let x = &sim.actors[p];
    let mut v = Values::new();
    put(&mut v, "held", if held { 1.0 } else { 0.0 });
    put(&mut v, "time_held", el);
    put(&mut v, "hps", m.heal / el);
    put(&mut v, "overheal_share", m.overheal / (m.heal + m.overheal).max(1e-9));
    put(&mut v, "shield_share", absorbed / (m.heal + absorbed).max(1e-9));
    put(&mut v, "tank_share", sim.metrics[tank].healed_in / m.heal.max(1e-9));
    put(&mut v, "self_share", sim.metrics[p].healed_in / m.heal.max(1e-9));
    put(&mut v, "taken_per_sec", taken / el);
    put(&mut v, "mana_left", if x.max_res[Res::Mana as usize] > 0.0 { x.res[Res::Mana as usize] / x.max_res[Res::Mana as usize] } else { 0.0 });
    put(&mut v, "deaths", party.iter().filter(|&&i| !sim.actors[i].alive).count() as f64);
    breakdown(&mut v, "hps:", &m.heal_by, el);
    breakdown(&mut v, "casts_per_min:", &m.casts, el / 60.0);
    line_counts(&mut v, &sim, p, el, 1.0, None);
    (held, v, sim.log.take(), sim.trace.take())
}

/// What one unit of a healing test's scale means: the boss's damage per
/// second before armor, dodges and shields, at your level.
fn heal_unit_dps(inp: &RepInput, h: &HealingDef) -> f64 {
    let w = inp.w;
    let e = &w.enemies[&h.boss];
    let mut probe = Sim::new(w, 0, inp.opts.to_vec());
    let bl = (inp.build.level as i32 + e.level_offset).max(1) as u32;
    let id = probe.spawn_enemy(e, bl, 1, 1, 1.0);
    let cx = crate::engine::Cx::new(id, None, "");
    let size = 2 + h.party.len() + h.raid.len();
    let mut dps = if e.attacks { probe.ev(&e.dmg, &cx) / e.speed } else { 0.0 };
    for s in &e.script {
        let n = match s.targets {
            ScriptTargets::Tank => 1,
            ScriptTargets::All => size,
            ScriptTargets::Random(n) => (n as usize).min(size),
            ScriptTargets::Others(n) => (n as usize).min(size - 1),
        };
        dps += probe.ev(&s.dmg, &cx) * n as f64 / s.every;
    }
    dps
}

/// A healing test: the damage the healer keeps the party alive through,
/// found per replication by bisection, and the details of the hardest fight
/// they held.
fn healing(inp: &RepInput, h: &HealingDef) -> RepOut {
    // the probes that held climb, so the last one is the hardest fight held:
    // its numbers are kept rather than played again (unless it's logged)
    let mut last: Option<(f64, Values)> = None;
    let (crit, held_at) = bisect(h.lo, h.hi, h.steps, |s| {
        let (held, v, _, _) = heal_fight(inp, h, s, false);
        if held {
            last = Some((s, v));
        }
        held
    });
    let (mut v, log, trace) = match last {
        Some((s, v)) if s == held_at && !inp.log => (v, None, None),
        _ => {
            let (_, v, log, trace) = heal_fight(inp, h, held_at, inp.log);
            (v, log, trace)
        }
    };
    put(&mut v, "heal_breakeven", crit * heal_unit_dps(inp, h));
    put(&mut v, "heal_breakeven_scale", crit);
    put(&mut v, "holds_standard", if crit >= 1.0 { 1.0 } else { 0.0 });
    (v, log, trace)
}
