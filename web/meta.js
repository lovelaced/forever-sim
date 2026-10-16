// Plain-English labels, icons and colours for the front end.
// Icons come from game-icons.net (CC BY 3.0); see icons.js.

export const CLASS_COLORS = {
  druid: '#ff7d0a', hunter: '#abd473', mage: '#69ccf0', paladin: '#f58cba', priest: '#ffffff',
  rogue: '#fff569', shaman: '#0070de', warlock: '#9482c9', warrior: '#c79c6e',
};

// Shaman blue is too dark to read on black; use a lighter tint for text.
export const CLASS_TEXT = { ...CLASS_COLORS, shaman: '#2f8cff' };

export const CLASS_ICONS = {
  druid: 'paw', hunter: 'bow-arrow', mage: 'wizard-staff', paladin: 'warhammer', priest: 'holy-grail',
  rogue: 'cloak-dagger', shaman: 'totem', warlock: 'warlock-hood', warrior: 'battle-axe',
};

export const CLASS_BLURB = {
  druid: 'Shapeshifter: cat, bear, caster or healer, often all in one fight.',
  hunter: 'Ranged damage with a pet that holds the enemy.',
  mage: 'Fire, Frost and Arcane spells. Fragile but deadly.',
  paladin: 'Holy warrior in plate with heals, seals and immunities.',
  priest: 'Healer or Shadow caster with shields and heals of its own.',
  rogue: 'Stealth, combo points, poisons and stuns.',
  shaman: 'Totems, shocks, weapon enchants and heals.',
  warlock: 'Curses, drains and a demon at your side.',
  warrior: 'Rage-fueled melee in plate.',
};

export const RACE_ICONS = {
  human: 'visored-helm', dwarf: 'dwarf-face', night_elf: 'elf-ear', gnome: 'cog', high_order: 'feathered-wing',
  orc: 'orc-head', undead: 'shambling-zombie', tauren: 'minotaur', troll: 'troll', windshaper: 'fluffy-wing',
};

export const RACE_BLURB = {
  'Ruthless': 'More critical strikes with everything.',
  'The Human Spirit': 'More Spirit.',
  'Earth Fury': 'Critical hits and heals land harder.',
  'Big Game Hunter': 'Extra damage to beasts.',
  'Quickness': 'Harder to hit.',
  'Expansive Mind': 'A bigger mana, rage or energy pool.',
  'Wind Blessed': 'Faster attacks and casts.',
  'Elemental Insight': 'Extra damage to elementals.',
  'Savage': 'A few more critical strikes.',
  'Hardiness': 'Shrugs off stuns more often.',
  'Touch of the Grave': 'Attacks and spells sometimes drain health from the enemy.',
  'Endurance': 'More health, and critical hits and heals land harder.',
  'Regeneration': 'Faster health regeneration, some of it during combat.',
  'Beast Slaying': 'Extra damage to beasts.',
};

// Background colour, drawn watermark and game icon for each talent tree, in
// catalogue order.
export const TREE_STYLE = {
  druid: [['#1c2848', 'moon', 'spell_nature_starfall'], ['#3a2412', 'claw-slashes', 'ability_racial_bearform'], ['#16331d', 'oak-leaf', 'spell_nature_healingtouch']],
  hunter: [['#3a1d12', 'wolf-howl', 'ability_hunter_beasttaming'], ['#2b2d15', 'bow-arrow', 'ability_marksmanship'], ['#16292a', 'bird-claw', 'ability_hunter_swiftstrike']],
  mage: [['#2b1842', 'crystal-ball', 'spell_holy_magicalsentry'], ['#3d150e', 'fire', 'spell_fire_firebolt02'], ['#11253f', 'snowflake-2', 'spell_frost_frostbolt02']],
  paladin: [['#3a3012', 'sun', 'spell_holy_holybolt'], ['#192637', 'checked-shield', 'spell_holy_devotionaura'], ['#3b1313', 'flanged-mace', 'spell_holy_auraoflight']],
  priest: [['#222a38', 'energy-shield', 'spell_holy_wordfortitude'], ['#393216', 'sun', 'spell_holy_holybolt'], ['#231239', 'shadow-grasp', 'spell_shadow_shadowwordpain']],
  rogue: [['#182a14', 'poison-bottle', 'ability_rogue_eviscerate'], ['#3a1f14', 'crossed-swords', 'ability_backstab'], ['#151b2b', 'hooded-assassin', 'ability_stealth']],
  shaman: [['#15283f', 'lightning-branches', 'spell_nature_lightning'], ['#33271a', 'whirlwind', 'spell_nature_lightningshield'], ['#10302d', 'waves', 'spell_nature_magicimmunity']],
  warlock: [['#1d2a10', 'evil-book', 'spell_shadow_deathcoil'], ['#2a1137', 'imp', 'spell_shadow_metamorphosis'], ['#3a170e', 'fire', 'spell_shadow_rainoffire']],
  warrior: [['#33251a', 'dripping-blade', 'ability_warrior_savageblow'], ['#3b1411', 'enrage', 'ability_warrior_innerrage'], ['#1a2432', 'checked-shield', 'ability_warrior_defensivestance']],
};

