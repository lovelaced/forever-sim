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
    /// Which part of the Assumptions panel it sits in ("" for the class and
    /// race numbers).
    pub group: String,
    /// For `kind = "choice"`: the names of the values 0, 1, 2...
    pub choices: Vec<String>,
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
    /// Movement, in yards and yards per second.
    pub run_speed: f64,
    pub npc_run_speed: f64,
    pub melee_reach: f64,
    /// How far past a wanted distance before moving again (no jitter).
    pub move_margin: f64,
    pub charge_speed: f64,
    /// A feared unit runs at its speed times this.
    pub fear_speed: f64,
    /// How close a player can come before an unaware mob notices.
    pub aggro_radius: f64,
    /// Engine steps at one moment before time is nudged on (a stuck loop).
    pub max_spins: u32,
    /// Threat needed to pull a mob off whoever it's on: this many times
    /// theirs from melee range, and from farther away.
    pub aggro_melee: f64,
    pub aggro_ranged: f64,
    /// Threat per point of healing (split across the mobs fighting you).
    pub heal_threat: f64,
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
    /// How far the spell reaches (yards added, or a multiplier).
    Range,
    /// How wide its area is.
    Radius,
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

impl EventKind {
    /// One bit per kind of event (payloads aside): who listens to what.
    pub fn bit(&self) -> u32 {
        let k = match self {
            EventKind::MeleeSwing => 0,
            EventKind::MeleeLanded => 1,
            EventKind::MeleeCrit => 2,
            EventKind::WhiteLanded => 3,
            EventKind::SpecialLanded => 4,
            EventKind::RangedLanded => 5,
            EventKind::SpellLanded => 6,
            EventKind::SpellCrit => 7,
            EventKind::Periodic => 8,
            EventKind::Heal => 9,
            EventKind::HealCrit => 10,
            EventKind::DamageDealt => 11,
            EventKind::Struck => 12,
            EventKind::StruckMelee => 13,
            EventKind::StruckCrit => 14,
            EventKind::Dodge => 15,
            EventKind::Parry => 16,
            EventKind::Block => 17,
            EventKind::AttackDodged => 18,
            EventKind::AttackParried => 19,
            EventKind::Kill => 20,
            EventKind::Cast(_) => 21,
            EventKind::AnyCast => 22,
            EventKind::Custom(_) => 23,
            EventKind::AuraApplied(_) => 24,
            EventKind::AuraExpired(_) => 25,
            EventKind::CombatStart => 26,
            EventKind::Summon => 27,
            EventKind::PetDied => 28,
        };
        1 << k
    }
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
    /// The friendly unit the cast is on (yourself when it's on nobody else).
    Ally,
    /// Every live member of your group (yourself included, no pets).
    Party,
    /// The cast's target, then up to `max - 1` jumps, each to the nearest
    /// enemy not yet hit within `jump` yards of the last (Chain Lightning).
    Chain { max: Expr, jump: f64 },
    /// The cast's ally first, then the most hurt others, up to a maximum
    /// (Chain Heal, Wild Growth).
    Allies(Expr),
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
    /// Each further target heals for this share of the one before (Chain Heal).
    pub falloff: Option<Expr>,
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
    /// Effects run by the caster of the aura they belong to, on the aura's
    /// holder (Prayer of Mending heals with the priest's power).
    FromSource(Vec<Effect>),
    Dispel { who: TargetSel, mechanics: Vec<Mechanic> },
    Taunt { who: TargetSel, duration: Expr },
    Threat { amount: Expr },
    Absorb { amount: Expr },
    /// Rush to the target (Charge, Intercept); the effects land on arrival.
    Charge(Vec<Effect>),
    /// Track the target's kind of creature (a hunter's tracking), if its
    /// Track spell is learned by then: (kind, level learned).
    Track(Vec<(CreatureType, u32)>),
    /// Walk back from the target to this many yards (kiting).
    StepBack(Expr),
    /// Jump this many yards straight away from the target (Blink).
    Teleport(Expr),
    /// Put a trap at your feet: it arms after `arm` seconds, lasts `lasts`,
    /// and the first enemy within `radius` sets it off (`then`, on that
    /// enemy, with areas of `area` yards around the trap).
    Trap { arm: f64, radius: f64, lasts: f64, area: f64, then: Vec<Effect> },
    /// Every mob forgets your threat (Vanish), each rolling to resist it if
    /// `resist` (Feign Death, like a spell's hit roll); one left with nobody
    /// to fight runs back and heals (evades).
    DropThreat { resist: bool },
    /// Take up to `count` magic buffs off each holder, the newest first
    /// (Purge).
    Purge { who: TargetSel, count: Expr },
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
    /// A heal line on the group's tank (yourself when there's no tank).
    Tank,
    /// A heal line on the most hurt ally the line's condition accepts.
    Lowest,
    /// A line on any enemy in the fight (the first, in the pick's order, the
    /// line accepts), or any but your target; casting it doesn't change
    /// your target.
    AnyEnemy(Pick),
    OtherEnemy(Pick),
}

