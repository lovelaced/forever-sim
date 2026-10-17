//! Compiled game data. Everything here is built from the TOML files by
//! `world.rs` and is read-only during simulation.

use crate::expr::Expr;
use crate::vars::*;
use std::collections::{BTreeMap, HashMap};

pub type SpellId = u32;
pub type AuraId = u32;

#[derive(Clone, Debug)]
pub struct OptDef {
    pub name: String,
    pub default: f64,
    /// What the number means, in full.
    pub desc: String,
    /// Short label for the page, a longer note under it, and how to show it:
    /// "toggle" (0/1), "pct" (0..1 shown as a percentage) or "num" with `unit`.
    pub label: String,
    pub help: String,
    pub kind: String,
    pub unit: String,
    /// Where the default comes from: "game" (Forever's own data), "community"
    /// (players' measurements), "classic" (the original game's value) or
    /// "estimate"; `cite` says exactly where.
    pub source: String,
    pub cite: String,
    /// Classes and races it matters to (empty: everyone).
    pub applies: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct CombatCfg {
    pub rage_conv: [f64; 3],
    pub rage_dealt: f64,
    pub rage_taken: f64,
    /// Rage from a critical basic attack, as a multiple of a normal hit's.
    pub rage_crit_mult: f64,
    pub melee_miss: Vec<f64>,
    pub mob_dodge: Vec<f64>,
    pub mob_parry: Vec<f64>,
    pub glance_chance: Vec<f64>,
    pub glance_mult: Vec<f64>,
    pub crit_suppression: Vec<f64>,
    pub spell_hit: Vec<f64>,
    pub spell_hit_cap: f64,
    pub dual_wield_miss: f64,
    pub defense_per_point: f64,
    pub gcd: f64,
    pub energy_gcd: f64,
    /// Seconds a wand winds up (standing still) before its first shot.
    pub auto_repeat_windup: f64,
    pub pushback: f64,
    pub max_pushbacks: u32,
    pub heal_to_spell_damage: f64,
    pub melee_crit_mult: f64,
    pub spell_crit_mult: f64,
    pub downrank_offset: f64,
    pub armor_cap: f64,
    pub armor_k_per_level: f64,
    pub low_level_penalty_level: f64,
    /// Seconds after a crowd-control effect ends during which the next one of the same kind is diminished (0 = off).
    pub dr_window: f64,
    pub low_level_penalty_per: f64,
    pub armor_k_base: f64,
    pub tick: f64,
    pub energy_per_tick: f64,
    pub five_second_rule: f64,
    pub normalized_1h: f64,
    pub normalized_2h: f64,
    pub normalized_dagger: f64,
    pub normalized_ranged: f64,
    pub block_crit_allowed: bool,
}

impl CombatCfg {
    pub fn rage_conversion(&self, level: f64) -> f64 {
        self.rage_conv[0] * level * level + self.rage_conv[1] * level + self.rage_conv[2]
    }
    /// Classic armour formula: damage reduction against an attacker of a level.
    pub fn armor_dr(&self, armor: f64, attacker_level: f64) -> f64 {
        if armor <= 0.0 {
            return 0.0;
        }
        let k = self.armor_k_per_level * attacker_level + self.armor_k_base;
        (armor / (armor + k)).min(self.armor_cap)
    }
    pub fn by_diff(table: &[f64], diff: i32) -> f64 {
        if table.is_empty() {
            return 0.0;
        }
        let i = diff.clamp(0, table.len() as i32 - 1) as usize;
        table[i]
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ModOp {
    Add,
    Mul,
}

#[derive(Clone, Debug)]
pub struct StatMod {
    pub stat: Stat,
    pub op: ModOp,
    pub value: Expr,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpellProp {
    Cost,
    CastTime,
    Cooldown,
    Damage,
    Heal,
    Crit,
    CritBonus,
    Threat,
    Gcd,
}

#[derive(Clone, Debug)]
pub struct SpellMod {
    /// Empty = all spells.
    pub spells: Vec<SpellId>,
    pub school: Option<School>,
    pub prop: SpellProp,
    pub op: ModOp,
    pub value: Expr,
}

impl SpellMod {
    pub fn applies(&self, spell: SpellId, school: School) -> bool {
        (self.spells.is_empty() || self.spells.contains(&spell)) && self.school.map_or(true, |s| s == school)
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum EventKind {
    MeleeSwing,
    MeleeLanded,
    MeleeCrit,
    WhiteLanded,
    SpecialLanded,
    RangedLanded,
    SpellLanded,
    SpellCrit,
    Periodic,
    Heal,
    HealCrit,
    DamageDealt,
    Struck,
    StruckMelee,
    StruckCrit,
    Dodge,
    Parry,
    Block,
    AttackDodged,
    AttackParried,
    Kill,
    Cast(SpellId),
    AnyCast,
    Custom(u16),
    AuraApplied(AuraId),
    AuraExpired(AuraId),
    CombatStart,
    Summon,
    PetDied,
}

#[derive(Clone, Debug)]
pub struct Proc {
    pub on: EventKind,
    pub cond: Option<Expr>,
    pub chance: Option<Expr>,
    /// Procs per minute (chance = ppm * weapon speed / 60).
    pub ppm: Option<Expr>,
    pub icd: f64,
    pub effects: Vec<Effect>,
    /// Unique id for internal-cooldown bookkeeping.
    pub key: u32,
}

#[derive(Clone, Debug)]
pub struct Passive {
    pub name: String,
    pub cond: Option<Expr>,
    pub mods: Vec<StatMod>,
    pub spell_mods: Vec<SpellMod>,
    pub procs: Vec<Proc>,
}

#[derive(Clone, Debug)]
pub enum TargetSel {
    Me,
    Target,
    Pet,
    Owner,
    /// All live hostile actors, up to a maximum.
    Enemies(Expr),
    /// The acting unit's current hostile target (for auras the unit holds).
    MyTarget,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum HitKind {
    White,
    Special,
    RangedWhite,
    RangedSpecial,
    Spell,
    /// A wand shot: can miss (ranged table), can't be dodged, parried or
    /// blocked; crits with ranged crit, for the spell crit bonus.
    Wand,
    /// Cannot miss (DoT ticks, procs that already hit).
    Auto,
}

#[derive(Clone, Debug)]
pub struct DamageFx {
    pub school: Option<School>,
    pub hit: HitKind,
    pub weapon: Option<Hand>,
    pub weapon_mult: Option<Expr>,
    pub normalized: bool,
    pub min: Option<Expr>,
    pub max: Option<Expr>,
    pub amount: Option<Expr>,
    pub coef: Option<Expr>,
    pub mult: Option<Expr>,
    pub crit: Option<Expr>,
    pub crit_bonus: Option<Expr>,
    pub can_crit: bool,
    pub threat: Option<Expr>,
    pub targets: TargetSel,
    pub tag: Option<String>,
    pub on_hit: Vec<Effect>,
    pub on_crit: Vec<Effect>,
    pub on_miss: Vec<Effect>,
    pub lifesteal: Option<Expr>,
    pub periodic: bool,
    pub no_proc: bool,
    /// Ignores every damage modifier on the attacker and the target (spell
    /// modifiers, damage done and taken): Touch of the Grave in Forever.
    pub fixed: bool,
}

#[derive(Clone, Debug)]
pub struct HealFx {
    pub targets: TargetSel,
    pub min: Option<Expr>,
    pub max: Option<Expr>,
    pub amount: Option<Expr>,
    pub coef: Option<Expr>,
    pub mult: Option<Expr>,
    pub can_crit: bool,
    pub crit: Option<Expr>,
    pub tag: Option<String>,
    pub periodic: bool,
    pub on_crit: Vec<Effect>,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CdOp {
    Reset,
    Reduce,
    Set,
}

#[derive(Clone, Debug)]
pub enum Effect {
    Damage(Box<DamageFx>),
    Heal(Box<HealFx>),
    Resource { who: TargetSel, res: Res, amount: Expr, set: bool },
    Combo(Expr),
    ResetCombo,
    ApplyAura { who: TargetSel, aura: AuraId, stacks: Expr, duration: Option<Expr> },
    RemoveAura { who: TargetSel, aura: AuraId },
    RemoveSelf,
    ConsumeCharge,
    Emit(u16),
    Cooldown { spell: SpellId, op: CdOp, value: Expr },
    Summon(u16),
    Dismiss,
    Sacrifice,
    ExtraAttack { hand: Hand, count: Expr, ap_bonus: Option<Expr> },
    Cast { spell: SpellId, who: TargetSel },
    Interrupt { who: TargetSel, lockout: Expr },
    If { cond: Expr, then: Vec<Effect>, els: Vec<Effect> },
    Chance { p: Expr, then: Vec<Effect> },
    Dispel { who: TargetSel, mechanics: Vec<Mechanic> },
    Taunt { duration: Expr },
    Threat { amount: Expr },
    Absorb { amount: Expr },
}

#[derive(Clone, Debug)]
pub struct RankDef {
    pub level: u32,
    pub fields: Vec<(u16, f64)>,
    /// Per-level growth of fields (`<field>_per_level` in the data): the
    /// game raises a rank's numbers with the caster's level until the rank's
    /// `max_level`.
    pub grow: Vec<(u16, f64)>,
    pub max_level: Option<u32>,
}

impl RankDef {
    /// A field's value for a caster of `level`.
    pub fn value(&self, field: u16, level: u32) -> Option<f64> {
        let v = self.fields.iter().find(|(f, _)| *f == field)?.1;
        let Some(&(_, g)) = self.grow.iter().find(|(f, _)| *f == field) else { return Some(v) };
        let top = self.max_level.unwrap_or(u32::MAX).min(level);
        Some(v + g * top.saturating_sub(self.level) as f64)
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum FormReq {
    Any,
    NoForm,
    Forms(Vec<u16>),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum SpellTarget {
    Enemy,
    Me,
    Friendly,
    Pet,
}

#[derive(Clone, Debug)]
pub struct ChannelDef {
    pub duration: Expr,
    pub interval: Expr,
    pub effects: Vec<Effect>,
}

#[derive(Clone, Debug)]
pub struct SpellDef {
    pub id: String,
    pub qid: String,
    pub name: String,
    pub class: Option<usize>,
    pub race: Option<u16>,
    pub school: School,
    pub talent: Option<u16>,
    pub learn_level: u32,
    pub ranks: Vec<RankDef>,
    pub cost: Vec<(Res, Expr)>,
    pub cast_time: Expr,
    pub channel: Option<ChannelDef>,
    pub gcd: Option<Expr>,
    pub off_gcd: bool,
    pub cooldown: Expr,
    pub cd_group: Option<u32>,
    pub requires: Option<Expr>,
    pub forms: FormReq,
    pub next_swing: bool,
    /// Starts an auto-repeating ranged attack (a wand's Shoot): its effects
    /// fire on every ranged swing until another spell is cast.
    pub auto_repeat: bool,
    pub target: SpellTarget,
    pub effects: Vec<Effect>,
    pub finisher: bool,
    pub usable_while: Vec<Mechanic>,
    pub is_attack: bool,
    pub source: String,
    pub text: String,
    /// The game's icon name (e.g. "ability_warrior_charge"), for the website.
    pub icon: Option<String>,
}

impl SpellDef {
    /// Highest rank available at a level (index), or None if not learned.
    pub fn rank_at(&self, level: u32) -> Option<usize> {
        if self.ranks.is_empty() {
            return if level >= self.learn_level { Some(usize::MAX) } else { None };
        }
        let mut best = None;
        for (i, r) in self.ranks.iter().enumerate() {
            if r.level <= level {
                best = Some(i);
            }
        }
        best
    }
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Refresh {
    Refresh,
    Ignore,
    Replace,
}

#[derive(Clone, Debug)]
pub struct AuraDef {
    pub id: String,
    pub qid: String,
    pub name: String,
    pub class: Option<usize>,
    pub duration: Expr,
    pub max_stacks: u32,
    pub charges: Option<Expr>,
    pub per_source: bool,
    pub debuff: bool,
    pub mechanic: Option<Mechanic>,
    pub break_on_damage: Option<f64>,
    pub tick: Option<(Expr, Vec<Effect>)>,
    pub mods: Vec<StatMod>,
    pub spell_mods: Vec<SpellMod>,
    pub procs: Vec<Proc>,
    pub on_apply: Vec<Effect>,
    pub on_refresh: Vec<Effect>,
    pub on_expire: Vec<Effect>,
    pub on_remove: Vec<Effect>,
    pub absorb: Option<Expr>,
    pub group: Option<u16>,
    pub form: Option<u16>,
    pub consume_on_cast: bool,
    pub refresh: Refresh,
    pub dispel_type: Option<String>,
    pub text: String,
    /// The game's icon name; without one the website uses the icon of the
    /// spell that applies the aura.
    pub icon: Option<String>,
    /// A bookkeeping aura (a proc window, an internal cooldown) that the
    /// player never sees in the game; the fight log leaves it out.
    pub hidden: bool,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub spell: SpellId,
    pub cond: Option<Expr>,
    pub target: SpellTarget,
    pub line: String,
}

#[derive(Clone, Debug)]
pub struct Rotation {
    pub id: String,
    pub desc: String,
    pub auto_melee: bool,
    pub auto_ranged: bool,
    pub precombat: Vec<Action>,
    /// Run free at the start of every fight (Charge, stealth openers).
    pub opener: Vec<Action>,
    pub actions: Vec<Action>,
    pub rest: Vec<Action>,
    /// Out-of-combat resting thresholds (eat / drink below these fractions).
    pub eat_below: f64,
    pub drink_below: f64,
}

#[derive(Clone, Debug)]
pub struct TalentDef {
    pub id: String,
    pub name: String,
    pub tree: String,
    pub points: u32,
    pub ranks: u32,
    pub requires: Option<u16>,
    pub text: String,
    /// Game icon file name (e.g. "ability_ghoulfrenzy"); only the web page uses it.
    pub icon: String,
    pub source: String,
}

#[derive(Clone, Debug)]
pub struct BaseAnchor {
    pub level: f64,
    pub vals: [f64; 7], // str agi sta int spi hp mana
}

#[derive(Clone, Debug)]
pub struct ClassFormulas {
    pub ap: Expr,
    pub rap: Expr,
    pub melee_crit: Expr,
    pub ranged_crit: Expr,
    pub spell_crit: Expr,
    pub dodge: Expr,
    pub parry: Expr,
    pub block: Expr,
    pub mana_regen: Expr,
    pub hp_regen: Expr,
}

#[derive(Clone, Debug)]
pub struct ClassDef {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub resources: Vec<Res>,
    pub base: Vec<BaseAnchor>,
    pub f: ClassFormulas,
    pub talents: Vec<TalentDef>,
    pub talent_index: HashMap<String, u16>,
    pub trees: Vec<String>,
    pub passives: Vec<Passive>,
    pub rotations: BTreeMap<String, Rotation>,
    pub can_parry: bool,
    pub can_block: bool,
    pub can_dual_wield: bool,
    pub energy_gcd: bool,
    pub source: String,
}

impl ClassDef {
    pub fn base_at(&self, level: u32) -> [f64; 7] {
        let l = level as f64;
        let b = &self.base;
        if b.len() == 1 || l <= b[0].level {
            return b[0].vals;
        }
        for w in b.windows(2) {
            if l <= w[1].level {
                let f = (l - w[0].level) / (w[1].level - w[0].level);
                let mut out = [0.0; 7];
                for i in 0..7 {
                    out[i] = w[0].vals[i] + f * (w[1].vals[i] - w[0].vals[i]);
                }
                return out;
            }
        }
        b[b.len() - 1].vals
    }
}

#[derive(Clone, Debug)]
pub struct RaceDef {
    pub id: String,
    pub name: String,
    pub icon: String,
    pub faction: String,
    pub classes: Vec<String>,
    pub stats: [f64; 5],
    pub passives: Vec<Passive>,
    pub creature: CreatureType,
    pub source: String,
}

#[derive(Clone, Debug)]
pub struct PetDef {
    pub id: String,
    pub name: String,
    pub class: usize,
    pub hp: Expr,
    pub armor: Expr,
    pub dmg_min: Expr,
    pub dmg_max: Expr,
    pub speed: f64,
    pub mana: Expr,
    pub ap: Expr,
    pub sp: Expr,
    pub crit: Expr,
    pub dodge: Expr,
    pub creature: CreatureType,
    pub mods: Vec<StatMod>,
    pub procs: Vec<Proc>,
    pub rotation: Option<Rotation>,
    pub auto_melee: bool,
}

#[derive(Clone, Copy, Debug)]
pub struct Weapon {
    pub min: f64,
    pub max: f64,
    pub speed: f64,
    pub wtype: WeaponType,
    /// Damage school: physical, or a wand's school.
    pub school: School,
}

impl Weapon {
    pub fn dps(&self) -> f64 {
        (self.min + self.max) / 2.0 / self.speed
    }
}

#[derive(Clone, Debug, Default)]
pub struct GearStats {
    pub stats: Vec<(Stat, f64)>,
    pub mh: Option<Weapon>,
    pub oh: Option<Weapon>,
    pub ranged: Option<Weapon>,
    pub shield: Option<(f64, f64)>,
}

#[derive(Clone, Debug)]
pub struct GearKit {
    pub id: String,
    pub desc: String,
    pub anchors: Vec<(u32, GearStats)>,
}

#[derive(Clone, Debug)]
pub struct EnemyDef {
    pub id: String,
    pub name: String,
    pub creature: CreatureType,
    pub level_offset: i32,
    pub hp: Expr,
    pub armor: Expr,
    pub dmg: Expr,
    pub dmg_spread: f64,
    pub speed: f64,
    pub crit: f64,
    pub dodge: Option<Expr>,
    pub parry: Option<Expr>,
    pub block: f64,
    pub attacks: bool,
    pub boss: bool,
    pub player_like: bool,
    pub behind: bool,
    pub school: School,
    pub rotation: Option<Rotation>,
    pub stuns: Vec<(f64, f64)>,
    pub mods: Vec<StatMod>,
}

#[derive(Clone, Debug)]
pub enum ScenarioKind {
    Leveling { pull: u32, minutes: f64, enemy: String, travel: f64 },
    Boss { duration: f64, enemy: String, execute_by_time: bool },
    Duel { max_time: f64, opponent: Option<String> },
    Pressure { attacker: String, attackers: u32, lo: f64, hi: f64, steps: u32, max_time: f64, win_needs_kill: bool },
}

#[derive(Clone, Debug)]
pub struct ScenarioDef {
    pub id: String,
    pub desc: String,
    pub kind: ScenarioKind,
    pub primary: String,
    pub behind: bool,
    pub opts: Vec<(u16, f64)>,
}

#[derive(Clone, Debug)]
pub struct RestCfg {
    /// (level, food health, drink mana, seconds to consume)
    pub table: Vec<(u32, f64, f64, f64)>,
}

impl RestCfg {
    pub fn at(&self, level: u32) -> (f64, f64, f64) {
        let mut best = self.table[0];
        for &row in &self.table {
            if row.0 <= level {
                best = row;
            }
        }
        (best.1, best.2, best.3)
    }
}

#[derive(Clone, Debug, Default)]
pub struct WorldMeta {
    pub data_build: String,
    pub patches: Vec<String>,
}

#[derive(Clone, Debug)]
pub struct World {
    pub meta: WorldMeta,
    pub combat: CombatCfg,
    pub opts: Vec<OptDef>,
    pub classes: Vec<ClassDef>,
    pub races: Vec<RaceDef>,
    pub spells: Vec<SpellDef>,
    pub auras: Vec<AuraDef>,
    pub pets: Vec<PetDef>,
    pub forms: Vec<String>,
    pub groups: Vec<String>,
    pub customs: Vec<String>,
    pub rank_fields: Vec<String>,
    pub gear: BTreeMap<String, GearKit>,
    pub enemies: BTreeMap<String, EnemyDef>,
    pub scenarios: BTreeMap<String, ScenarioDef>,
    pub rest: RestCfg,
    pub common_passives: Vec<Passive>,
    pub spell_index: HashMap<String, SpellId>,
    pub aura_index: HashMap<String, AuraId>,
}

impl World {
    pub fn class_index(&self, id: &str) -> Option<usize> {
        self.classes.iter().position(|c| c.id == id)
    }
    pub fn race_index(&self, id: &str) -> Option<usize> {
        self.races.iter().position(|r| r.id == id)
    }
    pub fn opt_index(&self, name: &str) -> Option<u16> {
        self.opts.iter().position(|o| o.name == name).map(|i| i as u16)
    }
    pub fn find_spell(&self, class: Option<usize>, name: &str) -> Option<SpellId> {
        if let Some(c) = class {
            if let Some(&i) = self.spell_index.get(&format!("{}.{}", self.classes[c].id, name)) {
                return Some(i);
            }
        }
        self.spell_index.get(name).copied()
    }
    pub fn find_aura(&self, class: Option<usize>, name: &str) -> Option<AuraId> {
        if let Some(c) = class {
            if let Some(&i) = self.aura_index.get(&format!("{}.{}", self.classes[c].id, name)) {
                return Some(i);
            }
        }
        self.aura_index.get(name).copied()
    }
}