// Order and wording for the tests. `short` and `per` label the scorecard,
// `group` lets the summary say "the world PvP tests".
export const SCENARIOS = [
  { id: 'solo_1', phrase: 'leveling one mob at a time', abbr: 'kills/h', icon: 'broadsword', wow: 'inv_sword_05', title: 'Leveling: one mob at a time', short: 'One mob', group: 'leveling', unit: 'hour', per: 'kills per hour',
    help: 'An hour of killing same-level mobs one by one, with travel, eating, drinking and deaths counted.' },
  { id: 'solo_2', phrase: 'leveling two mobs at a time', abbr: 'kills/h', icon: 'crossed-swords', wow: 'ability_dualwield', title: 'Leveling: two mobs at a time', short: 'Two mobs', group: 'leveling', unit: 'hour', per: 'kills per hour',
    help: 'The same hour, pulling two mobs together.' },
  { id: 'solo_2_hard', phrase: 'tough pulls', abbr: 'kills/h', icon: 'crowned-skull', wow: 'spell_shadow_deathscream', title: 'Leveling: two tough mobs', short: 'Tough pulls', group: 'leveling', unit: 'hour', per: 'kills per hour',
    help: 'Two mobs that hit 40% harder. Shows which builds survive greedy pulls.' },
  { id: 'solo_3', phrase: 'leveling three mobs at a time', abbr: 'kills/h', icon: 'sword-array', wow: 'ability_whirlwind', title: 'Leveling: three mobs at a time', short: 'Three mobs', group: 'leveling', unit: 'hour', per: 'kills per hour',
    help: 'Three mobs per pull. Area damage and self-healing shine here.' },
  { id: 'boss', phrase: 'the dungeon boss', abbr: 'DPS', icon: 'dragon-head', wow: 'inv_misc_head_dragon_black', title: 'Dungeon boss', short: 'Boss', group: 'boss', unit: 'fight', per: 'damage per second',
    help: 'Two minutes on a boss two levels above you. A tank holds it and you hit it from behind.' },
  { id: 'pressure', phrase: 'the ambush', abbr: 'attacker DPS', icon: 'hooded-assassin', wow: 'ability_stealth', title: 'World PvP: ambushed', short: 'Ambushed', group: 'pvp', unit: 'ambush', per: 'attacker DPS you survive',
    help: 'A geared melee player opens with a stun. How hard can they hit before you lose?' },
  { id: 'pressure_2v1', phrase: 'the two-attacker ambush', abbr: 'attacker DPS', icon: 'two-shadows', wow: 'ability_rogue_ambush', title: 'World PvP: two attackers', short: 'Two attackers', group: 'pvp', unit: 'ambush', per: 'attacker DPS you survive',
    help: 'The same ambush with two attackers at once.' },
  { id: 'duel', phrase: 'the duel', abbr: 'pts', icon: 'fencer', wow: 'ability_parry', title: 'Duel', short: 'Duel', group: 'duel', unit: 'duel', per: 'duels won',
    help: 'Stand and fight until one of you dies, with a three-minute limit.' },
];

// Tests run in this order, quickest first, so the scorecard starts filling
// at once. The scorecard itself keeps the order above.
export const RUN_ORDER = ['boss', 'pressure', 'pressure_2v1', 'duel', 'solo_1', 'solo_2', 'solo_2_hard', 'solo_3'];

export const GROUP_NAMES = { leveling: 'leveling', boss: 'the dungeon boss', pvp: 'world PvP', duel: 'duels' };

// Builds are told apart by slot (A, B, C) in every chart and table; the class
// colour stays on the portrait. Chosen to stay distinct from each other and
// readable on the dark background.
export const SLOTS = [
  { letter: 'A', color: '#e0b35a' },
  { letter: 'B', color: '#67b6e6' },
  { letter: 'C', color: '#d58ad0' },
];

export const UNIT_PLURAL = { hour: 'hours', fight: 'fights', ambush: 'ambushes', duel: 'duels' };

