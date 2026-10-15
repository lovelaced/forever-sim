//! Data checks: which talents the simulation actually models.

use crate::expr::Expr;
use crate::model::*;
use crate::vars::VarRef;

fn ex(e: &Expr, hit: &mut Vec<bool>) {
    e.visit_vars(&mut |v| {
        if let VarRef::Talent(i) = v {
            if let Some(x) = hit.get_mut(*i as usize) {
                *x = true;
            }
        }
    });
}
fn exo(e: &Option<Expr>, hit: &mut Vec<bool>) {
    if let Some(e) = e {
        ex(e, hit)
    }
}
fn sel(s: &TargetSel, hit: &mut Vec<bool>) {
    if let TargetSel::Enemies(e) = s {
        ex(e, hit)
    }
}

fn effects(es: &[Effect], hit: &mut Vec<bool>) {
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
            Effect::Combo(e) | Effect::Taunt { duration: e } | Effect::Threat { amount: e } | Effect::Absorb { amount: e } => ex(e, hit),
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
            _ => {}
        }
    }
}

fn procs(ps: &[Proc], hit: &mut Vec<bool>) {
    for p in ps {
        exo(&p.cond, hit);
        exo(&p.chance, hit);
        exo(&p.ppm, hit);
        effects(&p.effects, hit);
    }
}

fn passive(p: &Passive, hit: &mut Vec<bool>) {
    exo(&p.cond, hit);
    for m in &p.mods {
        ex(&m.value, hit);
    }
    for m in &p.spell_mods {
        ex(&m.value, hit);
    }
    procs(&p.procs, hit);
}

fn rotation(r: &Rotation, hit: &mut Vec<bool>) {
    for a in r.precombat.iter().chain(&r.opener).chain(&r.actions).chain(&r.rest) {
        exo(&a.cond, hit);
    }
}

/// For each talent of a class: is it referenced anywhere in the data?
/// Talent passives (`mods`/`procs` written on the talent itself) count only
/// through what they reference, plus the talent's own condition, so a talent
/// with its own effects is always modelled.
pub fn modelled_talents(w: &World, class: usize) -> Vec<bool> {
    let c = &w.classes[class];
    let mut hit = vec![false; c.talents.len()];
    for p in &c.passives {
        // the talent-local passive's condition is `t.<id> > 0`
        passive(p, &mut hit);
    }
    for s in w.spells.iter().filter(|s| s.class == Some(class)) {
        if let Some(t) = s.talent {
            hit[t as usize] = true;
        }
        for (_, e) in &s.cost {
            ex(e, &mut hit);
        }
        ex(&s.cast_time, &mut hit);
        ex(&s.cooldown, &mut hit);
        exo(&s.gcd, &mut hit);
        exo(&s.requires, &mut hit);
        effects(&s.effects, &mut hit);
        if let Some(ch) = &s.channel {
            ex(&ch.duration, &mut hit);
            ex(&ch.interval, &mut hit);
            effects(&ch.effects, &mut hit);
        }
    }
    for a in w.auras.iter().filter(|a| a.class == Some(class)) {
        ex(&a.duration, &mut hit);
        exo(&a.charges, &mut hit);
        exo(&a.absorb, &mut hit);
        if let Some((iv, es)) = &a.tick {
            ex(iv, &mut hit);
            effects(es, &mut hit);
        }
        for m in &a.mods {
            ex(&m.value, &mut hit);
        }
        for m in &a.spell_mods {
            ex(&m.value, &mut hit);
        }
        procs(&a.procs, &mut hit);
        for es in [&a.on_apply, &a.on_refresh, &a.on_expire, &a.on_remove] {
            effects(es, &mut hit);
        }
    }
    for r in c.rotations.values() {
        rotation(r, &mut hit);
    }
    for p in w.pets.iter().filter(|p| p.class == class) {
        for e in [&p.hp, &p.armor, &p.dmg_min, &p.dmg_max, &p.mana, &p.ap, &p.sp, &p.crit, &p.dodge] {
            ex(e, &mut hit);
        }
        for m in &p.mods {
            ex(&m.value, &mut hit);
        }
        procs(&p.procs, &mut hit);
        if let Some(r) = &p.rotation {
            rotation(r, &mut hit);
        }
    }
    hit
}
