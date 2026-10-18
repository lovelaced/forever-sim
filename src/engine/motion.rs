//! Movement along one line, in yards. Everyone has a position and a speed;
//! a position is worked out when it's needed (x + v (t - t0)), and speeds
//! only change when the engine re-plans, right after everyone has acted.
//! Between re-plans the engine wakes exactly when a distance that matters is
//! crossed: a spell comes into range, a mob reaches melee, a charge lands.

use super::*;

/// What an actor is moving for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Goal {
    /// Fight the target from the distance it wants, closing in when too far.
    Engage,
    /// Back off from the target to this many yards, then fight on (kiting).
    Retreat(f64),
    /// Run from a unit until a time, at a multiple of the usual speed
    /// (a mob fleeing at low health).
    Away { from: usize, until: f64, mult: f64 },
    /// Rush to the unit at charge speed; what lands on arrival waits in
    /// `Sim::charges`.
    Charge(usize),
}

#[derive(Clone, Copy, Debug)]
pub struct Motion {
    /// Position at `t0`, and yards per second since (signed).
    pub x: f64,
    pub t0: f64,
    pub v: f64,
    pub goal: Goal,
    /// Next time a distance that matters to this actor is crossed.
    pub next: f64,
    /// Was closing in at the last re-plan (keeps going until there, so a
    /// target drifting a hair away doesn't start and stop the walk).
    pub closing: bool,
}

impl Default for Motion {
    fn default() -> Self {
        Motion { x: 0.0, t0: 0.0, v: 0.0, goal: Goal::Engage, next: INF, closing: false }
    }
}

/// A mob that joins a fight later (a fleeing mob's friend).
#[derive(Clone, Copy, Debug)]
pub struct Arrival<'w> {
    pub at: f64,
    pub def: &'w EnemyDef,
    pub level: u32,
    pub slot: u64,
    pub x: f64,
    pub target: usize,
    pub scale: f64,
}

const STEP: f64 = 1e-6;

impl<'w> Sim<'w> {
    pub fn pos(&self, a: usize) -> f64 {
        let m = &self.actors[a].mo;
        m.x + m.v * (self.t - m.t0)
    }
    pub fn dist(&self, a: usize, b: usize) -> f64 {
        (self.pos(a) - self.pos(b)).abs()
    }
    /// Put an actor somewhere, standing still.
    pub fn place(&mut self, a: usize, x: f64) {
        let t = self.t;
        let m = &mut self.actors[a].mo;
        m.x = x;
        m.t0 = t;
        m.v = 0.0;
        m.next = INF;
        m.closing = false;
    }
    fn set_v(&mut self, a: usize, v: f64) {
        let old = self.actors[a].mo.v;
        if (old - v).abs() > 1e-12 {
            let x = self.pos(a);
            let t = self.t;
            let m = &mut self.actors[a].mo;
            m.x = x;
            m.t0 = t;
            m.v = v;
            let rushing = matches!(self.actors[a].mo.goal, Goal::Charge(_)) && (old.abs() - v.abs()).abs() > 1e-9;
            if self.log.is_some() && (old == 0.0 || v == 0.0 || old.signum() != v.signum() || rushing) {
                self.note_move(a);
            }
        }
    }

    /// The fight log's line for starting, turning or stopping.
    fn note_move(&mut self, a: usize) {
        let x = &self.actors[a];
        let other = match x.mo.goal {
            Goal::Charge(o) | Goal::Away { from: o, .. } => Some(o),
            _ => x.target,
        };
        let Some(o) = other else { return };
        let d = self.dist(a, o);
        let what = if x.mo.v == 0.0 {
            "stops"
        } else if matches!(x.mo.goal, Goal::Charge(_)) {
            "charges"
        } else if matches!(x.mo.goal, Goal::Away { .. }) || x.mechs & Mechanic::Fear.bit() != 0 {
            "runs"
        } else if self.toward(a, o) * x.mo.v > 0.0 {
            "closes"
        } else {
            "backs"
        };
        self.logf(|s| format!("{} {} ({:.0} yd from {})", s.actors[a].name, match what { "closes" => "closes in", "backs" => "backs off", "runs" => "runs away", w => w }, d, s.actors[o].name));
        self.tracef(|s| TraceEv { amount: d, ..s.tev("move", a, Some(o), what) });
    }
    pub fn stop(&mut self, a: usize) {
        self.set_v(a, 0.0);
    }
    pub fn moving(&self, a: usize) -> bool {
        self.actors[a].mo.v != 0.0
    }