export const ACCURACY = [
  { id: 'quick', title: 'Quick', short: '±3%', note: 'Within about 3%', precision: 0.03, max_reps: 400 },
  { id: 'standard', title: 'Standard', short: '±1.5%', note: 'Within about 1.5%', precision: 0.015, max_reps: 1500 },
  { id: 'precise', title: 'Precise', short: '±0.5%', note: 'Within about 0.5%', precision: 0.005, max_reps: 6000 },
];

// What the headline number means, in one sentence.
export const PRIMARY_HELP = {
  kills_per_hour: 'Mobs killed in an hour of solo leveling, with resting, traveling and deaths included.',
  dps: 'Average damage per second over the two-minute boss fight.',
  breakeven: 'How much damage per second (before your armor) the attacker needs to beat you half the time. Higher means you survive harder ambushes.',
  win: 'Share of duels won against the chosen opponent.',
};

// How each metric is shown. kind: num | pct (0..1 share) | rate (0..1 chance) | sec
export const METRICS = {
  kills_per_hour: { label: 'Kills per hour', kind: 'num', digits: 1, good: 'high', more: 'more kills per hour', less: 'fewer kills per hour' },
  dps: { label: 'Damage per second', kind: 'num', digits: 1, good: 'high', more: 'more damage per second', less: 'less damage per second' },
  breakeven: { label: 'Attacker damage per second you can beat', short: 'Attacker damage you can beat', kind: 'num', digits: 0, good: 'high',
    more: 'more attacker damage per second', less: 'less attacker damage per second', verb: 'survives' },
  win: { label: 'Duels won', kind: 'rate', good: 'high', more: 'more duels won', less: 'fewer duels won' },
  deaths_per_hour: { label: 'Deaths per hour', kind: 'num', digits: 2, good: 'low' },
  dps_in_combat: { label: 'Damage per second while fighting', kind: 'num', digits: 1, good: 'high' },
  time_per_kill: { label: 'Seconds per kill, resting included', kind: 'num', digits: 1, good: 'low' },
  combat_share: { label: 'Time spent fighting', kind: 'pct' },
  rest_share: { label: 'Time spent eating and drinking', kind: 'pct', good: 'low' },
  dead_share: { label: 'Time spent dead', kind: 'pct', good: 'low' },
  taken_per_kill: { label: 'Damage taken per kill', kind: 'num', digits: 0, good: 'low' },
  healed_per_kill: { label: 'Self-healing per kill', kind: 'num', digits: 0, good: 'high' },
  eats_per_hour: { label: 'Meals per hour', kind: 'num', digits: 1, good: 'low' },
  drinks_per_hour: { label: 'Drinks per hour', kind: 'num', digits: 1, good: 'low', hideZero: true },
  stalled_pulls: { label: 'Fights that hit the time limit', kind: 'num', digits: 2, good: 'low', hideZero: true },
  crit_rate: { label: 'Critical strike rate', kind: 'pct' },
  died: { label: 'Chance of dying', kind: 'rate', good: 'low', hideZero: true },
  hps: { label: 'Healing per second', kind: 'num', digits: 1, good: 'high', hideZero: true },
  win_at_reference: { label: 'Wins against the standard attacker', kind: 'rate', good: 'high' },
  time_alive_at_reference: { label: 'Fight length against the standard attacker', kind: 'sec' },
  time_alive: { hidden: true },
  dealt: { label: 'Damage dealt per fight', kind: 'num', digits: 0, good: 'high' },
  healed: { label: 'Healing done per fight', kind: 'num', digits: 0, good: 'high', hideZero: true },
  taken: { label: 'Damage taken per fight', kind: 'num', digits: 0 },
  loss: { label: 'Duels lost', kind: 'rate', good: 'low' },
  timeout: { label: 'Duels that hit the time limit', kind: 'rate', hideZero: true },
  duration: { label: 'Duel length', kind: 'sec' },
  end_hp_pct: { label: 'Health left at the end', kind: 'pct' },
  breakeven_scale: { hidden: true },
};

export const GROUPS = {
  dmg_share: { title: 'Where the damage comes from', kind: 'pct' },
  dps: { title: 'Damage per second by ability', kind: 'num' },
  heal_per_kill: { title: 'Healing per kill by source', kind: 'num' },
  heal: { title: 'Healing by source', kind: 'num' },
  casts_per_kill: { title: 'Casts per kill', kind: 'num' },
  casts_per_min: { title: 'Casts per minute', kind: 'num' },
};

