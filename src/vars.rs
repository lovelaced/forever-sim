//! Names that formulas in the data files can refer to, resolved once at load
//! time into compact handles so evaluation during the simulation is a match
//! on an enum rather than a string lookup.

macro_rules! named_enum {
    ($(#[$m:meta])* $name:ident { $($var:ident = $s:literal),* $(,)? }) => {
        $(#[$m])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
        pub enum $name { $($var),* }
        impl $name {
            pub const ALL: &'static [$name] = &[$($name::$var),*];
            pub fn name(self) -> &'static str { match self { $($name::$var => $s),* } }
            pub fn from_name(s: &str) -> Option<$name> {
                match s { $($s => Some($name::$var),)* _ => None }
            }
        }
    };
}

named_enum!(
    /// Every stat an actor has. Values are (formula base + sum of `add`
    /// modifiers) times the product of `mul` modifiers. Stats that are
    /// multipliers (damage done, haste, ...) have a base of 1.
    Stat {
        Str = "str", Agi = "agi", Sta = "sta", Int = "int", Spi = "spi",
        StrPct = "str_pct", AgiPct = "agi_pct", StaPct = "sta_pct", IntPct = "int_pct", SpiPct = "spi_pct",
        Ap = "ap", ApMult = "ap_mult", Rap = "rap", RapMult = "rap_mult",
        Sp = "sp", SpHoly = "sp_holy", SpFire = "sp_fire", SpNature = "sp_nature",
        SpFrost = "sp_frost", SpShadow = "sp_shadow", SpArcane = "sp_arcane",
        HealPower = "heal_power",
        MeleeCrit = "melee_crit", RangedCrit = "ranged_crit", SpellCrit = "spell_crit", HealCrit = "heal_crit",
        Hit = "hit", SpellHit = "spell_hit", OffhandHit = "offhand_hit",
        MeleeHaste = "melee_haste", RangedHaste = "ranged_haste", CastHaste = "cast_haste",
        Armor = "armor", ArmorMult = "armor_mult", Dodge = "dodge", Parry = "parry", Block = "block",
        BlockValue = "block_value", Defense = "defense",
        Mp5 = "mp5", CastingRegen = "casting_regen", CombatHpRegen = "combat_hp_regen", Hp5 = "hp5",
        MaxHp = "max_hp_add", MaxHpMult = "max_hp_mult", MaxMana = "max_mana_add", MaxManaMult = "max_mana_mult",
        MaxEnergy = "max_energy_add",
        DmgDone = "dmg_done", PhysDmgDone = "phys_dmg_done", RangedDmgDone = "ranged_dmg_done", SpellDmgDone = "spell_dmg_done",
        HolyDmgDone = "holy_dmg_done", FireDmgDone = "fire_dmg_done", NatureDmgDone = "nature_dmg_done",
        FrostDmgDone = "frost_dmg_done", ShadowDmgDone = "shadow_dmg_done", ArcaneDmgDone = "arcane_dmg_done",
        DmgTaken = "dmg_taken", PhysDmgTaken = "phys_dmg_taken", SpellDmgTaken = "spell_dmg_taken",
        HealDone = "heal_done", HealTaken = "heal_taken",
        CritBonusMelee = "crit_bonus_melee", CritBonusSpell = "crit_bonus_spell",
        Threat = "threat", RageGen = "rage_gen", EnergyRegen = "energy_regen",
        EnergyActive = "energy_active", OffhandMult = "offhand_mult", OffhandRage = "offhand_rage",
        ArmorPen = "armor_pen", ArmorPenPct = "armor_pen_pct", Expertise = "expertise", PushbackResist = "pushback_resist",
        RedirectToPet = "redirect_to_pet", WeaponSpeedOverride = "weapon_speed_override",
        StunDuration = "stun_duration", FearDuration = "fear_duration",
        ImmuneFear = "immune_fear", ImmuneStun = "immune_stun", ImmuneDisarm = "immune_disarm",
        ImmuneIncap = "immune_incap", ImmuneSilence = "immune_silence", ImmuneRoot = "immune_root",
        CostMult = "cost_mult", CritDmgTaken = "crit_dmg_taken",
    }
);

impl Stat {
    /// Multiplier-type stats start at 1.
    pub fn base_default(self) -> f64 {
        use Stat::*;
        match self {
            ApMult | RapMult | MeleeHaste | RangedHaste | CastHaste | ArmorMult | MaxHpMult
            | MaxManaMult | DmgDone | PhysDmgDone | RangedDmgDone | SpellDmgDone | HolyDmgDone | FireDmgDone
            | NatureDmgDone | FrostDmgDone | ShadowDmgDone | ArcaneDmgDone | DmgTaken
            | PhysDmgTaken | SpellDmgTaken | HealDone | HealTaken | Threat | RageGen
            | EnergyRegen | StunDuration | FearDuration | CostMult | CritDmgTaken | OffhandRage => 1.0,
            OffhandMult => 0.5,
            _ => 0.0,
        }
    }
    pub fn is_primary(self) -> bool {
        use Stat::*;
        matches!(self, Str | Agi | Sta | Int | Spi | StrPct | AgiPct | StaPct | IntPct | SpiPct)
    }
}
pub const N_STATS: usize = Stat::ALL.len();

named_enum!(Res { Hp = "hp", Mana = "mana", Rage = "rage", Energy = "energy" });
pub const N_RES: usize = 4;

named_enum!(School {
    Physical = "physical", Holy = "holy", Fire = "fire", Nature = "nature",
    Frost = "frost", Shadow = "shadow", Arcane = "arcane",
});

impl School {
    pub fn sp_stat(self) -> Option<Stat> {
        match self {
            School::Physical => None,
            School::Holy => Some(Stat::SpHoly),
            School::Fire => Some(Stat::SpFire),
            School::Nature => Some(Stat::SpNature),
            School::Frost => Some(Stat::SpFrost),
            School::Shadow => Some(Stat::SpShadow),
            School::Arcane => Some(Stat::SpArcane),
        }
    }
    pub fn dmg_stat(self) -> Stat {
        match self {
            School::Physical => Stat::PhysDmgDone,
            School::Holy => Stat::HolyDmgDone,
            School::Fire => Stat::FireDmgDone,
            School::Nature => Stat::NatureDmgDone,
            School::Frost => Stat::FrostDmgDone,
            School::Shadow => Stat::ShadowDmgDone,
            School::Arcane => Stat::ArcaneDmgDone,
        }
    }
}

named_enum!(Mechanic {
    Stun = "stun", Fear = "fear", Incap = "incap", Root = "root", Slow = "slow",
    Silence = "silence", Disarm = "disarm", Freeze = "freeze", Immune = "immune",
});

named_enum!(Hand { Mh = "mh", Oh = "oh", Ranged = "ranged" });

named_enum!(WeaponType {
    Sword = "sword", Axe = "axe", Mace = "mace", Dagger = "dagger", Fist = "fist",
    Polearm = "polearm", Staff = "staff", Sword2h = "sword2h", Axe2h = "axe2h", Mace2h = "mace2h",
    Bow = "bow", Gun = "gun", Crossbow = "crossbow", Thrown = "thrown", Wand = "wand", Natural = "natural",
});

impl WeaponType {
    pub fn two_handed(self) -> bool {
        matches!(self, WeaponType::Polearm | WeaponType::Staff | WeaponType::Sword2h | WeaponType::Axe2h | WeaponType::Mace2h)
    }
    pub fn family(self) -> &'static str {
        match self {
            WeaponType::Sword | WeaponType::Sword2h => "sword",
            WeaponType::Axe | WeaponType::Axe2h => "axe",
            WeaponType::Mace | WeaponType::Mace2h => "mace",
            other => other.name(),
        }
    }
}

named_enum!(CreatureType {
    Humanoid = "humanoid", Beast = "beast", Undead = "undead", Demon = "demon",
    Elemental = "elemental", Dragonkin = "dragonkin", Giant = "giant", Mechanical = "mechanical",
});

named_enum!(Who { Me = "self", Target = "target", Owner = "owner", Pet = "pet" });

// Fields available on the event that triggered a proc.
named_enum!(EvField {
    Amount = "amount", Crit = "crit", Mh = "mh", Oh = "oh", Ranged = "ranged", Special = "special",
    Periodic = "periodic", Spell = "spell", Pct = "pct", Physical = "physical", White = "white",
    Overheal = "overheal", Rage = "rage", Extra = "extra",
});

named_enum!(WField { Speed = "speed", Dps = "dps", Min = "min", Max = "max", Equipped = "equipped" });

#[derive(Clone, Debug, PartialEq)]
pub enum VarRef {
    /// Rank of a talent in the evaluating actor's class (owner's for pets).
    Talent(u16),
    /// A named number on the rank of the spell being evaluated.
    Rank(u16),
    /// A user-tunable assumption (`opt.<name>`).
    Opt(u16),
    Stat(Who, Stat),
    Res(Who, Res),
    ResMax(Who, Res),
    ResPct(Who, Res),
    BaseMana(Who),
    BaseHp(Who),
    Combo,
    Level(Who),
    Time,
    CombatTime,
    AuraUp(Who, u32),
    AuraStacks(Who, u32),
    AuraRemains(Who, u32),
    AuraCharges(Who, u32),
    /// Stack count of the aura whose formula is being evaluated.
    Stacks,
    CdReady(u32),
    CdRemains(u32),
    Known(u32),
    Mech(Who, Mechanic),
    Casting(Who),
    IsPlayer(Who),
    Boss(Who),
    Creature(Who, CreatureType),
    Alive(Who),
    Behind,
    Enemies,
    Weapon(Hand, WField),
    WeaponFamily(Hand, u16),
    HasShield,
    DualWield,
    TwoHand,
    Form(u16),
    InAnyForm,
    Race(u16),
    Class(u16),
    Event(EvField),
    EventPet(u16),
    PetActive(Option<u16>),
    GcdRemains,
    InCombat,
    Spell(u16),
}
