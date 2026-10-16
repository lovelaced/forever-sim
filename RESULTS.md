# Results on the shipped data

Forever build 1.60.1.70425 (the October 15 beta update), default options,
standard gear kits, seed 20261104. Every number is the center of a 95%
interval no wider than ±1% (boss), ±1.5% (solo), ±2% (hard doubles) or ±3%
(PvP), except where the run hit 4,000 replications. Regenerate the table with
`python3 tools/results_table.py` (`--at-build 1.60.1.70338` runs the previous
build, for builds that don't use talents added since), or any column with,
for example:

```sh
fsim run boss builds/*/*.toml
fsim run solo_1 builds/*/*.toml --level 20 --precision 0.015
fsim run solo_2_hard builds/*/*.toml --precision 0.02
fsim run pressure builds/*/*.toml --precision 0.03
```

### Standard specs

| Class | Build | Boss DPS (60) | Solo kills/h (20) | (30) | (60) | Hard doubles kills/h (60) | Deaths/h | PvP breakeven (60) |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Druid | Balance | 302 | 86 | 95 | 126 | 126 | 0.01 | 658 |
| Druid | Feral | 305 | 134 | 130 | 107 | 99 | 2.48 | 579 |
| Druid | Restoration (healer) | 222 | 81 | 83 | 106 | 105 | 0.06 | 700 |
| Hunter | Beast Mastery | 331 | 157 | 141 | 131 | 140 | 0.00 | 648 |
| Hunter | Marksmanship | 311 | 166 | 152 | 138 | 143 | 0.00 | 442 |
| Hunter | Survival | 302 | 140 | 137 | 133 | 123 | 0.00 | 625 |
| Mage | Arcane | 440 | 85 | 107 | 132 | 82 | 13.19 | 416 |
| Mage | Fire | 358 | 90 | 94 | 120 | 90 | 10.27 | 436 |
| Mage | Frost | 383 | 104 | 110 | 134 | 136 | 3.97 | 523 |
| Paladin | Holy (healer) | 203 | 113 | 109 | 104 | 93 | 0.00 | 1095 |
| Paladin | Protection (tank) | 119 | 104 | 91 | 92 | 111 | 0.00 | 2459 |
| Paladin | Retribution | 272 | 139 | 140 | 139 | 133 | 0.01 | 1181 |
| Priest | Discipline (healer) | 162 | 68 | 71 | 72 | 72 | 0.00 | 571 |
| Priest | Holy (healer) | 160 | 70 | 62 | 65 | 64 | 0.00 | 568 |
| Priest | Shadow | 449 | 90 | 84 | 165 | 181 | 0.00 | 884 |
| Rogue | Assassination | 274 | 123 | 114 | 100 | 40 | 16.77 | 476 |
| Rogue | Combat | 277 | 128 | 117 | 111 | 88 | 6.73 | 622 |
| Rogue | Subtlety | 260 | 144 | 137 | 119 | 58 | 14.28 | 565 |
| Shaman | Elemental | 222 | 93 | 101 | 111 | 102 | 1.47 | 578 |
| Shaman | Enhancement | 320 | 147 | 146 | 144 | 145 | 0.36 | 632 |
| Shaman | Restoration (healer) | 164 | 122 | 113 | 96 | 107 | 0.00 | 1172 |
| Warlock | Affliction | 344 | 135 | 128 | 134 | 128 | 0.00 | 569 |
| Warlock | Demonology | 352 | 118 | 124 | 111 | 85 | 8.58 | 652 |
| Warlock | Destruction | 269 | 149 | 102 | 129 | 122 | 0.13 | 476 |
| Warrior | Arms | 263 | 124 | 124 | 118 | 78 | 7.87 | 517 |
| Warrior | Fury | 288 | 124 | 124 | 121 | 82 | 5.89 | 492 |
| Warrior | Protection (tank) | 156 | 85 | 92 | 114 | 105 | 0.00 | 1200 |

### Forever experiments

| Class | Build | Boss DPS (60) | Solo kills/h (20) | (30) | (60) | Hard doubles kills/h (60) | Deaths/h | PvP breakeven (60) |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Druid | Bear (35/0/16) | 191 | 81 | 85 | 78 | 101 | 0.62 | 1381 |
| Druid | Moonclaw+ (31/9/11) | 299 | 131 | 114 | 115 | 103 | 1.63 | 577 |
| Druid | Warden-9 (26/9/16) | 275 | 131 | 127 | 112 | 105 | 0.72 | 656 |
| Hunter | BM Ranged (31/20) | 283 | 152 | 139 | 123 | 144 | 0.02 | 476 |
| Hunter | Survival Melee (20/0/31) | 248 | 139 | 126 | 122 | 121 | 0.00 | 557 |
| Mage | Battlemage (10/0/41) | 325 | 88 | 88 | 134 | 151 | 1.82 | 563 |
| Paladin | Shield Battery (20/31/0) | 145 | 109 | 98 | 111 | 137 | 0.00 | 1453 |
| Paladin | Shockadin (31/0/20) | 235 | 121 | 117 | 120 | 113 | 0.00 | 896 |
| Priest | Plague Saint (31/20) | 199 | 76 | 80 | 82 | 79 | 0.55 | 546 |
| Rogue | Combat Swords (20/31) | 272 | 130 | 128 | 122 | 94 | 5.72 | 646 |
| Rogue | Mutilate (41/10) | 270 | 145 | 127 | 113 | 53 | 12.94 | 494 |
| Shaman | Stormwarden (0/20/31) | 228 | 116 | 112 | 127 | 135 | 0.00 | 1109 |
| Warlock | Pact (20/31) | 307 | 148 | 98 | 141 | 124 | 0.85 | 568 |
| Warrior | Conventional Fury (20/31) | 286 | 123 | 121 | 127 | 76 | 6.93 | 417 |
| Warrior | Vampire Flurry-5 (38/13) | 258 | 123 | 120 | 146 | 115 | 2.31 | 693 |
| Warrior | Vampire v2 (36/15) | 249 | 122 | 121 | 143 | 110 | 2.63 | 681 |

* Solo kills/h: an hour of single same-level humanoids (kills, travel,
  eating, drinking, deaths all included). Levels 20 and 30 use each build's
  leveling order (or main tree first where the build file has none).
* Hard doubles: two mobs at once that hit 40% harder; deaths per hour shows
  which builds can afford to pull like that.
* PvP breakeven: the attacker damage per second (before your armor) at which
  you beat a geared rogue-like player half the time. Above ~1,000 it mostly
  means crowd control or immunities win the fight outright (see Limits in the
  README).

## The builds

There is a standard build for every talent tree (27), plus the Forever
experiments the simulator started with. Each standard build follows the
community's Forever guides (Method, Warcraft Tavern, Icy Veins, wowtbc.gg,
stillsixty and others; the sources are listed in each build file). Their
leveling orders to level 30 come from those guides; points past 30 are
theorycrafted until launch, since the beta stops at 30. Healers (Holy,
Discipline, Restoration) are shown by their solo play, which is all these
tests measure until the healing test arrives; tanks likewise.

