# Patch files

Every `*.toml` file in this folder is an overlay on `data/base`, applied in
client-build order. When Blizzard ships a new beta or live build, put the
changes for it here; nothing in `data/base` has to be edited.

```toml
# data/patches/1.60.2.70501.toml
[meta]
build = "1.60.2.70501"            # sort key, and what --at-build compares against
notes = "Gore Drinker buffed, Victory Rush cooldown 20 sec"

[classes.warrior.auras.gore_drinker]
duration = 20                     # a single number: replaced

[classes.warrior.spells.victory_rush]
cooldown = 20

[classes.warrior.spells.bloodthirst]
ranks = [                         # lists are replaced whole: restate every rank
  { level = 40, bonus = 34 }, { level = 48, bonus = 41 },
  { level = 54, bonus = 47 }, { level = 60, bonus = 52 },
]

[classes.warrior.talents.piercing_howl]
__delete = true                   # removes the talent (builds using it now fail `fsim check`)
```

Rules: tables merge key by key, anything else (numbers, strings, lists)
replaces, and a table with `__delete = true` removes that key.

Useful commands while writing a patch:

* `fsim show classes.warrior.spells.bloodthirst` prints the merged result.
* `fsim check builds/*/*.toml` recompiles everything and names any typo with its path.
* `fsim whatif boss builds/warrior/vampire_v2.toml --with data/patches/1.60.2.70501.toml`
  runs the build with and without the file on the same random numbers.
* `fsim --at-build 1.60.1.70338 run ...` ignores patches newer than a build.
* `fsim --no-patches run ...` ignores this folder entirely.