/// The order a multi-target line tries enemies in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Pick {
    /// Your target first, then the nearest.
    Current,
    Nearest,
    LowestHp,
    HighestHp,
    /// Those without the line's own aura from you first (spreading a DoT).
    Undotted,
}

/// Which rank of a spell a rotation line casts.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum RankPick {
    /// The highest known.
    #[default]
    Max,
    /// The smallest rank that heals the target's missing health.
    Fit,
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
    /// Yards to the target it can be cast at (0: no limit, a spell on
    /// yourself), and the closest (a bow's dead zone, Charge).
    pub range: f64,
    pub min_range: f64,
    /// Moves you (Charge, stepping back): not while rooted or frozen.
    pub moves: bool,
    /// Bursts from your totem of this element (Fire Nova), not from you.
    pub from_totem: Option<u8>,
    /// Hurts an enemy (damage or a debuff): held back while you're close to
    /// pulling a tank's mob.
    pub harm: bool,
    /// For spells that hit every enemy: only those within this many yards
    /// (0: all of them), around you, or around the target for a spell that
    /// reaches farther than its radius (Blizzard).
    pub radius: f64,
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
    pub break_on_damage: Option<Expr>,
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
    /// Lasts only while its holder is in combat (Anger Management).
    pub combat_only: bool,
    /// The events its procs listen to (`EventKind::bit`).
    pub listens: u32,
    /// Movement: a slow (0.5 = half speed; the strongest one counts) or a
    /// speed boost (0.4 = 40% faster; they add up), worked out when the aura
    /// lands (a rank's own amount).
    pub slow: Option<Expr>,
    pub speed: Option<Expr>,
    /// Hidden (Stealth, Prowl): mobs don't notice you walking up, and you
    /// don't swing until you strike.
    pub stealth: bool,
    /// Its diminishing-returns group (`DR_GROUPS`), or none.
    pub dr: Option<u8>,
    /// An absorb that only takes one school (Fire Ward), and one paid for in
    /// mana, this much per point (Mana Shield).
    pub absorb_school: Option<School>,
    pub mana_per_absorb: f64,
    /// Its holder can't run away at low health (Curse of Recklessness).
    pub no_flee: bool,
    /// Its holder looks away (Distract): hits land from behind.
    pub distracts: bool,
    /// Its holder ignores slows and roots (Blessing of Freedom).
    pub free: bool,
    /// Doesn't alert an unaware mob or its pack (Sap).
    pub quiet: bool,
    /// A totem: casting the aura puts a totem on the ground instead.
    pub totem: Option<TotemDef>,
    /// The totem's health when it depends on the rank (Stoneclaw Totem).
    pub totem_health: Option<Expr>,
    /// A proc or window the player has to notice before acting on it (the
    /// "Play like" reaction time).
    pub react: bool,
}

#[derive(Clone, Debug)]
pub struct Action {
    pub spell: SpellId,
    pub cond: Option<Expr>,
    pub target: SpellTarget,
    pub rank: RankPick,
    /// The condition reads the rank's numbers (`r.`), so it's checked after
    /// the rank is chosen.
    pub cond_rank: bool,
    /// Upkeep: the condition watches an aura the spell itself puts up (a
    /// DoT, a debuff, a short buff), which a typical player reapplies late.
    pub upkeep: bool,
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
    /// Indexes of the `actions` lines that can go during the global
    /// cooldown (off-GCD and next-swing moves).
    pub off_gcd: Vec<u16>,
    pub rest: Vec<Action>,
    /// Out-of-combat resting thresholds (eat / drink below these fractions).
    pub eat_below: f64,
    pub drink_below: f64,
    /// A play style for healing a group (the healing tests).
    pub healer: bool,
    /// Yards it fights from (-1: worked out for each character, see
    /// `Sim::find_stand`); `keep_min` backs off from closer (a bow's dead
    /// zone).
    pub stand_at: f64,
    pub keep_min: f64,
    /// Every distance at which one of its enemy lines comes into or out of
    /// range, for waking up exactly when moving changes what it can cast.
    pub ranges: Vec<f64>,
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
    /// Timed attacks on the party (the healing tests' bosses).
    pub script: Vec<ScriptHit>,
    /// Movement: yards per second (0: the usual), melee reach, and where a
    /// caster stands (0: in melee).
    pub run_speed: f64,
    pub reach: f64,
    pub stand_at: f64,
    /// Running away at low health: below this share of health, with this
    /// chance (once), for `flee_time` seconds (a range), sometimes coming
    /// back with a friend (`flee_add`, joining 1.5 sec later).
    pub flee_below: f64,
    pub flee_chance: f64,
    pub flee_time: (f64, f64),
    pub flee_add_chance: f64,
    pub flee_add: Option<String>,
    /// A party member's steady threat on the mobs (the healing tests' tank).
    pub threat_per_sec: Option<Expr>,
    /// Attack power (taken out of the weapon, so it hits as hard until
    /// something lowers it, like Demoralizing Shout), and mana (casters).
    pub ap: Option<Expr>,
    pub mana: Option<Expr>,
    /// A variant's share of its base template's health, armor and damage.
    pub hp_mult: f64,
    pub armor_mult: f64,
    pub dmg_mult: f64,
}

