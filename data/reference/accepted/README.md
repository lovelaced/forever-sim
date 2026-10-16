# Deliberate differences from the game's spell data

`fsim audit` compares the class files with Forever's spell tables
(`../spellbook.json`). When the simulator differs on purpose, list it here,
one file per class, with the reason:

```toml
[shaman.lightning_bolt]
gcd = "The game lists 1.5 s; Forever's patch notes ..."

[shaman.searing_totem_pulse]
match = "Internal: the totem's bolt, which the game casts as a separate spell"
```

The key under `[<class>.<spell id>]` is the field the audit reports
(`cost.mana`, `cast`, `cooldown`, `gcd`, `damage.min`, `targets`,
`duration.<aura>`, `match`, ...), or `*` for every field of that spell.
