# Results on the shipped data

Forever build 1.60.1.70338, default options, standard gear kits, seed 20261104.
Every number is the centre of a 95% interval no wider than ±1% (boss),
±1.5% (solo), ±2% (hard doubles) or ±3% (PvP), except where the run hit 1,000
replications. Regenerate any column with, for example:

```sh
fsim run boss builds/*/*.toml
fsim run solo_1 builds/*/*.toml --level 20 --precision 0.015
fsim run solo_2_hard builds/*/*.toml --precision 0.02
fsim run pressure builds/*/*.toml --precision 0.03
```

| Class | Build | Boss DPS (60) | Solo kills/h (20) | (30) | (60) | Hard doubles kills/h (60) | Deaths/h | PvP breakeven (60) |
|---|---|---:|---:|---:|---:|---:|---:|---:|
| Druid | Bear (35/0/16) | 157 | 82 | 89 | 74 | 91 | 1.21 | 1269 |
| Druid | Moonclaw+ (31/9/11) | 271 | 130 | 106 | 114 | 94 | 2.66 | 577 |
| Druid | Warden-9 (26/9/16) | 252 | 130 | 120 | 112 | 104 | 0.29 | 625 |
| Hunter | BM Ranged (31/20) | 268 | 148 | 136 | 122 | 138 | 0.00 | 407 |
| Hunter | Survival Melee (20/0/31) | 290 | 167 | 149 | 141 | 134 | 0.00 | 591 |
| Mage | Battlemage (10/0/41) | 161 | 87 | 86 | 115 | 120 | 2.49 | 453 |
| Mage | Fire (10/41) | 304 | 82 | 92 | 113 | 62 | 15.82 | 311 |
| Mage | Frost (10/0/41) | 347 | 94 | 92 | 147 | 124 | 5.92 | 342 |
| Paladin | Retribution (20/0/31) | 262 | 130 | 138 | 134 | 122 | 0.04 | 1312 |
| Paladin | Shield Battery (20/31/0) | 132 | 101 | 91 | 111 | 141 | 0.00 | 990 |
| Paladin | Shockadin (31/0/20) | 207 | 116 | 105 | 111 | 113 | 0.00 | 821 |
| Priest | Plague Saint (31/20) | 179 | 70 | 80 | 90 | 77 | 2.52 | 662 |
| Priest | Shadow (20/0/31) | 443 | 69 | 78 | 147 | 159 | 0.00 | 1031 |
| Rogue | Combat Swords (20/31) | 282 | 131 | 145 | 142 | 80 | 8.76 | 673 |
| Rogue | Mutilate (41/10) | 297 | 171 | 150 | 143 | 70 | 9.89 | 515 |
| Shaman | Elemental (31/20/0) | 235 | 93 | 97 | 111 | 106 | 0.69 | 555 |
| Shaman | Enhancement (20/31/0) | 314 | 132 | 135 | 142 | 140 | 0.84 | 622 |
| Shaman | Stormwarden (0/20/31) | 218 | 125 | 109 | 125 | 138 | 0.00 | 1111 |
| Warlock | Affliction Drain (31/20) | 259 | 121 | 119 | 126 | 113 | 0.00 | 524 |
| Warlock | Destruction (0/20/31) | 283 | 101 | 95 | 111 | 76 | 11.18 | 461 |
| Warlock | Pact (20/31) | 297 | 126 | 99 | 136 | 127 | 0.78 | 568 |
| Warrior | Conventional Fury (20/31) | 276 | 118 | 120 | 124 | 73 | 8.10 | 387 |
| Warrior | Vampire Flurry-5 (38/13) | 254 | 118 | 117 | 147 | 117 | 2.09 | 660 |
| Warrior | Vampire v2 (36/15) | 245 | 117 | 117 | 143 | 113 | 2.26 | 653 |

* Solo kills/h: an hour of single same-level humanoids (kills, travel,
  eating, drinking, deaths all included). Levels 20 and 30 use each build's
  levelling order (or main tree first where the build file has none).
* Hard doubles: two mobs at once that hit 40% harder; deaths per hour shows
  which builds can afford to pull like that.
* PvP breakeven: the attacker damage per second (before your armour) at which
  you beat a geared rogue-like player half the time. Above ~1,000 it mostly
  means crowd control or immunities win the fight outright (see Limits in the
  README).

Things worth checking in game before trusting them:

* Shadow priest boss DPS (443) comes from Forever's Shadowform (-50% Shadow
  mana cost) plus Improved Mind Flay (+20%) and Darkness; it is the highest
  number here and the most sensitive to coefficients the client does not show.
* Seal of Light/Wisdom are assumed to trigger on every hit (tooltip wording);
  `--opt seal_on_hit_chance=0.2` gives the Classic behaviour.
* Summon Hawk's dive rate is a guess (`hawk_dive_interval`, 6 s).