    /// How far an actor's weapon reaches.
    pub fn reach(&self, a: usize) -> f64 {
        self.actors[a].enemy.map(|e| e.reach).filter(|&r| r > 0.0).unwrap_or(self.w.combat.melee_reach)
    }

    /// Yards per second before slows (a fleeing or feared unit's own pace).
    fn base_speed(&self, a: usize) -> f64 {
        let c = &self.w.combat;
        let x = &self.actors[a];
        match x.enemy {
            Some(e) if e.run_speed > 0.0 => e.run_speed,
            Some(_) if x.kind == Kind::Enemy => c.npc_run_speed,
            _ => c.run_speed,
        }
    }

    /// Yards per second right now: 0 when rooted, frozen or stunned; the
    /// strongest slow counts, speed boosts add up.
    pub fn speed(&self, a: usize) -> f64 {
        let x = &self.actors[a];
        // Blessing of Freedom: no roots or slows
        let stuck = Mechanic::Stun.bit() | Mechanic::Incap.bit() | if x.free { 0 } else { Mechanic::Root.bit() | Mechanic::Freeze.bit() };
        if x.mechs & stuck != 0 {
            return 0.0;
        }
        let (mut slow, mut fast) = (0.0f64, x.st[Stat::MoveSpeed as usize]);
        for i in &x.auras {
            slow = slow.max(i.slow);
            fast += i.fast;
        }
        if x.free {
            slow = 0.0;
        }
        self.base_speed(a) * (1.0 - slow).max(0.0) * (1.0 + fast)
    }

    /// The distance an actor fights its target from, and the closest it lets
    /// the target come (0: no limit).
    pub fn stand(&self, a: usize) -> (f64, f64) {
        let c = &self.w.combat;
        let x = &self.actors[a];
        let melee = (self.reach(a) - c.move_margin).max(0.5);
        // a caster mob keeps its distance while it has mana for a bolt and
        // isn't locked out, then comes in to fight
        let casting_on = x.max_res[Res::Mana as usize] <= 0.0
            || (x.res[Res::Mana as usize] >= x.max_res[Res::Mana as usize] / 6.5 && x.lockout <= self.t && x.mechs & Mechanic::Silence.bit() == 0);
        let mut want = match x.enemy {
            Some(e) if x.kind == Kind::Enemy && e.stand_at > 0.0 && casting_on => e.stand_at - c.move_margin,
            _ if x.stand_at > 0.0 => x.stand_at - c.move_margin,
            _ => melee,
        };
        // an opener goes from its own range (Charge from 25 yards, Cheap Shot in melee)
        if let (Some(k), Some(r)) = (x.opening, x.rotation) {
            if let Some(act) = r.opener.get(k) {
                let sp = &self.w.spells[act.spell as usize];
                if sp.range > 0.0 && act.target == SpellTarget::Enemy {
                    let range = self.range_with(sp, &self.spell_mod_vals(a, Some(act.spell), None));
                    want = want.min((range - c.move_margin).max(sp.min_range + 0.5).max(0.5));
                }
            }
        }
        // a better spell that's only out of reach: walk up to its range
        if let Some(r) = x.chase {
            want = want.min((r - c.move_margin).max(0.5));
        }
        let keep = x.rotation.map_or(0.0, |r| r.keep_min);
        (want, keep)
    }