// Character sheet layout: [section, [stat keys]]. Rows with no meaning for
// the class (zero everywhere) are dropped.
export const SHEET = [
  ['Attributes', ['health', 'mana', 'energy', 'strength', 'agility', 'stamina', 'intellect', 'spirit']],
  ['Melee', ['attack power', 'melee crit %', 'hit %', 'main hand dps', 'main hand speed', 'off hand dps', 'off hand speed', 'melee haste x']],
  ['Ranged', ['ranged attack power', 'ranged crit %', 'ranged dps']],
  ['Spells', ['spell power', 'healing power', 'spell crit %', 'spell hit %', 'mp5', 'cast haste x']],
  ['Defense', ['armor', 'dodge %', 'parry %', 'block %', 'block value', 'defense (bonus)', 'damage taken x']],
  ['Damage', ['damage done x', 'physical damage x']],
];

export const STAT_LABELS = {
  'health': 'Health', 'mana': 'Mana', 'energy': 'Energy', 'strength': 'Strength', 'agility': 'Agility',
  'stamina': 'Stamina', 'intellect': 'Intellect', 'spirit': 'Spirit', 'attack power': 'Attack power',
  'ranged attack power': 'Ranged attack power', 'melee crit %': 'Critical strike', 'ranged crit %': 'Critical strike',
  'spell crit %': 'Critical strike', 'hit %': 'Hit chance bonus', 'spell hit %': 'Hit chance bonus', 'spell power': 'Spell damage',
  'healing power': 'Bonus healing', 'armor': 'Armor', 'dodge %': 'Dodge', 'parry %': 'Parry', 'block %': 'Block',
  'block value': 'Block value', 'mp5': 'Mana every 5 seconds', 'defense (bonus)': 'Defense skill bonus',
  'main hand dps': 'Main hand damage per second', 'main hand speed': 'Main hand speed',
  'off hand dps': 'Off hand damage per second', 'off hand speed': 'Off hand speed', 'ranged dps': 'Weapon damage per second',
  'melee haste x': 'Attack speed', 'cast haste x': 'Casting speed', 'damage done x': 'All damage done',
  'physical damage x': 'Physical damage done', 'damage taken x': 'Damage taken',
};

