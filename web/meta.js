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
    help: 'An hour of same-level mobs, one at a time: the open world’s mix of beasts, humanoids (some cast, some run at low health), undead and the rest. Walking up to them, travel, eating and deaths all count.' },
  { id: 'solo_2', phrase: 'leveling two mobs at a time', abbr: 'kills/h', icon: 'crossed-swords', wow: 'ability_dualwield', title: 'Leveling: two mobs at a time', short: 'Two mobs', group: 'leveling', unit: 'hour', per: 'kills per hour',
    help: 'The same hour, two mobs per pull.' },
  { id: 'solo_2_hard', phrase: 'tough pulls', abbr: 'kills/h', icon: 'crowned-skull', wow: 'spell_shadow_deathscream', title: 'Leveling: two tough mobs', short: 'Tough pulls', group: 'leveling', unit: 'hour', per: 'kills per hour',
    help: 'Two mobs per pull that hit 40% harder.' },
  { id: 'solo_3', phrase: 'leveling three mobs at a time', abbr: 'kills/h', icon: 'sword-array', wow: 'ability_whirlwind', title: 'Leveling: three mobs at a time', short: 'Three mobs', group: 'leveling', unit: 'hour', per: 'kills per hour',
    help: 'Three mobs per pull.' },
  { id: 'boss', phrase: 'the dungeon boss', abbr: 'DPS', icon: 'dragon-head', wow: 'inv_misc_head_dragon_black', title: 'Dungeon boss', short: 'Boss', group: 'boss', unit: 'fight', per: 'damage per second',
    help: 'Two minutes on a boss two levels above you. A Protection warrior of your level tanks it and the rest of the party buffs you; you give the tank a head start and ease off when you’re close to pulling the boss.' },
  { id: 'pressure', phrase: 'the ambush', abbr: 'attacker DPS', icon: 'hooded-assassin', wow: 'ability_stealth', title: 'World PvP: ambushed', short: 'Ambushed', group: 'pvp', unit: 'ambush', per: 'attacker DPS you survive',
    help: 'A geared melee player opens with a stun. The number is how hard they can hit before you lose.' },
  { id: 'pressure_2v1', phrase: 'the two-attacker ambush', abbr: 'attacker DPS', icon: 'two-shadows', wow: 'ability_rogue_ambush', title: 'World PvP: two attackers', short: 'Two attackers', group: 'pvp', unit: 'ambush', per: 'attacker DPS you survive',
    help: 'The same ambush, two attackers at once.' },
  { id: 'duel', phrase: 'the duel', abbr: 'pts', icon: 'fencer', wow: 'ability_parry', title: 'Duel', short: 'Duel', group: 'duel', unit: 'duel', per: 'duel score',
    help: 'Stand and fight until one of you dies, with a three-minute limit.' },
  { id: 'heal_dungeon', phrase: 'dungeon healing', abbr: 'boss DPS', icon: 'healing', wow: 'spell_holy_renew', title: 'Healing: dungeon', short: 'Dungeon', group: 'healing', unit: 'fight', per: 'boss damage per second you heal through',
    help: 'Two minutes keeping a tank and three others alive against a dungeon boss two levels up. The number is how hard the boss can hit before someone dies.' },
  { id: 'heal_tank', phrase: 'main tank healing', abbr: 'boss DPS', icon: 'heart-plus', wow: 'spell_holy_greaterheal', title: 'Healing: main tank', short: 'Main tank', group: 'healing', unit: 'fight', per: 'boss damage per second you heal through',
    help: 'Three minutes keeping one tank alive against a raid boss three levels up, crushing blows and all.' },
  { id: 'heal_raid', phrase: 'raid healing', abbr: 'boss DPS', icon: 'angel-wings', wow: 'spell_holy_prayerofhealing02', title: 'Healing: raid', short: 'Raid', group: 'healing', unit: 'fight', per: 'boss damage per second you heal through', minLevel: 60,
    help: 'Two and a half minutes healing ten players in two groups against Onyxia, alone. The tank and you have to live, and no more than two others can fall.' },
];

