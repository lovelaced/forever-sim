//! The combat engine: a discrete-event simulation. Time jumps from one
//! event (swing, cast completion, aura tick or expiry, resource tick,
//! cooldown becoming ready) to the next; actors pick actions from their
//! rotation whenever they are free.

pub mod act;
pub mod combat;
pub mod motion;
pub mod totems;
pub use motion::{Arrival, Goal, Motion};

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
    pub range_add: f64,
    pub range_mul: f64,
    pub radius_add: f64,
    pub radius_mul: f64,
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
            range_add: 0.0,
            range_mul: 1.0,
            radius_add: 0.0,
            radius_mul: 1.0,
        }
    }
}

/// Evaluation context: who "self" and "target" are, which spell rank and
/// aura stack count formulas see, and the triggering event.
#[derive(Clone, Copy, Debug)]
pub struct Cx<'w> {
    pub me: usize,
    pub tgt: Option<usize>,
    /// The friendly unit a heal or buff is cast on, when it isn't you.
    pub ally: Option<usize>,
    pub rank: Option<(SpellId, usize)>,
    pub stacks: f64,
    pub ev: Option<Ev>,
    pub aura: Option<(usize, u64)>,
    pub spell: Option<SpellId>,
    pub depth: u8,
    pub tag: &'w str,
    pub sm: SmVals,
    /// Which hit of a chain or area this is (0 for the first).
    pub hit: u8,
    /// Where the effect comes from when it isn't you: a totem or trap.
    pub at: Option<usize>,
}

impl<'w> Cx<'w> {
    pub fn new(me: usize, tgt: Option<usize>, tag: &'w str) -> Self {
        Cx { me, tgt, ally: None, rank: None, stacks: 1.0, ev: None, aura: None, spell: None, depth: 0, tag, sm: SmVals::default(), hit: 0, at: None }
    }
}

/// Key for the shared world streams (`Sim::env`), and what each is for.
pub const ENV_KEY: u64 = 0x454e_5600;
pub const ENV_PULL: u64 = 1;
pub const ENV_FIGHT: u64 = 2;
pub const ENV_DUEL: u64 = 3;

/// Who an effect lands on, in order: a fixed-size list, so picking targets
/// never allocates.
#[derive(Clone, Copy)]
pub struct Picked {
    n: usize,
    ids: [u32; 48],
}

impl Picked {
    pub fn new() -> Self {
        Picked { n: 0, ids: [0; 48] }
    }
    pub fn one(a: Option<usize>) -> Self {
        let mut p = Picked::new();
        if let Some(a) = a {
            p.push(a);
        }
        p
    }
    pub fn push(&mut self, a: usize) {
        if self.n < self.ids.len() {
            self.ids[self.n] = a as u32;
            self.n += 1;
        }
    }
    pub fn len(&self) -> usize {
        self.n
    }
    pub fn is_empty(&self) -> bool {
        self.n == 0
    }
    pub fn first(&self) -> Option<usize> {
        (self.n > 0).then(|| self.ids[0] as usize)
    }
    pub fn truncate(&mut self, n: usize) {
        self.n = self.n.min(n);
    }
    /// Move `a` to the front (swapping with whoever was there).
    pub fn lead_with(&mut self, a: usize) {
        if let Some(k) = self.ids[..self.n].iter().position(|&x| x as usize == a) {
            self.ids.swap(0, k);
        }
    }
}

impl Default for Picked {
    fn default() -> Self {
        Picked::new()
    }
}

pub struct PickedIter {
    p: Picked,
    k: usize,
}

impl Iterator for PickedIter {
    type Item = usize;
    fn next(&mut self) -> Option<usize> {
        (self.k < self.p.n).then(|| {
            self.k += 1;
            self.p.ids[self.k - 1] as usize
        })
    }
}

impl IntoIterator for Picked {
    type Item = usize;
    type IntoIter = PickedIter;
    fn into_iter(self) -> PickedIter {
        PickedIter { p: self, k: 0 }
    }
}

/// What a target selector is picking for: damage leads with the caster's
/// target; healing's "enemies" means your whole side.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SelUse {
    Effect,
    Damage,
    Heal,
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
    /// When its holder notices it (a proc window read only after a
    /// player's reaction; the moment it was applied for everyone else).
    pub seen: f64,
    /// Movement slow and speed boost it gives (worked out when it landed).
    pub slow: f64,
    pub fast: f64,
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
    /// When a reacting player on the other side notices the cast (and can
    /// interrupt it).
    pub seen: f64,
}

