# fsim: a Monte Carlo combat simulator for WoW: Forever

`fsim` simulates characters of all nine classes in four kinds of situation
(leveling, a dungeon boss, world-PvP pressure, duels) and reports every
result with a confidence interval. Every class, talent, spell, race, gear kit,
enemy and scenario lives in plain TOML files under `data/`. When a new client
build changes something, you add one small patch file; no Rust is touched.

Data: Forever client build **1.60.1.70425** (Blizzard's October 15 beta
update, as a patch over 1.60.1.70338). Spell ranks, costs, cast times,
cooldowns, durations, coefficients, target caps, proc chances, talent trees
and racials come from the game's own spell and talent tables and are checked
by `fsim audit` (see "Where the numbers come from"). Base stats,
attack tables and armor are Classic 1.12, because the client does not include
them; every number that is a guess is an assumption you can change.

## Quick start

```sh
cargo build --release                      # stable Rust; builds target/release/fsim
alias fsim=./target/release/fsim

fsim check builds/*/*.toml                 # load all data, validate every build
fsim run boss builds/warrior/vampire_v2.toml builds/warrior/conventional_fury.toml
fsim run solo_2_hard builds/druid/*.toml --level 30
fsim run pressure builds/paladin/shockadin.toml
fsim run duel builds/shaman/stormwarden.toml --vs builds/rogue/combat_swords.toml
fsim scan solo_1 builds/druid/warden9.toml --levels 20,25,30,40,60
fsim sheet builds/mage/frost.toml --level 30        # character sheet
fsim log boss builds/rogue/mutilate.toml --lines 80 # one fight, blow by blow
fsim whatif boss builds/warrior/vampire_v2.toml --with data/whatif/example_hotfix.toml
fsim show classes.priest.talents.penance            # merged data after patches
fsim list classes | races | talents <class> | spells <class> | rotations | scenarios | gear | enemies | options
fsim serve                                         # the website, at http://localhost:8080
```

Useful flags on `run`, `scan` and `whatif`: `--level N`, `--opt name=value`
(change an assumption, see `fsim list options`), `--precision 0.005`,
`--max-reps`, `--seed`, `--threads`, `--json out.json`, `--all` (every metric
and every paired difference). Global flags: `--data DIR`, `--patch FILE`
(extra overlay), `--at-build 1.60.1.70338`, `--no-patches`.

## The website (for people who don't use a terminal)

`web/` is a static site that runs the same engine in the visitor's browser.
Set up one to three builds (start from one in `builds/` or spend talent
points in WoW-style talent trees), press Simulate, and every build plays
through every test. Healers (the healer builds, and any build marked to heal
in the editor) also take three healing tests: a dungeon, a main tank and, at
level 60, a raid. Each test is a row with a scene from the world (Westfall,
Fargodeep Mine, Duskwood, the Westfall coast, the Deadmines, Stranglethorn
Vale, Hillsbrad, Gurubashi Arena) where the main build's hero stands, and
every build's result beside it. A row opens into four tabs: a summary, how
each build played (its priority list as a spellbook page), one fight step by
step (every move with the game's icon, and everyone's health above it), and
every number. The main build wears a gold dragon ring: the others are
measured against it, its fights are shown first, and the header shows its
side of the world. After you change a build and simulate again, the page
says what the change did in each test, measured on the same fights as
before. Class rankings (the inn's notice board) and Duels (the arena) run
every build on the site. "Share these builds" copies a link.

Each run is split between Web Workers, one per spare core: the heaviest
test (three builds, "Precise", two tough mobs) takes about 6 s on an
18-core laptop against 58 s on a single worker. The engine checks whether a
run is precise enough at fixed fight counts (32, then every 16), so the
result is the same on a phone with eight cores, a desktop with eighteen, or
the local server.

**Deploying (GitHub Pages).** Push to `main` with Settings > Pages > Source set
to "GitHub Actions". `.github/workflows/pages.yml` then runs the tests,
compiles the library to WebAssembly (`wasm32-unknown-unknown`, no
wasm-bindgen), checks it with `tools/wasm_smoke.mjs`, regenerates the talent
icons for the current data, and publishes `web/` plus `fsim.wasm`. Data and
example builds are compiled into the `.wasm`, so a patch file in
`data/patches/` reaches the website on the next push.

**Running it locally.** `fsim serve` serves `web/` at http://localhost:8080 and
answers the page's requests natively (the page uses `fsim.wasm` when present
and falls back to this server otherwise):

