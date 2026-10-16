//! The combat engine: a discrete-event simulation. Time jumps from one
//! event (swing, cast completion, aura tick or expiry, resource tick,
//! cooldown becoming ready) to the next; actors pick actions from their
//! rotation whenever they are free.

pub mod act;
pub mod combat;

use crate::build::Build;
use crate::expr::{Env, Expr};
use crate::model::*;
use crate::rng::Rng;
use crate::vars::*;
use std::collections::BTreeMap;

pub const INF: f64 = f64::INFINITY;
const EPS: f64 = 1e-9;

/// Data about the event that triggered a proc.
#[derive(Clone, Copy, Debug)]
pub struct Ev {
    pub amount: f64,
    pub crit: bool,
    pub hand: Option<Hand>,
    pub special: bool,
    pub periodic: bool,
    pub spell: bool,
    pub pct: f64,
    pub school: School,
    pub white: bool,
    pub overheal: f64,
    pub pet: Option<u16>,
    pub extra: bool,
    pub other: Option<usize>,
    /// The hit went through an attack table that allows crits (counts towards crit rate).
    pub rolled: bool,
    /// The spell whose effect produced this event, if any (`spell.<id>` in conditions).
    pub by: Option<u32>,
}

impl Default for Ev {
    fn default() -> Self {
        Ev {
            amount: 0.0,
            crit: false,
            hand: None,
            special: false,
            periodic: false,
            spell: false,
            pct: 0.0,
            school: School::Physical,
            white: false,
            overheal: 0.0,
            pet: None,
            extra: false,
            other: None,
            rolled: false,
            by: None,
        }
    }
}

/// Per-cast modifiers gathered from spell mods.
#[derive(Clone, Copy, Debug)]
pub struct SmVals {
    pub cost_add: f64,
    pub cost_mul: f64,
    pub cast_add: f64,
    pub cast_mul: f64,
    pub cd_add: f64,
    pub cd_mul: f64,
    pub dmg: f64,
    pub heal: f64,
    pub crit: f64,
    pub crit_bonus: f64,
    pub threat: f64,
    pub gcd_add: f64,
}

impl Default for SmVals {
    fn default() -> Self {
        SmVals {
            cost_add: 0.0,
            cost_mul: 1.0,
            cast_add: 0.0,
            cast_mul: 1.0,
            cd_add: 0.0,
            cd_mul: 1.0,
            dmg: 1.0,
            heal: 1.0,
            crit: 0.0,
            crit_bonus: 0.0,
            threat: 1.0,
            gcd_add: 0.0,
        }
    }
}

/// Evaluation context: who "self" and "target" are, which spell rank and
/// aura stack count formulas see, and the triggering event.
#[derive(Clone, Copy, Debug)]
pub struct Cx<'w> {
    pub me: usize,
    pub tgt: Option<usize>,
    pub rank: Option<(SpellId, usize)>,
    pub stacks: f64,
    pub ev: Option<Ev>,
    pub aura: Option<(usize, u64)>,
    pub spell: Option<SpellId>,
    pub depth: u8,
    pub tag: &'w str,
    pub sm: SmVals,
}

impl<'w> Cx<'w> {
    pub fn new(me: usize, tgt: Option<usize>, tag: &'w str) -> Self {
        Cx { me, tgt, rank: None, stacks: 1.0, ev: None, aura: None, spell: None, depth: 0, tag, sm: SmVals::default() }
    }
}

#[derive(Clone, Debug)]
pub struct AuraInst {
    pub def: AuraId,
    pub src: usize,
    pub rank: Option<(SpellId, usize)>,
    /// The spell whose own effects applied the aura (with or without ranks):
    /// its spell modifiers apply to the aura's ticks.
    pub spell: Option<SpellId>,
    pub expires: f64,
    pub stacks: u32,
    pub charges: Option<f64>,
    pub next_tick: f64,
    pub tick_iv: f64,
    pub absorb: f64,
    pub applied: f64,
    pub dmg_since: f64,
    pub uid: u64,
    /// Counter value when the aura was last applied or refreshed: a spell
    /// doesn't use up an aura its own effects just granted.
    pub fresh: u64,
}

#[derive(Clone, Debug)]
pub struct CastState {
    pub spell: SpellId,
    pub rank: Option<usize>,
    pub target: usize,
    pub end: f64,
    pub pushes: u32,
    pub channel: bool,
    pub next_tick: f64,
    pub tick_iv: f64,
    pub sm: SmVals,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Player,
    Pet(u16),
    Enemy,
}

#[derive(Clone, Debug)]
pub struct Actor<'w> {
    pub id: usize,
    pub name: String,
    pub kind: Kind,
    pub team: u8,
    pub class: Option<usize>,
    pub race: Option<usize>,
    pub level: u32,
    pub talents: Vec<u8>,
    pub base: [f64; 7],
    pub gear: Vec<(Stat, f64)>,
    pub fixed: Option<[f64; 7]>, // pets/enemies: hp, armor, ap, crit, dodge, mana, parry
    pub creature: CreatureType,
    pub st: [f64; N_STATS],
    pub hp: f64,
    pub max_hp: f64,
    pub res: [f64; N_RES],
    pub max_res: [f64; N_RES],
    pub has_res: [bool; N_RES],
    pub base_mana: f64,
    pub cp: f64,
    pub auras: Vec<AuraInst>,
    pub cds: Vec<(u32, f64)>,
    pub gcd_end: f64,
    pub cast: Option<CastState>,
    pub swing: [f64; 3],
    pub queued: Option<SpellId>,
    /// The auto-repeat spell (a wand's Shoot) firing on each ranged swing.
    pub wanding: Option<SpellId>,
    pub target: Option<usize>,
    pub alive: bool,
    pub rotation: Option<&'w Rotation>,
    pub auto_melee: bool,
    pub auto_ranged: bool,
    pub threat: Vec<f64>,
    pub taunt: Option<(usize, f64)>,
    pub last_mana_spend: f64,
    pub next_tick: f64,
    pub rng: [Rng; 3],
    pub icd: Vec<(u32, f64)>,
    pub pets: Vec<usize>,
    pub owner: Option<usize>,
    pub pet_def: Option<&'w PetDef>,
    pub enemy: Option<&'w EnemyDef>,
    pub behind: bool,
    pub form: Option<u16>,
    pub in_combat: bool,
    pub dmg_scale: f64,
    pub stun_sched: Vec<(f64, f64)>,
    /// PvP diminishing returns per mechanic: (window ends, applications in window).
    pub dr: [(f64, u8); 9],
    pub passives: Vec<&'w Passive>,
    pub weapons: [Option<Weapon>; 3],
    pub shield: Option<(f64, f64)>,
    pub timed_hp: Option<(f64, f64)>,
    pub slot: u64,
    pub lockout: f64,
}