    /// Where a character fights from, worked out once it's spawned: the
    /// rotation's `stand_at` if it has one, melee for a melee rotation, or
    /// else the range most of its known ranged lines share (30 for most
    /// casters, 35 for a hunter's shots).
    pub fn find_stand(&mut self, a: usize) {
        let Some(r) = self.actors[a].rotation else { return };
        let auto_melee = self.actors[a].auto_melee;
        let v = if r.stand_at >= 0.0 {
            // a bow's reach grows with talents (Hawk Eye)
            r.stand_at + if self.actors[a].auto_ranged && r.stand_at > 0.0 { self.actors[a].st[Stat::RangedRange as usize] } else { 0.0 }
        } else if auto_melee {
            0.0
        } else {
            let mut count: Vec<(f64, u32)> = vec![];
            for act in &r.actions {
                let sp = &self.w.spells[act.spell as usize];
                if act.target != SpellTarget::Enemy || sp.range < 20.0 || sp.radius > 0.0 || self.known(a, act.spell).is_none() {
                    continue;
                }
                let range = self.range_with(sp, &self.spell_mod_vals(a, Some(act.spell), None));
                match count.iter_mut().find(|c| (c.0 - range).abs() < 1e-9) {
                    Some(c) => c.1 += 1,
                    None => count.push((range, 1)),
                }
            }
            count.iter().max_by(|x, y| x.1.cmp(&y.1).then(x.0.partial_cmp(&y.0).unwrap())).map_or(0.0, |c| c.0)
        };
        self.actors[a].stand_at = v;
    }

    /// Whether the actor goes after its target at all (a boss that doesn't
    /// attack, or a party member in the healing tests, stays put).
    fn fights(&self, a: usize) -> bool {
        let x = &self.actors[a];
        x.auto_melee || x.auto_ranged || x.rotation.map_or(false, |r| !r.actions.is_empty() || !r.opener.is_empty())
    }

    /// Which way is toward `b` from `a` (+1 or -1).
    fn toward(&self, a: usize, b: usize) -> f64 {
        let d = self.pos(b) - self.pos(a);
        if d.abs() > 1e-9 {
            d.signum()
        } else if self.actors[a].team == 0 {
            1.0
        } else {
            -1.0
        }
    }

    /// Re-plan everyone's movement after they've acted: goals and speeds
    /// first, then when each next needs a look.
    pub fn plan_motion(&mut self) {
        let ids = self.live_snapshot();
        for &a in &ids {
            if self.actors[a].alive {
                self.plan_one(a);
            }
        }
        for &a in &ids {
            let n = if self.actors[a].alive { self.next_cross(a) } else { INF };
            self.actors[a].mo.next = n;
        }
        self.give_back(ids);
    }

