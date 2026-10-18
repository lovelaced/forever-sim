//! Casting, effects, auras and procs.

use super::*;

#[derive(Clone, Debug)]
pub struct Plan {
    pub spell: SpellId,
    pub rank: Option<usize>,
    pub target: usize,
    pub cast_time: f64,
    pub sm: SmVals,
}

impl<'w> Sim<'w> {
    /// Combine every spell modifier that applies to a spell for an actor.
    pub fn spell_mod_vals(&self, a: usize, spell: Option<SpellId>, school: Option<School>) -> SmVals {
        let mut v = SmVals::default();
        let Some(spell) = spell else { return v };
        let w = self.w;
        let school = school.unwrap_or(w.spells[spell as usize].school);
        let x = &self.actors[a];
        let cx0 = Cx::new(a, x.target, "");
        let mut apply = |m: &SpellMod, cx: &Cx<'w>, s: &Sim<'w>| {
            if !m.applies(spell, school) {
                return;
            }
            let val = s.ev(&m.value, cx);
            let (target, base_is_one) = match m.prop {
                SpellProp::Cost => (if m.op == ModOp::Add { &mut v.cost_add } else { &mut v.cost_mul }, false),
                SpellProp::CastTime => (if m.op == ModOp::Add { &mut v.cast_add } else { &mut v.cast_mul }, false),
                SpellProp::Cooldown => (if m.op == ModOp::Add { &mut v.cd_add } else { &mut v.cd_mul }, false),
                SpellProp::Damage => (&mut v.dmg, true),
                SpellProp::Heal => (&mut v.heal, true),
                SpellProp::Crit => (&mut v.crit, false),
                SpellProp::CritBonus => (&mut v.crit_bonus, false),
                SpellProp::Threat => (&mut v.threat, true),
                SpellProp::Gcd => (&mut v.gcd_add, false),
                SpellProp::Range => (if m.op == ModOp::Add { &mut v.range_add } else { &mut v.range_mul }, false),
                SpellProp::Radius => (if m.op == ModOp::Add { &mut v.radius_add } else { &mut v.radius_mul }, false),
            };
            match (m.op, base_is_one) {
                (ModOp::Mul, _) => *target *= val,
                (ModOp::Add, _) => *target += val,
            }
        };
        for &pi in &x.sm_passives {
            let p = &x.passives[pi as usize];
            if !p.spell_mods.iter().any(|m| m.applies(spell, school)) {
                continue;
            }
            if let Some(c) = &p.cond {
                if self.ev(c, &cx0) == 0.0 {
                    continue;
                }
            }
            for m in &p.spell_mods {
                apply(m, &cx0, self);
            }
        }
        for inst in &x.auras {
            let def = &w.auras[inst.def as usize];
            if def.spell_mods.is_empty() {
                continue;
            }
            let mut cx = cx0;
            cx.rank = inst.rank;
            cx.stacks = inst.stacks as f64;
            for m in &def.spell_mods {
                apply(m, &cx, self);
            }
        }
        v
    }

    fn costs(&self, a: usize, spell: SpellId, rank: Option<usize>, sm: &SmVals, tgt: usize) -> Vec<(Res, f64)> {
        self.cost_iter(a, spell, rank, sm, tgt).collect()
    }

    fn cost_iter<'s>(&'s self, a: usize, spell: SpellId, rank: Option<usize>, sm: &'s SmVals, tgt: usize) -> impl Iterator<Item = (Res, f64)> + 's {
        let sp = &self.w.spells[spell as usize];
        let mut cx = Cx::new(a, Some(tgt), "");
        cx.rank = rank.map(|r| (spell, r));
        cx.spell = Some(spell);
        let cm = self.actors[a].st[Stat::CostMult as usize];
        sp.cost.iter().enumerate().map(move |(i, (r, e))| {
            let mut c = self.ev(e, &cx) * sm.cost_mul;
            if i == 0 {
                c += sm.cost_add;
            }
            if *r != Res::Hp {
                c *= cm;
            }
            (*r, c.max(0.0))
        })
    }

    /// Whether the caster can pay a cast's costs (without listing them).
    fn affordable(&self, a: usize, spell: SpellId, rank: Option<usize>, sm: &SmVals, tgt: usize) -> bool {
        let x = &self.actors[a];
        self.cost_iter(a, spell, rank, sm, tgt).all(|(r, c)| if r == Res::Hp { x.hp > c } else { x.res[r as usize] + 1e-6 >= c })
    }

    fn afford(&self, a: usize, costs: &[(Res, f64)]) -> bool {
        let x = &self.actors[a];
        costs.iter().all(|(r, c)| {
            if *r == Res::Hp {
                x.hp > *c
            } else {
                x.res[*r as usize] + 1e-6 >= *c
            }
        })
    }

    fn pay(&mut self, a: usize, costs: &[(Res, f64)]) {
        let t = self.t;
        let x = &mut self.actors[a];
        for (r, c) in costs {
            if *r == Res::Hp {
                x.hp = (x.hp - c).max(1.0);
            } else {
                x.res[*r as usize] = (x.res[*r as usize] - c).max(0.0);
                if *r == Res::Mana && *c > 0.0 {
                    x.last_mana_spend = t;
                }
            }
        }
    }

    fn forms_ok(&self, a: usize, req: &FormReq) -> bool {
        let f = self.actors[a].form;
        match req {
            FormReq::Any => true,
            FormReq::NoForm => f.is_none(),
            FormReq::Forms(list) => match f {
                Some(x) => list.contains(&x),
                None => list.contains(&u16::MAX),
            },
        }
    }

    fn resolve_target(&self, a: usize, kind: SpellTarget) -> Option<usize> {
        let x = &self.actors[a];
        match kind {
            SpellTarget::Enemy => x.target.filter(|&t| self.actors[t].alive && self.hostile(a, t)),
            SpellTarget::Me | SpellTarget::Friendly => Some(a),
            SpellTarget::Pet => x.pets.iter().copied().find(|&p| self.actors[p].alive),
            SpellTarget::Tank => Some(self.tank_of(a)).filter(|&t| self.actors[t].alive),
            SpellTarget::Lowest => self.hurt_order(a).first().copied(),
            SpellTarget::AnyEnemy(p) | SpellTarget::OtherEnemy(p) => self.enemy_order(a, kind, p, None).first().copied(),
        }
    }

    /// The enemies a multi-target line tries, in its pick's order: live,
    /// fighting, not your target for `other_enemy`.
    fn enemy_order(&self, a: usize, kind: SpellTarget, pick: Pick, spell: Option<SpellId>) -> Vec<usize> {
        let me = &self.actors[a];
        let mut v: Vec<usize> = self.live.iter().copied().filter(|&i| {
            let y = &self.actors[i];
            y.alive && !y.evaded && y.team != me.team && !(kind != SpellTarget::AnyEnemy(pick) && me.target == Some(i))
        }).collect();
        let d = |i: usize| self.dist(a, i);
        let hp = |i: usize| self.actors[i].hp_pct(self.t);
        let mine = |i: usize| spell.map_or(false, |s| {
            let mut auras = vec![];
            crate::world::applied_auras(&self.w.spells[s as usize].effects, &mut auras);
            self.actors[i].auras.iter().any(|x| x.src == a && auras.contains(&x.def))
        });
        let cmp = |x: &usize, y: &usize| {
            let by = match pick {
                Pick::Current => (me.target != Some(*x)).cmp(&(me.target != Some(*y))).then(d(*x).partial_cmp(&d(*y)).unwrap()),
                Pick::Nearest => d(*x).partial_cmp(&d(*y)).unwrap(),
                Pick::LowestHp => hp(*x).partial_cmp(&hp(*y)).unwrap(),
                Pick::HighestHp => hp(*y).partial_cmp(&hp(*x)).unwrap(),
                Pick::Undotted => mine(*x).cmp(&mine(*y)).then(d(*x).partial_cmp(&d(*y)).unwrap()),
            };
            by.then(x.cmp(y))
        };
        v.sort_by(cmp);
        v
    }

    /// Everything that must hold for a spell to be cast right now.
    pub fn can_cast(&self, a: usize, spell: SpellId, kind: SpellTarget, relaxed: bool) -> Option<Plan> {
        let target = self.resolve_target(a, kind)?;
        self.can_cast_at(a, spell, kind, target, relaxed, None)
    }

