//! Attack tables, damage and healing.

use super::*;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Roll {
    Miss,
    Dodge,
    Parry,
    Glance,
    Block,
    Crit,
    /// A crushing blow: 150% damage, from a monster well above its target's level.
    Crush,
    Hit,
}

impl<'w> Sim<'w> {
    pub fn level_diff(&self, att: usize, def: usize) -> i32 {
        self.actors[def].level as i32 - self.actors[att].level as i32
    }

    /// Defense skill above the attacker's weapon skill (positive = harder).
    fn defense_delta(&self, att: usize, def: usize) -> f64 {
        let d = &self.actors[def];
        let a = &self.actors[att];
        5.0 * d.level as f64 + d.st[Stat::Defense as usize] - 5.0 * a.level as f64
    }

    /// Avoidance chances (miss, dodge, parry, block) in percent.
    fn avoid(&self, att: usize, def: usize, white: bool, ranged: bool, dual: bool, offhand: bool) -> (f64, f64, f64, f64) {
        let c = &self.w.combat;
        let a = &self.actors[att];
        let d = &self.actors[def];
        let diff = self.level_diff(att, def);
        let dd = self.defense_delta(att, def);
        let dpp = c.defense_per_point;
        let player_def = d.kind != Kind::Enemy || d.enemy.map_or(false, |e| e.player_like);
        let mut miss = if player_def { 5.0 + dd * dpp } else { CombatCfg::by_diff(&c.melee_miss, diff) };
        if white && dual && !ranged {
            miss += c.dual_wield_miss;
        }
        miss -= a.st[Stat::Hit as usize];
        if offhand {
            miss -= a.st[Stat::OffhandHit as usize];
        }
        let exp = a.st[Stat::Expertise as usize];
        let mut dodge = if player_def {
            d.st[Stat::Dodge as usize] + dd * dpp
        } else if d.st[Stat::Dodge as usize] >= 0.0 {
            d.st[Stat::Dodge as usize]
        } else {
            CombatCfg::by_diff(&c.mob_dodge, diff)
        };
        dodge -= exp;
        let front = !self.behind(att, def);
        let can_parry = !ranged
            && front
            && match d.class {
                Some(cl) => self.w.classes[cl].can_parry || d.st[Stat::Parry as usize] > 0.0,
                None => true,
            };
        let mut parry = if !can_parry {
            0.0
        } else if player_def {
            d.st[Stat::Parry as usize] + dd * dpp
        } else if d.st[Stat::Parry as usize] >= 0.0 {
            d.st[Stat::Parry as usize]
        } else {
            CombatCfg::by_diff(&c.mob_parry, diff)
        };
        if can_parry {
            parry -= exp;
        }
        let block = if front && d.shield.is_some() { d.st[Stat::Block as usize] + if player_def { dd * dpp } else { 0.0 } } else { 0.0 };
        (miss.max(0.0), dodge.max(0.0), parry.max(0.0), block.max(0.0))
    }

    fn crit_vs(&self, att: usize, def: usize, base: f64) -> f64 {
        let c = &self.w.combat;
        let d = &self.actors[def];
        let player_def = d.kind != Kind::Enemy || d.enemy.map_or(false, |e| e.player_like);
        if player_def {
            base - self.defense_delta(att, def).max(0.0) * c.defense_per_point
        } else {
            base - CombatCfg::by_diff(&c.crit_suppression, self.level_diff(att, def))
        }
    }

    /// Crushing blows (percent): monsters 2 or more levels above a player
    /// or party member, 2% per point of weapon skill over the target's
    /// level-based defense, minus 15% (15% for a boss 3 levels up).
    fn crush_chance(&self, att: usize, def: usize) -> f64 {
        let (a, d) = (&self.actors[att], &self.actors[def]);
        if a.kind != Kind::Enemy || a.enemy.map_or(false, |e| e.player_like) || d.kind == Kind::Enemy {
            return 0.0;
        }
        let skill = 5.0 * (a.level as f64 - d.level as f64);
        (2.0 * skill - 15.0).max(0.0)
    }

    pub fn roll_melee(&mut self, att: usize, def: usize, hand: Hand, white: bool, crit_add: f64) -> Roll {
        let ranged = hand == Hand::Ranged;
        let dual = self.actors[att].weapons[1].is_some();
        let (miss, dodge, parry, block) = self.avoid(att, def, white, ranged, dual, hand == Hand::Oh);
        let crit_stat = if ranged { Stat::RangedCrit } else { Stat::MeleeCrit };
        let crit = self.crit_vs(att, def, self.actors[att].st[crit_stat as usize] + crit_add);
        let player_target = self.actors[def].kind != Kind::Enemy;
        let attacker_player = self.actors[att].kind != Kind::Enemy;
        let glance = if white && !ranged && attacker_player && !player_target {
            CombatCfg::by_diff(&self.w.combat.glance_chance, self.level_diff(att, def))
        } else {
            0.0
        };
        let r = self.actors[att].rng[0].f64() * 100.0;
        if white {
            let mut edge = miss;
            if r < edge {
                return Roll::Miss;
            }
            edge += dodge;
            if r < edge {
                return Roll::Dodge;
            }
            edge += parry;
            if r < edge {
                return Roll::Parry;
            }
            edge += glance;
            if r < edge {
                return Roll::Glance;
            }
            edge += block;
            if r < edge {
                return Roll::Block;
            }
            edge += crit.max(0.0);
            if r < edge {
                return Roll::Crit;
            }
            let crush = self.crush_chance(att, def);
            if crush > 0.0 {
                edge += crush;
                if r < edge {
                    return Roll::Crush;
                }
            }
            Roll::Hit
        } else {
            let mut edge = miss;
            if r < edge {
                return Roll::Miss;
            }
            edge += dodge;
            if r < edge {
                return Roll::Dodge;
            }
            edge += parry;
            if r < edge {
                return Roll::Parry;
            }
            edge += block;
            if r < edge {
                return Roll::Block;
            }
            if self.actors[att].rng[0].f64() * 100.0 < crit {
                Roll::Crit
            } else {
                Roll::Hit
            }
        }
    }