    fn plan_one(&mut self, a: usize) {
        let c = &self.w.combat;
        let x = &self.actors[a];
        if self.phase != Phase::Fight || (x.kind == Kind::Enemy && !x.aware) {
            self.stop(a);
            return;
        }
        // a feared unit runs from whoever feared it
        if x.mechs & Mechanic::Fear.bit() != 0 {
            let w = self.w;
            let from = x.auras.iter().find(|i| w.auras[i.def as usize].mechanic == Some(Mechanic::Fear)).map(|i| i.src);
            if let Some(f) = from.filter(|&f| f != a) {
                let v = -self.toward(a, f) * self.base_speed(a) * c.fear_speed;
                self.set_v(a, v);
                return;
            }
        }
        let goal = x.mo.goal;
        let target = x.target.filter(|&t| self.actors[t].alive);
        // fleeing at low health
        if x.kind == Kind::Enemy && !x.fled {
            if let (Some(e), Some(t)) = (x.enemy, target) {
                if e.flee_below > 0.0 && x.hp < e.flee_below * x.max_hp {
                    self.consider_flight(a, e, t);
                    return self.plan_one(a);
                }
            }
        }
        if x.cast.is_some() {
            self.stop(a);
            return;
        }
        let sp = self.speed(a);
        match goal {
            Goal::Charge(to) => {
                if !self.actors[to].alive {
                    self.actors[a].mo.goal = Goal::Engage;
                    self.charges.retain(|c| c.0 != a);
                    return self.plan_one(a);
                }
                if self.dist(a, to) <= self.reach(a) + 1e-6 {
                    self.arrive(a);
                    return self.plan_one(a);
                }
                let v = self.toward(a, to) * c.charge_speed;
                self.set_v(a, v);
            }
            Goal::Away { from, until, mult } => {
                if self.t >= until - 1e-9 || !self.actors[from].alive {
                    self.actors[a].mo.goal = Goal::Engage;
                    return self.plan_one(a);
                }
                let v = -self.toward(a, from) * self.speed(a) * mult;
                self.set_v(a, v);
            }
            Goal::Retreat(to) => match target {
                Some(t) if self.dist(a, t) < to - 1e-6 && sp > 0.0 => {
                    let v = -self.toward(a, t) * sp;
                    self.set_v(a, v);
                }
                _ => {
                    self.actors[a].mo.goal = Goal::Engage;
                    self.stop(a);
                }
            },
            Goal::Engage => {
                let Some(t) = target else {
                    self.stop(a);
                    return;
                };
                if !self.fights(a) || sp <= 0.0 || !self.hostile(a, t) {
                    self.stop(a);
                    self.actors[a].mo.closing = false;
                    return;
                }
                let (want, keep) = self.stand(a);
                let d = self.dist(a, t);
                let closing = self.actors[a].mo.closing;
                if d > want + 1e-6 && (closing || d > want + c.move_margin + 1e-6) {
                    self.actors[a].mo.closing = true;
                    let v = self.toward(a, t) * sp;
                    self.set_v(a, v);
                    return;
                }
                self.actors[a].mo.closing = false;
                // too close: back off, if there's any getting away
                if keep > 0.0 && d < keep - 1e-6 {
                    let tt = &self.actors[t];
                    let chased = tt.target == Some(a) && self.fights(t);
                    if !chased || sp > self.speed(t) + 1e-9 {
                        // far enough to get a few shots off before it closes again
                        self.actors[a].mo.goal = Goal::Retreat((keep + 6.0).min(want).max(keep + c.move_margin));
                        return self.plan_one(a);
                    }
                }
                self.stop(a);
            }
        }
    }

    /// A mob at low health decides (once, on its spawn dice) whether to run.
    fn consider_flight(&mut self, a: usize, e: &'w EnemyDef, from: usize) {
        self.actors[a].fled = true;
        let on = self.w.opt_index("mobs_flee").map_or(1.0, |i| self.opts[i as usize]);
        let dice = self.actors[a].dice;
        if on <= 0.0 || dice[0] >= e.flee_chance || self.actors[a].no_flee {
            return;
        }
        let (lo, hi) = e.flee_time;
        let until = self.t + lo + (hi - lo) * dice[1];
        let mult = self.w.opt_index("flee_speed").map_or(0.75, |i| self.opts[i as usize]);
        self.actors[a].mo.goal = Goal::Away { from, until, mult };
        self.stop(a);
        self.tracef(|s| s.tev("flee", a, Some(from), ""));
        self.logf(|s| format!("{} runs away", s.actors[a].name));
        // sometimes it comes back with a friend
        if dice[2] < e.flee_add_chance {
            let def = e.flee_add.as_ref().and_then(|id| self.w.enemies.get(id)).unwrap_or(e);
            let dir = -self.toward(a, from);
            let x = self.pos(a) + dir * 15.0;
            let slot = self.actors[a].slot * 8 + 7;
            let (level, scale) = (self.actors[a].level, self.actors[a].dmg_scale);
            self.arrivals.push(Arrival { at: self.t + 1.5, def, level, slot, x, target: from, scale });
        }
    }

    /// Mobs due to join now.
    pub fn bring_arrivals(&mut self) {
        while let Some(k) = self.arrivals.iter().position(|r| r.at <= self.t + 1e-9) {
            let r = self.arrivals.swap_remove(k);
            if !self.actors[r.target].alive {
                continue;
            }
            let team = 1 - self.actors[r.target].team.min(1);
            let id = self.spawn_enemy(r.def, r.level, team, r.slot, r.scale);
            self.place(id, r.x);
            self.actors[id].target = Some(r.target);
            self.actors[id].in_combat = true;
            self.add_threat(id, r.target, 1.0);
            self.start_swings();
            self.emit(id, EventKind::CombatStart, Ev::default(), 0);
            self.tracef(|s| s.tev("joins", id, Some(r.target), ""));
            self.logf(|s| format!("{} joins the fight", s.actors[id].name));
        }
    }