    /// The plan for a rotation line: its spell can be cast and its condition
    /// holds. A line on the most hurt ally tries each ally in turn, most hurt
    /// first, and takes the first one the line accepts. `order` keeps that
    /// list between the lines of one decision.
    pub fn line_plan(&self, a: usize, act: &Action, relaxed: bool, order: &mut Option<Vec<usize>>) -> Option<Plan> {
        if self.phase == Phase::Prep && self.in_place(a, act.spell) {
            return None;
        }
        let known = self.ready(a, act.spell, relaxed)?;
        let mut sm = None;
        if act.target == SpellTarget::Lowest {
            let order = order.get_or_insert_with(|| self.hurt_order(a));
            return order.iter().find_map(|&t| self.plan_on(a, act, t, relaxed, known, &mut sm));
        }
        if let SpellTarget::AnyEnemy(p) | SpellTarget::OtherEnemy(p) = act.target {
            let cands = self.enemy_order(a, act.target, p, Some(act.spell));
            return cands.into_iter().find_map(|t| self.plan_on(a, act, t, relaxed, known, &mut sm));
        }
        let t = self.resolve_target(a, act.target)?;
        self.plan_on(a, act, t, relaxed, known, &mut sm)
    }

    /// One line on one target. The condition goes first (it's the cheapest
    /// check, and while nobody needs healing it's the one that fails), unless
    /// it reads the numbers of a rank still to be fitted.
    fn plan_on(&self, a: usize, act: &Action, target: usize, relaxed: bool, known: Option<usize>, sm: &mut Option<SmVals>) -> Option<Plan> {
        let early = !(act.rank == RankPick::Fit && act.cond_rank);
        if early {
            if let Some(c) = &act.cond {
                let cx = self.action_cx(a, &Plan { spell: act.spell, rank: known, target, cast_time: 0.0, sm: SmVals::default() });
                if self.ev(c, &cx) == 0.0 {
                    return None;
                }
            }
        }
        let sm = *sm.get_or_insert_with(|| self.spell_mod_vals(a, Some(act.spell), None));
        let mut plan = self.plan_at(a, act.spell, act.target, target, relaxed, known, sm)?;
        if act.rank == RankPick::Fit {
            let r = self.fit_rank(a, &plan);
            if r.is_some() && r != plan.rank {
                plan = self.plan_at(a, act.spell, act.target, target, relaxed, r, sm)?;
            }
        }
        if !early {
            if let Some(c) = &act.cond {
                let cx = self.action_cx(a, &plan);
                if self.ev(c, &cx) == 0.0 {
                    return None;
                }
            }
        }
        Some(plan)
    }

    /// Before a fight: whether a spell would only redo what's already there
    /// (its buffs are up with a minute or more left, or half of a short
    /// one's length; its pet is out).
    fn in_place(&self, a: usize, spell: SpellId) -> bool {
        let x = &self.actors[a];
        let mut any = false;
        for e in &self.w.spells[spell as usize].effects {
            match e {
                Effect::ApplyAura { who: TargetSel::Me | TargetSel::Ally | TargetSel::Party, aura, .. } => {
                    any = true;
                    // a minute left, or half of a short one (a 30 sec seal)
                    let fine = |i: &AuraInst| (i.expires - self.t) >= (0.5 * (i.expires - i.applied)).min(60.0) - EPS;
                    if !x.auras.iter().any(|i| i.def == *aura && fine(i)) {
                        return false;
                    }
                }
                Effect::Summon(pd) => {
                    any = true;
                    if !x.pets.iter().any(|&p| self.actors[p].alive && self.actors[p].kind == Kind::Pet(*pd)) {
                        return false;
                    }
                }
                _ => {}
            }
        }
        any
    }

    /// The smallest known rank whose heal covers the target's missing
    /// health, or the highest when none does.
    fn fit_rank(&self, a: usize, plan: &Plan) -> Option<usize> {
        let top = plan.rank?;
        let sp = &self.w.spells[plan.spell as usize];
        let t = &self.actors[plan.target];
        let need = t.max_hp - t.hp;
        let mut cx = self.action_cx(a, plan);
        for r in 0..top {
            cx.rank = Some((plan.spell, r));
            if self.heal_estimate(&cx, &sp.effects)? >= need {
                return Some(r);
            }
        }
        Some(top)
    }

    /// `can_cast` on a chosen target, optionally with a lower rank than the
    /// highest known.
    pub fn can_cast_at(&self, a: usize, spell: SpellId, kind: SpellTarget, target: usize, relaxed: bool, rank: Option<usize>) -> Option<Plan> {
        let known = self.ready(a, spell, relaxed)?;
        let sm = self.spell_mod_vals(a, Some(spell), None);
        self.plan_at(a, spell, kind, target, relaxed, known.map(|k| rank.unwrap_or(k).min(k)), sm)
    }

    /// What a cast needs of the caster alone, whoever it's on: the spell is
    /// known, nothing (stun, silence, form) stops it, and it's off its
    /// cooldown. Returns the highest known rank (None for a spell without ranks).
    fn ready(&self, a: usize, spell: SpellId, relaxed: bool) -> Option<Option<usize>> {
        let w = self.w;
        let x = &self.actors[a];
        if !x.alive || x.cast.is_some() {
            return None;
        }
        let known = self.known(a, spell)?;
        let rank = if known == usize::MAX { None } else { Some(known) };
        let sp = &w.spells[spell as usize];
        let hard = x.mechs & Mechanic::HARD;
        if hard != 0 && hard & !sp.usable_while.iter().fold(0, |m, x| m | x.bit()) != 0 {
            return None;
        }
        if sp.school != School::Physical && (x.lockout > self.t || x.has_mech(w, Mechanic::Silence)) {
            return None;
        }
        if !relaxed {
            if !sp.off_gcd && !sp.next_swing && x.gcd_end > self.t + EPS {
                return None;
            }
            if self.phase == Phase::Fight && x.react.is_some() && x.react_until > self.t + EPS {
                return None;
            }
            if x.cd_ready(self.cd_key(spell)) > self.t + EPS {
                return None;
            }
        }
        if !self.forms_ok(a, &sp.forms) {
            return None;
        }
        if sp.next_swing && x.queued == Some(spell) {
            return None;
        }
        Some(rank)
    }

    /// The rest of `can_cast` for one target and rank: what the spell
    /// requires there, its cost, and its cast time.
    #[allow(clippy::too_many_arguments)]
    fn plan_at(&self, a: usize, spell: SpellId, kind: SpellTarget, target: usize, relaxed: bool, rank: Option<usize>, sm: SmVals) -> Option<Plan> {
        let x = &self.actors[a];
        let sp = &self.w.spells[spell as usize];
        let mut cx = Cx::new(a, Some(target), sp.name.as_str());
        if matches!(kind, SpellTarget::Me | SpellTarget::Tank | SpellTarget::Lowest) {
            cx.ally = Some(target).filter(|&t| t != a);
            cx.tgt = x.target.or(Some(a));
        }
        cx.rank = rank.map(|r| (spell, r));
        cx.spell = Some(spell);
        // a move needs legs, and one move at a time
        if self.phase == Phase::Fight && sp.moves && (self.speed(a) <= 0.0 || x.mo.goal != Goal::Engage) {
            return None;
        }
        if let Some(r) = &sp.requires {
            if self.ev(r, &cx) == 0.0 {
                return None;
            }
        }
        if !relaxed && !self.affordable(a, spell, rank, &sm, target) {
            return None;
        }
        // a burst from your totem needs the totem, and an enemy near it
        if let Some(e) = sp.from_totem {
            let Some(t) = self.totem_of(a, e) else { return None };
            let (team, r) = (x.team, self.radius_with(sp, &sm));
            if self.phase == Phase::Fight && !self.live.iter().any(|&i| { let y = &self.actors[i]; y.alive && y.team != team && !y.is_pet() && self.dist(t, i) <= r + 1e-6 }) {
                return None;
            }
        }
        // an area around you needs an enemy in it
        if self.phase == Phase::Fight && sp.radius > 0.0 && sp.range <= sp.radius + 1e-9 && target == a && sp.from_totem.is_none() {
            let team = x.team;
            let r = self.radius_with(sp, &sm);
            if !self.live.iter().any(|&i| { let y = &self.actors[i]; y.alive && y.team != team && !y.is_pet() && self.dist(a, i) <= r + 1e-6 }) {
                return None;
            }
        }
        if self.phase == Phase::Fight && !self.in_range(a, spell, target, &sm) {
            // the first line held back only by distance: worth walking up for
            // (a ranged spell at your target; an area around you only when
            // the target is busy with someone else, like a tanked boss)
            let c = &self.w.combat;
            let around_me = sp.radius > 0.0 && sp.range <= sp.radius + 1e-9;
            let busy = self.actors[target].target.map_or(false, |t| t != a);
            if self.chase_probe.get() == INF && x.target == Some(target) && sp.range > c.melee_reach + 1e-9 && (!around_me || busy) && !sp.moves {
                self.chase_probe.set(self.range_with(sp, &sm));
            }
            return None;
        }
        let cast_time = if sp.channel.is_some() || relaxed {
            0.0
        } else {
            ((self.ev(&sp.cast_time, &cx) + sm.cast_add) * sm.cast_mul / x.st[Stat::CastHaste as usize].max(0.05)).max(0.0)
        };
        Some(Plan { spell, rank, target, cast_time, sm })
    }