impl<'w> Actor<'w> {
    pub fn is_pet(&self) -> bool {
        matches!(self.kind, Kind::Pet(_))
    }
    pub fn hp_pct(&self, t: f64) -> f64 {
        if let Some((start, dur)) = self.timed_hp {
            return (1.0 - (t - start) / dur).clamp(0.0, 1.0);
        }
        if self.max_hp > 0.0 { self.hp / self.max_hp } else { 0.0 }
    }
    pub fn aura_index(&self, def: AuraId, src: Option<usize>) -> Option<usize> {
        if let Some(s) = src {
            if let Some(i) = self.auras.iter().position(|a| a.def == def && a.src == s) {
                return Some(i);
            }
        }
        self.auras.iter().position(|a| a.def == def)
    }
    pub fn has_mech(&self, w: &World, m: Mechanic) -> bool {
        self.auras.iter().any(|a| w.auras[a.def as usize].mechanic == Some(m))
    }
    pub fn cd_ready(&self, key: u32) -> f64 {
        self.cds.iter().find(|c| c.0 == key).map_or(0.0, |c| c.1)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Metrics<'w> {
    pub dmg: f64,
    pub dmg_by: BTreeMap<&'w str, f64>,
    pub heal: f64,
    pub heal_by: BTreeMap<&'w str, f64>,
    pub overheal: f64,
    pub taken: f64,
    pub absorbed: f64,
    pub casts: BTreeMap<&'w str, f64>,
    pub kills: f64,
    pub deaths: f64,
    pub combat_time: f64,
    pub rest_time: f64,
    pub eats: f64,
    pub drinks: f64,
    pub crits: f64,
    pub hits: f64,
    /// Times each line of the rotation's fight list fired (by line index).
    pub lines: Vec<f64>,
    /// Times each line of its between-pulls list fired.
    pub rest_lines: Vec<f64>,
}

/// One thing that happened in a traced fight: the website's timeline.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TraceEv {
    pub t: f64,
    /// cast, hit, miss, dodge, parry, heal, gain, lose or death
    pub kind: &'static str,
    pub who: String,
    pub team: u8,
    pub pet: bool,
    pub target: Option<String>,
    pub name: String,
    pub amount: f64,
    pub crit: bool,
    pub tick: bool,
    pub stacks: f64,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stop {
    /// Run until the time, a team-0 player dies, or all team-1 actors die.
    Fight { until: f64 },
    /// Run until the actor has nothing left to do in its rest list.
    Rest { actor: usize, until: f64 },
    /// Just let time pass (travel).
    Until(f64),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Timeout,
    PlayerDied,
    EnemiesDead,
    Idle,
}

pub struct Sim<'w> {
    pub w: &'w World,
    pub t: f64,
    pub actors: Vec<Actor<'w>>,
    pub opts: Vec<f64>,
    pub seed: u64,
    pub combat_start: f64,
    pub rest_phase: bool,
    pub metrics: Vec<Metrics<'w>>,
    pub uid: u64,
    pub log: Option<Vec<String>>,
    /// Structured events, recorded whenever `log` is on.
    pub trace: Option<Vec<TraceEv>>,
    /// Coefficient penalties switched by assumptions (see downrank()).
    pub low_level_penalty: f64,
    pub downrank_penalty: f64,
    /// Scales rage from damage taken (assumption `rage_taken_mult`).
    pub rage_taken_mult: f64,
}

struct EnvRef<'a, 'w> {
    s: &'a Sim<'w>,
    cx: &'a Cx<'w>,
}

impl<'a, 'w> Env for EnvRef<'a, 'w> {
    fn var(&self, v: &VarRef) -> f64 {
        self.s.var(v, self.cx)
    }
}

#[inline]
fn b(x: bool) -> f64 {
    if x { 1.0 } else { 0.0 }
}

impl<'w> Sim<'w> {
    pub fn new(w: &'w World, seed: u64, opts: Vec<f64>) -> Self {
        let opt = |name: &str| w.opt_index(name).and_then(|i| opts.get(i as usize).copied()).unwrap_or(1.0);
        let (low_level_penalty, downrank_penalty) = (opt("low_level_penalty"), opt("downrank_penalty"));
        let rage_taken_mult = opt("rage_taken_mult");
        Sim {
            low_level_penalty,
            downrank_penalty,
            rage_taken_mult,
            w,
            t: 0.0,
            actors: vec![],
            opts,
            seed,
            combat_start: 0.0,
            rest_phase: false,
            metrics: vec![],
            uid: 0,
            log: None,
            trace: None,
        }
    }

    /// Headline numbers for a character sheet.
    pub fn sheet(&self, a: usize) -> Vec<(&'static str, f64)> {
        let x = &self.actors[a];
        let st = |s: Stat| x.st[s as usize];
        let wpn = |h: usize| x.weapons[h].map_or(0.0, |w| w.dps());
        let wsp = |h: usize| x.weapons[h].map_or(0.0, |w| w.speed);
        vec![
            ("health", x.max_hp),
            ("mana", x.max_res[Res::Mana as usize]),
            ("energy", x.max_res[Res::Energy as usize]),
            ("strength", st(Stat::Str)),
            ("agility", st(Stat::Agi)),
            ("stamina", st(Stat::Sta)),
            ("intellect", st(Stat::Int)),
            ("spirit", st(Stat::Spi)),
            ("attack power", st(Stat::Ap)),
            ("ranged attack power", st(Stat::Rap)),
            ("melee crit %", st(Stat::MeleeCrit)),
            ("ranged crit %", st(Stat::RangedCrit)),
            ("spell crit %", st(Stat::SpellCrit)),
            ("hit %", st(Stat::Hit)),
            ("spell hit %", st(Stat::SpellHit)),
            ("spell power", st(Stat::Sp)),
            ("healing power", st(Stat::HealPower)),
            ("melee haste x", st(Stat::MeleeHaste)),
            ("cast haste x", st(Stat::CastHaste)),
            ("armor", st(Stat::Armor)),
            ("defense (bonus)", st(Stat::Defense)),
            ("dodge %", st(Stat::Dodge)),
            ("parry %", st(Stat::Parry)),
            ("block %", st(Stat::Block)),
            ("block value", st(Stat::BlockValue)),
            ("mp5", st(Stat::Mp5)),
            ("damage done x", st(Stat::DmgDone)),
            ("physical damage x", st(Stat::PhysDmgDone)),
            ("damage taken x", st(Stat::DmgTaken)),
            ("main hand dps", wpn(0)),
            ("main hand speed", wsp(0)),
            ("off hand dps", wpn(1)),
            ("off hand speed", wsp(1)),
            ("ranged dps", wpn(2)),
        ]
    }

