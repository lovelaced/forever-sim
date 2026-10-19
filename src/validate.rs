//! Data checks: which talents the simulation actually models.

use crate::expr::Expr;
use crate::model::*;
use crate::vars::{Res, VarRef, Who};

type Visit<'a, 'e> = &'a mut dyn FnMut(&'e Expr);

fn ex<'e>(e: &'e Expr, hit: Visit<'_, 'e>) {
    hit(e)
}
fn exo<'e>(e: &'e Option<Expr>, hit: Visit<'_, 'e>) {
    if let Some(e) = e {
        ex(e, hit)
    }
}
fn sel<'e>(s: &'e TargetSel, hit: Visit<'_, 'e>) {
    if let TargetSel::Enemies(e) | TargetSel::OthersNearMe(e) | TargetSel::Chain { max: e, .. } = s {
        ex(e, hit)
    }
}

fn effects<'e>(es: &'e [Effect], hit: Visit<'_, 'e>) {
    for e in es {
        match e {
            Effect::Damage(d) => {
                for o in [&d.weapon_mult, &d.min, &d.max, &d.amount, &d.coef, &d.mult, &d.crit, &d.crit_bonus, &d.threat, &d.lifesteal] {
                    exo(o, hit);
                }
                sel(&d.targets, hit);
                effects(&d.on_hit, hit);
                effects(&d.on_crit, hit);
                effects(&d.on_miss, hit);
            }
            Effect::Heal(h) => {
                for o in [&h.min, &h.max, &h.amount, &h.coef, &h.mult, &h.crit] {
                    exo(o, hit);
                }
                sel(&h.targets, hit);
                effects(&h.on_crit, hit);
            }
            Effect::Resource { amount, who, .. } => {
                ex(amount, hit);
                sel(who, hit)
            }
            Effect::Purge { count, who } | Effect::Cleanse { count, who, .. } => {
                ex(count, hit);
                sel(who, hit)
            }
            Effect::Taunt { duration, who } => {
                ex(duration, hit);
                sel(who, hit)
            }
            Effect::Combo(e) | Effect::Threat { amount: e } | Effect::Absorb { amount: e } => ex(e, hit),
            Effect::ApplyAura { stacks, duration, who, .. } => {
                ex(stacks, hit);
                exo(duration, hit);
                sel(who, hit)
            }
            Effect::Cooldown { value, .. } => ex(value, hit),
            Effect::ExtraAttack { count, ap_bonus, .. } => {
                ex(count, hit);
                exo(ap_bonus, hit)
            }
            Effect::Interrupt { lockout, .. } => ex(lockout, hit),
            Effect::If { cond, then, els } => {
                ex(cond, hit);
                effects(then, hit);
                effects(els, hit)
            }
            Effect::Chance { p, then } => {
                ex(p, hit);
                effects(then, hit)
            }
            Effect::FromSource(then) | Effect::Charge(then) | Effect::Trap { then, .. } => effects(then, hit),
            Effect::StepBack(e) | Effect::Teleport(e) => ex(e, hit),
            _ => {}
        }
    }
}

fn procs<'e>(ps: &'e [Proc], hit: Visit<'_, 'e>) {
    for p in ps {
        exo(&p.cond, hit);
        exo(&p.chance, hit);
        exo(&p.ppm, hit);
        effects(&p.effects, hit);
    }
}

fn passive<'e>(p: &'e Passive, hit: Visit<'_, 'e>) {
    exo(&p.cond, hit);
    for m in &p.mods {
        ex(&m.value, hit);
    }
    for m in &p.spell_mods {
        ex(&m.value, hit);
    }
    procs(&p.procs, hit);
}

fn rotation<'e>(r: &'e Rotation, hit: Visit<'_, 'e>) {
    for a in r.precombat.iter().chain(&r.opener).chain(&r.actions).chain(&r.rest) {
        exo(&a.cond, hit);
    }
}