/// How a player reacts and slips (the "Play like" assumptions). Mobs, pets
/// and allies have none and never wait or slip.
#[derive(Clone, Copy, Debug, Default)]
pub struct React {
    /// Average reaction time, and how far either side one can fall.
    pub base: f64,
    pub spread: f64,
    /// Chance a choice takes the next usable ability instead of the best.
    pub slip: f64,
    /// Chance a proc window goes unnoticed.
    pub miss: f64,
    /// Seconds late with long cooldowns, and with reapplying upkeep.
    pub late_cd: f64,
    pub lapse: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub enum Kind {
    Player,
    Pet(u16),
    Enemy,
    /// A party member in the healing tests: it takes hits and heals, nothing more.
    Ally,
    /// A shaman's totem or a hunter's trap on the ground.
    Totem,
}

#[derive(Clone, Debug)]
pub struct Actor<'w> {
    pub id: usize,
    pub name: String,
    pub kind: Kind,
    pub team: u8,
    /// The party within the team (raids have two).
    pub group: u8,
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
    /// The mechanics (`Mechanic::bit`) the auras above carry, kept in step
    /// by `sync_mechs` whenever an aura comes or goes.
    pub mechs: u16,
    /// Hidden by Stealth or Prowl (also kept by `sync_mechs`).
    pub stealthed: bool,
    /// Looking away (Distract), ignoring slows (Blessing of Freedom), can't
    /// run (Curse of Recklessness): kept by `sync_mechs`.
    pub distracted: bool,
    pub free: bool,
    pub no_flee: bool,
    /// Ran back and healed after losing everyone it was fighting (it's out
    /// of this fight).
    pub evaded: bool,
    /// The events anything of this actor's listens to: its passives and
    /// pet procs (`static_listens`, set at spawn), and its auras' (kept by
    /// `sync_mechs`).
    pub listens: u32,
    pub static_listens: u32,
    /// Spells known and at what rank, worked out on first ask (talents and
    /// level don't change): -2 not asked yet, -1 not known.
    pub known_cache: std::cell::RefCell<Vec<i32>>,
    /// The next moment one of its auras needs a look (a tick, an expiry, a
    /// proc being noticed); kept by `sync_aura_next`.
    pub aura_next: f64,
    /// Its passives that change spells (indexes), so spell modifiers don't
    /// walk every talent.
    pub sm_passives: Vec<u16>,
    pub cds: Vec<(u32, f64)>,
    pub gcd_end: f64,
    pub cast: Option<CastState>,
    pub swing: [f64; 3],
    pub queued: Option<SpellId>,
    /// The auto-repeat spell (a wand's Shoot) firing on each ranged swing.
    pub wanding: Option<SpellId>,
    /// The next line of the rotation's opener, while the fight's opening
    /// moves are still being played.
    pub opening: Option<usize>,
    pub target: Option<usize>,
    pub alive: bool,
    pub rotation: Option<&'w Rotation>,
    pub auto_melee: bool,
    pub auto_ranged: bool,
    /// Threat by attacker id, in the order attackers first showed up.
    pub threat: Vec<(u32, f64)>,
    pub taunt: Option<(usize, f64)>,
    /// When the actor last left combat (Rage starts decaying after a delay).
    pub left_combat: f64,
    pub last_mana_spend: f64,
    pub next_tick: f64,
    pub rng: [Rng; 3],
    /// Draws for reaction times and mistakes, apart from the combat rolls.
    pub react_rng: Rng,
    /// Fixed rolls made at spawn (whether a mob flees, for how long, whether
    /// it brings a friend), so nothing the player does shifts them.
    pub dice: [f64; 6],
    /// Where it is and where it's going.
    pub mo: Motion,
    /// A mob that hasn't noticed anyone yet (it stands, and doesn't fight).
    pub aware: bool,
    /// Mobs of one pull notice together.
    pub pack: u32,
    /// Has decided whether to run at low health.
    pub fled: bool,
    /// Yards it fights from (0: melee); see `Sim::find_stand`.
    pub stand_at: f64,
    /// A better line that was only out of reach at its last decision: walk
    /// up to this range.
    pub chase: Option<f64>,
    /// Threat on this mob changed since it last chose whom to attack.
    pub aggro_dirty: bool,
    /// Kept above 1 health (the boss test's tank is healed by someone).
    pub unkillable: bool,
    /// Buffing up before a fight: the next precombat line to try (each once,
    /// in order, like an opener); None when done.
    pub prep_step: Option<usize>,
    /// The kind of creature a hunter is tracking.
    pub tracking: Option<CreatureType>,
    /// Your totems (fire, earth, water, air) and trap on the ground.
    pub totems: [Option<usize>; 5],
    /// A totem's or trap's own state.
    pub totem: Option<totems::TotemState<'w>>,
    pub react: Option<React>,
    /// Which option set the actor reads (0: the run's own; pets read their
    /// owner's).
    pub opt_set: u8,
    /// No move at all before this (reacting to a new target, or to coming
    /// out of a stun).
    pub react_until: f64,
    /// For each line of the fight list, when a typical player gets round to
    /// a cooldown or upkeep line that's wanted (INF: not pending).
    pub due: Vec<f64>,
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
    /// When each of the boss's scripted attacks comes next.
    pub script_next: Vec<f64>,
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
    /// A pet, totem or trap: someone else's unit, left out of target
    /// picking, party lists and enemy counts.
    pub fn is_pet(&self) -> bool {
        matches!(self.kind, Kind::Pet(_) | Kind::Totem)
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
    pub fn top_threat(&self) -> f64 {
        self.threat.iter().fold(0.0, |m, x| m.max(x.1))
    }
    pub fn has_mech(&self, _w: &World, m: Mechanic) -> bool {
        self.mechs & m.bit() != 0
    }
    pub fn sync_mechs(&mut self, w: &World) {
        self.mechs = self.auras.iter().filter_map(|a| w.auras[a.def as usize].mechanic).fold(0, |m, x| m | x.bit());
        self.stealthed = self.auras.iter().any(|a| w.auras[a.def as usize].stealth);
        self.distracted = self.auras.iter().any(|a| w.auras[a.def as usize].distracts);
        self.free = self.auras.iter().any(|a| w.auras[a.def as usize].free);
        self.no_flee = self.auras.iter().any(|a| w.auras[a.def as usize].no_flee);
        self.listens = self.auras.iter().fold(self.static_listens, |m, a| m | w.auras[a.def as usize].listens);
    }
    /// When its auras next need a look (see `aura_next`).
    pub fn sync_aura_next(&mut self, t: f64) {
        let mut n = INF;
        for i in &self.auras {
            n = n.min(i.expires);
            if i.tick_iv > 0.0 {
                n = n.min(i.next_tick);
            }
            if i.seen > t {
                n = n.min(i.seen);
            }
        }
        self.aura_next = n;
    }
    /// After passives or a pet's procs are set: which events it hears.
    pub fn sync_listens(&mut self, w: &World) {
        self.sm_passives = self.passives.iter().enumerate().filter(|(_, p)| !p.spell_mods.is_empty()).map(|(k, _)| k as u16).collect();
        let mut m = self.passives.iter().flat_map(|p| p.procs.iter()).fold(0, |m, q| m | q.on.bit());
        if let Some(pd) = self.pet_def {
            m |= pd.procs.iter().fold(0, |m, q| m | q.on.bit());
        }
        self.static_listens = m;
        self.sync_mechs(w);
    }
    pub fn cd_ready(&self, key: u32) -> f64 {
        self.cds.iter().find(|c| c.0 == key).map_or(0.0, |c| c.1)
    }
}

#[derive(Clone, Debug, Default)]
pub struct Metrics<'w> {
    pub dmg: f64,
    /// Enemy casts this unit cut short (its pets' included).
    pub interrupts: f64,
    pub dmg_by: BTreeMap<&'w str, f64>,
    pub heal: f64,
    pub heal_by: BTreeMap<&'w str, f64>,
    pub overheal: f64,
    /// Healing this unit received (from anyone).
    pub healed_in: f64,
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
    /// Times each line of its opener fired.
    pub opener_lines: Vec<f64>,
    /// Times each line of its precombat list fired.
    pub precombat_lines: Vec<f64>,
}