// Each test's scene by level bracket and side of the world: where it's set,
// the picture (web/art/<art>.png), and how far down the picture to look
// (top, in art pixels), so the hero stands on the same ground as the enemies
// (ground: art pixels up from the bottom of the view). Pictures are wider than
// the view and sit against its right edge, where the enemies are, so a
// narrow screen trims the empty left side, where the hero stands (rpad: art
// pixels of the picture's right edge left out). Brackets are the 10s,
// 20s, 30s, 40s and 50 to 60; a bracket or side without its own picture yet
// borrows the nearest one that has. The PvP scenes show the other faction
// attacking; the duel and other shared places can be one picture for both.
const S1 = (art, zone, top, rpad = 0) => ({ art, zone, top, rpad });
export const SCENES = {
  solo_1: {
    b10: { alliance: S1('scene-solo_1', 'Westfall', 21), horde: S1('scene-solo_1-horde', 'The Barrens', 46) },
    b20: { alliance: S1('scene-solo_1-b20', 'Redridge Mountains', 26), horde: S1('scene-solo_1-b20-horde', 'Stonetalon Mountains', 28) },
    b30: { alliance: S1('scene-solo_1-b30', 'Arathi Highlands', 32), horde: S1('scene-solo_1-b30-horde', 'Thousand Needles', 35) },
    b40: { alliance: S1('scene-solo_1-b40', 'Tanaris', 34), horde: S1('scene-solo_1-b40-horde', 'Feralas', 35) },
    b50: { alliance: S1('scene-solo_1-b50', 'Un’Goro Crater', 21), horde: S1('scene-solo_1-b50-horde', 'Felwood', 41) },
  },
  solo_2: {
    b10: { alliance: S1('scene-solo_2-b10', 'Loch Modan', 22), horde: S1('scene-solo_2-b10-horde', 'Silverpine Forest', 33) },
    b20: { alliance: S1('scene-solo_2-b20', 'Wetlands', 25), horde: S1('scene-solo_2-b20-horde', 'Hillsbrad Foothills', 36) },
    b30: { alliance: S1('scene-solo_2-b30', 'Alterac Mountains', 37), horde: S1('scene-solo_2-b30-horde', 'Desolace', 36) },
    b40: { alliance: S1('scene-solo_2-b40', 'The Hinterlands', 32), horde: S1('scene-solo_2-b40-horde', 'Badlands', 53) },
    b50: { alliance: S1('scene-solo_2-b50', 'Eastern Plaguelands', 40), horde: S1('scene-solo_2-b50-horde', 'Western Plaguelands', 34) },
  },
  solo_2_hard: {
    b10: { alliance: S1('scene-solo_2_hard-b10', 'Darkshore', 32), horde: S1('scene-solo_2_hard-b10-horde', 'The Barrens', 31) },
    b20: { alliance: S1('scene-solo_2_hard-b20', 'Vul’Gol, Duskwood', 43), horde: S1('scene-solo_2_hard-b20-horde', 'Ashenvale', 28) },
    b30: { alliance: S1('scene-solo_2_hard-b30', 'Stranglethorn Vale', 37), horde: S1('scene-solo_2_hard-horde', 'Dustwallow Marsh', 23) },
    b40: { alliance: S1('scene-solo_2_hard-b40', 'Searing Gorge', 36), horde: S1('scene-solo_2_hard-b40-horde', 'Searing Gorge', 29) },
    b50: { alliance: S1('scene-solo_2_hard-b50', 'Burning Steppes', 31), horde: S1('scene-solo_2_hard-b50-horde', 'Silithus', 40) },
  },
  solo_3: {
    b10: { alliance: S1('scene-solo_3', 'The Westfall coast', 28), horde: S1('scene-solo_3-horde', 'The Barrens', 36) },
    b20: { alliance: S1('scene-solo_3-b20', 'Raven Hill, Duskwood', 37), horde: S1('scene-solo_3-b20-horde', 'The Hillsbrad coast', 34) },
    b30: { alliance: S1('scene-solo_3-b30', 'The Stranglethorn coast', 32), horde: S1('scene-solo_3-b30-horde', 'Arathi Highlands', 38) },
    b40: { alliance: S1('scene-solo_3-b40', 'Azshara', 28), horde: S1('scene-solo_3-b40-horde', 'Zul’Farrak, Tanaris', 29) },
    b50: { alliance: S1('scene-solo_3-b50', 'Winterspring', 27), horde: S1('scene-solo_3-b50-horde', 'Un’Goro Crater', 21) },
  },
  boss: {
    b10: { alliance: S1('scene-boss', 'The Deadmines', 38), horde: S1('scene-boss-b10-horde', 'Ragefire Chasm', 36) },
    b20: { alliance: S1('scene-boss-b20', 'Blackfathom Deeps', 54), horde: S1('scene-boss-b20-horde', 'Shadowfang Keep', 33) },
    b30: { alliance: S1('scene-boss-b30', 'Gnomeregan', 35), horde: S1('scene-boss-horde', 'Razorfen Kraul', 40) },
    b40: { alliance: S1('scene-boss-b40', 'Uldaman', 32), horde: S1('scene-boss-b40-horde', 'Razorfen Downs', 31) },
    b50: { alliance: S1('scene-boss-b50', 'Blackrock Depths', 53), horde: S1('scene-boss-b50-horde', 'Stratholme', 42) },
  },
  pressure: {
    b10: { alliance: S1('scene-pressure-b10', 'The Barrens, near Ratchet', 36), horde: S1('scene-pressure-b10-horde', 'The Barrens, near the Wailing Caverns', 36) },
    b20: { alliance: S1('scene-pressure-b20', 'Ashenvale', 40), horde: S1('scene-pressure-b20-horde', 'Ashenvale', 31) },
    b30: { alliance: S1('scene-pressure', 'Stranglethorn Vale', 36), horde: S1('scene-pressure-horde', 'Stranglethorn Vale', 32) },
    b40: { alliance: S1('scene-pressure-b40', 'Tanaris', 39), horde: S1('scene-pressure-b40-horde', 'Feralas', 39) },
    b50: { alliance: S1('scene-pressure-b50', 'Silithus', 32), horde: S1('scene-pressure-b50-horde', 'Eastern Plaguelands', 36) },
  },
  pressure_2v1: {
    b10: { alliance: S1('scene-pressure_2v1-b10', 'The Barrens, near the Crossroads', 38), horde: S1('scene-pressure_2v1-b10-horde', 'The southern Barrens', 31) },
    b20: { alliance: S1('scene-pressure_2v1', 'Hillsbrad Foothills', 50), horde: S1('scene-pressure_2v1-horde', 'Hillsbrad Foothills', 56) },
    b30: { alliance: S1('scene-pressure_2v1-b30', 'Arathi Highlands', 27), horde: S1('scene-pressure_2v1-b30-horde', 'Arathi Highlands', 36) },
    b40: { alliance: S1('scene-pressure_2v1-b40', 'The Hinterlands, near Aerie Peak', 33), horde: S1('scene-pressure_2v1-b40-horde', 'The Hinterlands, near Revantusk', 45) },
    b50: { alliance: S1('scene-pressure_2v1-b50', 'Burning Steppes', 24), horde: S1('scene-pressure_2v1-b50-horde', 'Burning Steppes', 33) },
  },
  duel: {
    b10: { alliance: S1('scene-duel-b10', 'Goldshire', 33), horde: S1('scene-duel-b10-horde', 'The Crossroads', 34) },
    b20: { alliance: S1('scene-duel-b20', 'Stormwind City', 35), horde: S1('scene-duel-b20-horde', 'The ruins of Lordaeron', 48) },
    b30: { both: S1('scene-duel', 'Gurubashi Arena', 34) },
    b40: { both: S1('scene-duel-b40', 'Gadgetzan', 29) },
    b50: { alliance: S1('scene-duel-b50', 'The gates of Ironforge', 41), horde: S1('scene-duel-b50-horde', 'The gates of Orgrimmar', 41) },
  },
  heal_dungeon: {
    b10: { alliance: S1('scene-heal_dungeon-b10', 'Wailing Caverns', 32), horde: S1('scene-heal_dungeon-b10-horde', 'Wailing Caverns', 33) },
    b20: { alliance: S1('scene-heal_dungeon-b20', 'Shadowfang Keep', 38), horde: S1('scene-heal_dungeon-b20-horde', 'Blackfathom Deeps', 31) },
    b30: { alliance: S1('scene-heal_dungeon-b30', 'Scarlet Monastery', 20, 20), horde: S1('scene-heal_dungeon-b30-horde', 'Scarlet Monastery', 26, 10) },
    b40: { alliance: S1('scene-heal_dungeon-b40', 'Zul’Farrak', 33), horde: S1('scene-heal_dungeon-b40-horde', 'Zul’Farrak', 34) },
    b50: { alliance: S1('scene-heal_dungeon-b50', 'Upper Blackrock Spire', 32), horde: S1('scene-heal_dungeon-b50-horde', 'Upper Blackrock Spire', 34) },
  },
  heal_tank: {
    b10: { alliance: S1('scene-heal_tank-b10', 'The Deadmines', 28), horde: S1('scene-heal_tank-b10-horde', 'Ragefire Chasm', 26) },
    b20: { alliance: S1('scene-heal_tank-b20', 'Shadowfang Keep', 38), horde: S1('scene-heal_tank-b20-horde', 'Shadowfang Keep', 27) },
    b30: { alliance: S1('scene-heal_tank-b30', 'The Scarlet Monastery armory', 28), horde: S1('scene-heal_tank-b30-horde', 'The Scarlet Monastery armory', 27) },
    b40: { alliance: S1('scene-heal_tank-b40', 'Maraudon', 41), horde: S1('scene-heal_tank-b40-horde', 'Maraudon', 36) },
    b50: { alliance: S1('scene-heal_tank-b50', 'Molten Core', 25), horde: S1('scene-heal_tank-b50-horde', 'Molten Core', 25) },
  },
  heal_raid: {
    b50: { alliance: S1('scene-heal_raid-b50', 'Onyxia’s Lair', 38), horde: S1('scene-heal_raid-b50-horde', 'Onyxia’s Lair', 40) },
  },
};
export const BRACKETS = ['b10', 'b20', 'b30', 'b40', 'b50'];
export const bracketOf = (level) => BRACKETS[Math.min(4, Math.max(0, Math.floor((level || 10) / 10) - 1))];
// The scene for a test, a side (the main build's) and a level.
export function sceneFor(test, faction, level) {
  const t = SCENES[test];
  if (!t) return null;
  const at = BRACKETS.indexOf(bracketOf(level));
  // the nearest bracket with a picture for this side (or one for both sides)
  const order = BRACKETS.map((b, i) => [b, i]).sort((x, y) => Math.abs(x[1] - at) - Math.abs(y[1] - at) || y[1] - x[1]);
  const side = faction === 'horde' ? 'horde' : 'alliance';
  let v = null;
  for (const [b] of order) {
    v = t[b]?.[side] || t[b]?.both;
    if (v) break;
  }
  if (!v) for (const [b] of order) { v = t[b]?.alliance || t[b]?.horde; if (v) break; }
  return v && { file: `art/${v.art}.png`, zone: v.zone, top: v.top, rpad: v.rpad || 0, ground: v.ground ?? 14 };
}
// The side of the world things are drawn from: Horde art for a Horde build.
export const sideArt = (faction) => (faction === 'horde'
  ? { endcap: 'art/wyvern.png', inn: 'banner-inn-horde' }
  : { endcap: 'art/gryphon.png', inn: 'banner-inn' });
