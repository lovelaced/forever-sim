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
            };
            match (m.op, base_is_one) {
                (ModOp::Mul, _) => *target *= val,
                (ModOp::Add, _) => *target += val,
            }
        };
        for p in &x.passives {
            if p.spell_mods.is_empty() {
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
        let sp = &self.w.spells[spell as usize];
        let mut cx = Cx::new(a, Some(tgt), "");
        cx.rank = rank.map(|r| (spell, r));
        cx.spell = Some(spell);
        let cm = self.actors[a].st[Stat::CostMult as usize];
        sp.cost
            .iter()
            .enumerate()
            .map(|(i, (r, e))| {
                let mut c = self.ev(e, &cx) * sm.cost_mul;
                if i == 0 {
                    c += sm.cost_add;
                }
                if *r != Res::Hp {
                    c *= cm;
                }
                (*r, c.max(0.0))
            })
            .collect()
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
        }
    }

    /// Everything that must hold for a spell to be cast right now.
    pub fn can_cast(&self, a: usize, spell: SpellId, kind: SpellTarget, relaxed: bool) -> Option<Plan> {
        let w = self.w;
        let x = &self.actors[a];
        if !x.alive || x.cast.is_some() {
            return None;
        }
        let rank = self.known(a, spell)?;
        let rank = if rank == usize::MAX { None } else { Some(rank) };
        let sp = &w.spells[spell as usize];
        for m in self.hard_cc_mech(a) {
            if !sp.usable_while.contains(&m) {
                return None;
            }
        }
        if sp.school != School::Physical && (x.lockout > self.t || x.has_mech(w, Mechanic::Silence)) {
            return None;
        }
        if !relaxed {
            if !sp.off_gcd && !sp.next_swing && x.gcd_end > self.t + EPS {
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
        let target = self.resolve_target(a, kind)?;
        let mut cx = Cx::new(a, Some(target), sp.name.as_str());
        if kind == SpellTarget::Me {
            cx.tgt = x.target.or(Some(a));
        }
        cx.rank = rank.map(|r| (spell, r));
        cx.spell = Some(spell);
        if let Some(r) = &sp.requires {
            if self.ev(r, &cx) == 0.0 {
                return None;
            }
        }
        let sm = self.spell_mod_vals(a, Some(spell), None);
        if !relaxed {
            let costs = self.costs(a, spell, rank, &sm, target);
            if !self.afford(a, &costs) {
                return None;
            }
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
            cx.tgt = self.actors[a].target.or(Some(a));
        }
        cx.rank = plan.rank.map(|r| (plan.spell, r));
        cx.spell = Some(plan.spell);
        cx.sm = plan.sm;
        cx
    }

    /// Choose and start the first usable action in the rotation.
    pub fn act(&mut self, a: usize) -> bool {
        let x = &self.actors[a];
        if !x.alive || x.cast.is_some() {
            return false;
        }
        let Some(rot) = x.rotation else { return false };
        let list = if self.rest_phase { &rot.rest } else { &rot.actions };
        for (k, act) in list.iter().enumerate() {
            let Some(plan) = self.can_cast(a, act.spell, act.target, false) else { continue };
            if let Some(c) = &act.cond {
                let cx = self.action_cx(a, &plan);
                if self.ev(c, &cx) == 0.0 {
                    continue;
                }
            }
            if self.w.spells[act.spell as usize].auto_repeat && self.actors[a].wanding == Some(act.spell) {
                // the chosen action is the wand that is already shooting: keep it going
                return false;
            }
            if self.actors[a].owner.is_none() {
                let m = &mut self.metrics[a];
                let v = if self.rest_phase { &mut m.rest_lines } else { &mut m.lines };
                if v.len() <= k {
                    v.resize(k + 1, 0.0);
                }
                v[k] += 1.0;
            }
            self.begin(a, plan);
            return true;
        }
        false
    }

    pub fn can_act_rest(&self, a: usize) -> bool {
        let x = &self.actors[a];
        let Some(rot) = x.rotation else { return false };
        for act in &rot.rest {
            let Some(plan) = self.can_cast(a, act.spell, act.target, false) else { continue };
            if let Some(c) = &act.cond {
                let cx = self.action_cx(a, &plan);
                if self.ev(c, &cx) == 0.0 {
                    continue;
                }
            }
            return true;
        }
        false
    }

    /// Run the precombat list: buffs, pets, forms. Free and instant.
    pub fn precombat(&mut self, a: usize) {
        let Some(rot) = self.actors[a].rotation else { return };
        self.run_free(a, &rot.precombat);
        self.actors[a].cds.clear();
        self.actors[a].gcd_end = 0.0;
        self.fill(a);
        let pets = self.actors[a].pets.clone();
        for p in pets {
            self.fill(p);
        }
    }

    /// Run the opener list at the start of a fight (free, ignores cooldowns).
    pub fn opener(&mut self, a: usize) {
        let Some(rot) = self.actors[a].rotation else { return };
        self.run_free(a, &rot.opener);
    }

    fn run_free(&mut self, a: usize, list: &'w [Action]) {
        for act in list {
            let Some(plan) = self.can_cast(a, act.spell, act.target, true) else { continue };
            if let Some(c) = &act.cond {
                let cx = self.action_cx(a, &plan);
                if self.ev(c, &cx) == 0.0 {
                    continue;
                }
            }
            let cx = self.action_cx(a, &plan);
            let sp = &self.w.spells[plan.spell as usize];
            self.run_effects(&sp.effects, cx);
        }
    }

    pub fn begin(&mut self, a: usize, plan: Plan) {
        let w = self.w;
        let sp = &w.spells[plan.spell as usize];
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
        if plan.cast_time > 0.0 {
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
            });
            return;
        }
        self.complete(a, plan);
    }

    pub fn finish_cast(&mut self, a: usize) {
        let Some(c) = self.actors[a].cast.take() else { return };
        if c.channel {
            return;
        }
        let sp = &self.w.spells[c.spell as usize];
        let target = c.target;
        if sp.target == SpellTarget::Enemy && !self.actors[target].alive {
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
        let owner = self.actors[a].owner.unwrap_or(a);
        *self.metrics[owner].casts.entry(sp.name.as_str()).or_insert(0.0) += 1.0;
        self.logf(|s| format!("{} casts {}", s.actors[a].name, sp.name));
        self.tracef(|s| s.tev("cast", a, Some(plan.target).filter(|&t| t != a), sp.name.as_str()));
        if let Some(ch) = &sp.channel {
            let dur = self.ev(&ch.duration, &cx);
            let iv = self.ev(&ch.interval, &cx).max(0.1);
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

    fn holders(&self, sel: &TargetSel, cx: &Cx<'w>) -> Vec<usize> {
        match sel {
            TargetSel::Me => vec![cx.me],
            TargetSel::Target => cx.tgt.or(self.actors[cx.me].target).into_iter().collect(),
            TargetSel::Pet => self.who(cx, Who::Pet).into_iter().collect(),
            TargetSel::Owner => self.actors[cx.me].owner.into_iter().collect(),
            TargetSel::MyTarget => self.actors[cx.me].target.filter(|&t| self.actors[t].alive).into_iter().collect(),
            TargetSel::Enemies(max) => {
                let n = self.ev(max, cx) as usize;
                let team = self.actors[cx.me].team;
                self.actors.iter().filter(|x| x.alive && x.team != team).map(|x| x.id).take(n).collect()
            }
        }
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
                            self.heal_raw(cx.me, h, v, cx.tag, false);
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
                let tgt = self.holders(who, &cx).first().copied();
                let sp = &w.spells[*spell as usize];
                let mut c2 = Cx::new(cx.me, tgt, sp.name.as_str());
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
            Effect::Taunt { duration } => {
                let d = self.ev(duration, &cx);
                if let Some(t) = cx.tgt {
                    if self.actors[t].kind == Kind::Enemy {
                        let top = self.actors[t].threat.iter().cloned().fold(0.0, f64::max);
                        let me = cx.me;
                        let tt = &mut self.actors[t];
                        if tt.threat.len() <= me {
                            tt.threat.resize(me + 1, 0.0);
                        }
                        tt.threat[me] = tt.threat[me].max(top + 1.0);
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
            let dr_kind = matches!(m, Mechanic::Stun | Mechanic::Fear | Mechanic::Incap | Mechanic::Root | Mechanic::Freeze | Mechanic::Silence);
            let window = w.combat.dr_window;
            if pvp && dr_kind && window > 0.0 && d > 0.0 {
                let k = if m == Mechanic::Freeze { Mechanic::Root as usize } else { m as usize };
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
                    let charges = def.charges.as_ref().map(|c| self.ev(c, &c2));
                    let absorb = def.absorb.as_ref().map(|a| self.ev(a, &c2));
                    self.uid += 1;
                    let fresh = self.uid;
                    let inst = &mut self.actors[holder].auras[k];
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
            uid,
            fresh: uid,
        };
        let st = inst.stacks as f64;
        self.actors[holder].auras.push(inst);
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
            self.tracef(|s| TraceEv { stacks: st, ..s.tev("gain", holder, Some(src).filter(|&x| x != holder), def.name.as_str()) });
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
        let def = &w.auras[inst.def as usize];
        if self.log.is_some() {
            self.logf(|s| format!("{} loses {}", s.actors[holder].name, def.name));
            self.tracef(|s| s.tev("lose", holder, None, def.name.as_str()));
        }
        if def.form.is_some() && self.actors[holder].form == def.form {
            self.actors[holder].form = None;
        }
        if !def.mods.is_empty() || def.form.is_some() {
            self.refresh_stats(holder);
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
        if depth > 4 || !self.actors[a].alive {
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