/// Visit every formula that belongs to a class: its passives, spells, auras,
/// pets and (when `rotations`) its rotations' conditions.
pub fn class_exprs<'e>(w: &'e World, class: usize, rotations: bool, hit: Visit<'_, 'e>) {
    let c = &w.classes[class];
    for p in &c.passives {
        passive(p, hit);
    }
    for s in w.spells.iter().filter(|s| s.class == Some(class)) {
        for (_, e) in &s.cost {
            ex(e, hit);
        }
        ex(&s.cast_time, hit);
        ex(&s.cooldown, hit);
        exo(&s.gcd, hit);
        exo(&s.requires, hit);
        effects(&s.effects, hit);
        if let Some(ch) = &s.channel {
            ex(&ch.duration, hit);
            ex(&ch.interval, hit);
            effects(&ch.effects, hit);
        }
    }
    for a in w.auras.iter().filter(|a| a.class == Some(class)) {
        ex(&a.duration, hit);
        exo(&a.charges, hit);
        exo(&a.absorb, hit);
        if let Some((iv, es)) = &a.tick {
            ex(iv, hit);
            effects(es, hit);
        }
        for m in &a.mods {
            ex(&m.value, hit);
        }
        for m in &a.spell_mods {
            ex(&m.value, hit);
        }
        procs(&a.procs, hit);
        for es in [&a.on_apply, &a.on_refresh, &a.on_expire, &a.on_remove] {
            effects(es, hit);
        }
    }
    if rotations {
        for r in c.rotations.values() {
            rotation(r, hit);
        }
    }
    for p in w.pets.iter().filter(|p| p.class == class) {
        for e in [&p.hp, &p.armor, &p.dmg_min, &p.dmg_max, &p.mana, &p.ap, &p.sp, &p.crit, &p.dodge] {
            ex(e, hit);
        }
        for m in &p.mods {
            ex(&m.value, hit);
        }
        procs(&p.procs, hit);
        if rotations {
            if let Some(r) = &p.rotation {
                rotation(r, hit);
            }
        }
    }
}

/// The talents (indexes) a formula reads.
pub fn talents_in(e: &Expr) -> Vec<u16> {
    let mut v = Vec::new();
    e.visit_vars(&mut |x| {
        if let VarRef::Talent(_, i) = x {
            v.push(*i);
        }
    });
    v
}

/// For each talent of a class: is it referenced anywhere in the data?
/// Talent passives (`mods`/`procs` written on the talent itself) count only
/// through what they reference, plus the talent's own condition, so a talent
/// with its own effects is always modelled.
pub fn modelled_talents(w: &World, class: usize) -> Vec<bool> {
    let c = &w.classes[class];
    let mut hit = vec![false; c.talents.len()];
    for s in w.spells.iter().filter(|s| s.class == Some(class)) {
        if let Some(t) = s.talent {
            hit[t as usize] = true;
        }
    }
    class_exprs(w, class, true, &mut |e| {
        for i in talents_in(e) {
            if let Some(x) = hit.get_mut(i as usize) {
                *x = true;
            }
        }
    });
    hit
}

/// Rotation lines that should probably be emergencies: a long cooldown
/// (a minute or more) taken on your own health, which a typical player
/// would otherwise get round to late.
pub fn rotation_warnings(w: &World) -> Vec<String> {
    let mut out = vec![];
    for c in &w.classes {
        for (rid, r) in &c.rotations {
            for act in r.actions.iter().filter(|a| !a.emergency) {
                let sp = &w.spells[act.spell as usize];
                let long = sp.cooldown.is_const().map_or(false, |cd| cd >= 60.0);
                let mut own_hp = false;
                if let Some(e) = &act.cond {
                    e.visit_vars(&mut |v| own_hp |= matches!(v, VarRef::ResPct(Who::Me, Res::Hp) | VarRef::Res(Who::Me, Res::Hp)));
                }
                if long && own_hp {
                    out.push(format!("classes.{}.rotations.{rid}: `{}` reads your health on a {} sec cooldown; mark it `emergency = true` if it saves your life", c.id, act.line, sp.cooldown.is_const().unwrap_or(0.0)));
                }
            }
        }
    }
    out
}