```sh
cargo run --release -- serve            # then open http://localhost:8080
```

How the pieces fit:

| File | Role |
|---|---|
| `src/api.rs` | JSON requests (`catalog`, `sheet`, `start`, `step`, `run`, `add`, `result`, `pair`, `drop`); `fsim api '<json>'` answers one from the shell |
| `src/ffi.rs` | four C-ABI exports the browser calls (`fsim_alloc`, `fsim_call`, `fsim_out_ptr`, `fsim_out_len`) |
| `web/worker.js`, `web/engine.js` | compile `fsim.wasm` once, run it in a pool of Web Workers that each simulate a range of fights (`run`) while one collects them in order (`add`); fall back to `POST /api` |
| `web/app.js` | boot, the run bar, sharing |
| `web/state.js` | page state, build helpers, saving (builds and the last results are kept in the browser) |
| `web/runner.js` | runs the scorecard and keeps each test's results; "what your change did" pairs a build's per-fight results with its previous version's |
| `web/cards.js`, `editor.js`, `scorecard.js`, `results.js`, `played.js` | the build cards, the build editor, the tests with their scenes, one test in detail, its priority list and fight log |
| `web/rankings.js` | Class rankings and Duels |
| `web/css/` | `base.css` (colours, type, the pixel-art frames and buttons), then one file each for the Compare view, the editor and the two big views |
| `web/art/` | the pixel art (frames, buttons, scenes, heroes, badges), made with `tools/art.mjs` |
| `web/talents.js` | talent trees; the rules mirror `build.rs` (the engine re-validates everything) |
| `web/meta.js` | every label, icon rule and plain-English phrase, in one place |
| `tools/fetch_icons.mjs` | downloads the game icons the site uses into `web/icons/` |
| `tools/build_icons.mjs` | draws a stand-in for every icon (used when a game icon is missing) into `web/icons.js` |