    /// Apply any scripted stuns (pressure attackers) that are due now.
    pub fn scripted_stuns(&mut self, a: usize) {
        let t = self.t;
        let due: Vec<(f64, f64)> = self.actors[a].stun_sched.iter().copied().filter(|s| s.0 <= t + EPS).collect();
        if due.is_empty() {
            return;
        }
        self.actors[a].stun_sched.retain(|s| s.0 > t + EPS);
        if let (Some(tg), Some(st)) = (self.actors[a].target, self.w.aura_index.get("stun").copied()) {
            for (_, d) in due {
                if self.actors[tg].alive {
                    let cx = Cx::new(a, Some(tg), "stun");
                    self.apply_aura(tg, st, a, 1, Some(d), None, cx);
                }
            }
        }
    }

    /// A trace event for actor `who`, filled in by the caller.
    pub fn tev(&self, kind: &'static str, who: usize, target: Option<usize>, name: &str) -> TraceEv {
        let x = &self.actors[who];
        TraceEv {
            t: self.t, kind, who: x.name.clone(), team: x.team, pet: x.owner.is_some(),
            target: target.map(|t| self.actors[t].name.clone()), name: name.to_string(),
            amount: 0.0, crit: false, tick: false, stacks: 0.0,
        }
    }

    pub fn tracef(&mut self, ev: impl FnOnce(&Self) -> TraceEv) {
        if self.trace.is_some() {
            let e = ev(self);
            if let Some(t) = &mut self.trace {
                t.push(e);
            }
        }
    }

    pub fn logf(&mut self, msg: impl FnOnce(&Self) -> String) {
        if self.log.is_some() {
            let line = format!("{:8.2} {}", self.t, msg(self));
            if let Some(l) = &mut self.log {
                l.push(line);
            }
        }
    }

    #[inline]
    pub fn ev(&self, e: &Expr, cx: &Cx<'w>) -> f64 {
        if let Some(c) = e.is_const() {
            return c;
        }
        e.eval(&EnvRef { s: self, cx })
    }
    pub fn evo(&self, e: &Option<Expr>, cx: &Cx<'w>, d: f64) -> f64 {
        match e {
            Some(e) => self.ev(e, cx),
            None => d,
        }
    }

    pub fn who(&self, cx: &Cx<'w>, who: Who) -> Option<usize> {
        let me = &self.actors[cx.me];
        match who {
            Who::Me => Some(cx.me),
            Who::Target => cx.tgt.or(me.target),
            Who::Owner => me.owner,
            Who::Pet => me.pets.iter().copied().find(|&p| self.actors[p].alive),
        }
    }

    fn talent_owner(&self, a: usize) -> usize {
        match self.actors[a].owner {
            Some(o) if self.actors[a].talents.is_empty() => o,
            _ => a,
        }
    }

    pub fn known(&self, a: usize, spell: SpellId) -> Option<usize> {
        let sp = &self.w.spells[spell as usize];
        let act = &self.actors[a];
        let tal_actor = &self.actors[self.talent_owner(a)];
        if let Some(c) = sp.class {
            if tal_actor.class != Some(c) {
                return None;
            }
        }
        if let Some(r) = sp.race {
            if act.race != Some(r as usize) {
                return None;
            }
        }
        if let Some(t) = sp.talent {
            if tal_actor.talents.get(t as usize).copied().unwrap_or(0) == 0 {
                return None;
            }
        }
        // pets know their class's spells at the owner's level
        let lvl = if act.is_pet() { tal_actor.level } else { act.level };
        sp.rank_at(lvl)
    }

    /// A field of the rank being used, for the caster's level (ranks grow
    /// with level up to their `max_level`).
    pub fn rank_field(&self, rank: Option<(SpellId, usize)>, field: u16, level: u32) -> f64 {
        rank.and_then(|(s, r)| self.w.spells[s as usize].ranks.get(r)).and_then(|rd| rd.value(field, level)).unwrap_or(0.0)
    }