// The hero sprite for a race and class (web/art/hero-<race>-<class>.png).
export const heroArt = (race, cls) => `art/hero-${race}-${cls}.png`;

// Shown while the simulator starts, one at random.
export const LOADING_TIPS = [
  'Every build in a comparison faces the same dice rolls, so a gap between them comes from the builds, not from luck.',
  'Two tough mobs hit 40% harder than normal ones. That test shows which builds can afford greedy pulls.',
  'Talents with a grey corner aren’t simulated yet and change nothing.',
  'The simulated player plays like a typical player: half-second reactions and the odd slip. Switch to expert under Assumptions.',
  'Change a build and simulate again: the results say what your change did, measured on the same fights.',
  'Duels start on the same tick, a few yards apart, and whoever reacts first strikes first.',
  'Numbers the game doesn’t publish are guesses you can change under Assumptions.',
  'The leveling tests count everything: travel, eating, drinking and the corpse run after a death.',
  'Openers are real casts: Charge waits for its cooldown, and the first hit ends Stealth.',
  'The world PvP tests ask how hard an attacker has to hit to beat you half the time.',
  'Everyone moves: casters get a cast or two off before the mob arrives, Frost mages kite, and hurt humanoids run for help.',
  'Totems stand where the shaman put them down: walk away and their buffs stop reaching you.',
  'Precise accuracy runs the most fights. Use it when two builds look too close to call.',
  'Class rankings runs every build on the site through seven tests, and the healers through three more. It takes a few minutes.',
  'Spell numbers come from Forever’s own game data, checked one by one.',
  'Ranges are 95% confidence intervals: run a test again and the average would almost always land inside.',
  'In the boss test a tank holds the boss, and you hit it from behind.',
  'Share these builds from the top of the page: anyone who opens the link sees the same builds.',
  'Everything runs in your browser. Nothing you set up leaves your computer.',
  'Open a test to watch one of its fights step by step, every spell with its icon.',
  'Healers take three healing tests: a dungeon, a main tank and, at level 60, a raid.',
  'In the healing tests, heals are sized to what’s missing: a small gap gets a low rank.',
];