**Icons.** Talents, classes, races and spells carry the game's icon name in
the data (`icon = "ability_ghoulfrenzy"` in a talent's table; a patch file
can set it for a new talent). Spell icons come from the game's own tables
(`SpellMisc` and `ManifestInterfaceData`, fetched by `tools/forever_db.mjs`
into the spellbook): `python3 tools/spell_icons.py` writes them into the
class files, and `fsim audit` checks each one against the game. Auras use
their spell's or talent's icon; the trace request gives every event of a
fight its icon (auto attacks show the weapon), leaves out bookkeeping auras
marked `hidden = true` (proc windows, internal cooldowns) and numbers the
enemies. `tools/fetch_icons.mjs` copies those icons, as the game's
own 64 px textures, from a sparse checkout of
[Gethe/wow-ui-textures](https://github.com/Gethe/wow-ui-textures) into
`web/icons/` with a `manifest.json`; anything not found there comes from
Wowhead's or Blizzard's image servers. The deploy does this on every push. To
see the icons locally:

```sh
fsim api '{"op":"catalog"}' > catalog.json
git clone --depth 1 --filter=blob:none --sparse --no-checkout https://github.com/Gethe/wow-ui-textures .textures
node tools/fetch_icons.mjs catalog.json --textures .textures
```

Icons that are missing are drawn instead, from
[game-icons.net](https://game-icons.net) (CC BY 3.0). The game icons are
Blizzard Entertainment's artwork, used as fan sites commonly do; the credits
say so.

**Art.** The frames, buttons, scenes, heroes and badges are original fan
art. `art/pieces.json` lists every piece with its prompt and the words its
kind shares (real WoW places, classes and armor; no logos, text or named
characters), plus one hero per race and class the data allows.
`node tools/art.mjs gen [ids]` draws them with the
[codex CLI](https://github.com/openai/codex)'s image tool into `art/raw/`
(not committed), and `node tools/art.mjs fix` cleans each into true pixel
art with [pixfix](https://github.com/lovelaced/pixfix): one colour per grid
cell, a cut-out background, cropped and saved as a palette PNG in
`web/art/`. Scenes use one fixed pixel size; heroes are scaled so every race
stands at its height on one scale. The page draws one art pixel as a whole
number of screen pixels (`--art`, set from the device's pixel ratio), so the
art stays crisp.

**Type.** Ortica (Benedetta Bovani, Collletttivo) for display and Uncut Sans
(Kasper Nordkvist) for everything else, both under the SIL Open Font License
(copies in `web/fonts/`), found through [uncut.wtf](https://uncut.wtf).

## How the Monte Carlo works

* **Replications.** One replication is one complete hour of leveling, one
  boss fight, one duel, or one PvP pressure test. Results are averaged over
  replications, never over the fights inside one, so every replication is an
  independent draw.
* **Random numbers.** xoshiro256++ seeded through SplitMix64. Replication
  `r` of scenario `s` gets seed `hash(master seed, s, r)`, and inside a
  replication every unit has its own streams (attack table, procs, misc)
  keyed by its slot. Nothing depends on thread scheduling.
* **Common random numbers.** Every build in one `run` is simulated on the
  same replication seeds, so differences between builds are measured on the
  same dice. The paired difference report shows the gain ("CRN variance
  reduction x1.8" means the comparison needed 1.8x fewer replications than
  independent runs would).
* **Sequential stopping.** Replications run in batches (64 by default) until
  the primary metric's 95% interval is within ±1% for every build (or
  `--max-reps` is reached; the header says which). Means use Student-t
  intervals, rates (win, death in a fight) use Wilson intervals, and the PvP
  breakeven uses a distribution-free interval for the median.
* **Determinism.** Same data, seed and flags give byte-identical output on
  any machine and with any thread count. There is a test for it.
* **Breakeven.** In `pressure`, each replication finds, by bisection on the
  same seed, the attacker damage multiplier at which you stop winning, then
  converts it to the attacker's damage per second before your mitigation.
  The reported number is the median over replications.

`cargo test --release` runs unit tests (expression language, RNG moments,
interval coverage on skewed data and rare events) and integration tests
(every build in every scenario at levels 20, 40 and 60; legal talent orders;
thread-count independence; CRN variance reduction; patches change only what
they say).

## Scenarios

| id | what it is | primary metric |
|---|---|---|
| `solo_1`, `solo_2`, `solo_3` | an hour of same-level humanoids, 1/2/3 per pull, 8 s travel; eat/drink/rest between pulls, death = 2 min corpse run | kills per hour |
| `solo_2_hard` | double pulls of mobs hitting 40% harder | kills per hour (also deaths per hour) |
| `boss` | 120 s on a +2 level boss from behind, tank holds threat, boss immune to crowd control | DPS |
| `pressure`, `pressure_2v1` | a geared rogue-like player opens with a 4 s stun and stuns again at 10 s; PvP diminishing returns apply | breakeven attacker DPS |
| `duel` | stand-and-trade against another build (`--vs`) | win rate |
| `heal_dungeon` | 120 s healing a tank and three others against a +2 boss: melee and Mortal Strike on the tank, Fire Nova on everyone, Shadow Bolts on the others; fails if anyone dies | breakeven boss DPS (before armor) |
| `heal_tank` | 180 s keeping one tank alive against a +3 boss with crushing blows: melee, Mortal Strike and Flame Breath | breakeven boss DPS |
| `heal_raid` | level 60 only: 150 s, ten players in two groups against Onyxia: melee on the tank, Fireballs on two at random, Eruption on everyone; fails if the tank or you die, or three others do | breakeven boss DPS |

In the healing tests the healer plays its class's healing style (the `heal`
rotation in the class file, `healer = true`): heals go on the tank or on
whoever is most hurt (`target = "tank"` or `"lowest"` on a line), and
`rank = "fit"` casts the smallest rank that covers what they're missing. The
party members only take hits and heals. The breakeven is found for each
replication by bisecting the boss's damage, as in `pressure`, and the other
numbers come from the hardest fight the healer held. Only the healer builds
(and builds with a `heal_rotation`) take them: `fsim run heal_dungeon
builds/priest/holy.toml`.

Scenarios, enemies and gear kits are data too (`data/base/scenarios.toml`,
`enemies.toml`, `gear.toml`); add your own the same way.

## Where the numbers come from

* **Forever's spell tables.** `tools/forever_db.mjs` fetches the client's
  spell tables for a build from [wago.tools](https://wago.tools) (a cache under
  `.db2/`, never committed) and writes `data/reference/spellbook.json`: every
  class spell, racial, talent spell and the spells they trigger, with each rank's id,
  level, cost, cast time, cooldowns, duration, range, radius, target cap, chain
  targets, base value with its variance and per-level growth, spell and attack
  power coefficients, periodic interval, proc chance and summons.
  `python3 tools/spell.py "Name" [class|race]` looks spells up (`--text` adds
  the tooltip from the cache).
* **The audit.** `fsim audit [class]` works out every spell rank's numbers with
  the engine's own formulas and compares them with the spellbook. Differences
  that are deliberate (a helper spell that only exists in the simulator, a
  simplification waiting for engine work) are listed with a reason in
  `data/reference/accepted/<class>.toml`; `fsim audit --strict` fails on
  anything else. `tools/apply_spellbook.py` fills rank tables from the
  spellbook where a spell uses the standard fields.
* **Assumptions.** Numbers the game does not publish are options in
  `data/base/options.toml`, each with where its default comes from (`game`,
  `community`, `classic` or `estimate`) and a citation. The website lists them
  under Assumptions with that source.

## The data

```
data/base/core.toml          combat rules: attack tables, armor, rage, GCD, DR, rest food
data/base/options.toml       assumptions you can change with --opt
data/base/common.toml        shared auras (stun, fear, ...) and potions
data/base/races.toml         ten races with Forever racials
data/base/gear.toml          standard gear kits at levels 20/40/60 (interpolated)
data/base/enemies.toml       enemy templates
data/base/scenarios.toml     scenarios
data/base/classes/<class>.toml   one file per class
data/patches/*.toml          overlays per client build (see data/patches/README.md)
data/whatif/*.toml           overlays you test with --patch or `fsim whatif`
builds/<class>/*.toml        character builds
```

### A class file

```toml
name = "Warrior"
resources = ["rage"]
can_parry = true
base = [ { level = 1, str = 23, ... hp = 20, mana = 0 }, { level = 60, ... } ]

[formulas]                       # Classic stat formulas; any expression
ap = "str * 2 + level * 3 - 20"

[talents.cruelty]
tree = "Fury"
points = 0                       # points needed in the tree above its row
ranks = 5
text = "+5% melee crit."
mods = [{ stat = "melee_crit", add = "rank" }]       # passive stat change

[talents.flurry]
tree = "Fury"
points = 25
ranks = 5
requires = "death_wish"          # prerequisite at full rank
procs = [{ on = "melee_crit", effects = [{ type = "aura", aura = "flurry" }] }]

[auras.flurry]
duration = 15
charges = 3
mods = [{ stat = "melee_haste", mul = "1 + 0.05 * t.flurry" }]
procs = [{ on = "melee_swing", effects = [{ type = "consume" }] }]

[spells.bloodthirst]
talent = "bloodthirst"
cost = { rage = 30 }
cooldown = 6
ranks = [{ level = 40, bonus = 30 }, { level = 60, bonus = 48 }]   # r.bonus = current rank
effects = [{ type = "damage", school = "physical", amount = "0.45 * ap + r.bonus" }]

[rotations.fury_dw]              # a priority list; first usable line wins
precombat = ["berserker_stance", "battle_shout"]   # before each pull: free, cooldowns start
opener = ["charge"]              # played first, in order, each at most once
actions = [
  { cast = "execute", if = "target.hp_pct < 0.2" },
  { cast = "bloodthirst" },
  { cast = "heroic_strike", if = "rage >= 60" },
]
rest = [{ cast = "cannibalize", if = "hp_pct < 0.75" }]   # between pulls
```

Each rank can carry the game's spell `id` (so `fsim audit` can trace it) and,
for any field `x`, an `x_per_level` growth with the rank's `max_level`: the
game raises a rank's numbers with the caster's level until the next rank, and
the engine does the same. For example Lightning Bolt rank 10 is
`{ level = 56, id = 15208, lo = 185, lo_per_level = 1.135, hi = 207,
hi_per_level = 1.265, max_level = 61, ... }`: 185-207 when learned, about
190-212 at level 60.


A talent listed with no `mods`, `spell_mods` or `procs`, and not used by any
spell or formula, is legal in builds but has no effect; `fsim check` reports
these per build ("points in talents the sim does not model").

**Formulas** are strings (or plain numbers) with `+ - * /`, comparisons,
`&& || !` (or `and or not`), `a ? b : c`, and `min max clamp abs floor ceil
round pow sqrt if interp(x, x0,y0, x1,y1, ...)`. Names: any stat (`ap`,
`str`, `melee_crit`, `spell_hit`, `block_value`, ...), resources (`hp`,
`mana`, `rage`, `energy`, `hp_pct`, `max_mana`, `base_mana`, `base_hp`), `level`,
`cp`, `stacks`, `t.<talent>` (rank), `rank` (inside a talent), `r.<field>`
(current spell rank), `opt.<option>`, `aura.<id>[.up|.stacks|.remains|.charges]`,
`cd.<spell>.ready|.remains`, `known.<spell>`, `spell.<spell>` (the event came
from that spell), `event.amount|crit|periodic|pct|mh|oh|special|physical|...`,
`form.<form>`, `form.any`, `mh.dagger`, `mh.speed`, `mh.dps`, `two_hand`,
`dual_wield`, `has_shield`, `behind`, `enemies`, `in_combat`, `stunned`,
`feared`, `frozen`, `rooted`, `casting`, `is_player`, `boss`, creature types
(`humanoid`, `undead`, ...), `pet.active`, `pet.<pet>.active`, `class.<id>`,
`race.<id>`; prefix with `target.`, `owner.` or `pet.` for another unit.
Unknown names are errors at load time, with the path to the offending line.

**Effects:** `damage` (school, hit = white/melee/ranged/spell/auto, weapon,
weapon_mult, normalized, min/max/amount, coef, mult, crit, crit_bonus,
can_crit, threat, target, lifesteal, periodic, no_proc, fixed (ignores every
damage modifier on both sides), on_hit/on_crit/on_miss),
`heal`, `resource` (add or `set`), `combo`, `aura` (stacks, duration, target),
`remove_aura`, `remove_self`, `consume`, `emit` (custom event), `cooldown`
(reset/reduce/set), `summon`, `dismiss`, `sacrifice`, `extra_attack`, `cast`,
`interrupt`, `if`, `chance`, `dispel`, `taunt`, `threat`, `absorb`. Targets:
`self`, `target`, `pet`, `owner`, `my_target`, `enemies` (+ `max_targets`).

**Proc events (`on =`):** `melee_swing melee_landed melee_crit white_landed
special_landed ranged_landed spell_landed spell_crit periodic heal heal_crit
damage_dealt struck struck_melee struck_crit dodge parry block attack_dodged
attack_parried kill cast combat_start summon pet_died`, plus `cast:<spell>`,
`custom:<name>`, `aura_applied:<aura>`, `aura_expired:<aura>`. Procs take
`if`, `chance`, `ppm` and `icd`.

**Spell modifiers** (`spell_mods`, on talents and auras) change `cost
cast_time cooldown damage heal crit crit_bonus threat gcd` of listed `spells`
or a whole `school`. An aura with `consume_on_cast = true` is used up by the
spells its modifiers apply to.

**Coefficients.** `coef` multiplies spell power (healing power for heals).
Forever's spell data gives ranks learned below level 20 their full
coefficient, so the original game's 3.75% cut per level below 20 is off by
default (option `low_level_penalty`). A lower rank used while a higher one is
known is scaled by (rank level + 11) / your level, as in the original game
(option `downrank_penalty`). Forever's rule that healing power counts 1/3
towards spell damage is in `core.toml` (`heal_to_spell_damage`).

### A build file

```toml
name = "Warden-9 (26/9/16)"
kind = "experimental"             # or "standard": one standard build per talent tree
# role = "healer"                 # for healing or tank specs: "healer" or "tank"
class = "druid"
race = "tauren"
level = 60
gear = "leather_feral"            # a kit from gear.toml
rotation = "feral_cat"            # a rotation from the class file
order = ["ferocity", ...]         # optional: one talent per level from 10
[talents]
ferocity = 5
...
[gear_override]                   # optional: add or replace kit stats
crit = 1
[opts]                            # optional: per-build assumptions
druid_omen = 1
```

`--level 30` trims the talents to the level's budget using `order`; without
one, points go to the main tree first, top rows first (and the run says so).

## When a new client build ships

1. Find the build number on wago.tools (the build menu of any table page),
   then write the old and new spellbooks and compare them:
   `node tools/forever_db.mjs --build <old> --out old.json`,
   `node tools/forever_db.mjs --build <new>` (the new one becomes
   `data/reference/spellbook.json`), and
   `python3 tools/spellbook_diff.py old.json data/reference/spellbook.json`
   lists every spell, racial and talent number that moved, by class.
   `fsim audit` then shows what the class files disagree with. Read the patch
   notes for what the tables don't show (server-side rules, proc behavior).
2. Write `data/patches/<build>.toml` with `[meta] build = "<build>"` and only
   the changed keys. `data/patches/README.md` has a worked example: changing
   a duration, replacing a rank table, deleting a talent.
   `data/base` keeps describing the older build, so `--at-build` can
   reproduce it; the builds in `builds/` follow the newest one.
3. `fsim check builds/*/*.toml` - every typo comes back with its path.
4. `fsim whatif <scenario> <builds> --with data/patches/<build>.toml` before
   you commit it, to see exactly what the change does to each build on the
   same random numbers. `fsim show <path>` prints the merged result.
5. Old results stay reproducible with `--at-build <old build>`.

A brand-new talent or spell is the same: add its block in the patch (or the
class file), with `mods`, `spell_mods`, `procs` or `effects`.

## Assumptions to know about (change them with `--opt`)

`fsim list options` shows each with its default and where it comes from. The
ones that move results most:

* `warrior_dws_offhand_rage`: whether Dual Wield Specialization still gives
  off-hand Rage. The talent tree says yes, Blizzard's notes say it was
  removed; off by default, worth about 6% of a Fury warrior's damage.
* `omen_ppm` and `omen_spell_chance`: how often Omen of Clarity procs for
  druids. Forever's data gives only its 10 sec internal cooldown.
* `tog_drain_pct`: how much Touch of the Grave drains ("up to 5%").
* `seal_on_hit_chance`, `paladin_soc_ppm`: how often paladin seals trigger.
* `pet_owner_ap`, `pet_owner_sta`, `demon_owner_sp`, `hawk_dive_interval`:
  how pets scale with their owner and how often a hawk attacks.
* `rage_taken_mult`: Rage from damage taken (Blizzard: "20% to 40%").
* `wand_spell_crit`, `wand_melee_crit`: which crit chance wands use and how
  hard their crits hit.

## Limits

* No positioning: no range, kiting, movement or line of sight. Roots and
  slows do nothing; Frost Nova and Frostbite freezes stop the target acting
  until it takes 25% of its health, standing in for kiting.
* Totems are buffs on the shaman (Healing Stream and Mana Spring reach your
  group in the healing tests). Chain Lightning hits one target. Ground effects (Consecration, Flamestrike's burn) are per-target
  auras. Pet focus is not modeled (fixed ability cadence).
* Gear kits are generic stat totals, the same for every build of a role, so
  results compare talents and rotations, not items. Cloth kits carry a
  typical wand for the level. The website shows each kit's stats.
* Wands are played the typical way: started once nothing better is ready,
  never squeezed into a global cooldown, and stopped by any cast.
* In a `duel` the challenger opens first, which is worth several percentage
  points of win rate (Mutilate beats Vampire v2 76% of the time as the
  challenger, 63% as the opponent). The website runs every duel both ways
  round and averages them; `fsim run duel` reports one direction.
* Enemy players in `pressure` are scripted (two stuns, no trinkets, no
  defensive cooldowns). Builds with long crowd control or immunities
  (paladins, bears) get very high breakevens there; read it as "this build
  cannot be killed by that attacker", not as an exact number.
* The healing tests are one healer against scripted damage. Party members
  don't act, buff, dispel or heal. Nobody moves, so range and line of sight
  don't matter. A heal can't be stopped mid-cast when its target is topped up
  (one on someone who died fizzles). Prayer of Mending heals its holder once
  instead of jumping.
* Spell coefficients, Classic base stats and some proc rates are assumptions
  where the Forever client shows no number.

## Results on the shipped data (build 70425, default options)

See `RESULTS.md` (regenerate with the commands at its top).

## Cross-check against the Python prototype

`data/whatif/python_parity.toml` puts back the prototype's assumptions
(Battle Shout 232, Bloodthirst +30, Victory Rush 45% AP without cooldown, no
crit suppression). With it, the boss scenario gives Vampire v2 271 DPS and
Conventional Fury 305 DPS at level 60, against 272 and 304 from the
prototype. Leveling comes out 5-8% higher (158 and 151 kills per hour
against 151 and 140), from differences in resting and death handling. PvP
breakeven is 17% higher for Vampire v2 (678 against 578), because this version
applies PvP diminishing returns to the attacker's second stun and lets the
attacker's opening stun land first.

## License

GPL-3.0-or-later (see `LICENSE`). The fonts in `web/fonts/` are under the SIL
Open Font License. The game icons are not in this repository: the deploy
fetches them, and they remain Blizzard Entertainment's artwork.