    /// A charge lands: what it does (the stun, the Rage) happens now.
    fn arrive(&mut self, a: usize) {
        self.actors[a].mo.goal = Goal::Engage;
        self.stop(a);
        if let Some(k) = self.charges.iter().position(|c| c.0 == a) {
            let (_, _, effs, cx) = self.charges.swap_remove(k);
            self.run_effects(effs, cx);
        }
    }

    /// Start a charge at the target.
    pub fn charge(&mut self, effs: &'w [Effect], cx: Cx<'w>) {
        let Some(t) = cx.tgt else { return };
        self.charges.retain(|c| c.0 != cx.me);
        self.charges.push((cx.me, t, effs, cx));
        self.actors[cx.me].mo.goal = Goal::Charge(t);
        self.actors[cx.me].mo.closing = false;
    }

    /// When the next distance that matters to `a` is crossed: its spell
    /// ranges, where it wants to stand, melee reach, a charge's end, an
    /// unaware mob's aggro radius.
    fn next_cross(&self, a: usize) -> f64 {
        let x = &self.actors[a];
        let c = &self.w.combat;
        let mut n = INF;
        if x.kind == Kind::Enemy && !x.aware {
            for &p in &self.live {
                let y = &self.actors[p];
                if y.alive && (y.kind == Kind::Player || (y.owner.is_some() && y.kind != Kind::Totem)) && y.team != x.team && !y.stealthed {
                    n = n.min(self.cross(a, p, &[c.aggro_radius]));
                }
            }
            return n;
        }
        let other = match x.mo.goal {
            Goal::Charge(to) => Some(to),
            Goal::Away { from, until, .. } => {
                n = n.min(until);
                Some(from)
            }
            _ => x.target.filter(|&t| self.actors[t].alive),
        };
        let Some(o) = other else { return n };
        let (want, keep) = self.stand(a);
        let mut thr: [f64; 24] = [0.0; 24];
        let mut k = 0;
        for v in [want, want + c.move_margin, keep, keep + c.move_margin, self.reach(a)] {
            if v > 0.0 {
                thr[k] = v;
                k += 1;
            }
        }
        if let Goal::Retreat(to) = x.mo.goal {
            thr[k] = to;
            k += 1;
        }
        if let Some(r) = x.rotation {
            for &v in r.ranges.iter().take(thr.len() - k) {
                thr[k] = v;
                k += 1;
            }
        }
        n.min(self.cross(a, o, &thr[..k]))
    }

    /// The first time the distance between two actors reaches one of the
    /// given values (each way), at their current speeds.
    fn cross(&self, a: usize, b: usize, thr: &[f64]) -> f64 {
        let r = self.pos(b) - self.pos(a);
        let rv = self.actors[b].mo.v - self.actors[a].mo.v;
        if rv.abs() < 1e-12 {
            return INF;
        }
        let mut best = INF;
        for &d in thr {
            for s in [d, -d] {
                let tau = (s - r) / rv;
                if tau > STEP && tau < best {
                    best = tau;
                }
            }
        }
        if best == INF {
            INF
        } else {
            self.t + best
        }
    }