    pub fn roll_spell(&mut self, att: usize, def: usize, can_crit: bool, crit_add: f64, heal: bool) -> Roll {
        let c = &self.w.combat;
        if !heal && self.hostile(att, def) {
            let diff = self.level_diff(att, def);
            let base = if diff < 0 { (96.0 - diff as f64).min(99.0) } else { CombatCfg::by_diff(&c.spell_hit, diff) };
            let a = &self.actors[att];
            let hit = (base + a.st[Stat::Hit as usize] + a.st[Stat::SpellHit as usize]).min(c.spell_hit_cap);
            if self.actors[att].rng[0].f64() * 100.0 >= hit {
                return Roll::Miss;
            }
        }
        if !can_crit {
            return Roll::Hit;
        }
        let a = &self.actors[att];
        let mut crit = a.st[Stat::SpellCrit as usize] + crit_add;
        if heal {
            crit += a.st[Stat::HealCrit as usize];
        } else {
            crit = self.crit_vs(att, def, crit);
        }
        if self.actors[att].rng[0].f64() * 100.0 < crit {
            Roll::Crit
        } else {
            Roll::Hit
        }
    }

    /// Weapon damage roll including attack power.
    pub fn weapon_roll(&mut self, a: usize, hand: Hand, normalized: bool, ap_bonus: f64) -> f64 {
        let c = &self.w.combat;
        let x = &self.actors[a];
        let Some(wp) = x.weapons[hand as usize] else { return 0.0 };
        // wands take nothing from attack power
        let ap = (if wp.wtype == WeaponType::Wand { 0.0 } else if hand == Hand::Ranged { x.st[Stat::Rap as usize] } else { x.st[Stat::Ap as usize] } + ap_bonus).max(0.0);
        let ov = x.st[Stat::WeaponSpeedOverride as usize];
        let roll = self.actors[a].rng[0].range(wp.min, wp.max);
        if ov > 0.0 && hand != Hand::Ranged {
            // shapeshifted: weapon DPS at the form's attack speed
            let avg = (wp.min + wp.max) / 2.0;
            let scaled = if avg > 0.0 { roll / avg * wp.dps() * ov } else { 0.0 };
            return scaled + ap / 14.0 * ov;
        }
        let speed = if normalized {
            match wp.wtype {
                WeaponType::Dagger => c.normalized_dagger,
                t if t.two_handed() => c.normalized_2h,
                WeaponType::Bow | WeaponType::Gun | WeaponType::Crossbow | WeaponType::Thrown => c.normalized_ranged,
                _ => c.normalized_1h,
            }
        } else {
            wp.speed
        };
        roll + ap / 14.0 * speed
    }

    pub fn sp_for(&self, a: usize, school: School) -> f64 {
        let x = &self.actors[a];
        let mut sp = x.st[Stat::Sp as usize] + x.st[Stat::HealPower as usize] * self.w.combat.heal_to_spell_damage;
        if let Some(s) = school.sp_stat() {
            sp += x.st[s as usize];
        }
        sp
    }

    pub fn downrank(&self, a: usize, rank: Option<(SpellId, usize)>) -> f64 {
        if let Some((s, r)) = rank {
            if let Some(rd) = self.w.spells[s as usize].ranks.get(r) {
                let c = &self.w.combat;
                let rl = rd.level as f64;
                // Classic: spells learned below level 20 lose 3.75% of their
                // coefficient per level below 20 (assumption `low_level_penalty`;
                // Forever's spell data gives low ranks their full coefficient).
                let low = (1.0 - self.low_level_penalty * (c.low_level_penalty_level - rl).max(0.0) * c.low_level_penalty_per).max(0.0);
                let next_higher_exists = self.w.spells[s as usize].ranks.len() > r + 1;
                if next_higher_exists && self.downrank_penalty >= 0.5 {
                    let lvl = self.actors[a].level as f64;
                    return ((rl + c.downrank_offset) / lvl).min(1.0) * low;
                }
                return low;
            }
        }
        1.0
    }