## What moved since the previous table

**Openers and pre-fight buffs are now real casts.** A build's opening moves
used to be free and instant at every pull, and every cooldown was reset
before each fight and after each death. Now the opener is played like the
rest of the rotation (energy, mana, cooldowns, the global cooldown), Stealth
and Prowl end on the first damage you deal or take, buffs cast before the
pull start their cooldowns, and dying leaves cooldowns running.

* Rogues lose 11 to 17% leveling and 10 to 29% in hard
  doubles, where they also die more often (Assassination 12 to 17 deaths an
  hour). Ambush, Garrote and Cheap Shot used to land together for free; now
  each costs energy, and Cheap Shot can't follow Ambush because the first
  hit ends Stealth. Evasion is no longer ready again after every death.
  The Combat builds at level 20 are unchanged (no opener yet), and the boss
  drops 1 to 3%.
* Subtlety loses 15% in PvP: jumped while in Stealth, the first hit now ends
  it, so there is no free Ambush once the stun wears off.
* Cats at level 60 lose 6 to 8% leveling (Pounce costs energy), hunters 1 to
  4% (Hunter's Mark takes a global cooldown).
* Frost mages and Battlemages lose 11 to 12% in PvP: the Ice Barrier cast
  before the fight used to be ready again at once.

**Before that**, the previous table was build 70338 before the data was
checked against the game's own spell tables. Three things changed the
numbers then. (Fire,
Frost, Shadow, Affliction, Destruction, Retribution, Elemental and
Enhancement have since been rebuilt as the standard specs, so their rows
above show the new talents; the notes below describe the builds as they
were.)

**The data, checked against Forever's spell and talent tables** (every spell
rank and talent value now matches the game or has a written reason):