    /// An unaware mob notices: it and its whole pack join the fight, and so
    /// does whoever it turns on.
    pub fn wake(&mut self, a: usize, by: Option<usize>) {
        if self.actors[a].aware || self.actors[a].kind != Kind::Enemy {
            return;
        }
        let (pack, team) = (self.actors[a].pack, self.actors[a].team);
        let ids = self.live_snapshot();
        for &m in &ids {
            let y = &self.actors[m];
            if !y.alive || y.aware || y.kind != Kind::Enemy || y.pack != pack || y.team != team {
                continue;
            }
            self.actors[m].aware = true;
            self.actors[m].in_combat = true;
            // the one who woke it is who it goes for first
            let foe = by.filter(|&b| self.hostile(m, b)).or_else(|| {
                self.live.iter().copied().find(|&p| self.actors[p].alive && self.actors[p].kind == Kind::Player && self.actors[p].team != team)
            });
            if let Some(f) = foe {
                self.add_threat(m, f, 1.0);
                self.actors[m].target = Some(f);
            }
            self.emit(m, EventKind::CombatStart, Ev::default(), 0);
            self.tracef(|s| s.tev("notice", m, foe, ""));
        }
        // the players (and pets) it turns on are in the fight from now
        for &p in &ids {
            let y = &self.actors[p];
            if y.alive && y.team != team && !y.in_combat && y.aware {
                self.actors[p].in_combat = true;
                if self.woke_at.is_none() {
                    self.woke_at = Some(self.t);
                    self.combat_start = self.t;
                }
                self.emit(p, EventKind::CombatStart, Ev::default(), 0);
                if self.actors[p].totems.iter().any(|t| t.is_some()) {
                    self.bring_totems(p);
                }
            }
        }
        self.give_back(ids);
    }

    /// Unaware mobs that someone has walked up to.
    pub fn check_aggro(&mut self) {
        let r = self.w.combat.aggro_radius;
        let mut hit: Option<(usize, usize)> = None;
        for &m in &self.live {
            let y = &self.actors[m];
            if !y.alive || y.aware || y.kind != Kind::Enemy {
                continue;
            }
            for &p in &self.live {
                let z = &self.actors[p];
                if z.alive && z.team != y.team && !z.stealthed && (z.kind == Kind::Player || (z.owner.is_some() && z.kind != Kind::Totem)) && self.dist(m, p) <= r + 1e-6 {
                    hit = Some((m, p));
                    break;
                }
            }
            if hit.is_some() {
                break;
            }
        }
        if let Some((m, p)) = hit {
            self.wake(m, Some(p));
            self.check_aggro();
        }
    }

    /// Whether `a` hits `d` from behind: `d` is facing someone else, or
    /// hasn't noticed anyone yet.
    pub fn behind(&self, a: usize, d: usize) -> bool {
        let x = &self.actors[a];
        let y = &self.actors[d];
        x.behind || !y.aware || y.distracted || y.target.map_or(false, |t| t != a)
    }

    /// Whether `a` can reach `target` with a spell (range and dead zone).
    pub fn in_range(&self, a: usize, spell: SpellId, target: usize, sm: &SmVals) -> bool {
        if target == a {
            return true;
        }
        let sp = &self.w.spells[spell as usize];
        let friendly = self.actors[target].team == self.actors[a].team;
        let (range, min) = if friendly {
            (if sp.range > 0.0 { self.range_with(sp, sm) } else { 40.0 }, 0.0)
        } else {
            (self.range_with(sp, sm), sp.min_range)
        };
        if range <= 0.0 {
            return true;
        }
        // a weapon strike reaches as far as the striker's arm
        let range = if !friendly && range <= self.w.combat.melee_reach + 1e-9 { self.reach(a) } else { range };
        let d = self.dist(a, target);
        d <= range + 1e-6 && d >= min - 1e-6
    }

    /// A spell's range for this caster (talents can lengthen it); melee
    /// strikes reach as far as the arm.
    pub fn range_with(&self, sp: &SpellDef, sm: &SmVals) -> f64 {
        // an area around you reaches as far as its radius
        if sp.radius > 0.0 && sp.range <= sp.radius + 1e-9 {
            return self.radius_with(sp, sm);
        }
        if sp.range <= self.w.combat.melee_reach + 1e-9 {
            return sp.range;
        }
        (sp.range + sm.range_add) * sm.range_mul
    }

    /// How wide a spell's area is for this caster.
    pub fn radius_with(&self, sp: &SpellDef, sm: &SmVals) -> f64 {
        if sp.radius <= 0.0 {
            return 0.0;
        }
        (sp.radius + sm.radius_add) * sm.radius_mul
    }
}