    pub fn auto_attack(&mut self, a: usize, hand: Hand) {
        if let Some(t) = self.actors[a].target {
            self.rouse(t, a);
        }
        if hand == Hand::Ranged {
            if let Some(spell) = self.actors[a].wanding {
                let sp = &self.w.spells[spell as usize];
                let mut cx = Cx::new(a, self.actors[a].target, sp.name.as_str());
                cx.spell = Some(spell);
                cx.sm = self.spell_mod_vals(a, Some(spell), None);
                self.actors[a].in_combat = true;
                self.run_effects(&sp.effects, cx);
                return;
            }
        }
        if hand == Hand::Mh {
            if let Some(q) = self.actors[a].queued.take() {
                if self.fire_queued(a, q) {
                    return;
                }
            }
        }
        self.white_swing(a, hand, false, 0.0);
    }

    pub fn white_swing(&mut self, a: usize, hand: Hand, extra: bool, ap_bonus: f64) {
        let Some(tgt) = self.actors[a].target else { return };
        if !self.actors[tgt].alive {
            return;
        }
        self.actors[a].in_combat = true;
        let tag: &'w str = match hand {
            Hand::Mh => "melee",
            Hand::Oh => "melee (off-hand)",
            Hand::Ranged => "auto shot",
        };
        let mut ev = Ev { hand: Some(hand), white: true, extra, other: Some(tgt), rolled: true, ..Ev::default() };
        self.emit(a, EventKind::MeleeSwing, ev, 0);
        if !self.actors[a].alive || !self.actors[tgt].alive {
            return;
        }
        let roll = self.roll_melee(a, tgt, hand, true, 0.0);
        if self.avoided(a, tgt, roll, hand) {
            return;
        }
        let mut dmg = self.weapon_roll(a, hand, false, ap_bonus);
        if hand == Hand::Oh {
            dmg *= self.actors[a].st[Stat::OffhandMult as usize];
        }
        let x = &self.actors[a];
        dmg *= x.st[Stat::DmgDone as usize] * x.st[Stat::PhysDmgDone as usize] * x.dmg_scale * self.vs_mult(a, tgt);
        if hand == Hand::Ranged {
            dmg *= x.st[Stat::RangedDmgDone as usize];
        }
        let crit = roll == Roll::Crit;
        if crit {
            dmg *= 1.0 + (self.w.combat.melee_crit_mult - 1.0) * (1.0 + x.st[Stat::CritBonusMelee as usize]);
        }
        if roll == Roll::Glance {
            dmg *= CombatCfg::by_diff(&self.w.combat.glance_mult, self.level_diff(a, tgt));
        }
        if roll == Roll::Crush {
            dmg *= 1.5;
        }
        dmg = self.mitigate(a, tgt, dmg, School::Physical, false);
        if roll == Roll::Block {
            dmg = (dmg - self.actors[tgt].st[Stat::BlockValue as usize]).max(0.0);
        }
        ev.amount = dmg;
        ev.crit = crit;
        ev.pct = dmg / self.actors[tgt].max_hp.max(1.0);
        let dealt = self.apply_damage(a, tgt, dmg, ev, tag);
        // rage from white damage
        if self.actors[a].has_res[Res::Rage as usize] {
            let conv = self.w.combat.rage_conversion(self.actors[a].level as f64);
            let mut r = self.w.combat.rage_dealt * dealt / conv * self.actors[a].st[Stat::RageGen as usize];
            // Forever: critical strikes with basic attacks give extra rage
            if crit {
                r *= self.w.combat.rage_crit_mult;
            }
            if hand == Hand::Oh {
                r *= self.actors[a].st[Stat::OffhandRage as usize];
            }
            let x = &mut self.actors[a];
            x.res[Res::Rage as usize] = (x.res[Res::Rage as usize] + r).min(x.max_res[Res::Rage as usize]);
        }
        self.after_hit(a, tgt, ev, hand == Hand::Ranged, false);
    }

    /// One of a boss's scripted attacks (the healing tests), scaled like its
    /// melee by the test's damage scale.
    pub fn script_hit(&mut self, a: usize, h: &'w ScriptHit) {
        let team = self.actors[a].team;
        let mut pool: Vec<usize> = self.live.iter().copied().filter(|&i| { let x = &self.actors[i]; x.alive && x.team != team && !x.is_pet() }).collect();
        let tank = self.actors[a].target.filter(|&t| self.actors[t].alive);
        let targets: Vec<usize> = match h.targets {
            ScriptTargets::Tank => tank.into_iter().collect(),
            ScriptTargets::All => pool,
            ScriptTargets::Random(n) | ScriptTargets::Others(n) => {
                if matches!(h.targets, ScriptTargets::Others(_)) {
                    pool.retain(|&x| Some(x) != tank);
                }
                let mut out = Vec::new();
                for _ in 0..n {
                    if pool.is_empty() {
                        break;
                    }
                    let i = ((self.actors[a].rng[2].f64() * pool.len() as f64) as usize).min(pool.len() - 1);
                    out.push(pool.swap_remove(i));
                }
                out
            }
        };
        let cx = Cx::new(a, None, h.name.as_str());
        let base = self.ev(&h.dmg, &cx) * self.actors[a].dmg_scale;
        let tag: &'w str = h.name.as_str();
        for t in targets {
            if !self.actors[t].alive || !self.actors[a].alive {
                continue;
            }
            let mut dmg = base * self.actors[a].rng[2].range(1.0 - h.spread, 1.0 + h.spread);
            let mut ev = Ev { other: Some(t), school: h.school, special: true, ..Ev::default() };
            if h.melee {
                let roll = self.roll_melee(a, t, Hand::Mh, false, 0.0);
                if self.avoided(a, t, roll, Hand::Mh) {
                    continue;
                }
                if roll == Roll::Crit {
                    dmg *= self.w.combat.melee_crit_mult;
                    ev.crit = true;
                }
                dmg = self.mitigate(a, t, dmg, h.school, false);
                if roll == Roll::Block {
                    dmg = (dmg - self.actors[t].st[Stat::BlockValue as usize]).max(0.0);
                }
                ev.rolled = true;
            } else {
                dmg = self.mitigate(a, t, dmg, h.school, false);
            }
            ev.amount = dmg;
            ev.pct = dmg / self.actors[t].max_hp.max(1.0);
            self.apply_damage(a, t, dmg, ev, tag);
            if self.actors[t].alive {
                let mut evd = ev;
                evd.other = Some(a);
                self.emit(t, EventKind::Struck, evd, 0);
                if h.melee {
                    self.emit(t, EventKind::StruckMelee, evd, 0);
                }
                if ev.crit {
                    self.emit(t, EventKind::StruckCrit, evd, 0);
                }
            }
        }
    }

    /// Handles avoidance events. Returns true if the attack did not land.
    fn avoided(&mut self, a: usize, tgt: usize, roll: Roll, hand: Hand) -> bool {
        let ev = Ev { hand: Some(hand), other: Some(tgt), ..Ev::default() };
        let evd = Ev { hand: Some(hand), other: Some(a), ..Ev::default() };
        if self.log.is_some() && matches!(roll, Roll::Miss | Roll::Dodge | Roll::Parry) {
            let what = match roll { Roll::Miss => "misses", Roll::Dodge => "is dodged by", _ => "is parried by" };
            self.logf(|s| format!("{} {} {}", s.actors[a].name, what, s.actors[tgt].name));
            let kind = match roll { Roll::Miss => "miss", Roll::Dodge => "dodge", _ => "parry" };
            self.tracef(|s| s.tev(kind, a, Some(tgt), ""));
        }
        match roll {
            Roll::Miss => {
                self.actors[tgt].in_combat = true;
                true
            }
            Roll::Dodge => {
                self.emit(a, EventKind::AttackDodged, ev, 0);
                self.emit(tgt, EventKind::Dodge, evd, 0);
                true
            }
            Roll::Parry => {
                self.emit(a, EventKind::AttackParried, ev, 0);
                self.emit(tgt, EventKind::Parry, evd, 0);
                true
            }
            Roll::Block => {
                self.emit(tgt, EventKind::Block, evd, 0);
                false
            }
            _ => false,
        }
    }

    /// Damage multipliers on the attacker side are applied by the caller;
    /// this applies the defender's: damage taken and armour.
    pub fn mitigate(&self, a: usize, tgt: usize, dmg: f64, school: School, periodic: bool) -> f64 {
        let d = &self.actors[tgt];
        let mut m = d.st[Stat::DmgTaken as usize];
        if school == School::Physical {
            m *= d.st[Stat::PhysDmgTaken as usize];
            if !periodic {
                let at = &self.actors[a].st;
                let armor = (d.st[Stat::Armor as usize] * (1.0 - at[Stat::ArmorPenPct as usize]).max(0.0) - at[Stat::ArmorPen as usize]).max(0.0);
                m *= 1.0 - self.w.combat.armor_dr(armor, self.actors[a].level as f64);
            }
        } else {
            m *= d.st[Stat::SpellDmgTaken as usize];
        }
        dmg * m
    }

    fn after_hit(&mut self, a: usize, tgt: usize, ev: Ev, ranged: bool, special: bool) {
        let mut ev = ev;
        ev.special = special;
        let landed = if ranged { EventKind::RangedLanded } else { EventKind::MeleeLanded };
        self.emit(a, landed, ev, 0);
        if !ranged {
            self.emit(a, if special { EventKind::SpecialLanded } else { EventKind::WhiteLanded }, ev, 0);
            if ev.crit {
                self.emit(a, EventKind::MeleeCrit, ev, 0);
            }
        }
        self.emit(a, EventKind::DamageDealt, ev, 0);
        if self.actors[tgt].alive {
            let mut evd = ev;
            evd.other = Some(a);
            self.emit(tgt, EventKind::Struck, evd, 0);
            if !ranged {
                self.emit(tgt, EventKind::StruckMelee, evd, 0);
            }
            if ev.crit {
                self.emit(tgt, EventKind::StruckCrit, evd, 0);
            }
        }
    }

    /// Run a damage effect against its targets. Returns true if anything landed.
    pub fn do_damage(&mut self, fx: &'w DamageFx, cx: Cx<'w>) -> bool {
        let targets = self.select(&fx.targets, &cx, SelUse::Damage);
        let mut any = false;
        for (k, t) in targets.into_iter().enumerate() {
            if self.actors[t].alive && self.actors[cx.me].alive {
                let mut c = cx;
                c.hit = k.min(255) as u8;
                any |= self.damage_one(fx, c, t);
            }
        }
        any
    }

    fn damage_one(&mut self, fx: &'w DamageFx, cx0: Cx<'w>, tgt: usize) -> bool {
        let a = cx0.me;
        let mut cx = cx0;
        cx.tgt = Some(tgt);
        // a weapon effect without its own school deals the weapon's (a wand's)
        let weapon_school = fx.weapon.and_then(|h| self.actors[a].weapons[h as usize]).map(|wp| wp.school).filter(|s| *s != School::Physical);
        let school = fx.school.or(weapon_school).unwrap_or_else(|| cx.spell.map_or(School::Physical, |s| self.w.spells[s as usize].school));
        let crit_add = self.evo(&fx.crit, &cx, 0.0) + cx.sm.crit;
        let hand = fx.weapon.unwrap_or(Hand::Mh);
        if fx.weapon.is_some() && hand != Hand::Ranged && self.actors[a].has_mech(self.w, Mechanic::Disarm)
            && self.actors[a].st[Stat::ImmuneDisarm as usize] <= 0.0
        {
            return false;
        }
        let roll = match fx.hit {
            HitKind::White => self.roll_melee(a, tgt, hand, true, crit_add),
            HitKind::Special => self.roll_melee(a, tgt, hand, false, crit_add),
            HitKind::RangedWhite => self.roll_melee(a, tgt, Hand::Ranged, true, crit_add),
            HitKind::RangedSpecial => self.roll_melee(a, tgt, Hand::Ranged, false, crit_add),
            HitKind::Spell => self.roll_spell(a, tgt, fx.can_crit, crit_add, false),
            HitKind::Wand => {
                let (miss, _, _, _) = self.avoid(a, tgt, false, true, false, false);
                let crit = self.crit_vs(a, tgt, self.actors[a].st[Stat::RangedCrit as usize] + crit_add);
                if self.actors[a].rng[0].f64() * 100.0 < miss {
                    Roll::Miss
                } else if fx.can_crit && self.actors[a].rng[0].f64() * 100.0 < crit {
                    Roll::Crit
                } else {
                    Roll::Hit
                }
            }
            HitKind::Auto => {
                if fx.can_crit {
                    let base = if school == School::Physical {
                        self.actors[a].st[Stat::MeleeCrit as usize]
                    } else {
                        self.actors[a].st[Stat::SpellCrit as usize]
                    };
                    if self.actors[a].rng[0].f64() * 100.0 < base + crit_add {
                        Roll::Crit
                    } else {
                        Roll::Hit
                    }
                } else {
                    Roll::Hit
                }
            }
        };
        let melee_like = matches!(fx.hit, HitKind::White | HitKind::Special | HitKind::RangedWhite | HitKind::RangedSpecial);
        let weapon_like = melee_like || fx.hit == HitKind::Wand;
        if weapon_like {
            if self.avoided(a, tgt, roll, hand) {
                self.run_effects(&fx.on_miss, cx);
                return false;
            }
        } else if roll == Roll::Miss {
            self.run_effects(&fx.on_miss, cx);
            return false;
        }
        let crit = roll == Roll::Crit && fx.can_crit;
        // amount
        let mut dmg = 0.0;
        if let Some(h) = fx.weapon {
            let wm = self.evo(&fx.weapon_mult, &cx, 1.0);
            dmg += self.weapon_roll(a, h, fx.normalized, 0.0) * wm;
        }
        if let (Some(lo), Some(hi)) = (&fx.min, &fx.max) {
            let (lo, hi) = (self.ev(lo, &cx), self.ev(hi, &cx));
            dmg += self.actors[a].rng[0].range(lo, hi);
        } else if let Some(lo) = &fx.min {
            dmg += self.ev(lo, &cx);
        }
        dmg += self.evo(&fx.amount, &cx, 0.0);
        if let Some(c) = &fx.coef {
            let coef = self.ev(c, &cx) * self.downrank(a, cx.rank);
            dmg += coef * self.sp_for(a, school);
        }
        let x = &self.actors[a];
        let mut mult = self.evo(&fx.mult, &cx, 1.0) * cx.sm.dmg * x.st[Stat::DmgDone as usize] * x.st[school.dmg_stat() as usize] * self.vs_mult(a, tgt);
        if fx.weapon == Some(Hand::Ranged) {
            mult *= x.st[Stat::RangedDmgDone as usize];
        }
        if school != School::Physical {
            mult *= x.st[Stat::SpellDmgDone as usize];
        }
        if x.kind == Kind::Enemy {
            mult *= x.dmg_scale;
        }
        if fx.fixed {
            mult = self.evo(&fx.mult, &cx, 1.0);
        }
        dmg *= mult;
        if crit {
            let spell_like = school != School::Physical && !melee_like;
            let (base, bonus_stat) = if spell_like {
                (self.w.combat.spell_crit_mult - 1.0, Stat::CritBonusSpell)
            } else {
                (self.w.combat.melee_crit_mult - 1.0, Stat::CritBonusMelee)
            };
            let bonus = 1.0 + self.actors[a].st[bonus_stat as usize] + self.evo(&fx.crit_bonus, &cx, 0.0) + cx.sm.crit_bonus;
            dmg *= 1.0 + base * bonus;
            dmg *= self.actors[tgt].st[Stat::CritDmgTaken as usize];
        }
        if !fx.fixed {
            dmg = self.mitigate(a, tgt, dmg, school, fx.periodic);
        }
        if roll == Roll::Block {
            dmg = (dmg - self.actors[tgt].st[Stat::BlockValue as usize]).max(0.0);
        }
        let ev = Ev {
            amount: dmg,
            crit,
            hand: fx.weapon,
            special: true,
            periodic: fx.periodic,
            spell: !melee_like,
            pct: dmg / self.actors[tgt].max_hp.max(1.0),
            school,
            other: Some(tgt),
            rolled: fx.can_crit && !fx.periodic,
            by: cx.spell,
            ..Ev::default()
        };
        let tag: &'w str = fx.tag.as_deref().unwrap_or(cx.tag);
        let threat_mult = self.evo(&fx.threat, &cx, 1.0) * cx.sm.threat;
        let dealt = self.apply_damage_t(a, tgt, dmg, ev, tag, threat_mult);
        if let Some(ls) = &fx.lifesteal {
            let f = self.ev(ls, &cx);
            if f > 0.0 {
                self.heal_raw(a, a, dealt * f, tag, false, cx.spell);
            }
        }
        if fx.no_proc {
            // proc-generated damage does not trigger further procs on the attacker
        } else if weapon_like {
            self.after_hit(a, tgt, ev, matches!(fx.hit, HitKind::RangedSpecial | HitKind::RangedWhite | HitKind::Wand), true);
        } else {
            self.emit(a, EventKind::SpellLanded, ev, cx.depth);
            if fx.periodic {
                self.emit(a, EventKind::Periodic, ev, cx.depth);
            }
            if crit {
                self.emit(a, EventKind::SpellCrit, ev, cx.depth);
            }
            self.emit(a, EventKind::DamageDealt, ev, cx.depth);
            if self.actors[tgt].alive {
                let mut evd = ev;
                evd.other = Some(a);
                self.emit(tgt, EventKind::Struck, evd, cx.depth);
                if crit {
                    self.emit(tgt, EventKind::StruckCrit, evd, cx.depth);
                }
            }
        }
        if self.actors[a].alive {
            let mut cx2 = cx;
            cx2.ev = Some(ev);
            self.run_effects(&fx.on_hit, cx2);
            if crit {
                self.run_effects(&fx.on_crit, cx2);
            }
        }
        true
    }

    pub fn apply_damage(&mut self, src: usize, dst: usize, amount: f64, ev: Ev, tag: &'w str) -> f64 {
        self.apply_damage_t(src, dst, amount, ev, tag, 1.0)
    }

    pub fn apply_damage_t(&mut self, src: usize, dst: usize, amount: f64, ev: Ev, tag: &'w str, threat_mult: f64) -> f64 {
        if !self.actors[dst].alive || amount <= 0.0 {
            return 0.0;
        }
        if !self.actors[dst].aware && self.hostile(src, dst) {
            self.wake(dst, Some(src));
        }
        if self.hostile(src, dst) {
            self.rouse(dst, src);
        }
        let w = self.w;
        let mut rest = amount;
        // absorbs
        let mut k = 0;
        let mut shield: Option<u32> = None;
        while k < self.actors[dst].auras.len() && rest > 0.0 {
            let def = &w.auras[self.actors[dst].auras[k].def as usize];
            // a ward takes only its school; Mana Shield pays in mana
            let fits = def.absorb_school.map_or(true, |s| s == ev.school);
            let mpa = def.mana_per_absorb;
            let mana = self.actors[dst].res[Res::Mana as usize];
            let inst = &mut self.actors[dst].auras[k];
            if inst.absorb > 0.0 && fits {
                shield = Some(inst.def);
                let mut used = inst.absorb.min(rest);
                if mpa > 0.0 {
                    used = used.min(mana / mpa);
                }
                inst.absorb -= used;
                rest -= used;
                if mpa > 0.0 {
                    self.actors[dst].res[Res::Mana as usize] -= used * mpa;
                }
                self.metrics[dst].absorbed += used;
                if self.actors[dst].auras[k].absorb <= 1e-6 {
                    self.remove_aura_at(dst, k, false);
                    continue;
                }
            }
            k += 1;
        }
        let rest_before_redirect = rest;
        // redirect to pet (Soul Link)
        let redirect = self.actors[dst].st[Stat::RedirectToPet as usize];
        if redirect > 0.0 && rest > 0.0 {
            if let Some(p) = self.actors[dst].pets.iter().copied().find(|&p| self.actors[p].alive) {
                let part = rest * redirect;
                rest -= part;
                self.actors[p].hp -= part;
                if self.actors[p].hp <= 0.0 {
                    self.kill(p, src);
                }
            }
        }
        let dealt = rest.min(self.actors[dst].hp.max(0.0));
        let src_owner = self.actors[src].owner.unwrap_or(src);
        self.metrics[src].dmg += rest;
        *self.metrics[src].dmg_by.entry(tag).or_insert(0.0) += rest;
        if src_owner != src {
            self.metrics[src_owner].dmg += rest;
            *self.metrics[src_owner].dmg_by.entry(tag).or_insert(0.0) += rest;
        }
        self.metrics[dst].taken += rest;
        if ev.rolled {
            if ev.crit {
                self.metrics[src].crits += 1.0;
            }
            self.metrics[src].hits += 1.0;
        }
        self.actors[dst].hp -= rest;
        if self.actors[dst].unkillable && self.actors[dst].hp < 1.0 {
            self.actors[dst].hp = 1.0;
        }
        if self.log.is_some() {
            let kind = if ev.crit { " CRIT" } else if ev.periodic { " (tick)" } else { "" };
            self.logf(|s| format!("{} {} -> {} {:.0}{} [{:.0}/{:.0}]", s.actors[src].name, tag, s.actors[dst].name, rest, kind, s.actors[dst].hp.max(0.0), s.actors[dst].max_hp));
            let absorbed = amount - rest_before_redirect;
            self.tracef(|s| TraceEv {
                amount: rest, crit: ev.crit, tick: ev.periodic, key: s.spell_key(ev.by), school: ev.school.name(),
                absorbed, via: shield.filter(|_| absorbed > 0.0).map(|a| w.auras[a as usize].name.clone()).unwrap_or_default(),
                hp: Some((s.actors[dst].hp / s.actors[dst].max_hp).clamp(0.0, 1.0)),
                ..s.tev("hit", src, Some(dst), tag)
            });
        }
        self.actors[dst].in_combat = true;
        self.actors[src].in_combat = true;
        let _ = w;
        // threat
        if self.actors[dst].kind == Kind::Enemy {
            let th = rest * self.actors[src].st[Stat::Threat as usize] * threat_mult;
            self.add_threat(dst, src, th);
        }
        // rage from damage taken
        if self.actors[dst].has_res[Res::Rage as usize] && rest > 0.0 {
            let conv = self.w.combat.rage_conversion(self.actors[dst].level as f64);
            let r = self.w.combat.rage_taken * self.rage_taken_mult * rest / conv * self.actors[dst].st[Stat::RageGen as usize];
            let x = &mut self.actors[dst];
            x.res[Res::Rage as usize] = (x.res[Res::Rage as usize] + r).min(x.max_res[Res::Rage as usize]);
        }
        // crowd control that breaks on damage
        let mut j = 0;
        while j < self.actors[dst].auras.len() {
            if let Some(th) = self.actors[dst].auras[j].break_at {
                let maxhp = self.actors[dst].max_hp;
                self.actors[dst].auras[j].dmg_since += rest;
                if self.actors[dst].auras[j].dmg_since > th * maxhp {
                    self.remove_aura_at(dst, j, false);
                    continue;
                }
            }
            j += 1;
        }
        // spell pushback
        let pr = self.actors[dst].st[Stat::PushbackResist as usize];
        let pushback = self.w.combat.pushback;
        let maxp = self.w.combat.max_pushbacks;
        if let Some(c) = &self.actors[dst].cast {
            if c.pushes < maxp && rest > 0.0 {
                let resisted = self.actors[dst].rng[1].f64() < pr;
                if !resisted {
                    let c = self.actors[dst].cast.as_mut().unwrap();
                    c.pushes += 1;
                    if c.channel {
                        c.end -= pushback;
                    } else {
                        c.end += pushback;
                    }
                }
            }
        }
        if self.actors[dst].hp <= 0.0 {
            self.kill(dst, src);
        }
        dealt
    }

    pub fn kill(&mut self, victim: usize, killer: usize) {
        if !self.actors[victim].alive {
            return;
        }
        let w = self.w;
        self.logf(|s| format!("{} dies", s.actors[victim].name));
        self.tracef(|s| s.tev("death", victim, None, ""));
        let v = &mut self.actors[victim];
        v.alive = false;
        v.hp = 0.0;
        v.cast = None;
        v.swing = [INF; 3];
        let owner = v.owner;
        let kind = v.kind.clone();
        self.metrics[victim].deaths += 1.0;
        let _ = w;
        // a totem or trap knocked down; a shaman's totems go when it dies
        if self.actors[victim].totem.is_some() {
            self.despawn_totem(victim);
        }
        for k in 0..self.actors[victim].totems.len() {
            if let Some(t) = self.actors[victim].totems[k] {
                self.despawn_totem(t);
            }
        }
        if let Some(o) = owner {
            self.actors[o].pets.retain(|&p| p != victim);
            if let Kind::Pet(pd) = kind {
                let ev = Ev { pet: Some(pd), ..Ev::default() };
                self.emit(o, EventKind::PetDied, ev, 0);
            }
        }
        if self.hostile(victim, killer) {
            let ev = Ev { other: Some(victim), ..Ev::default() };
            let k = self.actors[killer].owner.unwrap_or(killer);
            if !self.actors[victim].is_pet() {
                self.metrics[k].kills += 1.0;
            }
            if self.actors[k].alive {
                self.emit(k, EventKind::Kill, ev, 0);
            }
        }
    }

    pub fn do_heal(&mut self, fx: &'w HealFx, cx: Cx<'w>) {
        let targets = self.select(&fx.targets, &cx, SelUse::Heal);
        let a = cx.me;
        let falloff = fx.falloff.as_ref().map(|f| self.ev(f, &cx));
        for (k, t) in targets.into_iter().enumerate() {
            if !self.actors[t].alive {
                continue;
            }
            let mut c2 = cx;
            c2.tgt = Some(t);
            c2.hit = k.min(255) as u8;
            let mut rolled = 0.0;
            if let (Some(lo), Some(hi)) = (&fx.min, &fx.max) {
                let (lo, hi) = (self.ev(lo, &c2), self.ev(hi, &c2));
                rolled = self.actors[a].rng[0].range(lo, hi);
            }
            let mut amt = self.heal_amount(fx, &c2, rolled);
            if let Some(f) = falloff {
                amt *= f.powi(k as i32);
            }
            let crit = if fx.can_crit {
                let ca = self.evo(&fx.crit, &c2, 0.0) + cx.sm.crit;
                self.roll_spell(a, t, true, ca, true) == Roll::Crit
            } else {
                false
            };
            if crit {
                amt *= 1.0 + (self.w.combat.spell_crit_mult - 1.0) * (1.0 + self.actors[a].st[Stat::CritBonusHeal as usize]);
            }
            let tag: &'w str = fx.tag.as_deref().unwrap_or(cx.tag);
            let eff = self.heal_raw(a, t, amt, tag, crit, cx.spell);
            let ev = Ev { amount: eff, crit, periodic: fx.periodic, overheal: amt - eff, other: Some(t), by: cx.spell, ..Ev::default() };
            self.emit(a, EventKind::Heal, ev, cx.depth);
            if crit {
                self.emit(a, EventKind::HealCrit, ev, cx.depth);
                let mut c3 = c2;
                c3.ev = Some(ev);
                self.run_effects(&fx.on_crit, c3);
            }
        }
    }

    /// A heal effect's amount before crits and the target's healing taken,
    /// from its rolled base (between `min` and `max`).
    pub fn heal_amount(&self, fx: &HealFx, cx: &Cx<'w>, rolled: f64) -> f64 {
        let a = cx.me;
        let mut amt = rolled;
        amt += self.evo(&fx.amount, cx, 0.0);
        if let Some(c) = &fx.coef {
            amt += self.ev(c, cx) * self.downrank(a, cx.rank) * self.actors[a].st[Stat::HealPower as usize];
        }
        amt * (self.evo(&fx.mult, cx, 1.0) * cx.sm.heal * self.actors[a].st[Stat::HealDone as usize])
    }

    /// What a cast's direct heals land for on average (no crits), or None
    /// when it has none.
    pub fn heal_estimate(&self, cx: &Cx<'w>, effs: &[Effect]) -> Option<f64> {
        let mut total = None;
        for e in effs {
            if let Effect::Heal(fx) = e {
                if fx.periodic || !matches!(fx.targets, TargetSel::Me | TargetSel::Ally | TargetSel::Allies(_)) {
                    continue;
                }
                let t = self.ally_of(cx);
                let mut c2 = *cx;
                c2.tgt = Some(t);
                let mid = match (&fx.min, &fx.max) {
                    (Some(lo), Some(hi)) => 0.5 * (self.ev(lo, &c2) + self.ev(hi, &c2)),
                    _ => 0.0,
                };
                *total.get_or_insert(0.0) += self.heal_amount(fx, &c2, mid) * self.actors[t].st[Stat::HealTaken as usize];
            }
        }
        total
    }

    /// Apply healing; returns effective healing.
    pub fn heal_raw(&mut self, src: usize, dst: usize, amount: f64, tag: &'w str, crit: bool, by: Option<u32>) -> f64 {
        if !self.actors[dst].alive || amount <= 0.0 {
            return 0.0;
        }
        let amount = amount * self.actors[dst].st[Stat::HealTaken as usize];
        let x = &mut self.actors[dst];
        let eff = amount.min(x.max_hp - x.hp).max(0.0);
        x.hp += eff;
        let m = &mut self.metrics[src];
        m.heal += eff;
        m.overheal += amount - eff;
        *m.heal_by.entry(tag).or_insert(0.0) += eff;
        self.metrics[dst].healed_in += eff;
        if self.log.is_some() && eff > 0.0 {
            self.logf(|s| format!("{} {} heals {} {:.0} [{:.0}/{:.0}]", s.actors[src].name, tag, s.actors[dst].name, eff, s.actors[dst].hp, s.actors[dst].max_hp));
            self.tracef(|s| TraceEv {
                amount: eff, crit, key: s.spell_key(by), over: amount - eff,
                hp: Some((s.actors[dst].hp / s.actors[dst].max_hp).clamp(0.0, 1.0)),
                ..s.tev("heal", src, Some(dst), tag)
            });
        }
        // healing threat, split across the mobs already fighting the healer's side
        if eff > 0.0 && self.actors[src].in_combat {
            let team = self.actors[src].team;
            let actors = &self.actors;
            let foes: Vec<usize> = self.live.iter().copied().filter(|&i| {
                let e = &actors[i];
                e.alive && e.aware && e.team != team && e.kind == Kind::Enemy && e.threat.iter().any(|x| x.1 > 0.0 && actors[x.0 as usize].team == team)
            }).collect();
            if !foes.is_empty() {
                let th = self.w.combat.heal_threat * eff * self.actors[src].st[Stat::Threat as usize] / foes.len() as f64;
                for f in foes {
                    self.add_threat(f, src, th);
                }
            }
        }
        eff
    }
}