/// One thing that happened in a traced fight: the website's timeline.
#[derive(Clone, Debug, serde::Serialize)]
pub struct TraceEv {
    pub t: f64,
    /// cast, hit, miss, dodge, parry, heal, gain, lose, death, pull, rest or eat
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
    /// The spell or aura behind it (its qualified id in the data, e.g.
    /// "warrior.rend"), for the website's icons; empty for auto attacks.
    pub key: String,
    /// The damage or healing school.
    pub school: &'static str,
    /// Damage soaked up by a shield before the hit landed, and the shield.
    pub absorbed: f64,
    pub via: String,
    /// The game icon to show, filled in by the trace request.
    pub icon: String,
    /// The target's health after a hit or heal, as a share of its maximum.
    pub hp: Option<f64>,
    /// Healing past full health (heals only).
    pub over: f64,
    /// Actor numbers, so the trace request can tell same-named mobs apart.
    #[serde(skip)]
    pub who_id: usize,
    #[serde(skip)]
    pub target_id: Option<usize>,
    #[serde(skip)]
    pub target_team: Option<u8>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Stop {
    /// Run until the time, a team-0 player dies, or all team-1 actors die.
    Fight { until: f64 },
    /// Run until the actor has nothing left to do in its rest list.
    Rest { actor: usize, until: f64 },
    /// Getting ready for a fight: run until these players (one or two) have
    /// cast everything their precombat lists want.
    Prep { who: [usize; 2], until: f64 },
    /// Just let time pass (travel).
    Until(f64),
    /// A healing test: run until the time, or until the party fails (you or
    /// the tank die, or `max_dead` of the others do).
    Party { until: f64, max_dead: u32 },
}

/// What the characters are doing, which picks the rotation list they play.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Phase {
    /// The `actions` list (and the opener).
    Fight,
    /// Between pulls: the `rest` list, no auto attacks.
    Rest,
    /// Buffing up before a fight: the `precombat` list, cast for real.
    Prep,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Outcome {
    Timeout,
    PlayerDied,
    EnemiesDead,
    /// Everything still alive lost track of you and went home (Vanish,
    /// Feign Death).
    Escaped,
    Idle,
    /// The party failed a healing test.
    Failed,
}

pub struct Sim<'w> {
    pub w: &'w World,
    pub t: f64,
    pub actors: Vec<Actor<'w>>,
    pub opts: Vec<f64>,
    pub seed: u64,
    pub combat_start: f64,
    pub phase: Phase,
    /// Option sets for actors that don't share the run's own (a duel
    /// opponent): set k is `opt_sets[k - 1]`.
    pub opt_sets: Vec<Vec<f64>>,
    /// Charges under way: who, at whom, and what lands on arrival.
    pub charges: Vec<(usize, usize, &'w [Effect], Cx<'w>)>,
    /// Mobs on their way to join (a fleeing mob's friend).
    pub arrivals: Vec<Arrival<'w>>,
    /// When the first mob of this pull noticed you (the walk up to it
    /// doesn't count as fighting).
    pub woke_at: Option<f64>,
    /// Every enemy spawned since the last `spawned.clear()`.
    pub spawned: Vec<usize>,
    /// Times the engine had to nudge the clock past a stuck moment.
    pub spins: u64,
    /// While an actor decides: the range of the first line held back only
    /// by distance (INF: none).
    pub chase_probe: std::cell::Cell<f64>,
    /// The character the test is about (its death ends the fight); the
    /// other players (the boss test's tank) are there for it.
    pub main: Option<usize>,
    /// The boss test's watch on aggro: (boss, tank, time it spent on the
    /// tank, times it turned on the main character).
    pub aggro_watch: Option<(usize, usize, f64, u32)>,
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
    /// Rage lost per second out of combat (assumption `rage_decay`).
    pub rage_decay: f64,
    /// The player team's tank in the healing tests (yourself otherwise).
    pub tank: Option<usize>,
    /// Living actors, ascending ids (dead ones are pruned at the top of each
    /// loop of `run`): the engine's loops walk this instead of every actor
    /// ever spawned, which an hour of leveling makes hundreds.
    pub live: Vec<usize>,
    /// A reusable copy of `live` for loops that change who is alive.
    scratch: Vec<usize>,
    scratch_b: Vec<usize>,
    /// A buff totem is handing out its buff (not a new totem going down).
    pub totem_buffing: bool,
    /// Whether precombat lines put totems down (not before a leveling pull,
    /// where you'd walk away from them).
    pub prep_totems: bool,
    /// Things that happen at the same moment go highest id first (a duel's
    /// world draw, so neither side always goes first).
    pub flip: bool,
    /// Where the `threat_margin` assumption sits (looked up once: it's read
    /// before every damaging line in the boss test).
    threat_margin_i: Option<u16>,
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
        let rage_decay = opt("rage_decay");
        Sim {
            rage_decay,
            low_level_penalty,
            downrank_penalty,
            rage_taken_mult,
            w,
            t: 0.0,
            actors: vec![],
            opts,
            seed,
            combat_start: 0.0,
            phase: Phase::Fight,
            opt_sets: vec![],
            charges: vec![],
            arrivals: vec![],
            woke_at: None,
            spawned: vec![],
            spins: 0,
            chase_probe: std::cell::Cell::new(INF),
            main: None,
            aggro_watch: None,
            metrics: vec![],
            uid: 0,
            log: None,
            trace: None,
            tank: None,
            live: Vec::new(),
            scratch: Vec::new(),
            scratch_b: Vec::new(),
            totem_buffing: false,
            prep_totems: true,
            flip: false,
            threat_margin_i: w.opt_index("threat_margin"),
        }
    }

    /// The living actors as of now, for a loop that may kill or summon:
    /// like `0..actors.len()` with the dead skipped, newcomers excluded.
    /// A stream of world draws that every build in a comparison shares
    /// (common random numbers): what kind of pull, how far away, whether
    /// totems carry over. Keyed by what it's for and the pull, fight or duel
    /// number, never by anything the player did.
    pub fn env(&self, what: u64, i: u64) -> Rng {
        Rng::from_keys(self.seed, &[ENV_KEY, what, i])
    }

    /// An option as an actor reads it (its own set, or its owner's).
    pub fn opt(&self, a: usize, i: usize) -> f64 {
        let o = self.actors[a].owner.unwrap_or(a);
        match self.actors[o].opt_set {
            0 => self.opts[i],
            k => self.opt_sets[k as usize - 1][i],
        }
    }
    pub fn opt_named(&self, a: usize, name: &str) -> f64 {
        self.w.opt_index(name).map_or(0.0, |i| self.opt(a, i as usize))
    }
    /// Damage bonus against what `t` is (Beast Slaying, Murder, Improved
    /// Tracking): read on every hit, so it follows the target.
    pub fn vs_mult(&self, a: usize, t: usize) -> f64 {
        let x = &self.actors[a];
        let c = self.actors[t].creature;
        let mut m = x.st[c.dmg_vs() as usize];
        if x.tracking == Some(c) {
            m *= x.st[Stat::DmgVsTracked as usize];
        }
        m
    }

    /// The boss test's party buffs a player (data/base/party.toml): each
    /// from a party member of the buff's class at the rank it knows at your
    /// level, unless you have that buff already.
    pub fn party_buffs(&mut self, p: usize) {
        let w = self.w;
        let level = self.actors[p].level;
        let x = &self.actors[p];
        let mana = x.max_res[Res::Mana as usize] > 0.0;
        let melee = x.auto_melee && x.stand_at <= 0.0;
        let physical = melee || x.auto_ranged;
        let mut helpers: Vec<(usize, usize)> = vec![];
        for pb in &w.party {
            if pb.option.map_or(false, |i| self.opt(p, i as usize) <= 0.0) {
                continue;
            }
            let wanted = match pb.for_.as_str() {
                "mana" => mana,
                "physical" => physical,
                "caster" => mana && !physical,
                "melee" => melee,
                _ => true,
            };
            let sp = &w.spells[pb.spell as usize];
            let Some(rank) = sp.rank_at(level) else { continue };
            if !wanted {
                continue;
            }
            let rank = if rank == usize::MAX { None } else { Some((pb.spell, rank)) };
            // a party member of the buff's class, at your level, out of the fight
            let Some(class) = sp.class else { continue };
            let src = match helpers.iter().find(|h| h.0 == class) {
                Some(h) => h.1,
                None => {
                    let h = self.new_actor("Party".into(), Kind::Ally, self.actors[p].team, level, 900 + class as u64);
                    self.actors[h].class = Some(class);
                    self.actors[h].alive = false;
                    self.live.retain(|&i| i != h);
                    self.metrics[h] = Metrics::default();
                    helpers.push((class, h));
                    h
                }
            };
            for e in &sp.effects {
                if let Effect::ApplyAura { aura, who: TargetSel::Me | TargetSel::Party | TargetSel::Ally, .. } = e {
                    if self.actors[p].auras.iter().any(|i| i.def == *aura) {
                        continue;
                    }
                    let mut cx = Cx::new(src, Some(p), sp.name.as_str());
                    cx.rank = rank;
                    cx.spell = Some(pb.spell);
                    self.apply_aura(p, *aura, src, 1, Some(3600.0), rank, cx);
                }
            }
        }
    }

    /// Give a player its own option set (a duel opponent).
    pub fn give_opts(&mut self, a: usize, opts: Vec<f64>) {
        self.opt_sets.push(opts);
        self.actors[a].opt_set = self.opt_sets.len() as u8;
        self.setup_react(a);
    }
    /// Reactions and mistakes from the "Play like" assumptions (an expert
    /// makes no mistakes).
    pub fn setup_react(&mut self, a: usize) {
        let spread = self.opt_named(a, "react_spread");
        let r = if self.opt_named(a, "play_like") >= 0.5 {
            React { base: self.opt_named(a, "react_expert"), spread, ..React::default() }
        } else {
            React {
                base: self.opt_named(a, "react_typical"),
                spread,
                slip: self.opt_named(a, "slip_typical"),
                miss: self.opt_named(a, "miss_proc_typical"),
                late_cd: self.opt_named(a, "late_cd_typical"),
                lapse: self.opt_named(a, "lapse_typical"),
            }
        };
        self.actors[a].react = Some(r);
    }
    /// Whether the actor waits on its reactions now (players, in a fight).
    pub fn reacting(&self, a: usize) -> bool {
        self.phase == Phase::Fight && self.actors[a].react.is_some()
    }
    /// One reaction time for the actor (0 for those that don't react).
    pub fn reaction(&mut self, a: usize) -> f64 {
        let base = self.actors[a].react.map_or(0.0, |r| r.base);
        self.delay(a, base)
    }
    /// A delay around `base`, varying like a reaction (0 for those that
    /// don't react).
    pub fn delay(&mut self, a: usize, base: f64) -> f64 {
        if !self.reacting(a) || base <= 0.0 {
            return 0.0;
        }
        let spread = self.actors[a].react.unwrap().spread;
        let u = self.actors[a].react_rng.f64();
        (base * (1.0 + spread * (2.0 * u - 1.0))).max(0.0)
    }
    /// A chance roll on the reaction stream (false for those that don't react).
    pub fn slips(&mut self, a: usize, p: f64) -> bool {
        self.reacting(a) && p > 0.0 && self.actors[a].react_rng.f64() < p
    }
    /// When the holder notices a proc: after a reaction, or never if missed.
    pub fn notice_proc(&mut self, holder: usize) -> f64 {
        let miss = self.actors[holder].react.map_or(0.0, |r| r.miss);
        if self.slips(holder, miss) {
            return INF;
        }
        self.t + self.reaction(holder)
    }

    fn live_snapshot(&mut self) -> Vec<usize> {
        let mut v = std::mem::take(&mut self.scratch);
        v.clear();
        v.extend_from_slice(&self.live);
        v
    }
    fn give_back(&mut self, v: Vec<usize>) {
        self.scratch = v;
    }
    fn prune_live(&mut self) {
        let actors = &self.actors;
        self.live.retain(|&i| actors[i].alive);
    }
    /// Bring a dead actor back (a corpse run's resurrection).
    pub fn revive(&mut self, a: usize) {
        self.actors[a].alive = true;
        if let Err(k) = self.live.binary_search(&a) {
            self.live.insert(k, a);
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
            key: String::new(), school: "", absorbed: 0.0, via: String::new(),
            icon: String::new(), hp: None, over: 0.0, who_id: who, target_id: target,
            target_team: target.map(|t| self.actors[t].team),
        }
    }

    /// The qualified id of the spell an event came from, for the trace.
    pub fn spell_key(&self, by: Option<u32>) -> String {
        by.map(|i| self.w.spells[i as usize].qid.clone()).unwrap_or_default()
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
            Who::Ally => Some(self.ally_of(cx)),
            Who::Tank => Some(self.tank_of(cx.me)),
            Who::Lowest => self.hurt_order(cx.me).first().copied(),
        }
    }

    /// The friendly unit a cast is on: its ally, or yourself.
    pub fn ally_of(&self, cx: &Cx<'w>) -> usize {
        cx.ally.unwrap_or(cx.me)
    }

    /// Your team's tank, or yourself when there's none.
    pub fn tank_of(&self, a: usize) -> usize {
        self.tank.filter(|&t| self.actors[t].team == self.actors[a].team).unwrap_or(a)
    }

    /// Live team-mates (you included, no pets), optionally only your group.
    pub fn allies(&self, a: usize, group: bool) -> impl Iterator<Item = usize> + '_ {
        let (team, g) = (self.actors[a].team, self.actors[a].group);
        self.live.iter().map(move |&i| &self.actors[i]).filter(move |x| x.alive && x.team == team && !x.is_pet() && (!group || x.group == g)).map(|x| x.id)
    }

    /// Live team-mates, most hurt first (share of health; ties keep spawn
    /// order, so you come first among the unhurt).
    pub fn hurt_order(&self, a: usize) -> Vec<usize> {
        let t = self.t;
        let mut v: Vec<(f64, usize)> = self.allies(a, false).map(|i| (self.actors[i].hp_pct(t), i)).collect();
        v.sort_by(|x, y| x.0.total_cmp(&y.0).then(x.1.cmp(&y.1)));
        v.into_iter().map(|(_, i)| i).collect()
    }

    /// Who a friendly effect target reaches, or None for the other targets.
    /// The one place effects pick who they land on.
    pub fn select(&self, sel: &TargetSel, cx: &Cx<'w>, u: SelUse) -> Picked {
        let me = &self.actors[cx.me];
        match sel {
            TargetSel::Me => Picked::one(Some(cx.me)),
            TargetSel::Target => Picked::one(if u == SelUse::Heal { cx.tgt } else { cx.tgt.or(me.target) }),
            TargetSel::Pet => Picked::one(self.who(cx, Who::Pet)),
            TargetSel::Owner => Picked::one(me.owner),
            TargetSel::MyTarget => Picked::one(me.target.filter(|&t| self.actors[t].alive)),
            TargetSel::Enemies(max) => {
                let team = me.team;
                let mut p = Picked::new();
                if u == SelUse::Heal {
                    for &i in &self.live {
                        if self.actors[i].alive && self.actors[i].team == team {
                            p.push(i);
                        }
                    }
                    return p;
                }
                // an area spell reaches only so far, around you or its target
                // (or around the totem it comes from)
                let area = cx.spell.map(|s| &self.w.spells[s as usize]).filter(|sp| sp.radius > 0.0).map(|sp| {
                    let around = if let Some(at) = cx.at { at } else if sp.range > sp.radius + 1e-9 { cx.tgt.or(me.target).unwrap_or(cx.me) } else { cx.me };
                    (around, self.radius_with(sp, &cx.sm))
                }).or_else(|| cx.at.and_then(|at| self.actors[at].totem.map(|st| (at, (st.area + cx.sm.radius_add) * cx.sm.radius_mul))));
                for &i in &self.live {
                    if self.actors[i].alive && self.actors[i].team != team && area.map_or(true, |(c, r)| self.dist(c, i) <= r + 1e-6) {
                        p.push(i);
                    }
                }
                if u == SelUse::Damage {
                    if let Some(t) = cx.tgt.or(me.target) {
                        p.lead_with(t);
                    }
                }
                p.truncate(self.ev(max, cx) as usize);
                p
            }
            TargetSel::Chain { max, jump } => {
                // the target, then each jump to the nearest enemy not yet hit
                let mut p = Picked::one(cx.tgt.or(me.target).filter(|&t| self.actors[t].alive));
                let n = self.ev(max, cx).max(0.0) as usize;
                let team = me.team;
                while p.len() < n {
                    let Some(last) = p.into_iter().last() else { break };
                    let next = self.live.iter().copied().filter(|&i| {
                        let y = &self.actors[i];
                        y.alive && y.team != team && y.aware && !p.into_iter().any(|q| q == i) && self.dist(last, i) <= jump + 1e-6
                    }).min_by(|&x, &y| self.dist(last, x).partial_cmp(&self.dist(last, y)).unwrap().then(x.cmp(&y)));
                    match next {
                        Some(i) => p.push(i),
                        None => break,
                    }
                }
                p
            }
            TargetSel::Ally | TargetSel::Party | TargetSel::Allies(_) => {
                let mut p = Picked::new();
                for i in self.friendly_sel(sel, cx).unwrap_or_default() {
                    p.push(i);
                }
                p
            }
        }
    }

    /// Held by crowd control that damage breaks (no area damage near it).
    pub fn soft_cc(&self, a: usize) -> bool {
        let w = self.w;
        self.actors[a].auras.iter().any(|i| {
            let d = &w.auras[i.def as usize];
            d.break_on_damage.is_some() && matches!(d.mechanic, Some(Mechanic::Incap | Mechanic::Fear))
        })
    }

    pub fn friendly_sel(&self, sel: &TargetSel, cx: &Cx<'w>) -> Option<Vec<usize>> {
        Some(match sel {
            TargetSel::Ally => {
                let a = self.ally_of(cx);
                if self.actors[a].alive { vec![a] } else { vec![] }
            }
            TargetSel::Party => self.allies(cx.me, true).collect(),
            TargetSel::Allies(max) => {
                let n = self.ev(max, cx).max(0.0) as usize;
                let first = self.ally_of(cx);
                let mut v: Vec<usize> = Vec::with_capacity(n);
                if self.actors[first].alive {
                    v.push(first);
                }
                v.extend(self.hurt_order(cx.me).into_iter().filter(|&i| i != first));
                v.truncate(n);
                v
            }
            _ => return None,
        })
    }

    fn talent_owner(&self, a: usize) -> usize {
        match self.actors[a].owner {
            Some(o) if self.actors[a].talents.is_empty() => o,
            _ => a,
        }
    }

    pub fn known(&self, a: usize, spell: SpellId) -> Option<usize> {
        {
            let c = self.actors[a].known_cache.borrow();
            if let Some(&v) = c.get(spell as usize) {
                if v != -2 {
                    return if v == -1 { None } else if v == i32::MAX { Some(usize::MAX) } else { Some(v as usize) };
                }
            }
        }
        let r = self.known_fresh(a, spell);
        let mut c = self.actors[a].known_cache.borrow_mut();
        if c.len() < self.w.spells.len() {
            c.resize(self.w.spells.len(), -2);
        }
        c[spell as usize] = match r {
            None => -1,
            Some(usize::MAX) => i32::MAX,
            Some(k) => k as i32,
        };
        r
    }

    fn known_fresh(&self, a: usize, spell: SpellId) -> Option<usize> {
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
            VarRef::Talent(c, i) => {
                // a class's talent, read only on a character of that class (a
                // party buff from another class reads nothing of yours)
                let o = &self.actors[self.talent_owner(cx.me)];
                if o.class != Some(*c as usize) {
                    return 0.0;
                }
                o.talents.get(*i as usize).copied().unwrap_or(0) as f64
            }
            VarRef::Rank(f) => self.rank_field(cx.rank, *f, self.actors[cx.me].level),
            VarRef::Opt(i) => self.opt(cx.me, *i as usize),
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
            VarRef::Dist(who) => {
                let o = if *who == Who::Me { self.actors[cx.me].target } else { self.who(cx, *who) };
                o.map_or(0.0, |o| self.dist(cx.me, o))
            }
            VarRef::Moving(who) => self.who(cx, *who).map_or(0.0, |a| b(self.moving(a))),
            VarRef::Fleeing(who) => self.who(cx, *who).map_or(0.0, |a| b(matches!(self.actors[a].mo.goal, Goal::Away { .. }))),
            VarRef::MagicBuffs(who) => self.who(cx, *who).map_or(0.0, |a| self.actors[a].auras.iter().filter(|i| self.purgeable(i.def)).count() as f64),
            VarRef::UnitInCombat(who) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].in_combat && self.actors[a].aware)),
            VarRef::ThreatPct => self.threat_pct(cx.me),
            VarRef::EnemiesWithin(yd) => {
                let team = self.actors[cx.me].team;
                self.live.iter().filter(|&&i| { let y = &self.actors[i]; y.alive && y.aware && y.team != team && !y.is_pet() && self.dist(cx.me, i) <= *yd as f64 + 1e-6 }).count() as f64
            }
            VarRef::EnemiesActive => {
                let team = self.actors[cx.me].team;
                self.live.iter().filter(|&&i| { let y = &self.actors[i]; y.alive && y.aware && y.team != team && !y.is_pet() && !self.soft_cc(i) }).count() as f64
            }
            VarRef::CcNear => {
                let me = &self.actors[cx.me];
                let team = me.team;
                let tgt = me.target;
                self.live.iter().filter(|&&i| {
                    let y = &self.actors[i];
                    y.alive && y.team != team && self.soft_cc(i) && (self.dist(cx.me, i) <= 10.0 || tgt.map_or(false, |t| t != i && self.dist(t, i) <= 10.0))
                }).count() as f64
            }
            VarRef::HitIndex => cx.hit as f64,
            VarRef::TotemActive(e) => b(self.totem_of(cx.me, *e).is_some()),
            VarRef::TotemRemains(e) => self.totem_of(cx.me, *e).and_then(|t| self.actors[t].totem).map_or(0.0, |st| (st.until - self.t).max(0.0).min(1e9)),
            VarRef::TotemInRange(e) => b(self.totem_of(cx.me, *e).map_or(false, |t| self.totem_reaches(t))),
            VarRef::HasAggro => b(self.actors[cx.me].target.map_or(false, |t| self.actors[t].target == Some(cx.me))),
            VarRef::Time => self.t,
            VarRef::CombatTime => self.t - self.combat_start,
            VarRef::AuraUp(who, id) => self.who(cx, *who).map_or(0.0, |a| {
                let x = &self.actors[a];
                let i = x.aura_index(*id, if *who == Who::Me { None } else { Some(cx.me) });
                // your own proc windows count once you've noticed them
                b(i.map_or(false, |i| *who != Who::Me || x.auras[i].seen <= self.t + EPS))
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
            VarRef::Casting(who) => self.who(cx, *who).map_or(0.0, |a| {
                let seen = |c: &CastState| *who == Who::Me || !self.reacting(cx.me) || c.seen <= self.t + EPS;
                b(self.actors[a].cast.as_ref().map_or(false, seen))
            }),
            VarRef::Wanding(who) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].wanding.is_some())),
            VarRef::IsPlayer(who) => self.who(cx, *who).map_or(0.0, |a| {
                let x = &self.actors[a];
                b(x.kind == Kind::Player || x.enemy.map_or(false, |e| e.player_like))
            }),
            VarRef::Boss(who) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].enemy.map_or(false, |e| e.boss))),
            VarRef::Creature(who, c) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].creature == *c)),
            VarRef::Alive(who) => self.who(cx, *who).map_or(0.0, |a| b(self.actors[a].alive)),
            VarRef::Behind => b(self.actors[cx.me].target.map_or(self.actors[cx.me].behind, |t| self.behind(cx.me, t))),
            VarRef::Enemies => {
                let team = self.actors[cx.me].team;
                self.live.iter().filter(|&&i| { let x = &self.actors[i]; x.alive && x.team != team && !x.is_pet() }).count() as f64
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
                        WField::SwingRemains => {
                            let s = a.swing[*h as usize];
                            if s == INF { 0.0 } else { (s - self.t).max(0.0) }
                        }
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
            VarRef::Deficit(who) => self.who(cx, *who).map_or(0.0, |a| (self.actors[a].max_hp - self.actors[a].hp).max(0.0)),
            VarRef::Absorb(who) => self.who(cx, *who).map_or(0.0, |a| self.actors[a].auras.iter().map(|i| i.absorb).sum()),
            VarRef::IsTank(who) => self.who(cx, *who).map_or(0.0, |a| b(a == self.tank_of(cx.me))),
            VarRef::IsMe(who) => self.who(cx, *who).map_or(0.0, |a| b(a == cx.me)),
            VarRef::Hurt { group, pct } => {
                let t = self.t;
                let line = *pct as f64 / 100.0;
                self.allies(cx.me, *group).filter(|&i| self.actors[i].hp_pct(t) < line).count() as f64
            }
            VarRef::AllyCount(group) => self.allies(cx.me, *group).count() as f64,
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
        self.live.push(id);
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
            mechs: 0,
            stealthed: false,
            distracted: false,
            free: false,
            no_flee: false,
            evaded: false,
            listens: u32::MAX,
            static_listens: u32::MAX,
            known_cache: Default::default(),
            aura_next: INF,
            sm_passives: vec![],
            cds: vec![],
            gcd_end: 0.0,
            cast: None,
            swing: [INF; 3],
            queued: None,
            wanding: None,
            opening: None,
            target: None,
            alive: true,
            rotation: None,
            auto_melee: false,
            auto_ranged: false,
            threat: vec![],
            taunt: None,
            left_combat: 0.0,
            last_mana_spend: -100.0,
            next_tick: self.t + phase,
            rng: [mk(1), mk(2), mk(3)],
            react_rng: mk(4),
            mo: Motion::default(),
            aware: true,
            pack: 0,
            fled: false,
            stand_at: 0.0,
            chase: None,
            aggro_dirty: false,
            unkillable: false,
            prep_step: None,
            tracking: None,
            totems: [None; 5],
            totem: None,
            dice: {
                let mut r = mk(5);
                [r.f64(), r.f64(), r.f64(), r.f64(), r.f64(), r.f64()]
            },
            react: None,
            opt_set: 0,
            react_until: 0.0,
            due: vec![],
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
            script_next: vec![],
            dr: [(0.0, 0); 9],
            passives: vec![],
            weapons: [None; 3],
            shield: None,
            timed_hp: None,
            slot,
            lockout: 0.0,
            group: 0,
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
        self.actors[id].sync_listens(w);
        // what it knows, worked out once (talents and level are set)
        let known: Vec<i32> = (0..w.spells.len()).map(|s| match self.known_fresh(id, s as SpellId) {
            None => -1,
            Some(usize::MAX) => i32::MAX,
            Some(k) => k as i32,
        }).collect();
        *self.actors[id].known_cache.borrow_mut() = known;
        self.refresh_stats(id);
        self.fill(id);
        self.setup_react(id);
        self.find_stand(id);
        id
    }

    pub fn spawn_enemy(&mut self, e: &'w EnemyDef, level: u32, team: u8, slot: u64, scale: f64) -> usize {
        let id = self.new_actor(e.name.clone(), Kind::Enemy, team, level, slot);
        self.spawned.push(id);
        let cx = Cx::new(id, None, "");
        let hp = self.ev(&e.hp, &cx) * e.hp_mult;
        let armor = self.ev(&e.armor, &cx) * e.armor_mult;
        let dmg = self.ev(&e.dmg, &cx) * e.dmg_mult;
        let dodge = e.dodge.as_ref().map_or(-1.0, |x| self.ev(x, &cx));
        let parry = e.parry.as_ref().map_or(-1.0, |x| self.ev(x, &cx));
        let ap = e.ap.as_ref().map_or(0.0, |x| self.ev(x, &cx)).max(0.0) * e.dmg_mult;
        let mana = e.mana.as_ref().map_or(0.0, |x| self.ev(x, &cx)).max(0.0);
        // the attack power's share of a hit comes out of the weapon
        let from_ap = ap / 14.0 * e.speed;
        let dmg = (dmg - from_ap).max(0.0);
        {
            let a = &mut self.actors[id];
            a.enemy = Some(e);
            a.creature = e.creature;
            a.fixed = Some([hp, armor, ap, e.crit, dodge, mana, parry]);
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
        self.actors[id].sync_listens(self.w);
        self.refresh_stats(id);
        self.fill(id);
        id
    }

    /// A party member from an ally template, at your level.
    pub fn spawn_ally(&mut self, e: &'w EnemyDef, level: u32, team: u8, group: u8, slot: u64) -> usize {
        let id = self.spawn_enemy(e, level, team, slot, 1.0);
        let a = &mut self.actors[id];
        a.kind = Kind::Ally;
        a.group = group;
        a.auto_melee = false;
        a.rotation = None;
        a.has_res = [true, false, false, false];
        self.refresh_stats(id);
        self.fill(id);
        id
    }

    /// Start a boss's scripted attacks on their timers.
    pub fn start_script(&mut self, a: usize) {
        let t = self.t;
        if let Some(e) = self.actors[a].enemy {
            self.actors[a].script_next = e.script.iter().map(|h| t + h.first).collect();
        }
    }

    fn script_hits(&mut self, a: usize) {
        let Some(e) = self.actors[a].enemy else { return };
        if e.script.is_empty() || !self.actors[a].in_combat {
            return;
        }
        let t = self.t;
        for (k, h) in e.script.iter().enumerate() {
            while self.actors[a].script_next.get(k).map_or(false, |&n| n <= t + EPS) {
                self.actors[a].script_next[k] += h.every;
                self.script_hit(a, h);
                if !self.actors[a].alive {
                    return;
                }
            }
        }
    }

    /// Whether a healing test's party has failed.
    pub fn party_failed(&self, max_dead: u32) -> bool {
        let mut dead = 0;
        for x in &self.actors {
            if x.team != 0 || x.alive {
                continue;
            }
            match x.kind {
                Kind::Player => return true,
                Kind::Ally if Some(x.id) == self.tank => return true,
                Kind::Ally => dead += 1,
                _ => {}
            }
        }
        dead >= max_dead
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
            a.target = otarget;
            a.in_combat = ocombat;
        }
        self.actors[owner].pets.push(id);
        // it appears at its owner's side
        let x = self.pos(owner);
        self.place(id, x);
        self.actors[id].sync_listens(self.w);
        self.refresh_stats(id);
        self.fill(id);
        self.find_stand(id);
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
                    // a party member's buffs read their caster's talents
                    // (a priest's Inspiration on the tank)
                    if x.kind == Kind::Ally {
                        cx.me = inst.src;
                    }
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
        self.actors[a].mechs & Mechanic::HARD != 0
    }

    pub fn hostile(&self, a: usize, b_: usize) -> bool {
        self.actors[a].team != self.actors[b_].team
    }

    /// Pick a live hostile target for an actor if it has none.
    pub fn retarget(&mut self, a: usize) {
        let me = &self.actors[a];
        if !me.alive || !me.aware {
            return;
        }
        // a mob turns on whoever passes its target's threat (a player-like
        // attacker sticks to its victim)
        if me.kind == Kind::Enemy && !me.enemy.map_or(false, |e| e.player_like) {
            let dirty = me.aggro_dirty;
            if let Some(t) = me.target.filter(|&t| self.actors[t].alive && self.hostile(a, t)) {
                if !dirty {
                    return;
                }
                self.actors[a].aggro_dirty = false;
                if let Some(n) = self.aggro_pick(a, t) {
                    if n != t {
                        self.turn_on(a, n);
                    }
                }
                return;
            }
            self.actors[a].aggro_dirty = false;
        } else if let Some(t) = me.target {
            // a player leaves a target it has put to sleep for one that's free
            if self.actors[t].alive && self.hostile(a, t) && !(me.kind == Kind::Player && self.soft_cc(t) && self.free_enemy(a).is_some()) {
                return;
            }
        }
        let me = &self.actors[a];
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
                self.free_enemy(a).or_else(|| self.live.iter().copied().find(|&i| { let x = &self.actors[i]; x.alive && x.team != team && !x.is_pet() }))
            }
        });
        let lost = self.actors[a].target.is_some();
        if self.actors[a].target != new {
            self.actors[a].wanding = None;
            self.actors[a].chase = None;
            if lost && new.is_some() && self.reacting(a) {
                let r = self.reaction(a);
                let x = &mut self.actors[a];
                x.react_until = x.react_until.max(self.t + r);
            }
        }
        self.actors[a].target = new;
    }

    /// The first enemy that isn't held by crowd control damage would break.
    fn free_enemy(&self, a: usize) -> Option<usize> {
        let team = self.actors[a].team;
        self.live.iter().copied().find(|&i| { let x = &self.actors[i]; x.alive && x.team != team && !x.is_pet() && !self.soft_cc(i) })
    }

    /// Your threat on your target as a share of whoever it's attacking.
    pub fn threat_pct(&self, a: usize) -> f64 {
        let Some(t) = self.actors[a].target else { return 0.0 };
        let mob = &self.actors[t];
        let Some(h) = mob.target else { return 0.0 };
        if h == a {
            return 1.0;
        }
        let of = |i: usize| mob.threat.iter().find(|x| x.0 as usize == i).map_or(0.0, |x| x.1);
        let theirs = of(h);
        let mine = of(a);
        if theirs <= 0.0 {
            return if mine > 0.0 { 1.0 } else { 0.0 };
        }
        mine / theirs
    }

    /// Whom a mob on `cur` should be attacking now: a taunter while the
    /// taunt runs, else whoever has passed 110% of `cur`'s threat from
    /// melee range (130% from farther).
    fn aggro_pick(&self, e: usize, cur: usize) -> Option<usize> {
        let me = &self.actors[e];
        if let Some((t, until)) = me.taunt {
            if until > self.t && self.actors[t].alive {
                return Some(t);
            }
        }
        let c = &self.w.combat;
        let mine = |i: usize| me.threat.iter().find(|x| x.0 as usize == i).map_or(0.0, |x| x.1);
        let held = mine(cur);
        let mut best: Option<(usize, f64)> = None;
        for &(i, th) in &me.threat {
            let i = i as usize;
            if i == cur || !self.actors[i].alive || self.actors[i].team == me.team || th <= 0.0 {
                continue;
            }
            let need = if self.dist(e, i) <= self.reach(e) + 1e-6 { c.aggro_melee } else { c.aggro_ranged };
            if th > held * need && best.map_or(true, |(bi, b)| th > b || (th == b && i < bi)) {
                best = Some((i, th));
            }
        }
        Some(best.map_or(cur, |x| x.0))
    }

    /// A mob switches to a new victim.
    fn turn_on(&mut self, e: usize, n: usize) {
        self.actors[e].target = Some(n);
        self.actors[e].mo.closing = false;
        if let Some((boss, _, _, ref mut pulls)) = self.aggro_watch {
            if boss == e && Some(n) == self.main {
                *pulls += 1;
            }
        }
        self.tracef(|s| s.tev("aggro", e, Some(n), ""));
        self.logf(|s| format!("{} turns on {}", s.actors[e].name, s.actors[n].name));
    }

    pub fn threat_target(&self, e: usize) -> Option<usize> {
        let me = &self.actors[e];
        if let Some((t, until)) = me.taunt {
            if until > self.t && self.actors[t].alive {
                return Some(t);
            }
        }
        let mut best: Option<(usize, f64)> = None;
        for &(i, th) in &me.threat {
            let i = i as usize;
            if self.actors[i].alive && self.actors[i].team != me.team && th > 0.0 {
                // ties go to the lower id
                if best.map_or(true, |(bi, b)| th > b || (th == b && i < bi)) {
                    best = Some((i, th));
                }
            }
        }
        best.map(|x| x.0).or_else(|| self.live.iter().copied().find(|&i| { let x = &self.actors[i]; x.alive && x.team != me.team && !x.is_pet() }))
    }

    pub fn add_threat(&mut self, enemy: usize, who: usize, amount: f64) {
        if !self.actors[enemy].alive || self.actors[enemy].kind != Kind::Enemy {
            return;
        }
        if !self.actors[enemy].aware && self.hostile(enemy, who) {
            self.wake(enemy, Some(who));
        }
        self.actors[enemy].aggro_dirty = true;
        *threat_of(&mut self.actors[enemy].threat, who) += amount;
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
        if !x.alive || (x.cast.is_some() && !shoots_through_casts) || self.phase != Phase::Fight {
            return false;
        }
        let wand = hand == Hand::Ranged && x.weapons[2].map_or(false, |wp| wp.wtype == WeaponType::Wand);
        // shooting a wand stops melee auto attacks
        let on = if wand { x.wanding.is_some() } else if hand == Hand::Ranged { x.auto_ranged } else { x.auto_melee && x.wanding.is_none() };
        if !on || x.weapons[hand as usize].is_none() {
            return false;
        }
        let t = match x.target {
            Some(t) if self.actors[t].alive && self.hostile(a, t) => t,
            _ => return false,
        };
        if self.is_cc(a) || !x.aware || x.stealthed || matches!(x.mo.goal, Goal::Away { .. } | Goal::Charge(_)) {
            return false;
        }
        // in reach: melee as far as the arm goes; bows and guns from 8 to
        // 35 yards and wands to 30, standing still
        let d = self.dist(a, t);
        if hand == Hand::Ranged {
            let (lo, hi) = if wand { (0.0, 30.0) } else { (8.0, 35.0 + x.st[Stat::RangedRange as usize]) };
            if x.mo.v != 0.0 || d > hi + 1e-6 || d < lo - 1e-6 {
                return false;
            }
        } else if d > self.reach(a) + 1e-6 {
            return false;
        }
        if hand != Hand::Ranged && x.has_mech(self.w, Mechanic::Disarm) && x.st[Stat::ImmuneDisarm as usize] <= 0.0 {
            return false;
        }
        true
    }

    /// Start auto-attacks for every actor that should be swinging.
    pub fn start_swings(&mut self) {
        let t = self.t;
        for &a in &self.live {
            let x = &mut self.actors[a];
            for h in [Hand::Mh, Hand::Oh, Hand::Ranged] {
                if x.weapons[h as usize].is_some() && x.swing[h as usize] == INF {
                    let off = if h == Hand::Oh { 0.2 } else { 0.0 };
                    x.swing[h as usize] = t + off;
                }
            }
        }
    }

    pub fn enter_combat(&mut self) {
        self.combat_start = self.t;
        self.phase = Phase::Fight;
        self.prune_live();
        // who is fighting already: everyone who has noticed a foe that has
        // noticed someone too (a pull of unaware mobs starts when they wake)
        let ids = self.live_snapshot();
        let engaged = |s: &Self, a: usize| {
            let x = &s.actors[a];
            x.aware && s.live.iter().any(|&o| s.actors[o].alive && s.actors[o].aware && s.actors[o].team != x.team)
        };
        self.woke_at = ids.iter().any(|&a| self.actors[a].alive && engaged(self, a)).then_some(self.t);
        let mut on = std::mem::take(&mut self.scratch_b);
        on.clear();
        for &a in &ids {
            if self.actors[a].alive {
                if engaged(self, a) {
                    self.actors[a].in_combat = true;
                    on.push(a);
                }
                self.retarget(a);
            }
        }
        self.give_back(ids);
        self.start_swings();
        for &a in &on {
            if self.actors[a].alive {
                self.emit(a, EventKind::CombatStart, Ev::default(), 0);
            }
        }
        self.scratch_b = on;
    }

    pub fn leave_combat(&mut self) {
        let w = self.w;
        self.charges.clear();
        self.arrivals.clear();
        for a in 0..self.actors.len() {
            while let Some(k) = self.actors[a].auras.iter().position(|i| w.auras[i.def as usize].combat_only) {
                self.remove_aura_at(a, k, false);
            }
            let x = &mut self.actors[a];
            x.left_combat = self.t;
            x.in_combat = false;
            x.due.clear();
            x.swing = [INF; 3];
            x.queued = None;
            x.wanding = None;
            x.opening = None;
            x.cast = None;
            x.threat.clear();
            x.taunt = None;
            x.cp = 0.0;
            x.mo.goal = Goal::Engage;
            x.mo.closing = false;
            x.mo.next = INF;
            x.chase = None;
            self.stop(a);
        }
    }

    // ------------------------------------------------------------ main loop

    fn next_time(&self, stop: &Stop) -> f64 {
        let mut n = match stop {
            Stop::Fight { until } | Stop::Rest { until, .. } | Stop::Prep { until, .. } | Stop::Until(until) | Stop::Party { until, .. } => *until,
        };
        for r in &self.arrivals {
            n = n.min(r.at);
        }
        for &i in &self.live {
            let x = &self.actors[i];
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
            if x.react.is_some() {
                if x.react_until > self.t {
                    n = n.min(x.react_until);
                }
                for &d in &x.due {
                    if d > self.t {
                        n = n.min(d);
                    }
                }
            }
            n = n.min(x.aura_next);
            if x.totem.is_some() {
                n = n.min(self.totem_next(i));
            }
            if let Some(c) = &x.cast {
                if c.seen > self.t {
                    n = n.min(c.seen);
                }
            }
            n = n.min(x.next_tick);
            if x.mo.next > self.t {
                n = n.min(x.mo.next);
            }
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
            if x.in_combat {
                for &st in &x.script_next {
                    n = n.min(st.max(self.t));
                }
            }
        }
        n
    }

    fn process(&mut self) {
        if !self.arrivals.is_empty() {
            self.bring_arrivals();
        }
        let t = self.t;
        let w = self.w;
        let mut ids = self.live_snapshot();
        if self.flip {
            ids.reverse();
        }
        for &a in &ids {
            if !self.actors[a].alive {
                continue;
            }
            // aura ticks then expiries (only when one is due)
            let mut guard = if self.actors[a].aura_next > t + EPS { 65 } else { 0 };
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
                self.actors[a].sync_aura_next(t);
                let def = &w.auras[inst.def as usize];
                // a totem pulses only while what it works on is in reach
                let totem = self.actors[a].kind == Kind::Totem;
                if let Some((_, effs)) = def.tick.as_ref().filter(|_| !totem || self.totem_reaches(a)) {
                    let src = if self.actors[inst.src].alive || inst.src == a { inst.src } else { a };
                    let mut cx = Cx::new(src, Some(a), def.name.as_str());
                    cx.rank = inst.rank;
                    cx.stacks = inst.stacks as f64;
                    cx.aura = Some((a, inst.uid));
                    cx.sm = self.spell_mod_vals(src, inst.rank.map(|r| r.0).or(inst.spell), None);
                    if totem {
                        cx.at = Some(a);
                        cx.tgt = self.actors[src].target;
                    }
                    self.run_effects(effs, cx);
                }
                if !self.actors[a].alive {
                    break;
                }
            }
            if !self.actors[a].alive {
                continue;
            }
            if self.actors[a].aura_next <= t + EPS {
                loop {
                    let x = &self.actors[a];
                    let Some(k) = x.auras.iter().position(|i| i.expires <= t + EPS) else { break };
                    self.remove_aura_at(a, k, true);
                }
                self.actors[a].sync_aura_next(t);
            }
            if self.actors[a].totem.is_some() && !self.tend_totem(a) {
                continue;
            }
            // scripted stuns (pressure attackers) and attacks (healing bosses)
            self.scripted_stuns(a);
            self.script_hits(a);
            if !self.actors[a].alive {
                continue;
            }
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
        self.give_back(ids);
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
        // a party member's steady threat (the healing tests' tank)
        if x.kind == Kind::Ally && x.in_combat {
            if let Some(e) = x.enemy.and_then(|e| e.threat_per_sec.as_ref()) {
                let th = self.ev(e, &cx) * tick;
                let team = x.team;
                let foes: Vec<usize> = self.live.iter().copied().filter(|&i| { let y = &self.actors[i]; y.alive && y.aware && y.team != team && y.kind == Kind::Enemy }).collect();
                for f in foes {
                    self.add_threat(f, a, th);
                }
            }
        }
        let x = &self.actors[a];
        let decay = x.has_res[Res::Rage as usize] && !x.in_combat && self.t - x.left_combat >= x.st[Stat::RageDecayDelay as usize] - EPS;
        let rage_decay = self.rage_decay;
        let x = &mut self.actors[a];
        if decay {
            x.res[Res::Rage as usize] = (x.res[Res::Rage as usize] - rage_decay * tick).max(0.0);
        }
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
        let mut ids = self.live_snapshot();
        if self.flip {
            ids.reverse();
        }
        for &a in &ids {
            if !self.actors[a].alive {
                continue;
            }
            if let Stop::Rest { actor, .. } = stop {
                if a != *actor {
                    continue;
                }
            }
            if let Stop::Prep { who, .. } = stop {
                if !who.contains(&a) {
                    continue;
                }
            }
            for _ in 0..12 {
                if !self.act(a) {
                    break;
                }
            }
        }
        self.give_back(ids);
    }

    fn players_alive(&self) -> bool {
        if let Some(m) = self.main {
            return self.actors[m].alive;
        }
        self.live.iter().any(|&i| { let x = &self.actors[i]; x.team == 0 && x.kind == Kind::Player && x.alive })
    }
    fn enemies_alive(&self) -> bool {
        self.live.iter().any(|&i| { let x = &self.actors[i]; x.team != 0 && x.alive && !x.is_pet() && !x.evaded })
    }
    fn any_evaded(&self) -> bool {
        self.live.iter().any(|&i| { let x = &self.actors[i]; x.team != 0 && x.alive && x.evaded })
    }

    /// A mob left with nobody to fight runs back to its spot and heals: it's
    /// out of this fight.
    pub fn evade(&mut self, e: usize) {
        let x = &mut self.actors[e];
        if x.evaded || !x.alive {
            return;
        }
        x.evaded = true;
        x.hp = x.max_hp;
        x.target = None;
        x.threat.clear();
        x.taunt = None;
        x.cast = None;
        x.in_combat = false;
        self.stop(e);
        let main = self.main.unwrap_or(0);
        self.tracef(|s| s.tev("evade", e, Some(main), ""));
        self.logf(|s| format!("{} evades", s.actors[e].name));
    }

    pub fn run(&mut self, stop: Stop) -> Outcome {
        let mut iters: u64 = 0;
        let mut still: u32 = 0;
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
                        return if self.any_evaded() { Outcome::Escaped } else { Outcome::EnemiesDead };
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
                Stop::Prep { who, until } => {
                    if self.t >= until - EPS || who.iter().any(|&a| !self.actors[a].alive) {
                        return Outcome::Timeout;
                    }
                }
                Stop::Party { until, max_dead } => {
                    if self.party_failed(max_dead) {
                        return Outcome::Failed;
                    }
                    if self.t >= until - EPS {
                        return Outcome::Timeout;
                    }
                }
            }
            self.prune_live();
            if self.phase == Phase::Fight {
                self.check_aggro();
                let ids = self.live_snapshot();
                for &a in &ids {
                    if self.actors[a].alive {
                        self.retarget(a);
                    }
                }
                self.give_back(ids);
            }
            self.act_all(&stop);
            if self.phase == Phase::Fight {
                self.plan_motion();
            }
            if let Stop::Rest { actor, .. } = stop {
                if self.idle(actor) {
                    return Outcome::Idle;
                }
            }
            if let Stop::Prep { who, .. } = stop {
                if who.iter().all(|&a| self.idle(a)) {
                    return Outcome::Idle;
                }
            }
            if matches!(stop, Stop::Fight { .. }) && (!self.players_alive() || !self.enemies_alive()) {
                continue;
            }
            if let Stop::Party { max_dead, .. } = stop {
                if self.party_failed(max_dead) {
                    continue;
                }
            }
            let n = self.next_time(&stop);
            // a moment the engine can't get past (it shouldn't happen): nudge on
            if n <= self.t + 1e-12 {
                still += 1;
                if still > self.w.combat.max_spins {
                    self.spins += 1;
                    self.t += 1e-3;
                    still = 0;
                }
            } else {
                still = 0;
            }
            if let Some((b, tk, ref mut held, _)) = self.aggro_watch {
                if self.actors[b].target == Some(tk) {
                    *held += (n - self.t).max(0.0);
                }
            }
            self.t = n.max(self.t);
            self.process();
        }
    }
}

/// One attacker's entry in a threat table, made at zero if it has none.
pub fn threat_of(t: &mut Vec<(u32, f64)>, who: usize) -> &mut f64 {
    let k = match t.iter().position(|x| x.0 as usize == who) {
        Some(k) => k,
        None => {
            t.push((who as u32, 0.0));
            t.len() - 1
        }
    };
    &mut t[k].1
}