    pub fn var(&self, v: &VarRef, cx: &Cx<'w>) -> f64 {
        let w = self.w;
        match v {
            VarRef::Talent(i) => {
                let o = self.talent_owner(cx.me);
                self.actors[o].talents.get(*i as usize).copied().unwrap_or(0) as f64
            }
            VarRef::Rank(f) => self.rank_field(cx.rank, *f, self.actors[cx.me].level),
            VarRef::Opt(i) => self.opts[*i as usize],
            VarRef::Stat(who, s) => self.who(cx, *who).map_or(0.0, |a| self.actors[a].st[*s as usize]),
            VarRef::Res(who, r) => self.who(cx, *who).map_or(0.0, |a| {
                let x = &self.actors[a];
                if *r == Res::Hp { x.hp } else { x.res[*r as usize] }
            }),
            VarRef::ResMax(who, r) => self.who(cx, *who).map_or(0.0, |a| {
                let x = &self.actors[a];
                if *r == Res::Hp { x.max_hp } else { x.max_res[*r as usize] }
            }),
            VarRef::ResPct(who, r) => self.who(cx, *who).map_or(0.0, |a| {
                let x = &self.actors[a];
                if *r == Res::Hp {
                    x.hp_pct(self.t)
                } else if x.max_res[*r as usize] > 0.0 {
                    x.res[*r as usize] / x.max_res[*r as usize]
                } else {
                    0.0
                }
            }),
            VarRef::BaseMana(who) => self.who(cx, *who).map_or(0.0, |a| self.actors[a].base_mana),
            VarRef::BaseHp(who) => self.who(cx, *who).map_or(0.0, |a| self.actors[a].base[5]),
            VarRef::Combo => self.actors[cx.me].cp,
            VarRef::Level(who) => self.who(cx, *who).map_or(0.0, |a| self.actors[a].level as f64),
            VarRef::Time => self.t,
            VarRef::CombatTime => self.t - self.combat_start,
            VarRef::AuraUp(who, id) => self.who(cx, *who).map_or(0.0, |a| {
                b(self.actors[a].aura_index(*id, if *who == Who::Me { None } else { Some(cx.me) }).is_some())
            }),
            VarRef::AuraStacks(who, id) => self.who(cx, *who).map_or(0.0, |a| {
                let x = &self.actors[a];
                x.aura_index(*id, Some(cx.me)).map_or(0.0, |i| x.auras[i].stacks as f64)
            }),
            VarRef::AuraRemains(who, id) => self.who(cx, *who).map_or(0.0, |a| {
                let x = &self.actors[a];
                x.aura_index(*id, Some(cx.me)).map_or(0.0, |i| (x.auras[i].expires - self.t).max(0.0).min(1e6))
            }),
            VarRef::AuraCharges(who, id) => self.who(cx, *who).map_or(0.0, |a| {
                let x = &self.actors[a];
                x.aura_index(*id, Some(cx.me)).map_or(0.0, |i| x.auras[i].charges.unwrap_or(0.0))
            }),
            VarRef::Stacks => cx.stacks,
            VarRef::CdReady(s) => {
                let key = self.cd_key(*s);
                b(self.actors[cx.me].cd_ready(key) <= self.t + EPS)
            }
            VarRef::CdRemains(s) => {
                let key = self.cd_key(*s);
                (self.actors[cx.me].cd_ready(key) - self.t).max(0.0)
            }
            VarRef::Known(s) => b(self.known(cx.me, *s).is_some()),
            VarRef::Mech(who, m) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].has_mech(w, *m))),
            VarRef::Casting(who) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].cast.is_some())),
            VarRef::Wanding(who) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].wanding.is_some())),
            VarRef::IsPlayer(who) => self.who(cx, *who).map_or(0.0, |a| {
                let x = &self.actors[a];
                b(x.kind == Kind::Player || x.enemy.map_or(false, |e| e.player_like))
            }),
            VarRef::Boss(who) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].enemy.map_or(false, |e| e.boss))),
            VarRef::Creature(who, c) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].creature == *c)),
            VarRef::Alive(who) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].alive)),
            VarRef::Behind => b(self.actors[cx.me].behind),
            VarRef::Enemies => {
                let team = self.actors[cx.me].team;
                self.actors.iter().filter(|x| x.alive && x.team != team && !x.is_pet()).count() as f64
            }
            VarRef::Weapon(h, f) => {
                let a = &self.actors[cx.me];
                match a.weapons[*h as usize] {
                    None => 0.0,
                    Some(wp) => match f {
                        WField::Speed => {
                            let o = a.st[Stat::WeaponSpeedOverride as usize];
                            if o > 0.0 && *h != Hand::Ranged { o } else { wp.speed }
                        }
                        WField::Dps => wp.dps(),
                        WField::Min => wp.min,
                        WField::Max => wp.max,
                        WField::Equipped => 1.0,
                    },
                }
            }
            VarRef::WeaponFamily(h, wt) => {
                let fam = WeaponType::ALL[*wt as usize].family();
                b(self.actors[cx.me].weapons[*h as usize].map_or(false, |wp| wp.wtype.family() == fam))
            }
            VarRef::HasShield => b(self.actors[cx.me].shield.is_some()),
            VarRef::DualWield => b(self.actors[cx.me].weapons[1].is_some()),
            VarRef::TwoHand => b(self.actors[cx.me].weapons[0].map_or(false, |wp| wp.wtype.two_handed())),
            VarRef::Form(f) => b(self.actors[cx.me].form == Some(*f)),
            VarRef::InAnyForm => b(self.actors[cx.me].form.is_some()),
            VarRef::Race(r) => b(self.actors[self.talent_owner(cx.me)].race == Some(*r as usize)),
            VarRef::Class(c) => b(self.actors[self.talent_owner(cx.me)].class == Some(*c as usize)),
            VarRef::Event(f) => match &cx.ev {
                None => 0.0,
                Some(e) => match f {
                    EvField::Amount => e.amount,
                    EvField::Crit => b(e.crit),
                    EvField::Mh => b(e.hand == Some(Hand::Mh)),
                    EvField::Oh => b(e.hand == Some(Hand::Oh)),
                    EvField::Ranged => b(e.hand == Some(Hand::Ranged)),
                    EvField::Special => b(e.special),
                    EvField::Periodic => b(e.periodic),
                    EvField::Spell => b(e.spell),
                    EvField::Pct => e.pct,
                    EvField::Physical => b(e.school == School::Physical),
                    EvField::White => b(e.white),
                    EvField::Overheal => e.overheal,
                    EvField::Rage => e.amount,
                    EvField::Extra => b(e.extra),
                },
            },
            VarRef::EventPet(p) => b(cx.ev.map_or(false, |e| e.pet == Some(*p))),
            VarRef::PetActive(p) => {
                let me = &self.actors[cx.me];
                b(me.pets.iter().any(|&pi| {
                    let pa = &self.actors[pi];
                    pa.alive && match p {
                        None => true,
                        Some(d) => pa.kind == Kind::Pet(*d),
                    }
                }))
            }
            VarRef::GcdRemains => (self.actors[cx.me].gcd_end - self.t).max(0.0),
            VarRef::InCombat => b(self.actors[cx.me].in_combat),
            VarRef::Spell(s) => b(cx.spell == Some(*s as u32) || cx.ev.map_or(false, |e| e.by == Some(*s as u32))),
        }
    }

    pub fn cd_key(&self, spell: SpellId) -> u32 {
        match self.w.spells[spell as usize].cd_group {
            Some(g) => 0x8000_0000 | g,
            None => spell,
        }
    }

    // ------------------------------------------------------------ spawning

    fn new_actor(&mut self, name: String, kind: Kind, team: u8, level: u32, slot: u64) -> usize {
        let id = self.actors.len();
        let mk = |k: u64| Rng::from_keys(self.seed, &[slot, k]);
        let phase = mk(99).range(0.0, self.w.combat.tick);
        self.actors.push(Actor {
            id,
            name,
            kind,
            team,
            class: None,
            race: None,
            level,
            talents: vec![],
            base: [20.0, 20.0, 20.0, 20.0, 20.0, 100.0, 0.0],
            gear: vec![],
            fixed: None,
            creature: CreatureType::Humanoid,
            st: [0.0; N_STATS],
            hp: 1.0,
            max_hp: 1.0,
            res: [0.0; N_RES],
            max_res: [0.0; N_RES],
            has_res: [true, false, false, false],
            base_mana: 0.0,
            cp: 0.0,
            auras: vec![],
            cds: vec![],
            gcd_end: 0.0,
            cast: None,
            swing: [INF; 3],
            queued: None,
            wanding: None,
            target: None,
            alive: true,
            rotation: None,
            auto_melee: false,
            auto_ranged: false,
            threat: vec![],
            taunt: None,
            last_mana_spend: -100.0,
            next_tick: self.t + phase,
            rng: [mk(1), mk(2), mk(3)],
            icd: vec![],
            pets: vec![],
            owner: None,
            pet_def: None,
            enemy: None,
            behind: false,
            form: None,
            in_combat: false,
            dmg_scale: 1.0,
            stun_sched: vec![],
            dr: [(0.0, 0); 9],
            passives: vec![],
            weapons: [None; 3],
            shield: None,
            timed_hp: None,
            slot,
            lockout: 0.0,
        });
        self.metrics.push(Metrics::default());
        id
    }

    pub fn spawn_player(&mut self, bd: &Build, team: u8, slot: u64) -> usize {
        let w = self.w;
        let c = &w.classes[bd.class];
        let r = &w.races[bd.race];
        let id = self.new_actor(bd.name.clone(), Kind::Player, team, bd.level, slot);
        let mut base = c.base_at(bd.level);
        for i in 0..5 {
            base[i] += r.stats[i];
        }
        {
            let a = &mut self.actors[id];
            a.class = Some(bd.class);
            a.race = Some(bd.race);
            a.talents = bd.talents.clone();
            a.base = base;
            a.base_mana = base[6];
            a.gear = bd.gear.stats.clone();
            a.creature = r.creature;
            a.weapons = [bd.gear.mh, bd.gear.oh, bd.gear.ranged];
            a.shield = bd.gear.shield;
            if a.shield.is_some() {
                a.weapons[1] = None;
            }
            a.rotation = c.rotations.get(&bd.rotation);
            a.has_res = [true, false, false, false];
            for res in &c.resources {
                a.has_res[*res as usize] = true;
            }
            let mut ps: Vec<&'w Passive> = c.passives.iter().collect();
            ps.extend(r.passives.iter());
            ps.extend(w.common_passives.iter());
            a.passives = ps;
            if let Some(rot) = a.rotation {
                a.auto_melee = rot.auto_melee;
                a.auto_ranged = rot.auto_ranged;
            }
        }
        self.refresh_stats(id);
        self.fill(id);
        id
    }

    pub fn spawn_enemy(&mut self, e: &'w EnemyDef, level: u32, team: u8, slot: u64, scale: f64) -> usize {
        let id = self.new_actor(e.name.clone(), Kind::Enemy, team, level, slot);
        let cx = Cx::new(id, None, "");
        let hp = self.ev(&e.hp, &cx);
        let armor = self.ev(&e.armor, &cx);
        let dmg = self.ev(&e.dmg, &cx);
        let dodge = e.dodge.as_ref().map_or(-1.0, |x| self.ev(x, &cx));
        let parry = e.parry.as_ref().map_or(-1.0, |x| self.ev(x, &cx));
        {
            let a = &mut self.actors[id];
            a.enemy = Some(e);
            a.creature = e.creature;
            a.fixed = Some([hp, armor, 0.0, e.crit, dodge, 0.0, parry]);
            a.weapons = [Some(Weapon { min: dmg * (1.0 - e.dmg_spread), max: dmg * (1.0 + e.dmg_spread), speed: e.speed, wtype: WeaponType::Natural, school: School::Physical }), None, None];
            a.auto_melee = e.attacks;
            a.rotation = e.rotation.as_ref();
            a.behind = e.behind;
            a.dmg_scale = scale;
            a.has_res = [true, true, false, false];
            if e.block > 0.0 {
                a.shield = Some((0.0, 0.0));
            }
        }
        self.refresh_stats(id);
        self.fill(id);
        id
    }

    pub fn spawn_pet(&mut self, owner: usize, pet: u16) -> usize {
        let w = self.w;
        let pd = &w.pets[pet as usize];
        let ol = self.actors[owner].level;
        let slot = self.actors[owner].slot * 64 + 7 + pet as u64;
        let team = self.actors[owner].team;
        let id = self.new_actor(pd.name.clone(), Kind::Pet(pet), team, ol, slot);
        let cx = Cx::new(owner, None, "");
        let hp = self.ev(&pd.hp, &cx);
        let armor = self.ev(&pd.armor, &cx);
        let ap = self.ev(&pd.ap, &cx);
        let crit = self.ev(&pd.crit, &cx);
        let dodge = self.ev(&pd.dodge, &cx);
        let mana = self.ev(&pd.mana, &cx);
        let mn = self.ev(&pd.dmg_min, &cx);
        let mx = self.ev(&pd.dmg_max, &cx);
        let (otarget, ocombat) = (self.actors[owner].target, self.actors[owner].in_combat);
        {
            let a = &mut self.actors[id];
            a.owner = Some(owner);
            a.pet_def = Some(pd);
            a.creature = pd.creature;
            a.fixed = Some([hp, armor, ap, crit, dodge, mana, 5.0]);
            a.weapons = [Some(Weapon { min: mn, max: mx, speed: pd.speed, wtype: WeaponType::Natural, school: School::Physical }), None, None];
            a.auto_melee = pd.auto_melee;
            a.rotation = pd.rotation.as_ref();
            a.has_res = [true, mana > 0.0, false, false];
            a.behind = true;
            a.target = otarget;
            a.in_combat = ocombat;
        }
        self.actors[owner].pets.push(id);
        self.refresh_stats(id);
        self.fill(id);
        if self.actors[id].in_combat {
            let s = self.actors[id].weapons[0].map_or(2.0, |w| w.speed);
            self.actors[id].swing[0] = self.t + 0.5 * s;
        }
        id
    }

    /// Full health and mana, empty rage, full energy.
    pub fn fill(&mut self, a: usize) {
        let x = &mut self.actors[a];
        x.hp = x.max_hp;
        x.res[Res::Mana as usize] = x.max_res[Res::Mana as usize];
        x.res[Res::Energy as usize] = x.max_res[Res::Energy as usize];
        x.res[Res::Rage as usize] = 0.0;
    }

    // ------------------------------------------------------------ stats

    pub fn refresh_stats(&mut self, a: usize) {
        let w = self.w;
        let mut add = [0.0f64; N_STATS];
        let mut mul = [1.0f64; N_STATS];
        let mut item_armor = 0.0;
        {
            let x = &self.actors[a];
            for &(s, v) in &x.gear {
                if s == Stat::Armor {
                    item_armor += v;
                } else {
                    add[s as usize] += v;
                }
            }
            if let Some((ar, bv)) = x.shield {
                item_armor += ar;
                add[Stat::BlockValue as usize] += bv;
            }
        }
        // gather every active modifier: (mod, cx)
        let mut mods: Vec<(&'w StatMod, Cx<'w>)> = Vec::new();
        {
            let x = &self.actors[a];
            let cx0 = Cx::new(a, x.target, "");
            for p in &x.passives {
                if p.mods.is_empty() {
                    continue;
                }
                if let Some(c) = &p.cond {
                    if self.ev(c, &cx0) == 0.0 {
                        continue;
                    }
                }
                for m in &p.mods {
                    mods.push((m, cx0));
                }
            }
            if let Some(pd) = x.pet_def {
                for m in &pd.mods {
                    mods.push((m, cx0));
                }
            }
            if let Some(e) = x.enemy {
                for m in &e.mods {
                    mods.push((m, cx0));
                }
            }
            for inst in &x.auras {
                let def = &w.auras[inst.def as usize];
                for m in &def.mods {
                    let mut cx = cx0;
                    cx.rank = inst.rank;
                    cx.stacks = inst.stacks as f64;
                    cx.tgt = Some(inst.src);
                    mods.push((m, cx));
                }
            }
        }
        // phase 1: primary stats
        for (m, cx) in &mods {
            if m.stat.is_primary() {
                let v = self.ev(&m.value, cx);
                match m.op {
                    ModOp::Add => add[m.stat as usize] += v,
                    ModOp::Mul => mul[m.stat as usize] *= v,
                }
            }
        }
        let base = self.actors[a].base;
        let mut st = self.actors[a].st;
        let prim = [(Stat::Str, Stat::StrPct), (Stat::Agi, Stat::AgiPct), (Stat::Sta, Stat::StaPct), (Stat::Int, Stat::IntPct), (Stat::Spi, Stat::SpiPct)];
        for (i, (s, pct)) in prim.iter().enumerate() {
            st[*pct as usize] = add[*pct as usize];
            st[*s as usize] = (base[i] + add[*s as usize]) * (1.0 + add[*pct as usize]) * mul[*s as usize];
        }
        self.actors[a].st = st;
        // phase 2: class formulas (players) or fixed values (pets/enemies)
        let mut basev = [0.0f64; N_STATS];
        for s in Stat::ALL {
            basev[*s as usize] = s.base_default();
        }
        let fixed = self.actors[a].fixed;
        let class = self.actors[a].class;
        if let Some(c) = class {
            let f = &w.classes[c].f;
            let cx = Cx::new(a, None, "");
            basev[Stat::Ap as usize] = self.ev(&f.ap, &cx);
            basev[Stat::Rap as usize] = self.ev(&f.rap, &cx);
            basev[Stat::MeleeCrit as usize] = self.ev(&f.melee_crit, &cx);
            basev[Stat::RangedCrit as usize] = self.ev(&f.ranged_crit, &cx);
            basev[Stat::SpellCrit as usize] = self.ev(&f.spell_crit, &cx);
            basev[Stat::Dodge as usize] = self.ev(&f.dodge, &cx);
            basev[Stat::Parry as usize] = self.ev(&f.parry, &cx);
            basev[Stat::Block as usize] = if self.actors[a].shield.is_some() { self.ev(&f.block, &cx) } else { 0.0 };
        } else if let Some(fx) = fixed {
            basev[Stat::Ap as usize] = fx[2];
            basev[Stat::MeleeCrit as usize] = fx[3];
            basev[Stat::SpellCrit as usize] = fx[3];
            basev[Stat::Dodge as usize] = fx[4];
            basev[Stat::Parry as usize] = fx[6];
            if let Some(e) = self.actors[a].enemy {
                basev[Stat::Block as usize] = e.block;
            }
        }
        for (m, cx) in &mods {
            if !m.stat.is_primary() {
                let v = self.ev(&m.value, cx);
                match m.op {
                    ModOp::Add => add[m.stat as usize] += v,
                    ModOp::Mul => mul[m.stat as usize] *= v,
                }
            }
        }
        for s in Stat::ALL {
            if s.is_primary() {
                continue;
            }
            let i = *s as usize;
            st[i] = (basev[i] + add[i]) * mul[i];
        }
        // armour: item armour scales with armour_mult, agility adds 2 each
        let fixed_armor = fixed.map_or(0.0, |f| f[1]);
        st[Stat::Armor as usize] = ((item_armor + fixed_armor) * st[Stat::ArmorMult as usize]
            + if class.is_some() { 2.0 * st[Stat::Agi as usize] } else { 0.0 }
            + add[Stat::Armor as usize])
            * mul[Stat::Armor as usize];
        st[Stat::Ap as usize] *= st[Stat::ApMult as usize];
        st[Stat::Rap as usize] *= st[Stat::RapMult as usize];
        // health, mana, energy
        let sta = st[Stat::Sta as usize];
        let int = st[Stat::Int as usize];
        let (max_hp, max_mana) = match fixed {
            Some(fx) => (
                (fx[0] + add[Stat::MaxHp as usize]) * mul[Stat::MaxHp as usize] * st[Stat::MaxHpMult as usize],
                (fx[5] + add[Stat::MaxMana as usize]) * st[Stat::MaxManaMult as usize],
            ),
            None => {
                let hp = base[5] + sta.min(20.0) + 10.0 * (sta - 20.0).max(0.0);
                let mana = if base[6] > 0.0 { base[6] + int.min(20.0) + 15.0 * (int - 20.0).max(0.0) } else { 0.0 };
                (
                    (hp + add[Stat::MaxHp as usize]) * st[Stat::MaxHpMult as usize],
                    (mana + add[Stat::MaxMana as usize]) * st[Stat::MaxManaMult as usize],
                )
            }
        };
        let x = &mut self.actors[a];
        x.st = st;
        x.max_hp = max_hp.max(1.0);
        if x.hp > x.max_hp {
            x.hp = x.max_hp;
        }
        x.max_res[Res::Hp as usize] = x.max_hp;
        x.max_res[Res::Mana as usize] = if x.has_res[Res::Mana as usize] { max_mana } else { 0.0 };
        x.max_res[Res::Rage as usize] = if x.has_res[Res::Rage as usize] { 100.0 } else { 0.0 };
        x.max_res[Res::Energy as usize] = if x.has_res[Res::Energy as usize] { 100.0 + add[Stat::MaxEnergy as usize] } else { 0.0 };
        for r in 1..N_RES {
            if x.res[r] > x.max_res[r] {
                x.res[r] = x.max_res[r];
            }
        }
    }

    // ------------------------------------------------------------ helpers

    pub fn is_cc(&self, a: usize) -> bool {
        let w = self.w;
        self.actors[a].auras.iter().any(|x| {
            matches!(w.auras[x.def as usize].mechanic, Some(Mechanic::Stun) | Some(Mechanic::Fear) | Some(Mechanic::Incap) | Some(Mechanic::Freeze))
        })
    }
    fn hard_cc_mech(&self, a: usize) -> Vec<Mechanic> {
        let w = self.w;
        self.actors[a]
            .auras
            .iter()
            .filter_map(|x| w.auras[x.def as usize].mechanic)
            .filter(|m| matches!(m, Mechanic::Stun | Mechanic::Fear | Mechanic::Incap | Mechanic::Freeze))
            .collect()
    }

    pub fn hostile(&self, a: usize, b_: usize) -> bool {
        self.actors[a].team != self.actors[b_].team
    }

    /// Pick a live hostile target for an actor if it has none.
    pub fn retarget(&mut self, a: usize) {
        let me = &self.actors[a];
        if !me.alive {
            return;
        }
        if let Some(t) = me.target {
            if self.actors[t].alive && self.hostile(a, t) {
                return;
            }
        }
        let new = if let Some(o) = me.owner {
            self.actors[o].target.filter(|&t| self.actors[t].alive)
        } else {
            None
        };
        let team = me.team;
        let new = new.or_else(|| {
            // players prefer non-pet enemies; enemies use threat
            if self.actors[a].kind == Kind::Enemy {
                self.threat_target(a)
            } else {
                self.actors.iter().find(|x| x.alive && x.team != team && !x.is_pet()).map(|x| x.id)
            }
        });
        if self.actors[a].target != new {
            self.actors[a].wanding = None;
        }
        self.actors[a].target = new;
    }

    pub fn threat_target(&self, e: usize) -> Option<usize> {
        let me = &self.actors[e];
        if let Some((t, until)) = me.taunt {
            if until > self.t && self.actors[t].alive {
                return Some(t);
            }
        }
        let mut best: Option<(usize, f64)> = None;
        for (i, &th) in me.threat.iter().enumerate() {
            if i < self.actors.len() && self.actors[i].alive && self.actors[i].team != me.team && th > 0.0 {
                if best.map_or(true, |(_, b)| th > b) {
                    best = Some((i, th));
                }
            }
        }
        best.map(|x| x.0).or_else(|| self.actors.iter().find(|x| x.alive && x.team != me.team && !x.is_pet()).map(|x| x.id))
    }

    pub fn add_threat(&mut self, enemy: usize, who: usize, amount: f64) {
        if !self.actors[enemy].alive || self.actors[enemy].kind != Kind::Enemy {
            return;
        }
        let t = &mut self.actors[enemy].threat;
        if t.len() <= who {
            t.resize(who + 1, 0.0);
        }
        t[who] += amount;
    }

    pub fn swing_speed(&self, a: usize, hand: Hand) -> f64 {
        let x = &self.actors[a];
        let base = x.weapons[hand as usize].map_or(2.0, |w| w.speed);
        let ov = x.st[Stat::WeaponSpeedOverride as usize];
        let base = if ov > 0.0 && hand != Hand::Ranged { ov } else { base };
        let haste = if hand == Hand::Ranged { x.st[Stat::RangedHaste as usize] } else { x.st[Stat::MeleeHaste as usize] };
        base / haste.max(0.05)
    }

    pub fn can_swing(&self, a: usize, hand: Hand) -> bool {
        let x = &self.actors[a];
        // Auto Shot keeps firing while the hunter casts (Forever's Oct 15
        // notes); melee swings and wands wait for the cast to end.
        let shoots_through_casts = hand == Hand::Ranged && x.weapons[2].map_or(false, |wp| wp.wtype != WeaponType::Wand);
        if !x.alive || (x.cast.is_some() && !shoots_through_casts) || self.rest_phase {
            return false;
        }
        let wand = hand == Hand::Ranged && x.weapons[2].map_or(false, |wp| wp.wtype == WeaponType::Wand);
        // shooting a wand stops melee auto attacks
        let on = if wand { x.wanding.is_some() } else if hand == Hand::Ranged { x.auto_ranged } else { x.auto_melee && x.wanding.is_none() };
        if !on || x.weapons[hand as usize].is_none() {
            return false;
        }
        match x.target {
            Some(t) if self.actors[t].alive && self.hostile(a, t) => {}
            _ => return false,
        }
        if self.is_cc(a) {
            return false;
        }
        if hand != Hand::Ranged && x.has_mech(self.w, Mechanic::Disarm) && x.st[Stat::ImmuneDisarm as usize] <= 0.0 {
            return false;
        }
        true
    }

    /// Start auto-attacks for every actor that should be swinging.
    pub fn start_swings(&mut self) {
        for a in 0..self.actors.len() {
            for h in [Hand::Mh, Hand::Oh, Hand::Ranged] {
                if self.actors[a].weapons[h as usize].is_some() && self.actors[a].swing[h as usize] == INF {
                    let off = if h == Hand::Oh { 0.2 } else { 0.0 };
                    self.actors[a].swing[h as usize] = self.t + off;
                }
            }
        }
    }

    pub fn enter_combat(&mut self) {
        self.combat_start = self.t;
        self.rest_phase = false;
        for a in 0..self.actors.len() {
            if self.actors[a].alive {
                self.actors[a].in_combat = true;
                self.retarget(a);
            }
        }
        self.start_swings();
        for a in 0..self.actors.len() {
            if self.actors[a].alive {
                self.emit(a, EventKind::CombatStart, Ev::default(), 0);
            }
        }
    }

    pub fn leave_combat(&mut self) {
        for a in 0..self.actors.len() {
            let x = &mut self.actors[a];
            x.in_combat = false;
            x.swing = [INF; 3];
            x.queued = None;
            x.wanding = None;
            x.cast = None;
            x.threat.clear();
            x.taunt = None;
            x.cp = 0.0;
        }
    }

    // ------------------------------------------------------------ main loop

    fn next_time(&self, stop: &Stop) -> f64 {
        let mut n = match stop {
            Stop::Fight { until } | Stop::Rest { until, .. } | Stop::Until(until) => *until,
        };
        for (i, x) in self.actors.iter().enumerate() {
            if !x.alive {
                continue;
            }
            for h in [Hand::Mh, Hand::Oh, Hand::Ranged] {
                let st = x.swing[h as usize];
                if st < INF && self.can_swing(i, h) {
                    n = n.min(st.max(self.t));
                }
            }
            if let Some(c) = &x.cast {
                n = n.min(c.end);
                if c.channel {
                    n = n.min(c.next_tick);
                }
            }
            if x.gcd_end > self.t {
                n = n.min(x.gcd_end);
            }
            for inst in &x.auras {
                n = n.min(inst.expires);
                if inst.tick_iv > 0.0 {
                    n = n.min(inst.next_tick);
                }
            }
            n = n.min(x.next_tick);
            for &(_, r) in &x.cds {
                if r > self.t {
                    n = n.min(r);
                }
            }
            for &(st, _) in &x.stun_sched {
                if st >= self.t {
                    n = n.min(st);
                }
            }
            if let Some((_, u)) = x.taunt {
                if u > self.t {
                    n = n.min(u);
                }
            }
        }
        n
    }

    fn process(&mut self) {
        let t = self.t;
        let w = self.w;
        for a in 0..self.actors.len() {
            if !self.actors[a].alive {
                continue;
            }
            // aura ticks then expiries
            let mut guard = 0;
            loop {
                guard += 1;
                if guard > 64 {
                    break;
                }
                let x = &self.actors[a];
                let due = x.auras.iter().position(|i| i.tick_iv > 0.0 && i.next_tick <= t + EPS && i.next_tick <= i.expires + EPS);
                let Some(k) = due else { break };
                let inst = self.actors[a].auras[k].clone();
                self.actors[a].auras[k].next_tick += inst.tick_iv;
                let def = &w.auras[inst.def as usize];
                if let Some((_, effs)) = &def.tick {
                    let src = if self.actors[inst.src].alive || inst.src == a { inst.src } else { a };
                    let mut cx = Cx::new(src, Some(a), def.name.as_str());
                    cx.rank = inst.rank;
                    cx.stacks = inst.stacks as f64;
                    cx.aura = Some((a, inst.uid));
                    cx.sm = self.spell_mod_vals(src, inst.rank.map(|r| r.0).or(inst.spell), None);
                    self.run_effects(effs, cx);
                }
                if !self.actors[a].alive {
                    break;
                }
            }
            if !self.actors[a].alive {
                continue;
            }
            loop {
                let x = &self.actors[a];
                let Some(k) = x.auras.iter().position(|i| i.expires <= t + EPS) else { break };
                self.remove_aura_at(a, k, true);
            }
            // scripted stuns (pressure attackers)
            self.scripted_stuns(a);
            // resource tick
            if self.actors[a].next_tick <= t + EPS {
                self.actors[a].next_tick += w.combat.tick;
                self.regen_tick(a);
            }
            // casts
            let done = matches!(&self.actors[a].cast, Some(c) if c.end <= t + EPS);
            let tick = matches!(&self.actors[a].cast, Some(c) if c.channel && c.next_tick <= t + EPS && c.next_tick <= c.end + EPS);
            if tick {
                self.channel_tick(a);
            }
            if done && self.actors[a].cast.is_some() {
                self.finish_cast(a);
            }
            // swings
            for h in [Hand::Mh, Hand::Oh, Hand::Ranged] {
                if !self.actors[a].alive {
                    break;
                }
                if self.actors[a].swing[h as usize] <= t + EPS && self.can_swing(a, h) {
                    let sp = self.swing_speed(a, h);
                    self.actors[a].swing[h as usize] = t + sp;
                    self.auto_attack(a, h);
                }
            }
        }
    }

    fn regen_tick(&mut self, a: usize) {
        let w = self.w;
        let x = &self.actors[a];
        if !x.alive {
            return;
        }
        let tick = w.combat.tick;
        let cx = Cx::new(a, None, "");
        let (mana_regen, hp_regen) = match x.class {
            Some(c) => (self.ev(&w.classes[c].f.mana_regen, &cx), self.ev(&w.classes[c].f.hp_regen, &cx)),
            None => (0.0, 0.0),
        };
        let x = &self.actors[a];
        let five = self.t - x.last_mana_spend >= w.combat.five_second_rule;
        let mut mana = x.st[Stat::Mp5 as usize] * tick / 5.0;
        mana += if five { mana_regen } else { mana_regen * x.st[Stat::CastingRegen as usize] };
        let hp = x.st[Stat::Hp5 as usize] * tick / 5.0
            + if x.in_combat { hp_regen * x.st[Stat::CombatHpRegen as usize] } else { hp_regen };
        let energy_on = x.has_res[Res::Energy as usize]
            && (!x.has_res[Res::Mana as usize] || x.st[Stat::EnergyActive as usize] > 0.0);
        let x = &mut self.actors[a];
        if x.has_res[Res::Mana as usize] {
            x.res[Res::Mana as usize] = (x.res[Res::Mana as usize] + mana).min(x.max_res[Res::Mana as usize]);
        }
        if energy_on {
            x.res[Res::Energy as usize] =
                (x.res[Res::Energy as usize] + w.combat.energy_per_tick * x.st[Stat::EnergyRegen as usize]).min(x.max_res[Res::Energy as usize]);
        }
        if hp > 0.0 && x.hp < x.max_hp {
            x.hp = (x.hp + hp).min(x.max_hp);
        }
    }

    fn act_all(&mut self, stop: &Stop) {
        for a in 0..self.actors.len() {
            if !self.actors[a].alive {
                continue;
            }
            if let Stop::Rest { actor, .. } = stop {
                if a != *actor {
                    continue;
                }
            }
            for _ in 0..12 {
                if !self.act(a) {
                    break;
                }
            }
        }
    }

    fn players_alive(&self) -> bool {
        self.actors.iter().any(|x| x.team == 0 && x.kind == Kind::Player && x.alive)
    }
    fn enemies_alive(&self) -> bool {
        self.actors.iter().any(|x| x.team != 0 && x.alive && !x.is_pet())
    }

    pub fn run(&mut self, stop: Stop) -> Outcome {
        let mut iters: u64 = 0;
        loop {
            iters += 1;
            if iters > 5_000_000 {
                return Outcome::Timeout;
            }
            match stop {
                Stop::Fight { until } => {
                    if !self.players_alive() {
                        return Outcome::PlayerDied;
                    }
                    if !self.enemies_alive() {
                        return Outcome::EnemiesDead;
                    }
                    if self.t >= until - EPS {
                        return Outcome::Timeout;
                    }
                }
                Stop::Until(until) => {
                    if self.t >= until - EPS {
                        return Outcome::Timeout;
                    }
                }
                Stop::Rest { actor, until } => {
                    if self.t >= until - EPS || !self.actors[actor].alive {
                        return Outcome::Timeout;
                    }
                }
            }
            for a in 0..self.actors.len() {
                if self.actors[a].alive && !self.rest_phase {
                    self.retarget(a);
                }
            }
            self.act_all(&stop);
            if let Stop::Rest { actor, .. } = stop {
                let x = &self.actors[actor];
                if x.cast.is_none() && x.gcd_end <= self.t + EPS && !self.can_act_rest(actor) {
                    return Outcome::Idle;
                }
            }
            if matches!(stop, Stop::Fight { .. }) && (!self.players_alive() || !self.enemies_alive()) {
                continue;
            }
            let n = self.next_time(&stop);
            self.t = n.max(self.t);
            self.process();
        }
    }
}