// Talent icons. The first rule whose pattern matches the talent's id and
// name wins (then the same rules against its text). Each rule has a school
// (the icon's background colour) and a list of icons used in turn, so a tree
// with five Fire talents shows five different flames. Specific before general.
const RULES = [
  [/heroic strike/, 'steel', ['sword-slice']],
  [/overpower/, 'steel', ['sword-clash']],
  [/\bslam\b/, 'steel', ['hammer-drop']],
  [/hamstring|wing clip/, 'physical', ['boot-stomp']],
  [/tactical/, 'physical', ['swords-emblem']],
  [/revenge/, 'steel', ['shield-reflect']],
  [/disarm/, 'steel', ['drop-weapon']],
  [/thunder clap/, 'storm', ['thor-hammer']],
  [/sunder/, 'steel', ['cracked-shield']],
  [/\bkick\b|pummel/, 'physical', ['boot-kick']],
  [/counterspell/, 'arcane', ['magic-palm']],
  [/distract/, 'shadow', ['thrown-knife']],
  [/\bfade\b/, 'shadow', ['two-shadows']],
  [/reincarnation/, 'holy', ['ankh']],
  [/tracking/, 'nature', ['hunter-eyes']],
  [/channel/, 'arcane', ['concentration-orb']],
  [/purif/, 'holy', ['holy-water']],
  [/shifting power/, 'nature', ['leaf-swirl']],
  [/mortal strike/, 'blood', ['sword-wound']],
  [/bloodthirst/, 'blood', ['bloody-sword']],
  [/gore drinker/, 'blood', ['chalice-drops']],
  [/last stand/, 'physical', ['heart-tower']],
  [/shield wall/, 'steel', ['bordered-shield']],
  [/sweeping strikes/, 'steel', ['spinning-blades']],
  [/victory/, 'gold', ['laurel-crown']],
  [/stealth|camouflage|deception|preparation|vanish|elusive|cloak/, 'shadow', ['hooded-assassin', 'two-shadows', 'ninja-mask', 'bottled-shadow']],
  [/poison|venom|vile|toxic/, 'poison', ['poison-bottle', 'poison', 'poison-cloud', 'dripping-knife', 'chemical-drop']],
  [/backstab|ambush|mutilate|dagger|cutthroat|cheap shot/, 'steel', ['backstab', 'plain-dagger', 'diving-dagger', 'daggers', 'broad-dagger', 'knife-thrust', 'sacrificial-dagger']],
  [/combo point|seal fate|ruthless|relentless|initiative|setup/, 'gold', ['star-swirl', 'ninja-star', 'star-shuriken', 'triorb', 'barbed-star']],
  [/totem/, 'earth', ['totem-head', 'totem-mask', 'snake-totem']],
  [/water shield|mana tide|tidal|riptide|healing wave|healing way|purification|restorative/, 'water', ['waves', 'water-drop', 'wave-crest', 'water-splash', 'droplets', 'big-wave']],
  [/lightning|thunder|storm|overload|shock|electric/, 'storm', ['lightning-branches', 'chain-lightning', 'focused-lightning', 'lightning-storm', 'lightning-helix', 'lightning-arc', 'heavy-lightning', 'lightning-shield', 'lightning-tree', 'lightning-trio']],
  [/maelstrom|windfury|whirlwind|wind|tornado|flurry|alacrity|swift(?!mend)|haste|rage of the farseer/, 'water', ['whirlwind', 'wind-slap', 'pentarrows-tornado', 'crossed-air-flows', 'windy-stripes']],
  [/lava|magma/, 'fire', ['meteor-impact', 'burning-meteor', 'fire-zone']],
  [/frostfire/, 'arcane', ['frostfire']],
  [/pyro|combust|ignite|scorch|incinerat|conflag|immolat|fire|flame|burn|heating|wake of|brimstone|molten/, 'fire', ['fire', 'fireball', 'fire-ray', 'flame', 'burning-embers', 'fire-ring', 'fire-wave', 'small-fire', 'fire-spell-cast', 'flame-spin', 'fire-silhouette', 'fire-flower', 'fire-breath', 'flaming-claw', 'burning-skull', 'fire-bowl', 'fire-shield']],
  [/ice block|ice barrier|frost ward/, 'frost', ['ice-shield', 'magic-shield', 'frozen-block', 'ice-cube']],
  [/frost|\bice\b|\bicy\b|cold|chill|shatter|winter|blizzard|snow|permafrost|arctic/, 'frost', ['snowflake-2', 'ice-bolt', 'frozen-orb', 'ice-spear', 'ice-spell-cast', 'iceberg', 'frozen-arrow', 'snowing', 'ice-bomb', 'frozen-ring', 'tension-snowflake', 'brain-freeze', 'frozen-body', 'snowflake-1', 'icebergs', 'ice-iris']],
  [/arcane|missile|presence of mind|clearcast|concentration|instability/, 'arcane', ['crystal-ball', 'magic-swirl', 'orb-wand', 'unstable-orb', 'crystal-wand', 'magic-portal', 'floating-crystal', 'crystal-shine', 'triorb', 'spell-book', 'orbital-rays', 'crystal-growth', 'smoking-orb']],
  [/moon|lunar|eclipse|starfire|star/, 'storm', ['moon', 'falling-star', 'star-prominences', 'moon-orbit', 'polar-star', 'seven-pointed-star']],
  [/insect|swarm/, 'nature', ['spider-web', 'praying-mantis']],
  [/root|grasp|entangl|overgrowth/, 'nature', ['tree-roots', 'plant-roots', 'vine-whip', 'thorny-vine', 'curling-vines']],
  [/demon|imp\b|improved imp|felhunter|voidwalker|sayaad|succubus|fel |pact|sacrifice|summon/, 'blood', ['imp', 'imp-laugh', 'daemon-skull', 'devil-mask', 'evil-minion', 'horned-skull', 'evil-wings', 'tentacles-skull', 'diablo-skull']],
  [/drain|siphon|soul|harvest|life tap/, 'blood', ['heart-drop', 'soul', 'soul-vessel', 'marrow-drain', 'chalice-drops', 'heart-bottle', 'death-juice']],
  [/curse|bane|agony|exhaust|malediction|amplify|havoc/, 'shadow', ['evil-book', 'burning-book', 'cursed-star', 'black-book', 'evil-eyes', 'skull-staff', 'triple-skulls', 'evil-fork']],
  [/corrupt|wrack|pandemic|nightfall|shadow trance/, 'shadow', ['evil-hand', 'poison-cloud', 'evil-tree', 'evil-bud', 'goo-skull', 'leaky-skull', 'gooey-impact']],
  [/shadowform|shadow|dark|void|vampiric|mind flay|mind blast|silence|blackout|death/, 'shadow', ['shadow-grasp', 'death-skull', 'bottled-shadow', 'lightning-shadow', 'dread-skull', 'death-zone', 'burning-eye', 'evil-comet', 'tentacle-heart', 'third-eye', 'shadow-follower', 'brain-tentacle']],
  [/fear|scream|howl|terror|horror|psychic/, 'shadow', ['screaming', 'psychic-waves', 'evil-eyes', 'surprised-skull']],
  [/shield wall|bulwark|redoubt|shield spec|shield slam|holy shield|block|bastion|defiance|shield/, 'steel', ['checked-shield', 'shield-bash', 'spiked-shield', 'shield-impact', 'templar-shield', 'round-shield', 'cross-shield', 'shield-echoes', 'winged-shield', 'heart-shield']],
  [/power word|inner fire|divine aegis|aegis|ward/, 'holy', ['energy-shield', 'healing-shield', 'bolt-shield', 'surrounded-shield', 'temporary-shield', 'vibrating-shield']],
  [/holy|light|divine|seal|judg|consecrat|sacred|bless|benediction|righteous|crusader|templar|creed|vigil|redemption|penance|prayer|saint|faith|truth|reverence|smite|nova/, 'holy', ['sun', 'sunbeams', 'holy-symbol', 'heraldic-sun', 'sun-priest', 'angel-wings', 'barbed-sun', 'sun-radiations', 'templar-eye', 'jerusalem-cross', 'prayer', 'sun-spear', 'spiked-halo', 'striped-sun', 'cross-flare', 'gothic-cross', 'templar-heart', 'justice-star', 'holy-water', 'enlightenment', 'prayer-beads', 'law-star', 'holy-oak', 'aztec-calendar-sun']],
  [/heal|mend|renew|regrowth|rejuv|swiftmend|wild growth|recovery|spirit bond|blood craze|gift|regeneration|nurtur|naturalist|living spirit/, 'nature', ['healing', 'heart-plus', 'health-potion', 'shining-heart', 'heart-beats', 'heart-wings', 'glass-heart', 'heart-inside', 'crowned-heart', 'nested-hearts']],
  [/meditation|reflection|mindful|mental|intellect|mind|focus|wisdom|insight|clarity|tranquil/, 'arcane', ['brain', 'third-eye', 'enlightenment', 'crystal-eye', 'all-seeing-eye', 'hive-mind', 'brainstorm', 'book-aura', 'scroll-quill', 'open-book', 'white-book']],
  [/pet|beast|bestial|ferocity|frenzy|unleashed|intimidation|spirit bond|endurance training/, 'earth', ['wolf-howl', 'wolf-head', 'direwolf', 'bestial-fangs', 'paw-print', 'fangs', 'paw-heart', 'saber-toothed-cat-head', 'beast-eye', 'top-paw']],
  [/hawk|eagle|aspect/, 'gold', ['eagle-emblem', 'hawk-emblem', 'eagle-head', 'falcon-moon', 'bird-mask']],
  [/bear|thick hide|feral instinct|dire/, 'earth', ['bear-head', 'bear-face', 'polar-bear']],
  [/cat|claw|shred|rake|rip|bite|predat|savage|feral|primal|rend and tear|pack|furor|natural reaction|heart of the wild|blood frenzy|berserk/, 'earth', ['claw-slashes', 'claws', 'saber-toothed-cat-head', 'triple-claws', 'flat-paw-print', 'fangs', 'circle-claws', 'moon-claws', 'shining-claw', 'interleaved-claws', 'bestial-fangs', 'cat', 'midnight-claw', 'grasping-claws', 'fangs-circle', 'crossed-claws']],
  [/nature|genesis|naturalist|shapeshift|leaf|grace|splendor|reach/, 'nature', ['oak-leaf', 'leaf-swirl', 'maple-leaf', 'sunflower', 'vine-flower', 'tree-growth', 'falling-leaf', 'holy-oak', 'ginkgo-leaf', 'linden-leaf', 'fruit-tree', 'curled-leaf']],
  [/sniper|aimed|marks|careful aim|trueshot|lone wolf|barrage|rapid|hawk eye|shot|arrow|ranged|bow|sting/, 'nature', ['bow-arrow', 'heavy-arrow', 'arrow-cluster', 'target-arrows', 'barbed-arrow', 'striking-arrows', 'broadhead-arrow', 'bowman', 'crosshair-arrow', 'arrow-flights', 'supersonic-arrow', 'winged-arrow', 'arrowhead', 'thorned-arrow', 'chemical-arrow', 'crossbow', 'spiral-arrow']],
  [/trap|entrap|surefoot|survival|deterrence|counterattack|mongoose|strider|lacerat|expose prey|resourceful/, 'earth', ['bird-claw', 'wolf-trap', 'mantrap', 'hunter-eyes', 'boot-prints', 'fishing-net', 'spikes', 'box-trap']],
  [/bleed|rend|deep wound|rupture|hemorr|gore|blood|lacer|serrated/, 'blood', ['bleeding-heart', 'blood', 'dripping-sword', 'sword-wound', 'bleeding-eye', 'pierced-heart', 'blade-bite']],
  [/enrage|rage|fury|wrath|death wish|anger|cruelty|bloodthrill|lingering/, 'blood', ['enrage', 'brutal-helm', 'burning-passion', 'angry-eyes', 'fist', 'horned-helm', 'spiked-collar']],
  [/shout|voice|booming|horn/, 'physical', ['sonic-shout', 'shouting', 'lightning-shout', 'hunting-horn', 'mighty-horn']],
  [/stun|concussion|impact|bash|stomp|hammer of justice|kidney|gouge|brutal|repentance|pyroclasm|intimidat/, 'steel', ['anvil-impact', 'punch-blast', 'knocked-out-stars', 'fulguro-punch', 'mailed-fist', 'star-struck']],
  [/execute|mortal|impale|lethal|lethality|weapon expertise|weaponmaster|vindication/, 'blood', ['dripping-blade', 'sword-slice', 'blade-fall', 'piercing-sword', 'crescent-blade', 'relic-blade']],
  [/two-handed|two handed|axe/, 'steel', ['battle-axe', 'war-axe', 'axe-swing', 'sharp-axe', 'two-handed-sword', 'crossed-axes']],
  [/mace|hammer/, 'steel', ['flanged-mace', 'spiked-mace', 'mace-head', 'thor-hammer', 'bone-mace', 'flat-hammer']],
  [/dual wield|off-hand|one-handed|one handed|blade flurry|sword|hack and slash|riposte|sinister|eviscerate|aggression|ghostly|precision|hit/, 'steel', ['crossed-swords', 'sword-brandish', 'swords-power', 'spinning-sword', 'sword-spin', 'crossed-sabres', 'dervish-swords', 'shining-sword', 'rune-sword', 'pointy-sword', 'ancient-sword']],
  [/crit|malice|conviction|cruel|sharpen|eye for an eye|vengeance|quietus|opportunity/, 'gold', ['on-target', 'bullseye', 'targeted', 'crosshair', 'eye-target', 'convergence-target', 'triangle-target']],
  [/parry|deflect|dodge|evasion|reflex|anticipation|defense|feint/, 'physical', ['dodging', 'sword-break', 'return-arrow', 'shield-bounces', 'cape-armor']],
  [/armor|armour|tough|iron will|stoneskin|plate|harden|bulk/, 'steel', ['armor-vest', 'chest-armor', 'layered-armor', 'shoulder-armor', 'heavy-helm', 'spiked-armor', 'abdominal-armor', 'heart-armor', 'black-knight-helm']],
  [/charge|intercept|pursuit|speed|sprint|swiftness|pathfind|dash|ghost wolf|vanguard/, 'physical', ['sprint', 'charging-bull', 'running-shoe', 'run', 'fire-dash', 'walking-boot', 'jump-across']],
  [/stamina|health|endurance|survivalist|demonic embrace|strength|might/, 'physical', ['muscular-torso', 'biceps', 'heart-plus', 'muscle-up', 'heart-battery', 'fist']],
  [/mana|energy|cost|efficiency|thrift|moonglow|convection|cataclysm|conduit|focus|vigor|adrenaline|power infusion|arcane power/, 'arcane', ['magic-potion', 'round-potion', 'standing-potion', 'potion-ball', 'water-flask', 'crystal-bars', 'mineral-heart', 'fire-gem']],
  [/cooldown|time|duration|improved/, 'gold', ['hourglass', 'sands-of-time', 'stopwatch', 'sundial', 'time-trap', 'star-cycle', 'upgrade']],
];