// Tests run in this order, quickest first, so the scorecard starts filling
// at once. The scorecard itself keeps the order above.
export const RUN_ORDER = ['boss', 'pressure', 'pressure_2v1', 'duel', 'solo_1', 'solo_2', 'solo_2_hard', 'solo_3', 'heal_dungeon', 'heal_tank', 'heal_raid'];

export const GROUP_NAMES = { leveling: 'leveling', boss: 'the dungeon boss', pvp: 'world PvP', duel: 'duels', healing: 'healing' };

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
  score: 'Your score against the chosen opponent: a win counts 1, and a draw or a duel that runs out of time counts half.',
  heal_breakeven: 'How much damage per second (before armor) the boss can deal before the party falls, half the time. Higher means you heal through harder fights.',
};

// How each metric is shown. kind: num | pct (0..1 share) | rate (0..1 chance) | sec
export const METRICS = {
  kills_per_hour: { label: 'Kills per hour', kind: 'num', digits: 1, good: 'high', more: 'more kills per hour', less: 'fewer kills per hour' },
  dps: { label: 'Damage per second', kind: 'num', digits: 1, good: 'high', more: 'more damage per second', less: 'less damage per second' },
  breakeven: { label: 'Attacker damage per second you can beat', short: 'Attacker damage you can beat', kind: 'num', digits: 0, good: 'high',
    more: 'more attacker damage per second', less: 'less attacker damage per second', verb: 'survives' },
  win: { label: 'Duels won', kind: 'rate', good: 'high', more: 'more duels won', less: 'fewer duels won' },
  score: { label: 'Duel score', kind: 'rate', good: 'high', more: 'a better duel score', less: 'a worse duel score' },
  draw: { label: 'Duels drawn', kind: 'rate', hideZero: true },
  first: { label: 'Duels where you reacted first', kind: 'rate' },
  score_first: { hidden: true },
  score_second: { hidden: true },
  deaths_per_hour: { label: 'Deaths per hour', kind: 'num', digits: 2, good: 'low' },
  dps_in_combat: { label: 'Damage per second while fighting', kind: 'num', digits: 1, good: 'high' },
  time_per_kill: { label: 'Seconds of fighting per kill', kind: 'num', digits: 1, good: 'low' },
  combat_share: { label: 'Time spent fighting', kind: 'pct' },
  rest_share: { label: 'Time spent eating and drinking', kind: 'pct', good: 'low' },
  dead_share: { label: 'Time spent dead', kind: 'pct', good: 'low' },
  taken_per_kill: { label: 'Damage taken per kill', kind: 'num', digits: 0, good: 'low' },
  healed_per_kill: { label: 'Self-healing per kill', kind: 'num', digits: 0, good: 'high' },
  eats_per_hour: { label: 'Meals per hour', kind: 'num', digits: 1, good: 'low' },
  drinks_per_hour: { label: 'Drinks per hour', kind: 'num', digits: 1, good: 'low', hideZero: true },
  stalled_pulls: { label: 'Fights that hit the time limit', kind: 'num', digits: 2, good: 'low', hideZero: true },
  approach_share: { label: 'Time spent walking up to mobs', kind: 'pct', good: 'low' },
  interrupts_per_hour: { label: 'Casts interrupted per hour', kind: 'num', digits: 1, good: 'high', hideZero: true },
  escapes_per_hour: { label: 'Fights escaped per hour', kind: 'num', digits: 2, hideZero: true },
  engine_spins: { hidden: true },
  held_share: { label: 'Time the tank held the boss', kind: 'pct', good: 'high' },
  pulled_aggro: { label: 'Times you pulled the boss off the tank', kind: 'num', digits: 2, good: 'low', hideZero: true },
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
  heal_breakeven: { label: 'Boss damage per second you heal through', short: 'Damage you heal through', kind: 'num', digits: 0, good: 'high',
    more: 'more boss damage per second', less: 'less boss damage per second', verb: 'heals through' },
  heal_breakeven_scale: { hidden: true },
  holds_standard: { hidden: true },
  held: { hidden: true },
  time_held: { hidden: true },
  overheal_share: { label: 'Overhealing', kind: 'pct', good: 'low' },
  shield_share: { label: 'Healing done by shields', kind: 'pct', hideZero: true },
  tank_share: { label: 'Healing on the tank', kind: 'pct' },
  self_share: { label: 'Healing on yourself', kind: 'pct' },
  taken_per_sec: { label: 'Damage the party takes per second', kind: 'num', digits: 0 },
  mana_left: { label: 'Mana left at the end', kind: 'pct' },
  deaths: { label: 'Party members lost', kind: 'num', digits: 2, good: 'low', hideZero: true },
};

export const GROUPS = {
  dmg_share: { title: 'Where the damage comes from', kind: 'pct' },
  dps: { title: 'Damage per second by ability', kind: 'num' },
  heal_per_kill: { title: 'Healing per kill by source', kind: 'num' },
  heal: { title: 'Healing by source', kind: 'num' },
  casts_per_kill: { title: 'Casts per kill', kind: 'num' },
  casts_per_min: { title: 'Casts per minute', kind: 'num' },
  kills_share: { title: 'Kills by kind of mob', kind: 'pct' },
  time_per_kill: { title: 'Seconds per kill by kind of mob', kind: 'num' },
  deaths_by: { title: 'Deaths per hour by kind of mob', kind: 'num' },
  hps: { title: 'Healing per second by spell', kind: 'num' },
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