/// One of a boss's timed attacks: every `every` seconds from `first`.
#[derive(Clone, Debug)]
pub struct ScriptHit {
    pub name: String,
    pub icon: String,
    pub school: School,
    /// Average damage per target before armor, at the boss's level.
    pub dmg: Expr,
    pub spread: f64,
    pub every: f64,
    pub first: f64,
    pub targets: ScriptTargets,
    /// A weapon attack the tank can dodge, parry or block (else a spell
    /// that always lands).
    pub melee: bool,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ScriptTargets {
    Tank,
    All,
    /// Random party members, you and the tank included.
    Random(u32),
    /// Random party members other than the tank.
    Others(u32),
}

/// The party in a healing test, by ally template.
#[derive(Clone, Debug)]
pub struct HealingDef {
    pub boss: String,
    pub tank: String,
    /// Your group, besides you and the tank.
    pub party: Vec<String>,
    /// A second group (raids).
    pub raid: Vec<String>,
    pub duration: f64,
    /// The test fails when this many members besides you and the tank die.
    pub max_dead: u32,
    pub lo: f64,
    pub hi: f64,
    pub steps: u32,
    pub min_level: u32,
}

#[derive(Clone, Debug)]
pub enum ScenarioKind {
    /// `mix`: draw each pull's kind of mob from a mix (else always `enemy`),
    /// hitting `dmg_scale` times as hard.
    Leveling { pull: u32, minutes: f64, enemy: String, travel: f64, mix: Option<String>, dmg_scale: f64 },
    /// `tank`: a preset build (e.g. "warrior/protection") that tanks the
    /// boss at your level.
    Boss { duration: f64, enemy: String, execute_by_time: bool, tank: Option<String> },
    Duel { max_time: f64, opponent: Option<String> },
    /// Open-world PvP: you and one enemy player see each other 30 to 40
    /// yards apart and one strikes first (`you_first`), once against each
    /// of `opponents` (preset builds at your level).
    Skirmish { max_time: f64, you_first: bool, opponents: Vec<String> },
    Pressure { attacker: String, attackers: u32, lo: f64, hi: f64, steps: u32, max_time: f64, win_needs_kill: bool },
    Healing(HealingDef),
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

/// A leveling world's mobs: each pull's kind drawn by weight, and some of
/// one kind casting.
#[derive(Clone, Debug)]
pub struct Mix {
    /// (enemy template, weight), weights summing to 1.
    pub kinds: Vec<(String, f64)>,
    /// (of which kind, share, cast as which template).
    pub casters: Option<(String, f64, String)>,
}

/// The four elements a shaman's totems belong to (one of each at a time),
/// then a hunter's trap (one at a time, and what a trap leaves behind).
pub const ELEMENTS: &[&str] = &["fire", "earth", "water", "air", "trap"];

/// What reaches whom from a totem on the ground.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TotemReach {
    /// Works while you're within its radius (Mana Spring, Healing Stream).
    Owner,
    /// Works on your target while it's within its radius (Searing Totem).
    Target,
    /// Works on everything within its radius (Magma Totem, Earthbind).
    Area,
}

/// A totem: its element, health, radius, what it reaches, and whether its
/// aura is a buff on the party within the radius (Strength of Earth) or
/// pulses from the totem itself.
#[derive(Clone, Copy, Debug)]
pub struct TotemDef {
    pub element: u8,
    pub health: f64,
    pub radius: f64,
    pub reach: TotemReach,
    pub buff: bool,
}

/// Forever's diminishing-returns groups (the spell data's `diminish` bits:
/// root 1, stun 4, charm 8, incapacitate 16, fear 32, silence 64, Kidney
/// Shot 128, distract 256, random stuns 512).
pub const DR_GROUPS: &[&str] = &["root", "stun", "charm", "incap", "fear", "silence", "kidney", "distract", "random_stun"];

/// A buff the boss test's party gives you (data/base/party.toml).
#[derive(Clone, Debug)]
pub struct PartyBuff {
    pub id: String,
    pub spell: SpellId,
    /// all | mana | physical | caster | melee
    pub for_: String,
    pub option: Option<u16>,
}

#[derive(Clone, Debug)]
pub struct World {
    pub meta: WorldMeta,
    /// Different for every compiled world (copies share it): a key for
    /// things made from it, like preset builds.
    pub uid: u64,
    pub party: Vec<PartyBuff>,
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
    /// What the leveling tests meet (data/base/enemies.toml `[mixes]`).
    pub mixes: BTreeMap<String, Mix>,
    /// Party members in the healing tests.
    pub allies: BTreeMap<String, EnemyDef>,
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