// Painted-icon colours: [highlight, shadow] for the radial background.
const SCHOOL = {
  fire: ['#ffc15a', '#6e1500'], frost: ['#aee8ff', '#0b2a5e'], arcane: ['#f0b5ff', '#35105c'],
  nature: ['#b9ec6a', '#1b4510'], water: ['#8eeaf0', '#0a3644'], storm: ['#c3d2ff', '#1a1f5c'],
  shadow: ['#a77de0', '#12061d'], blood: ['#ff7a6b', '#3d0606'], holy: ['#fff1a0', '#7a4c00'],
  physical: ['#e2cfa6', '#3a2a17'], steel: ['#d3dce6', '#262e38'], earth: ['#e8b97e', '#45260d'],
  poison: ['#d0f56f', '#213805'], gold: ['#ffe27a', '#5c3a00'],
};
const ICON_SCHOOL = {
  'broadsword': 'steel', 'crossed-swords': 'steel', 'crowned-skull': 'shadow', 'sword-array': 'steel',
  'dragon-head': 'fire', 'hooded-assassin': 'shadow', 'two-shadows': 'blood', 'fencer': 'gold',
};

export function iconColors(name, school) {
  return SCHOOL[school || ICON_SCHOOL[name] || 'physical'];
}

// Icon and school for a talent. `used` counts how often each rule has
// been used so far in the tree, so repeats rotate through the rule's icons.
export function talentIcon(cls, t, used = {}) {
  const head = `${t.id.replace(/_/g, ' ')} ${t.name}`.toLowerCase();
  const hay = `${head} ${t.text}`.toLowerCase();
  let k = RULES.findIndex(([re]) => re.test(head));
  if (k < 0) k = RULES.findIndex(([re]) => re.test(hay));
  if (k < 0) return { icon: CLASS_ICONS[cls] || 'rune-stone', school: 'gold' };
  const [, school, icons] = RULES[k];
  const n = used[k] || 0;
  used[k] = n + 1;
  return { icon: icons[n % icons.length], school };
}

