# Results on the shipped data

Forever build 1.60.1.70425 (the October 15 beta update), default options
(playing like a typical player), standard gear kits, seed 20261104. Every number is the center of a 95%
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
fsim run heal_dungeon builds/priest/holy.toml builds/priest/discipline.toml \
  builds/paladin/holy.toml builds/druid/restoration.toml builds/shaman/restoration.toml --precision 0.015
```

### Standard specs

| Class | Build | Boss DPS (60) | Solo kills/h (20) | (30) | (60) | Hard doubles kills/h (60) | Deaths/h | PvP breakeven (60) | You strike first (60) | They strike first (60) |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Druid | Balance | 284 | 89 | 99 | 129 | 128 | 0.60 | 423 | 27% | 24% |
| Druid | Feral | 344 | 132 | 125 | 101 | 83 | 4.99 | 445 | 24% | 19% |
| Druid | Restoration (healer) | 209 | 77 | 85 | 106 | 106 | 0.82 | 551 | 33% | 35% |
| Hunter | Beast Mastery | 318 | 148 | 134 | 126 | 135 | 0.27 | 1326 | 67% | 62% |
| Hunter | Marksmanship | 293 | 157 | 145 | 129 | 122 | 0.93 | 802 | 45% | 40% |
| Hunter | Survival | 282 | 137 | 135 | 120 | 92 | 1.44 | 710 | 19% | 13% |
| Mage | Arcane | 376 | 89 | 102 | 134 | 81 | 9.54 | 373 | 34% | 21% |
| Mage | Fire | 349 | 83 | 91 | 108 | 77 | 7.33 | 424 | 38% | 21% |
| Mage | Frost | 372 | 106 | 112 | 129 | 155 | 0.18 | 896 | 52% | 42% |
| Paladin | Holy (healer) | 224 | 104 | 100 | 97 | 92 | 0.07 | 910 | 56% | 52% |
| Paladin | Protection (tank) | 164 | 96 | 83 | 87 | 92 | 0.21 | 1868 | 68% | 67% |
| Paladin | Retribution | 344 | 128 | 129 | 133 | 134 | 0.19 | 916 | 73% | 66% |
| Priest | Discipline (healer) | 157 | 73 | 80 | 75 | 70 | 1.83 | 544 | 54% | 48% |
| Priest | Holy (healer) | 165 | 73 | 67 | 68 | 62 | 2.85 | 525 | 56% | 48% |
| Priest | Shadow | 421 | 95 | 86 | 166 | 192 | 0.00 | 1311 | 93% | 88% |
| Rogue | Assassination | 316 | 111 | 104 | 91 | 40 | 11.88 | 454 | 21% | 13% |
| Rogue | Combat | 325 | 117 | 107 | 101 | 70 | 4.40 | 594 | 48% | 36% |
| Rogue | Subtlety | 295 | 135 | 131 | 113 | 62 | 9.40 | 496 | 20% | 14% |
| Shaman | Elemental | 216 | 91 | 96 | 103 | 92 | 4.73 | 471 | 31% | 28% |
| Shaman | Enhancement | 358 | 137 | 132 | 123 | 116 | 5.26 | 489 | 54% | 45% |
| Shaman | Restoration (healer) | 188 | 120 | 107 | 90 | 104 | 0.00 | 1020 | 84% | 79% |
| Warlock | Affliction | 336 | 133 | 126 | 127 | 124 | 0.25 | 452 | 59% | 45% |
| Warlock | Demonology | 354 | 123 | 141 | 118 | 124 | 0.15 | 386 | 59% | 56% |
| Warlock | Destruction | 269 | 143 | 101 | 119 | 117 | 0.13 | 308 | 47% | 30% |
| Warrior | Arms | 334 | 117 | 115 | 106 | 66 | 8.65 | 429 | 17% | 15% |
| Warrior | Fury | 345 | 115 | 116 | 109 | 72 | 7.37 | 428 | 24% | 16% |
| Warrior | Protection (tank) | 163 | 85 | 86 | 103 | 93 | 0.15 | 1062 | 47% | 45% |

### Forever experiments

| Class | Build | Boss DPS (60) | Solo kills/h (20) | (30) | (60) | Hard doubles kills/h (60) | Deaths/h | PvP breakeven (60) | You strike first (60) | They strike first (60) |
|---|---|---:|---:|---:|---:|---:|---:|---:|---:|---:|
| Druid | Bear (35/0/16) | 205 | 78 | 82 | 76 | 87 | 2.51 | 936 | 34% | 32% |
| Druid | Moonclaw+ (31/9/11) | 331 | 128 | 106 | 106 | 88 | 3.68 | 494 | 24% | 20% |
| Druid | Warden-9 (26/9/16) | 310 | 128 | 121 | 103 | 88 | 3.48 | 530 | 26% | 19% |
| Hunter | BM Ranged (31/20) | 290 | 145 | 133 | 122 | 138 | 0.06 | 1187 | 59% | 56% |
| Hunter | Survival Melee (20/0/31) | 237 | 135 | 125 | 122 | 101 | 0.78 | 863 | 31% | 27% |
| Mage | Battlemage (10/0/41) | 319 | 79 | 79 | 125 | 102 | 8.06 | 452 | 62% | 49% |
| Paladin | Shield Battery (20/31/0) | 194 | 98 | 89 | 98 | 117 | 0.01 | 1171 | 65% | 57% |
| Paladin | Shockadin (31/0/20) | 276 | 119 | 115 | 118 | 116 | 0.03 | 711 | 61% | 47% |
| Priest | Plague Saint (31/20) | 193 | 79 | 86 | 87 | 79 | 2.77 | 504 | 56% | 53% |
| Rogue | Combat Swords (20/31) | 322 | 124 | 118 | 111 | 76 | 3.96 | 600 | 49% | 36% |
| Rogue | Mutilate (41/10) | 312 | 134 | 117 | 102 | 49 | 9.60 | 470 | 24% | 15% |
| Shaman | Stormwarden (0/20/31) | 278 | 108 | 103 | 112 | 126 | 0.37 | 987 | 86% | 86% |
| Warlock | Pact (20/31) | 309 | 117 | 119 | 138 | 135 | 0.18 | 403 | 78% | 69% |
| Warrior | Conventional Fury (20/31) | 343 | 115 | 113 | 115 | 68 | 8.39 | 370 | 16% | 8% |
| Warrior | Vampire Flurry-5 (38/13) | 312 | 115 | 112 | 133 | 111 | 2.26 | 617 | 38% | 32% |
| Warrior | Vampire v2 (36/15) | 302 | 115 | 113 | 130 | 107 | 2.58 | 601 | 36% | 28% |

* Solo kills/h: an hour of single same-level mobs from the open world's mix
  of creatures (kills, walking up, travel, eating, drinking, deaths all
  included). Levels 20 and 30 use each build's leveling order (or main tree
  first where the build file has none).
* Hard doubles: two mobs at once that hit 40% harder; deaths per hour shows
  which builds can afford to pull like that.
* PvP breakeven: the attacker damage per second (before your armor) at which
  you beat a geared rogue-like player half the time. Above ~1,000 it mostly
  means crowd control or immunities win the fight outright (see Limits in the
  README).
* You strike first, they strike first: open-world PvP at your level against
  one build of each class (its main leveling spec), starting 30 to 40 yards
  apart; a win counts 1, a draw or a two-minute stalemate half. The side
  striking first moves at once, the other a reaction after its first hostile
  move.
* Gear: the Enhancement shaman, Retribution paladin and Shockadin wear the
  new hybrid kits (melee gear with spell damage and healing); Moonclaw and
  Warden-9 stay in Agility leather, which suits their cat play better, and
  Stormwarden keeps its one-hander and shield.

### Healers in the healing tests

Boss damage per second, before armor, that the healer keeps the party alive
through half the time (±1.5%). The bosses' damage is set so the Holy priest
breaks even near the same mark at every level, so read the numbers against
each other: 10% higher means heals that hold through a boss hitting 10%
harder.

| Class | Build | Dungeon (30) | Dungeon (60) | Main tank (30) | Main tank (60) | Raid (60) |
|---|---|---:|---:|---:|---:|---:|
| Druid | Restoration | 151 | 677 | 130 | 710 | 438 |
| Paladin | Holy | 169 | 703 | 111 | 581 | 538 |
| Priest | Discipline | 153 | 681 | 126 | 642 | 545 |
| Priest | Holy | 128 | 644 | 97 | 552 | 532 |
| Shaman | Restoration | 153 | 828 | 100 | 619 | 595 |

## The builds

There is a standard build for every talent tree (27), plus the Forever
experiments the simulator started with. Each standard build follows the
community's Forever guides (Method, Warcraft Tavern, Icy Veins, wowtbc.gg,
stillsixty and others; the sources are listed in each build file). Their
leveling orders to level 30 come from those guides; points past 30 are
theorycrafted until launch, since the beta stops at 30. Healers (Holy,
Discipline, Restoration) play their solo style in the tests above and heal a
group in the healing tests below; tanks are shown by their solo play.

## What moved and why

This release makes the fights behave like the game: everyone moves, threat
decides who mobs attack, the boss test has a real tank, leveling meets every
kind of creature, every class uses its crowd control, totems and traps are
units on the ground, duels start fair, and the simulated player plays like
a typical player instead of a perfect one. Each change was measured on its
own, in this order; the list under each one is the numbers it moved most.
The duel isn't in the tables above, so the fair-duel change shows nothing
here; the open-world PvP columns start at their own step.

**Rage drains out of combat, Anger Management stops between pulls, gear below level 20 scales down, buffs before a fight cost time and mana (and come back after a death), Execute and Slam are used the way players use them.** 130 numbers moved beyond their ranges; the biggest:
The Pact warlock and the two paladin hybrids fell this far because of a bug in this step (they recast their buffs before every pull, over and over), not because of the fixes; it was fixed with the crowd control step, where they come back.

* Pact (20/31) (warlock), (60): 141 to 24 (-83%)
* Pact (20/31) (warlock), (30): 109 to 29 (-73%)
* Pact (20/31) (warlock), Hard doubles kills/h (60): 109 to 50 (-54%)
* Shockadin (31/0/20) (paladin), (30): 117 to 62 (-47%)
* Shockadin (31/0/20) (paladin), (60): 120 to 65 (-46%)
* Shield Battery (20/31/0) (paladin), (60): 111 to 62 (-44%)

**The simulated player reacts like a typical player: about half a second to notice a proc, an interrupt chance or a new target, and the odd slip.** 194 numbers moved beyond their ranges; the biggest:

* Demonology (warlock), PvP breakeven (60): 565 to 382 (-32%)
* Destruction (warlock), PvP breakeven (60): 412 to 280 (-32%)
* Retribution (paladin), PvP breakeven (60): 1154 to 876 (-24%)
* Pact (20/31) (warlock), PvP breakeven (60): 362 to 279 (-23%)
* Arcane (mage), Hard doubles kills/h (60): 55 to 43 (-22%)
* Bear (35/0/16) (druid), PvP breakeven (60): 1191 to 971 (-18%)

**Everyone moves: packs start unaware 30 to 35 yards away, casters get casts off before contact, melee walks in, Charge rushes, Frost mages kite, hunters back out of their dead zone, humanoids run at low health.** 185 numbers moved beyond their ranges; the biggest:

* Demonology (warlock), Hard doubles kills/h (60): 59 to 137 (+130%)
* Beast Mastery (hunter), PvP breakeven (60): 523 to 917 (+75%)
* BM Ranged (31/20) (hunter), PvP breakeven (60): 498 to 837 (+68%)
* Frost (mage), Hard doubles kills/h (60): 97 to 143 (+47%)
* Shadow (priest), PvP breakeven (60): 725 to 1025 (+41%)
* Marksmanship (hunter), PvP breakeven (60): 375 to 510 (+36%)

**Threat decides who mobs attack, the boss test gets a real Protection warrior tank and the party's buffs, and the boss hits back.** 68 numbers moved beyond their ranges; the biggest:

* Marksmanship (hunter), Hard doubles kills/h (60): 132 to 72 (-45%)
* Protection (paladin), Boss DPS (60): 121 to 165 (+36%)
* Survival (hunter), Hard doubles kills/h (60): 114 to 77 (-32%)
* Arms (warrior), Boss DPS (60): 254 to 335 (+32%)
* Shield Battery (20/31/0) (paladin), Boss DPS (60): 148 to 195 (+32%)
* Demonology (warlock), Hard doubles kills/h (60): 137 to 97 (-29%)

**Leveling meets the open world's mix of creatures, some humanoids cast, and creature bonuses follow the target.** 160 numbers moved beyond their ranges; the biggest:

* Holy (priest), Hard doubles kills/h (60): 53 to 62 (+17%)
* Discipline (priest), Hard doubles kills/h (60): 63 to 70 (+11%)
* Conventional Fury (20/31) (warrior), Hard doubles kills/h (60): 64 to 70 (+9%)
* Battlemage (10/0/41) (mage), Solo kills/h (20): 72 to 79 (+9%)
* Plague Saint (31/20) (priest), Hard doubles kills/h (60): 72 to 78 (+9%)
* Subtlety (rogue), Hard doubles kills/h (60): 54 to 59 (+9%)

**Every class gets its crowd control, interrupts and escapes, and uses them on the add, on casters and when things go wrong; DoTs spread to a second mob and Chain Lightning jumps.** 121 numbers moved beyond their ranges; the biggest:

* Pact (20/31) (warlock), (60): 25 to 138 (+442%)
* Pact (20/31) (warlock), (30): 30 to 120 (+296%)
* Pact (20/31) (warlock), Hard doubles kills/h (60): 51 to 136 (+165%)
* Frost (mage), PvP breakeven (60): 417 to 875 (+110%)
* Survival (hunter), PvP breakeven (60): 427 to 712 (+67%)
* Fire (mage), Hard doubles kills/h (60): 49 to 80 (+63%)

**Totems are units on the ground: a shaman puts them down where it stands, their buffs reach only the party nearby, and Searing Totem only hits a target within 20 yards; hunters freeze an add with a trap before the pull.** 20 numbers moved beyond their ranges; the biggest:

* Enhancement (shaman), Hard doubles kills/h (60): 135 to 105 (-22%)
* Enhancement (shaman), (60): 137 to 121 (-11%)
* Elemental (shaman), Hard doubles kills/h (60): 102 to 92 (-9%)
* Restoration (shaman), Solo kills/h (20): 131 to 120 (-9%)
* Restoration (shaman), (30): 116 to 107 (-8%)
* Elemental (shaman), (30): 103 to 96 (-8%)

**Duels start fair: on the same tick, a few yards apart, whoever reacts first striking first.** Nothing moved beyond its range.


**Talents that lengthen a spell's range, widen an area or make you faster now count, shamans Purge players' buffs, and Improved Reincarnation brings you back stronger.** 36 numbers moved beyond their ranges; the biggest:

* Shadow (priest), PvP breakeven (60): 1024 to 1313 (+28%)
* Fire (mage), Hard doubles kills/h (60): 80 to 72 (-10%)
* Frost (mage), Hard doubles kills/h (60): 144 to 155 (+8%)
* Frost (mage), (60): 122 to 129 (+6%)
* Fire (mage), Solo kills/h (20): 85 to 81 (-5%)
* Arcane (mage), Hard doubles kills/h (60): 68 to 71 (+4%)

**Casters and hybrids step out of a rooted or frozen enemy's reach before casting, roots break on damage, and two open-world PvP tests arrive: you strike first, and they strike first.** 43 numbers moved beyond their ranges; the biggest:

* Marksmanship (hunter), PvP breakeven (60): 584 to 805 (+38%)
* Fire (mage), PvP breakeven (60): 326 to 427 (+31%)
* Arcane (mage), PvP breakeven (60): 306 to 372 (+22%)
* Moonclaw+ (31/9/11) (druid), PvP breakeven (60): 417 to 498 (+19%)
* Beast Mastery (hunter), PvP breakeven (60): 1117 to 1321 (+18%)
* Feral (druid), PvP breakeven (60): 391 to 448 (+15%)

**Four hybrid gear kits from Forever's own items (melee gear with spell damage and healing), and the Enhancement shaman, Retribution paladin and Shockadin wear theirs.** 29 numbers moved beyond their ranges; the biggest:

* Shockadin (31/0/20) (paladin), You strike first (60): 55% to 61% (+12%)
* Retribution (paladin), Hard doubles kills/h (60): 122 to 134 (+10%)
* Enhancement (shaman), Hard doubles kills/h (60): 106 to 116 (+10%)
* Shockadin (31/0/20) (paladin), Boss DPS (60): 258 to 276 (+7%)
* Shockadin (31/0/20) (paladin), (30): 108 to 115 (+6%)
* Holy (paladin), They strike first (60): 55% to 52% (-6%)

## Things worth checking in game before trusting them

* Shadow priests top the boss (421), hard doubles (192) and PvP (1,311 in
  the ambush; 93% striking first in the open world, 88% struck first). It comes from Forever's Shadowform (-50% Shadow mana cost),
  Improved Mind Flay and Darkness, and Shadow Reach now lets them open from
  farther away; it's also the most sensitive to coefficients the client
  doesn't show.
* Frost mages kite for real now: Frost Nova once a mob is on them, a step
  back, then shatter. That gives the best hard doubles among casters (155)
  with almost no deaths. Arcane (9.5 deaths an hour) and Fire (7.3) still
  die often on doubles.
* Assassination rogues manage 40 kills an hour on hard doubles with 11.9
  deaths, even with Sap, Blind and Vanish in their rotations. Worth checking
  against how rogues handle two mobs in practice.
* Arms warriors lose every duel to a Beast Mastery hunter at 40, and win
  only 17% of open-world fights even when striking first: kiting, slows and
  pets keep them from landing hits. Their rotation gets a full pass next;
  until then, read warrior PvP as a floor.
* Enhancement shamans now pay for their totems every pull (Strength of
  Earth, Grace of Air and Searing Totem cost about 760 mana at 60), which
  shows as 5.3 deaths an hour on hard doubles (with the new spell power
  kit). A shaman who skips buff totems on trash will do better.
* Destruction warlocks level faster at 20 than at 30 (143 against 101 kills
  an hour): worth a closer look at the level 30 talents and pet.
* Protection paladins' PvP number (1,868) and Beast Mastery hunters' (1,326)
  mean the scripted attacker can't get through their crowd control, pet or
  immunities, not an exact figure.
* Omen of Clarity's proc rates (`omen_ppm`, `omen_spell_chance`), Summon
  Hawk's dive rate (`hawk_dive_interval`), Stoneclaw Totem's taunt period and
  how often totems are still up for the next pull (`totem_carry`) are
  estimates.
* Whether Dual Wield Specialization still gives off-hand Rage
  (`warrior_dws_offhand_rage`): the talent tree says yes, Blizzard's notes
  say no. Turning it on adds about 6% to Fury warriors.