    fn action_cx(&self, a: usize, plan: &Plan) -> Cx<'w> {
        let sp = &self.w.spells[plan.spell as usize];
        let mut cx = Cx::new(a, Some(plan.target), sp.name.as_str());
        if sp.target == SpellTarget::Me {
            cx.ally = Some(plan.target).filter(|&t| t != a);
            cx.tgt = self.actors[a].target.or(Some(a));
        }
        cx.rank = plan.rank.map(|r| (plan.spell, r));
        cx.spell = Some(plan.spell);
        cx.sm = plan.sm;
        cx.at = sp.from_totem.and_then(|e| self.totem_of(a, e));
        cx
    }

    /// Choose and start the first usable action in the rotation.
    pub fn act(&mut self, a: usize) -> bool {
        let x = &self.actors[a];
        if !x.alive || x.cast.is_some() {
            return false;
        }
        let Some(rot) = x.rotation else { return false };
        // a mob that hasn't noticed anyone, or is running away, does nothing
        if !x.aware || matches!(x.mo.goal, Goal::Away { .. }) || self.held_back(a) {
            return false;
        }
        if self.phase == Phase::Fight && x.opening.is_some() {
            if let Some(cast) = self.open_step(a, rot) {
                return cast;
            }
        }
        if self.phase == Phase::Prep {
            return self.prep_line(a, rot);
        }
        let list = self.phase_list(rot);
        let mut order = None;
        // during the global cooldown only off-GCD and next-swing lines can go
        let busy = self.actors[a].gcd_end > self.t + EPS;
        let fight = self.phase == Phase::Fight && self.reacting(a);
        self.chase_probe.set(INF);
        // in a group, close to pulling the tank's mob: hold the damage
        let hold = self.phase == Phase::Fight && self.holds_back(a);
        let mut slipped: Option<(usize, Plan)> = None;
        // during the global cooldown only the off-GCD lines need a look
        let n = if busy && self.phase == Phase::Fight { rot.off_gcd.len() } else { list.len() };
        for i in 0..n {
            let k = if busy && self.phase == Phase::Fight { rot.off_gcd[i] as usize } else { i };
            let act = &list[k];
            let sp = &self.w.spells[act.spell as usize];
            let on_gcd = !sp.off_gcd && !sp.next_swing;
            if (busy && on_gcd) || (hold && sp.harm && act.target == SpellTarget::Enemy) {
                continue;
            }
            let Some(plan) = self.line_plan(a, act, false, &mut order) else {
                if fight {
                    self.clear_due(a, k);
                }
                continue;
            };
            // a typical player gets round to cooldowns and upkeep a little late
            if fight && !self.due_now(a, k, act, &plan) {
                continue;
            }
            if sp.auto_repeat && self.actors[a].wanding == Some(act.spell) {
                // the chosen action is the wand that is already shooting: keep it going
                self.note_chase(a);
                return false;
            }
            // a better line is only out of reach: walk up rather than start a
            // cast that roots you here (instants still go on the move)
            if self.phase == Phase::Fight && self.chase_probe.get() < INF && (plan.cast_time > 0.0 || sp.channel.is_some()) && self.actors[a].kind != Kind::Enemy {
                self.note_chase(a);
                return false;
            }
            // the target is held in place but could still hit you: step out of
            // its reach before standing still to cast (instants still go)
            if self.phase == Phase::Fight && (plan.cast_time > 0.0 || sp.channel.is_some()) {
                match self.make_distance(a, sp, &plan) {
                    Some(Ok(to)) => {
                        let m = &mut self.actors[a].mo;
                        if !matches!(m.goal, Goal::Retreat(_)) {
                            m.goal = Goal::Retreat(to);
                            m.closing = false;
                        }
                        continue;
                    }
                    // still taking in that it's held: no cast in its reach meanwhile
                    Some(Err(until)) => {
                        let x = &mut self.actors[a];
                        x.react_until = x.react_until.max(until);
                        self.note_chase(a);
                        return false;
                    }
                    None => {}
                }
            }
            // ...and now and then presses the next ability down instead of the best
            if fight && on_gcd && slipped.is_none() {
                let p = self.actors[a].react.map_or(0.0, |r| r.slip);
                if self.slips(a, p) {
                    slipped = Some((k, plan));
                    continue;
                }
            }
            self.note_chase(a);
            self.take_line(a, k, plan);
            return true;
        }
        self.note_chase(a);
        if let Some((k, plan)) = slipped {
            // nothing else to press: the best move after all
            self.take_line(a, k, plan);
            return true;
        }
        false
    }

    /// Whether to back off before this cast, and how far: the target is
    /// rooted or frozen for a while yet (it can't follow, but it still hits
    /// whoever stands within its reach; a stunned one can't, so you heal in
    /// place) (a typical player first takes a reaction to
    /// see it held, and backs off less far than an expert). Weapon strikes
    /// cast in melee (Slam) never do.
    /// Ok(yards) to back off now, Err(time) to wait until it has seen the
    /// hold.
    fn make_distance(&self, a: usize, sp: &SpellDef, plan: &Plan) -> Option<Result<f64, f64>> {
        let x = &self.actors[a];
        if x.kind != Kind::Player || self.speed(a) <= 0.0 {
            return None;
        }
        if plan.target != a && self.actors[plan.target].team != x.team && self.range_with(sp, &plan.sm) <= self.w.combat.melee_reach + 1e-9 {
            return None;
        }
        let t = x.target?;
        if !self.actors[t].alive || !self.hostile(a, t) {
            return None;
        }
        if let Goal::Retreat(to) = x.mo.goal {
            return Some(Ok(to));
        }
        if self.dist(a, t) > self.reach(t) + 1.0 {
            return None;
        }
        let w = self.w;
        let (since, until) = self.actors[t].auras.iter()
            .filter(|i| matches!(w.auras[i.def as usize].mechanic, Some(Mechanic::Root | Mechanic::Freeze)))
            .fold((INF, 0.0f64), |(s, u), i| (s.min(i.applied), u.max(i.expires)));
        if until - self.t < 1.5 {
            return None;
        }
        let r = x.react.unwrap_or_default();
        if self.t - since < r.base - EPS {
            return Some(Err(since + r.base));
        }
        let to = if r.kite > 0.0 { r.kite } else { 15.0 };
        Some(Ok(if x.stand_at > 0.0 { to.min(x.stand_at.max(12.0)) } else { to }))
    }

    /// Remember a better line that was only out of reach, so the movement
    /// planner walks up to its range.
    fn note_chase(&mut self, a: usize) {
        if self.phase != Phase::Fight || self.actors[a].kind == Kind::Enemy {
            return;
        }
        let r = self.chase_probe.get();
        self.actors[a].chase = (r < INF).then_some(r);
    }

    /// A player in a group (the boss test, with its tank) who is near
    /// pulling the mob off whoever it's on (assumption `threat_margin`).
    fn holds_back(&self, a: usize) -> bool {
        let x = &self.actors[a];
        if self.tank.is_none() || self.tank == Some(a) || x.kind != Kind::Player {
            return false;
        }
        let Some(t) = x.target else { return false };
        if self.actors[t].target == Some(a) {
            return false;
        }
        let margin = self.threat_margin_i.map_or(0.9, |i| self.opt(a, i as usize));
        margin > 0.0 && self.threat_pct(a) >= margin
    }

    fn take_line(&mut self, a: usize, k: usize, plan: Plan) {
        if self.actors[a].owner.is_none() {
            let m = &mut self.metrics[a];
            let v = match self.phase {
                Phase::Fight => &mut m.lines,
                Phase::Rest => &mut m.rest_lines,
                Phase::Prep => &mut m.precombat_lines,
            };
            if v.len() <= k {
                v.resize(k + 1, 0.0);
            }
            v[k] += 1.0;
        }
        self.clear_due(a, k);
        self.begin(a, plan);
    }

    fn clear_due(&mut self, a: usize, k: usize) {
        if let Some(d) = self.actors[a].due.get_mut(k) {
            *d = INF;
        }
    }

    /// Whether a typical player has got round to a wanted line: long
    /// cooldowns (a minute or more) and upkeep wait a moment from when they
    /// were first wanted; everything else goes at once.
    fn due_now(&mut self, a: usize, k: usize, act: &Action, plan: &Plan) -> bool {
        let Some(r) = self.actors[a].react else { return true };
        let late = if act.upkeep {
            r.lapse
        } else if r.late_cd > 0.0 {
            let sp = &self.w.spells[act.spell as usize];
            let cx = self.action_cx(a, plan);
            let cd = (self.ev(&sp.cooldown, &cx) + plan.sm.cd_add) * plan.sm.cd_mul;
            if cd >= 60.0 { r.late_cd } else { 0.0 }
        } else {
            0.0
        };
        if late <= 0.0 {
            return true;
        }
        if self.actors[a].due.len() <= k {
            self.actors[a].due.resize(k + 1, INF);
        }
        if self.actors[a].due[k] == INF {
            let d = self.t + self.delay(a, late);
            self.actors[a].due[k] = d;
        }
        self.actors[a].due[k] <= self.t + EPS
    }

    /// The rotation list for the current phase.
    pub fn phase_list(&self, rot: &'w Rotation) -> &'w [Action] {
        match self.phase {
            Phase::Fight => &rot.actions,
            Phase::Rest => &rot.rest,
            Phase::Prep => &rot.precombat,
        }
    }

    /// Done for now: not casting, off the global cooldown, and nothing in
    /// the phase's list (rest or precombat) wants casting.
    pub fn idle(&self, a: usize) -> bool {
        let x = &self.actors[a];
        if x.cast.is_some() || x.gcd_end > self.t + EPS {
            return false;
        }
        if self.phase == Phase::Prep {
            return x.prep_step.is_none();
        }
        let Some(rot) = x.rotation else { return true };
        let mut order = None;
        !self.phase_list(rot).iter().any(|act| self.line_plan(a, act, false, &mut order).is_some())
    }

    /// Buff up before a fight: the precombat lists of one or two players,
    /// cast for real (costs, cast times, global cooldowns, cooldowns), each
    /// line once and in order (a summon, then sacrificing it, then the
    /// next demon), skipping what's already in place.
    pub fn prep(&mut self, who: [usize; 2]) {
        if who.iter().all(|&a| self.actors[a].rotation.map_or(true, |r| r.precombat.is_empty())) {
            return;
        }
        for &a in &who {
            self.actors[a].prep_step = self.actors[a].rotation.filter(|r| !r.precombat.is_empty()).map(|_| 0);
        }
        let prev = self.phase;
        self.phase = Phase::Prep;
        self.run(Stop::Prep { who, until: self.t + 120.0 });
        self.phase = prev;
    }

    /// Get ready for a test fight: buff up for real, then top up health
    /// and mana (yours and your pets').
    pub fn precombat(&mut self, a: usize) {
        self.prep([a, a]);
        self.top_up(a);
    }
    pub fn top_up(&mut self, a: usize) {
        self.fill(a);
        let pets = self.actors[a].pets.clone();
        for p in pets {
            self.fill(p);
        }
    }

    /// Start a fight with the rotation's opener: its lines are played first,
    /// in order and at most once each, like any other cast (costs, cooldowns,
    /// global cooldown), before the rest of the rotation takes over.
    pub fn opener(&mut self, a: usize) {
        let Some(rot) = self.actors[a].rotation else { return };
        self.actors[a].opening = if rot.opener.is_empty() { None } else { Some(0) };
    }

    /// The next precombat line: cast it if it can go, wait for the global
    /// cooldown, or move on. Returns whether it cast.
    fn prep_line(&mut self, a: usize, rot: &'w Rotation) -> bool {
        let Some(mut k) = self.actors[a].prep_step else { return false };
        while k < rot.precombat.len() {
            let act = &rot.precombat[k];
            let sp = &self.w.spells[act.spell as usize];
            if !sp.off_gcd && self.actors[a].gcd_end > self.t + EPS {
                self.actors[a].prep_step = Some(k);
                return false;
            }
            // no totems put down a walk away from the fight
            if !self.prep_totems && self.places_totem(act.spell) {
                k += 1;
                continue;
            }
            if let Some(plan) = self.line_plan(a, act, false, &mut None) {
                self.actors[a].prep_step = Some(k + 1);
                if self.actors[a].owner.is_none() {
                    let v = &mut self.metrics[a].precombat_lines;
                    if v.len() <= k {
                        v.resize(k + 1, 0.0);
                    }
                    v[k] += 1.0;
                }
                self.begin(a, plan);
                return true;
            }
            k += 1;
        }
        self.actors[a].prep_step = None;
        false
    }

    /// The next opener move, if one is due. Returns Some(true) when it cast,
    /// Some(false) to wait (global cooldown), None when the opener is over.
    fn open_step(&mut self, a: usize, rot: &'w Rotation) -> Option<bool> {
        let mut k = self.actors[a].opening?;
        while k < rot.opener.len() {
            let act = &rot.opener[k];
            let sp = &self.w.spells[act.spell as usize];
            if !sp.off_gcd && self.actors[a].gcd_end > self.t + EPS {
                self.actors[a].opening = Some(k);
                return Some(false);
            }
            // too far: walk up to it first; too close (inside Charge's 8
            // yards): skip it
            if act.target == SpellTarget::Enemy && sp.range > 0.0 {
                if let Some(t) = self.actors[a].target {
                    let d = self.dist(a, t);
                    let sm = self.spell_mod_vals(a, Some(act.spell), None);
                    let reach = if sp.range <= self.w.combat.melee_reach + 1e-9 { self.reach(a) } else { self.range_with(sp, &sm) };
                    if d > reach + 1e-6 {
                        self.actors[a].opening = Some(k);
                        return Some(false);
                    }
                    if d < sp.min_range - 1e-6 {
                        k += 1;
                        continue;
                    }
                }
            }
            // a move that can't be used now (cooldown, cost, stance) is skipped
            if let Some(plan) = self.line_plan(a, act, false, &mut None) {
                self.actors[a].opening = Some(k + 1);
                if self.actors[a].owner.is_none() {
                    let v = &mut self.metrics[a].opener_lines;
                    if v.len() <= k {
                        v.resize(k + 1, 0.0);
                    }
                    v[k] += 1.0;
                }
                self.begin(a, plan);
                return Some(true);
            }
            k += 1;
        }
        self.actors[a].opening = None;
        None
    }

    pub fn begin(&mut self, a: usize, plan: Plan) {
        let w = self.w;
        let sp = &w.spells[plan.spell as usize];
        // a first hostile move: someone waiting for it starts to react
        if plan.target != a && self.hostile(a, plan.target) {
            self.rouse(plan.target, a);
        }
        if sp.auto_repeat {
            // the first shot waits for the wind-up (standing still), the
            // next ones keep the weapon's own timer
            let x = &mut self.actors[a];
            x.wanding = Some(plan.spell);
            let first = self.t + w.combat.auto_repeat_windup;
            let s = x.swing[Hand::Ranged as usize];
            x.swing[Hand::Ranged as usize] = if s == INF { first } else { s.max(first) };
            return;
        }
        if !sp.off_gcd || plan.cast_time > 0.0 {
            // casting anything stops a wand
            self.actors[a].wanding = None;
        }
        if sp.next_swing {
            self.actors[a].queued = Some(plan.spell);
            return;
        }
        if !sp.off_gcd {
            let cx = self.action_cx(a, &plan);
            let energy = self.actors[a].class.map_or(false, |c| w.classes[c].energy_gcd);
            let def = if energy { w.combat.energy_gcd } else { w.combat.gcd };
            let g = (self.evo(&sp.gcd, &cx, def) + plan.sm.gcd_add).max(0.0);
            self.actors[a].gcd_end = self.t + g;
        }
        if plan.cast_time > 0.0 || sp.channel.is_some() {
            // casting means standing still
            self.stop(a);
        }
        if plan.cast_time > 0.0 {
            let seen = self.t + self.notice(a, plan.target);
            self.actors[a].cast = Some(CastState {
                spell: plan.spell,
                rank: plan.rank,
                target: plan.target,
                end: self.t + plan.cast_time,
                pushes: 0,
                channel: false,
                next_tick: INF,
                tick_iv: 0.0,
                sm: plan.sm,
                seen,
            });
            return;
        }
        self.complete(a, plan);
    }

    /// How long the target of a cast takes to notice it (a reacting player
    /// on the other side; nobody else waits).
    fn notice(&mut self, caster: usize, target: usize) -> f64 {
        if target != caster && self.actors[target].team != self.actors[caster].team {
            self.reaction(target)
        } else {
            0.0
        }
    }

    pub fn finish_cast(&mut self, a: usize) {
        let Some(c) = self.actors[a].cast.take() else { return };
        if c.channel {
            return;
        }
        let target = c.target;
        // a cast on a unit that died meanwhile fizzles (heals included)
        if !self.actors[target].alive {
            return;
        }
        let plan = Plan { spell: c.spell, rank: c.rank, target, cast_time: 0.0, sm: c.sm };
        self.complete(a, plan);
    }

    pub fn channel_tick(&mut self, a: usize) {
        let Some(c) = self.actors[a].cast.clone() else { return };
        if let Some(cs) = self.actors[a].cast.as_mut() {
            cs.next_tick += cs.tick_iv;
        }
        if !self.actors[c.target].alive {
            self.actors[a].cast = None;
            return;
        }
        let sp = &self.w.spells[c.spell as usize];
        if let Some(ch) = &sp.channel {
            let plan = Plan { spell: c.spell, rank: c.rank, target: c.target, cast_time: 0.0, sm: c.sm };
            let cx = self.action_cx(a, &plan);
            self.run_effects(&ch.effects, cx);
        }
    }

    fn complete(&mut self, a: usize, plan: Plan) {
        let w = self.w;
        let sp = &w.spells[plan.spell as usize];
        let costs = self.costs(a, plan.spell, plan.rank, &plan.sm, plan.target);
        if !self.afford(a, &costs) {
            return;
        }
        self.pay(a, &costs);
        let cx = self.action_cx(a, &plan);
        let cd = ((self.ev(&sp.cooldown, &cx) + plan.sm.cd_add) * plan.sm.cd_mul).max(0.0);
        if cd > 0.0 {
            let key = self.cd_key(plan.spell);
            let t = self.t;
            let x = &mut self.actors[a];
            match x.cds.iter_mut().find(|c| c.0 == key) {
                Some(c) => c.1 = t + cd,
                None => x.cds.push((key, t + cd)),
            }
        }
        // buffing up before a fight shows under the precombat lines instead
        if self.phase != Phase::Prep {
            let owner = self.actors[a].owner.unwrap_or(a);
            *self.metrics[owner].casts.entry(sp.name.as_str()).or_insert(0.0) += 1.0;
        }
        self.logf(|s| format!("{} casts {}", s.actors[a].name, sp.name));
        self.tracef(|s| TraceEv { key: sp.qid.clone(), school: sp.school.name(), ..s.tev("cast", a, Some(plan.target).filter(|&t| t != a), sp.name.as_str()) });
        if let Some(ch) = &sp.channel {
            let dur = self.ev(&ch.duration, &cx);
            let iv = self.ev(&ch.interval, &cx).max(0.1);
            let seen = self.t + self.notice(a, plan.target);
            self.actors[a].cast = Some(CastState {
                spell: plan.spell,
                rank: plan.rank,
                target: plan.target,
                end: self.t + dur,
                pushes: 0,
                channel: true,
                next_tick: self.t + iv,
                tick_iv: iv,
                sm: plan.sm,
                seen,
            });
        }
        // auras granted or refreshed from here on came from this cast
        let mark = self.uid;
        self.run_effects(&sp.effects, cx);
        if sp.finisher {
            self.actors[a].cp = 0.0;
        }
        let ev = Ev { other: Some(plan.target), by: Some(plan.spell), ..Ev::default() };
        self.emit(a, EventKind::Cast(plan.spell), ev, 0);
        self.emit(a, EventKind::AnyCast, ev, 0);
        self.consume_on_cast(a, plan.spell, mark);
    }

    /// Use up auras whose modifiers applied to this cast, except ones the
    /// cast itself just granted (a Clearcasting proc from this spell's hit
    /// is kept for the next one).
    fn consume_on_cast(&mut self, a: usize, spell: SpellId, mark: u64) {
        let w = self.w;
        let school = w.spells[spell as usize].school;
        let mut k = 0;
        while k < self.actors[a].auras.len() {
            let inst = &self.actors[a].auras[k];
            let def = &w.auras[inst.def as usize];
            if def.consume_on_cast && inst.fresh <= mark && def.spell_mods.iter().any(|m| m.applies(spell, school)) {
                let ch = inst.charges;
                match ch {
                    Some(c) if c > 1.0 => {
                        self.actors[a].auras[k].charges = Some(c - 1.0);
                        k += 1;
                    }
                    _ => {
                        self.remove_aura_at(a, k, false);
                    }
                }
                continue;
            }
            k += 1;
        }
    }

    /// Next-swing abilities (Heroic Strike, Maul, ...): replace a white swing.
    pub fn fire_queued(&mut self, a: usize, spell: SpellId) -> bool {
        let Some(rank) = self.known(a, spell) else { return false };
        let rank = if rank == usize::MAX { None } else { Some(rank) };
        let Some(target) = self.actors[a].target else { return false };
        let sm = self.spell_mod_vals(a, Some(spell), None);
        let costs = self.costs(a, spell, rank, &sm, target);
        if !self.afford(a, &costs) {
            return false;
        }
        let plan = Plan { spell, rank, target, cast_time: 0.0, sm };
        let cx = self.action_cx(a, &plan);
        if let Some(r) = &self.w.spells[spell as usize].requires {
            if self.ev(r, &cx) == 0.0 {
                return false;
            }
        }
        self.complete(a, plan);
        true
    }

    // ------------------------------------------------------------ effects

    pub fn run_effects(&mut self, effs: &'w [Effect], cx: Cx<'w>) {
        for e in effs {
            if !self.actors[cx.me].alive && !matches!(e, Effect::RemoveSelf) {
                return;
            }
            self.run_effect(e, cx);
        }
    }

    fn holders(&self, sel: &TargetSel, cx: &Cx<'w>) -> Picked {
        self.select(sel, cx, SelUse::Effect)
    }

    fn run_effect(&mut self, e: &'w Effect, cx: Cx<'w>) {
        let w = self.w;
        match e {
            Effect::Damage(fx) => {
                self.do_damage(fx, cx);
            }
            Effect::Heal(fx) => self.do_heal(fx, cx),
            Effect::Resource { who, res, amount, set } => {
                let v = self.ev(amount, &cx);
                for h in self.holders(who, &cx) {
                    if *res == Res::Hp {
                        if *set {
                            self.actors[h].hp = v.min(self.actors[h].max_hp);
                        } else if v >= 0.0 {
                            self.heal_raw(cx.me, h, v, cx.tag, false, cx.spell);
                        } else {
                            let x = &mut self.actors[h];
                            x.hp = (x.hp + v).max(1.0);
                        }
                        continue;
                    }
                    let x = &mut self.actors[h];
                    let r = *res as usize;
                    x.res[r] = if *set { v } else { x.res[r] + v }.clamp(0.0, x.max_res[r]);
                }
            }
            Effect::Combo(n) => {
                let v = self.ev(n, &cx);
                let x = &mut self.actors[cx.me];
                x.cp = (x.cp + v).clamp(0.0, 5.0);
            }
            Effect::ResetCombo => self.actors[cx.me].cp = 0.0,
            Effect::ApplyAura { who, aura, stacks, duration } => {
                let n = self.ev(stacks, &cx).max(1.0) as u32;
                let d = duration.as_ref().map(|d| self.ev(d, &cx));
                for h in self.holders(who, &cx) {
                    self.apply_aura(h, *aura, cx.me, n, d, cx.rank, cx);
                }
            }
            Effect::RemoveAura { who, aura } => {
                for h in self.holders(who, &cx) {
                    if let Some(k) = self.actors[h].aura_index(*aura, None) {
                        self.remove_aura_at(h, k, false);
                    }
                }
            }
            Effect::RemoveSelf => {
                if let Some((h, uid)) = cx.aura {
                    if let Some(k) = self.actors[h].auras.iter().position(|i| i.uid == uid) {
                        self.remove_aura_at(h, k, false);
                    }
                }
            }
            Effect::ConsumeCharge => {
                if let Some((h, uid)) = cx.aura {
                    if let Some(k) = self.actors[h].auras.iter().position(|i| i.uid == uid) {
                        let c = self.actors[h].auras[k].charges.unwrap_or(1.0) - 1.0;
                        if c <= 0.0 {
                            self.remove_aura_at(h, k, false);
                        } else {
                            self.actors[h].auras[k].charges = Some(c);
                        }
                    }
                }
            }
            Effect::Emit(id) => {
                let ev = cx.ev.unwrap_or_default();
                self.emit(cx.me, EventKind::Custom(*id), ev, cx.depth + 1);
            }
            Effect::Cooldown { spell, op, value } => {
                let key = self.cd_key(*spell);
                let v = self.ev(value, &cx);
                let t = self.t;
                let x = &mut self.actors[cx.me];
                match op {
                    CdOp::Reset => x.cds.retain(|c| c.0 != key),
                    CdOp::Reduce => {
                        if let Some(c) = x.cds.iter_mut().find(|c| c.0 == key) {
                            c.1 -= v;
                        }
                    }
                    CdOp::Set => match x.cds.iter_mut().find(|c| c.0 == key) {
                        Some(c) => c.1 = t + v,
                        None => x.cds.push((key, t + v)),
                    },
                }
            }
            Effect::Summon(p) => {
                self.dismiss_pets(cx.me);
                let id = self.spawn_pet(cx.me, *p);
                let _ = id;
                let ev = Ev { pet: Some(*p), ..Ev::default() };
                self.emit(cx.me, EventKind::Summon, ev, cx.depth + 1);
            }
            Effect::Dismiss | Effect::Sacrifice => self.dismiss_pets(cx.me),
            Effect::ExtraAttack { hand, count, ap_bonus } => {
                let n = self.ev(count, &cx).max(0.0) as u32;
                let apb = self.evo(ap_bonus, &cx, 0.0);
                for _ in 0..n {
                    if self.actors[cx.me].weapons[*hand as usize].is_some() {
                        self.white_swing(cx.me, *hand, true, apb);
                    }
                }
            }
            Effect::Cast { spell, who } => {
                let Some(rank) = self.known(cx.me, *spell) else { return };
                let rank = if rank == usize::MAX { None } else { Some(rank) };
                let tgt = self.holders(who, &cx).first();
                let sp = &w.spells[*spell as usize];
                let mut c2 = Cx::new(cx.me, tgt, sp.name.as_str());
                c2.ally = cx.ally;
                c2.at = cx.at;
                c2.rank = rank.map(|r| (*spell, r));
                c2.spell = Some(*spell);
                c2.depth = cx.depth + 1;
                c2.sm = self.spell_mod_vals(cx.me, Some(*spell), None);
                if c2.depth < 5 {
                    self.run_effects(&sp.effects, c2);
                }
            }
            Effect::Interrupt { who, lockout } => {
                let l = self.ev(lockout, &cx);
                for h in self.holders(who, &cx) {
                    if self.actors[h].cast.is_some() {
                        self.actors[h].cast = None;
                        self.actors[h].lockout = self.t + l;
                        if self.hostile(cx.me, h) {
                            let o = self.actors[cx.me].owner.unwrap_or(cx.me);
                            self.metrics[o].interrupts += 1.0;
                            self.tracef(|s| s.tev("interrupt", cx.me, Some(h), ""));
                        }
                    }
                }
            }
            Effect::If { cond, then, els } => {
                if self.ev(cond, &cx) != 0.0 {
                    self.run_effects(then, cx);
                } else {
                    self.run_effects(els, cx);
                }
            }
            Effect::FromSource(then) => {
                let Some((holder, uid)) = cx.aura else { return self.run_effects(then, cx) };
                let src = self.actors[holder].auras.iter().find(|i| i.uid == uid).map_or(holder, |i| i.src);
                let src = if self.actors[src].alive { src } else { holder };
                let mut c2 = cx;
                c2.me = src;
                c2.tgt = Some(holder);
                c2.ally = Some(holder).filter(|&h| h != src);
                self.run_effects(then, c2);
            }
            Effect::Chance { p, then } => {
                let p = self.ev(p, &cx);
                if self.actors[cx.me].rng[1].chance(p) {
                    self.run_effects(then, cx);
                }
            }
            Effect::Dispel { who, mechanics } => {
                for h in self.holders(who, &cx) {
                    let mut k = 0;
                    while k < self.actors[h].auras.len() {
                        let m = w.auras[self.actors[h].auras[k].def as usize].mechanic;
                        if m.map_or(false, |m| mechanics.contains(&m)) {
                            self.remove_aura_at(h, k, false);
                        } else {
                            k += 1;
                        }
                    }
                }
            }
            Effect::Taunt { who, duration } => {
                let d = self.ev(duration, &cx);
                // a totem's taunt (Stoneclaw) draws the attacks to the totem
                let me = cx.at.filter(|&t| self.actors[t].kind == Kind::Totem).unwrap_or(cx.me);
                for t in self.holders(who, &cx) {
                    if self.actors[t].kind == Kind::Enemy && self.actors[t].alive {
                        // your threat goes up to the top's, and it attacks you while it lasts
                        let top = self.actors[t].top_threat();
                        let tt = &mut self.actors[t];
                        let mine = super::threat_of(&mut tt.threat, me);
                        *mine = mine.max(top);
                        tt.aggro_dirty = true;
                        tt.taunt = Some((me, self.t + d));
                        tt.target = Some(me);
                    }
                }
            }
            Effect::Threat { amount } => {
                let v = self.ev(amount, &cx);
                if let Some(t) = cx.tgt {
                    self.add_threat(t, cx.me, v);
                }
            }
            Effect::Charge(then) => self.charge(then, cx),
            Effect::Purge { who, count } => {
                let n = self.ev(count, &cx).max(0.0) as usize;
                for h in self.holders(who, &cx) {
                    for _ in 0..n {
                        let Some(k) = self.actors[h].auras.iter().rposition(|i| self.purgeable(i.def)) else { break };
                        self.remove_aura_at(h, k, false);
                    }
                }
            }
            Effect::Trap { arm, radius, lasts, area, then } => {
                let name = cx.spell.map_or(cx.tag, |sp| w.spells[sp as usize].name.as_str());
                self.spawn_trap(cx.me, then, (*arm, *radius, *lasts, *area), cx.rank, name);
            }
            Effect::Teleport(yd) => {
                let d = self.ev(yd, &cx);
                let me = cx.me;
                let away = self.actors[me].target.map_or(-1.0, |t| -(self.pos(t) - self.pos(me)).signum());
                let x = self.pos(me) + if away == 0.0 { -1.0 } else { away } * d;
                self.place(me, x);
                self.actors[me].mo.goal = Goal::Engage;
            }
            Effect::DropThreat { resist } => {
                let me = cx.me;
                let team = self.actors[me].team;
                let ids: Vec<usize> = self.live.iter().copied().filter(|&i| { let y = &self.actors[i]; y.alive && y.kind == Kind::Enemy && y.team != team }).collect();
                for e in ids {
                    if !self.actors[e].threat.iter().any(|x| x.0 as usize == me) {
                        continue;
                    }
                    // a resisted Feign Death: like a spell's hit roll by level
                    if *resist {
                        let diff = self.level_diff(me, e);
                        let miss = 100.0 - CombatCfg::by_diff(&self.w.combat.spell_hit, diff);
                        if self.actors[me].rng[1].f64() * 100.0 < miss {
                            self.tracef(|s| s.tev("resist", e, Some(me), "Feign Death"));
                            continue;
                        }
                    }
                    if let Some(x) = self.actors[e].threat.iter_mut().find(|x| x.0 as usize == me) {
                        x.1 = 0.0;
                    }
                    let y = &mut self.actors[e];
                    if y.taunt.map_or(false, |t| t.0 == me) {
                        y.taunt = None;
                    }
                    y.aggro_dirty = true;
                    if y.target == Some(me) {
                        // someone else to fight, or home it goes
                        let y = &self.actors[e];
                        let other = y.threat.iter().any(|x| x.1 > 0.0 && x.0 as usize != me && self.actors[x.0 as usize].alive);
                        if other {
                            let n = self.threat_target(e);
                            self.actors[e].target = n;
                        } else {
                            self.evade(e);
                        }
                    }
                }
            }
            Effect::Track(levels) => {
                if let Some(t) = cx.tgt.or(self.actors[cx.me].target) {
                    let c = self.actors[t].creature;
                    let lvl = self.actors[cx.me].level;
                    if levels.iter().any(|&(k, l)| k == c && lvl >= l) {
                        self.actors[cx.me].tracking = Some(c);
                    }
                }
            }
            Effect::StepBack(to) => {
                let to = self.ev(to, &cx);
                let m = &mut self.actors[cx.me].mo;
                m.goal = Goal::Retreat(to);
                m.closing = false;
            }
            Effect::Absorb { amount } => {
                let v = self.ev(amount, &cx);
                if let Some((h, uid)) = cx.aura {
                    if let Some(i) = self.actors[h].auras.iter_mut().find(|i| i.uid == uid) {
                        i.absorb += v;
                    }
                }
            }
        }
    }

    /// A buff that magic dispels take off (Purge).
    pub fn purgeable(&self, aura: AuraId) -> bool {
        let d = &self.w.auras[aura as usize];
        !d.debuff && !d.hidden && d.dispel_type.as_deref() == Some("magic")
    }

    pub fn dismiss_pets(&mut self, owner: usize) {
        let pets = std::mem::take(&mut self.actors[owner].pets);
        for p in pets {
            let x = &mut self.actors[p];
            x.alive = false;
            x.cast = None;
            x.swing = [INF; 3];
        }
    }

    // ------------------------------------------------------------ auras

    #[allow(clippy::too_many_arguments)]
    pub fn apply_aura(&mut self, holder: usize, aura: AuraId, src: usize, stacks: u32, dur: Option<f64>, rank: Option<(SpellId, usize)>, cx: Cx<'w>) {
        let w = self.w;
        let def = &w.auras[aura as usize];
        if !self.actors[holder].alive {
            return;
        }
        // a totem goes down on the ground (a party member's buff stays a buff)
        if def.totem.is_some() && self.actors[holder].kind != Kind::Totem && !self.totem_buffing && self.actors[src].alive {
            self.spawn_totem(src, aura, rank, cx);
            return;
        }
        if let Some(m) = def.mechanic {
            let st = &self.actors[holder].st;
            let immune = match m {
                Mechanic::Stun => st[Stat::ImmuneStun as usize] > 0.0,
                Mechanic::Fear => st[Stat::ImmuneFear as usize] > 0.0,
                Mechanic::Incap => st[Stat::ImmuneIncap as usize] > 0.0,
                Mechanic::Silence => st[Stat::ImmuneSilence as usize] > 0.0,
                Mechanic::Disarm => st[Stat::ImmuneDisarm as usize] > 0.0,
                Mechanic::Root | Mechanic::Freeze => st[Stat::ImmuneRoot as usize] > 0.0,
                _ => false,
            };
            if immune {
                return;
            }
        }
        // a debuff from a foe gets a mob's attention
        if def.debuff && !def.quiet && !self.actors[holder].aware && self.hostile(src, holder) {
            self.wake(holder, Some(src));
        }
        let mut c2 = cx;
        c2.me = src;
        c2.tgt = Some(holder);
        c2.rank = rank;
        let mut d = dur.unwrap_or_else(|| self.ev(&def.duration, &c2));
        match def.mechanic {
            Some(Mechanic::Stun) => d *= self.actors[holder].st[Stat::StunDuration as usize],
            Some(Mechanic::Fear) => d *= self.actors[holder].st[Stat::FearDuration as usize],
            _ => {}
        }
        // PvP diminishing returns: players (and player-like enemies) take 100%, 50%,
        // 25%, then immune from repeated crowd control of one kind within the window.
        if let Some(m) = def.mechanic {
            let pvp = {
                let h = &self.actors[holder];
                h.kind == Kind::Player || h.enemy.map_or(false, |e| e.player_like)
            };
            let _ = m;
            let window = w.combat.dr_window;
            if let (true, Some(k), true) = (pvp, def.dr, window > 0.0 && d > 0.0) {
                let k = k as usize;
                let (until, n) = self.actors[holder].dr[k];
                let n = if self.t < until { n.saturating_add(1) } else { 1 };
                let f = match n { 1 => 1.0, 2 => 0.5, 3 => 0.25, _ => 0.0 };
                if f == 0.0 {
                    return;
                }
                d *= f;
                self.actors[holder].dr[k] = (self.t + d + window, n);
            }
        }
        let expires = if d > 0.0 { self.t + d } else { INF };
        let existing = if def.per_source {
            self.actors[holder].auras.iter().position(|i| i.def == aura && i.src == src)
        } else {
            self.actors[holder].auras.iter().position(|i| i.def == aura)
        };
        if let Some(k) = existing {
            match def.refresh {
                Refresh::Ignore => return,
                Refresh::Replace => self.remove_aura_at(holder, k, false),
                Refresh::Refresh => {
                    // a fresh proc on top of one not yet noticed (or missed) is another chance
                    if def.react && self.actors[holder].auras[k].seen > self.t + EPS {
                        let seen = self.notice_proc(holder);
                        let inst = &mut self.actors[holder].auras[k];
                        inst.seen = inst.seen.min(seen);
                    }
                    let charges = def.charges.as_ref().map(|c| self.ev(c, &c2));
                    let absorb = def.absorb.as_ref().map(|a| self.ev(a, &c2));
                    let slow = def.slow.as_ref().map_or(0.0, |e| self.ev(e, &c2).clamp(0.0, 1.0));
                    let fast = def.speed.as_ref().map_or(0.0, |e| self.ev(e, &c2).max(0.0));
                    self.uid += 1;
                    let fresh = self.uid;
                    let inst = &mut self.actors[holder].auras[k];
                    inst.slow = slow;
                    inst.fast = fast;
                    inst.fresh = fresh;
                    inst.expires = expires;
                    inst.stacks = (inst.stacks + stacks).min(def.max_stacks.max(1));
                    inst.rank = rank;
                    inst.spell = rank.map(|r| r.0).or(cx.spell);
                    inst.src = src;
                    if charges.is_some() {
                        inst.charges = charges;
                    }
                    if let Some(ab) = absorb {
                        inst.absorb = ab;
                    }
                    let uid = inst.uid;
                    let st = inst.stacks as f64;
                    let mut c3 = c2;
                    c3.aura = Some((holder, uid));
                    c3.stacks = st;
                    self.run_effects(&def.on_refresh, c3);
                    let t = self.t;
                    self.actors[holder].sync_aura_next(t);
                    self.emit(holder, EventKind::AuraApplied(aura), Ev { other: Some(src), ..Ev::default() }, cx.depth + 1);
                    if !def.mods.is_empty() {
                        self.refresh_stats(holder);
                    }
                    return;
                }
            }
        }
        // exclusive groups and forms
        if def.group.is_some() || def.form.is_some() {
            let mut k = 0;
            while k < self.actors[holder].auras.len() {
                let o = &w.auras[self.actors[holder].auras[k].def as usize];
                let same_group = def.group.is_some() && o.group == def.group && self.actors[holder].auras[k].def != aura;
                let both_forms = def.form.is_some() && o.form.is_some();
                if same_group || both_forms {
                    self.remove_aura_at(holder, k, false);
                } else {
                    k += 1;
                }
            }
        }
        let charges = def.charges.as_ref().map(|c| self.ev(c, &c2));
        let absorb = def.absorb.as_ref().map_or(0.0, |a| self.ev(a, &c2));
        let tick_iv = def.tick.as_ref().map_or(0.0, |(iv, _)| self.ev(iv, &c2).max(0.0));
        self.uid += 1;
        let uid = self.uid;
        let seen = if def.react { self.notice_proc(holder) } else { self.t };
        let slow = def.slow.as_ref().map_or(0.0, |e| self.ev(e, &c2).clamp(0.0, 1.0));
        let fast = def.speed.as_ref().map_or(0.0, |e| self.ev(e, &c2).max(0.0));
        let break_at = def.break_on_damage.as_ref().map(|e| self.ev(e, &c2).max(0.0));
        let inst = AuraInst {
            def: aura,
            src,
            rank,
            spell: rank.map(|r| r.0).or(cx.spell),
            expires,
            stacks: stacks.min(def.max_stacks.max(1)),
            charges,
            next_tick: if tick_iv > 0.0 { self.t + tick_iv } else { INF },
            tick_iv,
            absorb,
            applied: self.t,
            dmg_since: 0.0,
            break_at,
            uid,
            fresh: uid,
            seen,
            slow,
            fast,
        };
        let st = inst.stacks as f64;
        self.actors[holder].auras.push(inst);
        self.actors[holder].sync_mechs(w);
        let t = self.t;
        self.actors[holder].sync_aura_next(t);
        if let Some(f) = def.form {
            self.actors[holder].form = Some(f);
        }
        if matches!(def.mechanic, Some(Mechanic::Stun) | Some(Mechanic::Fear) | Some(Mechanic::Incap) | Some(Mechanic::Freeze)) {
            self.actors[holder].cast = None;
            self.actors[holder].queued = None;
        }
        if !def.mods.is_empty() || def.form.is_some() {
            self.refresh_stats(holder);
        }
        if self.log.is_some() {
            self.logf(|s| format!("{} gains {}{}", s.actors[holder].name, def.name, if st > 1.0 { format!(" ({st})") } else { String::new() }));
            // a totem's own pulse aura is bookkeeping: its cast shows already
            if self.actors[holder].kind != Kind::Totem {
                self.tracef(|s| TraceEv { stacks: st, key: def.qid.clone(), ..s.tev("gain", holder, Some(src).filter(|&x| x != holder), def.name.as_str()) });
            }
        }
        let mut c3 = c2;
        c3.aura = Some((holder, uid));
        c3.stacks = st;
        self.run_effects(&def.on_apply, c3);
        self.emit(holder, EventKind::AuraApplied(aura), Ev { other: Some(src), ..Ev::default() }, cx.depth + 1);
    }

    pub fn remove_aura_at(&mut self, holder: usize, k: usize, expired: bool) {
        let w = self.w;
        if k >= self.actors[holder].auras.len() {
            return;
        }
        let inst = self.actors[holder].auras.remove(k);
        self.actors[holder].sync_mechs(w);
        let t = self.t;
        self.actors[holder].sync_aura_next(t);
        let def = &w.auras[inst.def as usize];
        if self.log.is_some() {
            self.logf(|s| format!("{} loses {}", s.actors[holder].name, def.name));
            if self.actors[holder].kind != Kind::Totem {
                self.tracef(|s| TraceEv { key: def.qid.clone(), ..s.tev("lose", holder, None, def.name.as_str()) });
            }
        }
        if def.form.is_some() && self.actors[holder].form == def.form {
            self.actors[holder].form = None;
        }
        if !def.mods.is_empty() || def.form.is_some() {
            self.refresh_stats(holder);
        }
        // coming out of a stun or fear takes a moment to react to
        if def.mechanic.map_or(false, |m| m.bit() & Mechanic::HARD != 0) && self.reacting(holder) && self.actors[holder].alive {
            let r = self.reaction(holder);
            let x = &mut self.actors[holder];
            x.react_until = x.react_until.max(self.t + r);
        }
        let src = if self.actors[inst.src].alive { inst.src } else { holder };
        let mut cx = Cx::new(src, Some(holder), def.name.as_str());
        cx.rank = inst.rank;
        cx.stacks = inst.stacks as f64;
        cx.aura = Some((holder, inst.uid));
        if expired {
            self.run_effects(&def.on_expire, cx);
        }
        self.run_effects(&def.on_remove, cx);
        if self.actors[holder].alive {
            self.emit(holder, EventKind::AuraExpired(inst.def), Ev { other: Some(inst.src), ..Ev::default() }, 1);
        }
    }

    // ------------------------------------------------------------ procs

    pub fn emit(&mut self, a: usize, kind: EventKind, ev: Ev, depth: u8) {
        if depth > 4 || !self.actors[a].alive || self.actors[a].listens & kind.bit() == 0 {
            return;
        }
        let w = self.w;
        struct Cand<'w> {
            p: &'w Proc,
            aura: Option<(u64, Option<(SpellId, usize)>, f64)>,
            tag: &'w str,
        }
        let mut cands: Vec<Cand<'w>> = Vec::new();
        {
            let x = &self.actors[a];
            let cx0 = Cx::new(a, ev.other, "");
            for p in &x.passives {
                if !p.procs.iter().any(|q| q.on == kind) {
                    continue;
                }
                if let Some(c) = &p.cond {
                    if self.ev(c, &cx0) == 0.0 {
                        continue;
                    }
                }
                for q in &p.procs {
                    if q.on == kind {
                        cands.push(Cand { p: q, aura: None, tag: p.name.as_str() });
                    }
                }
            }
            if let Some(pd) = x.pet_def {
                for q in &pd.procs {
                    if q.on == kind {
                        cands.push(Cand { p: q, aura: None, tag: pd.name.as_str() });
                    }
                }
            }
            for inst in &x.auras {
                let def = &w.auras[inst.def as usize];
                for q in &def.procs {
                    if q.on == kind {
                        cands.push(Cand { p: q, aura: Some((inst.uid, inst.rank, inst.stacks as f64)), tag: def.name.as_str() });
                    }
                }
            }
        }
        for c in cands {
            if !self.actors[a].alive {
                return;
            }
            if let Some((uid, _, _)) = c.aura {
                if !self.actors[a].auras.iter().any(|i| i.uid == uid) {
                    continue;
                }
            }
            if c.p.icd > 0.0 {
                if let Some(r) = self.actors[a].icd.iter().find(|x| x.0 == c.p.key) {
                    if r.1 > self.t + EPS {
                        continue;
                    }
                }
            }
            let mut cx = Cx::new(a, ev.other, c.tag);
            cx.ev = Some(ev);
            cx.depth = depth + 1;
            if let Some((uid, rank, st)) = c.aura {
                cx.aura = Some((a, uid));
                cx.rank = rank;
                cx.stacks = st;
            }
            if let Some(cond) = &c.p.cond {
                if self.ev(cond, &cx) == 0.0 {
                    continue;
                }
            }
            let p = if let Some(ppm) = &c.p.ppm {
                let hand = ev.hand.unwrap_or(Hand::Mh);
                let speed = self.actors[a].weapons[hand as usize].map_or(2.0, |w| w.speed);
                self.ev(ppm, &cx) * speed / 60.0
            } else {
                self.evo(&c.p.chance, &cx, 1.0)
            };
            if !self.actors[a].rng[1].chance(p) {
                continue;
            }
            if c.p.icd > 0.0 {
                let t = self.t + c.p.icd;
                let x = &mut self.actors[a];
                match x.icd.iter_mut().find(|x| x.0 == c.p.key) {
                    Some(r) => r.1 = t,
                    None => x.icd.push((c.p.key, t)),
                }
            }
            self.run_effects(&c.p.effects, cx);
        }
    }
}