export const ALL_ICON_NAMES = [
  ...Object.values(CLASS_ICONS), ...Object.values(RACE_ICONS), ...SCENARIOS.map(s => s.icon), ...RULES.flatMap(r => r[2]),
  'rune-stone', 'scroll-unfurled', 'cog', 'share', 'help', 'trophy', 'rolling-dices', 'trash-can', 'crossed-swords',
  'robe', 'leather-vest', 'chain-mail', 'breastplate', 'open-book', 'padlock',
];

// "Bear tank: Primal Bite, ..." -> title "Bear tank", detail "Primal Bite, ..."
export function splitDesc(desc) {
  const m = /^([^:(.]{3,60})(?:[:.]|(?=\())\s*(.*)$/.exec(desc || '');
  if (m) return [m[1].trim(), m[2].trim()];
  return [desc || '', ''];
}

// Short names for the gear kits in data/base/gear.toml (the kit's own
// description is shown underneath). Unknown kits fall back to that description.
export const GEAR_TITLES = {
  cloth_battlemage: 'Cloth with a sword and a wand', cloth_caster: 'Caster cloth with a staff and a wand', cloth_healer: 'Healer cloth with a wand',
  leather_agi: 'Agility leather, two weapons', leather_caster: 'Caster leather', leather_daggers: 'Agility leather, two daggers',
  leather_feral: 'Feral leather with a two-hander', mail_caster: 'Caster mail', mail_enh: 'Mail with a two-hander',
  mail_hunter: 'Agility mail with a bow', mail_shield: 'Mail with a one-hander and shield', plate_2h: 'Plate with a two-hander',
  plate_dw: 'Plate, two weapons', plate_holy: 'Healer plate with a shield', plate_ret: 'Plate with a two-hander and Intellect',
  plate_tank: 'Tank plate with a shield',
};

// Weapon types as the gear list names them.
export const WEAPON_WORDS = {
  sword: 'Sword', axe: 'Axe', mace: 'Mace', dagger: 'Dagger', fist: 'Fist weapon', polearm: 'Polearm', staff: 'Staff',
  sword2h: 'Two-handed sword', axe2h: 'Two-handed axe', mace2h: 'Two-handed mace', bow: 'Bow', gun: 'Gun',
  crossbow: 'Crossbow', thrown: 'Thrown weapon', wand: 'Wand',
};

// Game icons for the gear kits (a chest piece of the right armor type).
export function gearWowIcon(id) {
  if (id.startsWith('cloth')) return 'inv_chest_cloth_21';
  if (id.startsWith('leather')) return 'inv_chest_leather_09';
  if (id.startsWith('mail')) return 'inv_chest_chain_05';
  return 'inv_chest_plate16';
}

// Every game icon the page uses besides the ones in the game data (classes,
// races and talents carry their own `icon`). tools/fetch_icons.mjs reads this.
export function extraWowIcons() {
  return [
    ...SCENARIOS.map((s) => s.wow),
    ...Object.values(TREE_STYLE).flat().map((t) => t[2]),
    ...['cloth', 'leather', 'mail', 'plate'].map((a) => gearWowIcon(a)),
  ].filter(Boolean);
}

export function gearIcon(id) {
  if (id.startsWith('cloth')) return 'robe';
  if (id.startsWith('leather')) return 'leather-vest';
  if (id.startsWith('mail')) return 'chain-mail';
  return 'breastplate';
}