* Druids gain 10 to 22% on the boss: Faerie Fire was missing, Omen of
  Clarity is now on (Blizzard's October 15 notes fix a bug in it, so druids
  have it), crits give double Rage (bears), and Cat Form's abilities use the
  game's 1 sec global cooldown.
* Survival hunters lose 8 to 14%: Raptor Strike now replaces the next swing,
  and Resourcefulness never triggered before the October 15 fix. BM hunters
  gain 6%: pets inherit their owner's crit, and Auto Shot keeps firing while
  Aimed Shot casts.
* Elemental shamans lost a quarter of their boss damage (235 to 174) to a
  Mana Spring Totem that was 10 times too strong; the patch gives back about
  40% of that (198).
* Affliction warlocks gain about 18% from talent values that were too low.
* Paladins: Judgement no longer uses up the seal (Shockadin +17%). The
  weapon specializations now add only physical damage, as in the game, which
  costs Shield Battery and Retribution 4 to 6%.
* Mages: Clearcasting and Fingers of Frost now work as in the game, and
  armors chill attackers. Frost's hard doubles and PvP numbers jump because a
  frozen target is treated as unable to fight back until it takes 25% of its
  health; without movement in the simulator this is the closest stand-in for
  kiting, and it flatters Frost (see below).
* Casters now carry a wand and use it the way leveling guides describe:
  priests and mages finish mobs with it, warlocks wand after their damage
  over time spells are up, and everyone wands when out of mana. That adds 6
  to 26 kills an hour at level 20 and 10 to 29 damage per second on the boss
  for builds that run dry (Plague Saint, Destruction, Fire).
* Racials follow Forever's data: Touch of the Grave procs at most once every
  25 sec (Undead), Berserking is a flat 10% (Troll), and Endurance gives
  crit damage instead of hit (Tauren).

**Blizzard's October 15 update (build 70425)**, on top of that (70338 ->
70425 in a run with the same data):

* Fire mage +8% on the boss (Heating Up from every Fire Blast, Critical Mass
  to 10%), Frost mage +3% on the boss but -11% leveling at 60 and -13% in hard
  doubles (Ice Lance does 3x to frozen targets instead of 4x).
* Battlemage nearly doubles on the boss (184 to 325): the new Frost Blast,
  cast between its instants, stacks Winter's Chill for Cone of Cold and sets
  up Ice Lance. It keeps its instants against players.
* Elemental shaman +14% on the boss (Chain Lightning costs a third less,
  Elemental Mastery, a longer Flame Shock).
* Shadow priest +5% on the boss (Devouring Plague every 24 sec, Twin
  Disciplines on Mind Flay and Shadow Word: Death); Affliction warlock +4%
  (Shadow Mastery doubled).
* Survival hunter +8% on the boss (Resourcefulness now covers Strider Kick).
* Retribution paladin -4% (Champion of the Light halved, Consecration's
  coefficient cut).
* Mutilate rogues lose 8% in hard doubles: Touch of the Grave now waits 25
  sec between procs.

## Things worth checking in game before trusting them

* Shadow priest boss DPS (449) and hard doubles (181) are the highest numbers here. It comes from
  Forever's Shadowform (-50% Shadow mana cost), Improved Mind Flay and
  Darkness, and it is the most sensitive to coefficients the client doesn't
  show.
* Frost mage hard doubles and PvP lean on freezes standing in for kiting
  until the simulator models movement. Arcane mages hit hardest on the boss
  after Shadow (440) but die often in hard doubles (13 an hour).
* Rogues on two mobs at once have none of the tools real rogues use there
  yet (Sap, Blind, Vanish, Gouge); those arrive with crowd control, so their
  hard doubles numbers are a floor.
* Destruction warlocks level faster at 20 than at 30 (149 against 102 kills
  an hour): worth a closer look at the level 30 talents and pet.
* Protection paladins' PvP number (2,459) means the scripted attacker can't
  get through their crowd control and immunities, not an exact figure.
* Omen of Clarity's proc rates (`omen_ppm`, `omen_spell_chance`) and Summon
  Hawk's dive rate (`hawk_dive_interval`) are estimates.
* Whether Dual Wield Specialization still gives off-hand Rage
  (`warrior_dws_offhand_rage`): the talent tree says yes, Blizzard's notes
  say no. Turning it on adds about 6% to Fury warriors.
