#!/usr/bin/env node
// Builds data/reference/spellbook.json from WoW: Forever's own spell tables,
// as published by wago.tools (DB2 exports of product `wowf`). The spellbook
// holds, for every class spell, racial, talent spell and the spells they trigger:
// level, costs, cast time, cooldowns, duration, range, radius, target caps,
// chain targets, base values with variance and per-level growth, spell and
// attack power coefficients, periodic intervals, proc chances and summons.
// `fsim audit` checks the class files against it.
//
//   node tools/forever_db.mjs [--build 1.60.1.70425] [--cache .db2] [--out data/reference/spellbook.json]
//
// Tables are cached under .db2/<build>/ (gitignored, about 10 MB); only
// missing ones are fetched, a few seconds apart. The raw tables are
// Blizzard's and are not committed; the spellbook keeps only the numbers.
import { readFileSync, writeFileSync, existsSync, mkdirSync, statSync, renameSync } from 'node:fs';
import { join, dirname } from 'node:path';

const args = Object.fromEntries(process.argv.slice(2).reduce((acc, a, i, all) => (a.startsWith('--') ? [...acc, [a.slice(2), all[i + 1]]] : acc), []));
const BUILD = args.build || '1.60.1.70425';
const CACHE = join(args.cache || '.db2', BUILD);
const OUT = args.out || 'data/reference/spellbook.json';
const UA = 'forever-sim data tool (+https://github.com/lovelaced/forever-sim)';

const TABLES = [
  'SpellName', 'SpellEffect', 'SpellMisc', 'SpellLevels', 'SpellPower', 'SpellCooldowns', 'SpellCategories',
  'SpellCastTimes', 'SpellDuration', 'SpellRadius', 'SpellRange', 'SpellAuraOptions', 'SpellProcsPerMinute',
  'SpellTargetRestrictions', 'SpellClassOptions', 'SkillLineAbility', 'SummonProperties', 'SpellShapeshift',
  // tooltip text: cached for `tools/spell.py --text`, never written to the spellbook
  'Spell',
  // interface file names, for each spell's icon (SpellMisc.SpellIconFileDataID)
  'ManifestInterfaceData',
  // Forever's talent trees live in the trait tables (the old Talent table is
  // still Classic's): nodes, ranks, tiers, prerequisites, per-rank values.
  'TraitTree', 'TraitNode', 'TraitNodeEntry', 'TraitNodeXTraitNodeEntry', 'TraitDefinition',
  'TraitDefinitionEffectPoints', 'CurvePoint', 'TraitEdge', 'TraitCond', 'TraitNodeGroupXTraitNode', 'TraitNodeGroupXTraitCond',
];

// SpellClassOptions.SpellClassSet and SkillLineAbility.ClassMask bits.
const FAMILY = { 3: 'mage', 4: 'warrior', 5: 'warlock', 6: 'priest', 7: 'druid', 8: 'rogue', 9: 'hunter', 10: 'paladin', 11: 'shaman' };
// Racial skill lines (SkillLineAbility.SkillLine); both Skyborne races share one.
const RACE_SKILL = { 754: 'human', 101: 'dwarf', 126: 'night_elf', 753: 'gnome', 2980: 'skyborne', 125: 'orc', 220: 'undead', 124: 'tauren', 733: 'troll' };
const CLASS_BIT = { 1: 'warrior', 2: 'paladin', 4: 'hunter', 8: 'rogue', 16: 'priest', 64: 'shaman', 128: 'mage', 256: 'warlock', 1024: 'druid' };
const POWER = { '-2': 'health', 0: 'mana', 1: 'rage', 2: 'focus', 3: 'energy', 4: 'happiness' };
// Names for the effect and aura types the simulator cares about (others stay numeric).
const EFFECT = {
  2: 'school_damage', 3: 'dummy', 6: 'apply_aura', 10: 'heal', 17: 'weapon_damage_noschool', 28: 'summon', 30: 'energize',
  31: 'weapon_percent_damage', 35: 'area_aura_party', 41: 'jump', 58: 'weapon_damage', 62: 'power_burn', 64: 'trigger_spell',
  67: 'heal_max_health', 68: 'interrupt_cast', 74: 'summon_object_slot', 77: 'script', 80: 'add_combo_points', 87: 'summon_object_slot1',
  88: 'summon_object_slot2', 89: 'summon_object_slot3', 90: 'summon_object_slot4', 114: 'attack_me', 119: 'area_aura_pet',
  121: 'normalized_weapon_damage', 128: 'area_aura_friend', 129: 'area_aura_enemy', 136: 'heal_pct', 137: 'energize_pct',
};
const AURA = {
  3: 'periodic_damage', 4: 'dummy', 8: 'periodic_heal', 10: 'mod_threat', 12: 'mod_stun', 13: 'mod_damage_done', 15: 'damage_shield',
  22: 'mod_resistance', 23: 'periodic_trigger_spell', 24: 'periodic_energize', 26: 'mod_root', 27: 'mod_silence', 29: 'mod_stat',
  31: 'mod_increase_speed', 33: 'mod_decrease_speed', 42: 'proc_trigger_spell', 43: 'proc_trigger_damage', 47: 'mod_parry_percent',
  49: 'mod_dodge_percent', 52: 'mod_crit_percent', 53: 'periodic_leech', 54: 'mod_hit_chance', 55: 'mod_spell_hit_chance',
  57: 'mod_spell_crit_chance', 61: 'mod_scale', 65: 'mod_casting_speed', 69: 'school_absorb', 71: 'mod_spell_crit_chance_school',
  79: 'mod_damage_percent_done', 85: 'mod_power_regen', 87: 'mod_damage_percent_taken', 99: 'mod_attack_power',
  107: 'add_flat_modifier', 108: 'add_pct_modifier', 110: 'mod_power_regen_percent', 118: 'mod_healing_pct', 123: 'mod_target_resistance',
  124: 'mod_ranged_attack_power', 135: 'mod_healing_done', 137: 'mod_total_stat_percentage', 138: 'mod_melee_haste',
  140: 'mod_ranged_haste', 142: 'mod_base_resistance_pct', 189: 'mod_rating',
};

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function fetchTable(name) {
  const file = join(CACHE, `${name}.csv`);
  if (existsSync(file) && statSync(file).size > 64 && !/^[<{]/.test(readFileSync(file, 'utf8').slice(0, 1))) return file;
  mkdirSync(CACHE, { recursive: true });
  const url = `https://wago.tools/db2/${name}/csv?build=${BUILD}`;
  process.stderr.write(`fetching ${name} ... `);
  const res = await fetch(url, { headers: { 'User-Agent': UA } });
  const text = await res.text();
  if (!res.ok || text.startsWith('<') || text.startsWith('{')) throw new Error(`${name}: HTTP ${res.status} ${text.slice(0, 80)}`);
  writeFileSync(file, text);
  process.stderr.write(`${(text.length / 1024).toFixed(0)} KiB\n`);
  await sleep(3000);
  return file;
}

// RFC 4180 CSV: quoted fields may hold commas, quotes ("") and newlines.
function parseCsv(text) {
  const rows = [];
  let row = [];
  let field = '';
  let quoted = false;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quoted) {
      if (c === '"') {
        if (text[i + 1] === '"') { field += '"'; i++; } else quoted = false;
      } else field += c;
    } else if (c === '"') quoted = true;
    else if (c === ',') { row.push(field); field = ''; }
    else if (c === '\n' || c === '\r') {
      if (c === '\r' && text[i + 1] === '\n') i++;
      row.push(field); field = '';
      rows.push(row); row = [];
    } else field += c;
  }
  if (field !== '' || row.length) { row.push(field); rows.push(row); }
  const [head, ...body] = rows.filter((r) => r.length > 1 || r[0] !== '');
  return body.map((r) => Object.fromEntries(head.map((h, i) => [h, r[i] ?? ''])));
}

const load = (name) => parseCsv(readFileSync(join(CACHE, `${name}.csv`), 'utf8'));
const num = (v) => (v === '' || v == null ? 0 : Number(v));
const round = (x, d = 4) => Math.round(x * 10 ** d) / 10 ** d;
const byId = (rows) => new Map(rows.map((r) => [r.ID, r]));
const bySpell = (rows) => {
  const m = new Map();
  for (const r of rows) if (num(r.DifficultyID) === 0 && !m.has(r.SpellID)) m.set(r.SpellID, r);
  return m;
};

for (const t of TABLES) await fetchTable(t);

const names = new Map(load('SpellName').map((r) => [r.ID, r.Name_lang]));
// icon file ids to icon names, as the game and wow-ui-textures name them
const iconNames = new Map(load('ManifestInterfaceData')
  .filter((r) => /^interface[\\/]icons[\\/]?$/i.test(r.FilePath))
  .map((r) => [r.ID, r.FileName.replace(/\.[a-z0-9]+$/i, '').toLowerCase()]));
const misc = bySpell(load('SpellMisc'));
const levels = bySpell(load('SpellLevels'));
const cooldowns = bySpell(load('SpellCooldowns'));
const categories = bySpell(load('SpellCategories'));
const auraOpts = bySpell(load('SpellAuraOptions'));
const targets = bySpell(load('SpellTargetRestrictions'));
const classOpts = new Map(load('SpellClassOptions').map((r) => [r.SpellID, r]));
const shapes = new Map(load('SpellShapeshift').map((r) => [r.SpellID, r]));
const castTimes = byId(load('SpellCastTimes'));
const durations = byId(load('SpellDuration'));
const radii = byId(load('SpellRadius'));
const ranges = byId(load('SpellRange'));
const ppm = byId(load('SpellProcsPerMinute'));
const summonProps = byId(load('SummonProperties'));
const power = new Map();
for (const r of load('SpellPower')) {
  if (!power.has(r.SpellID)) power.set(r.SpellID, []);
  power.get(r.SpellID).push(r);
}
const effects = new Map();
for (const r of load('SpellEffect')) {
  if (num(r.DifficultyID) !== 0) continue;
  if (!effects.has(r.SpellID)) effects.set(r.SpellID, []);
  effects.get(r.SpellID).push(r);
}
const skills = new Map();
const classOf = new Map();
const raceOf = new Map();
const raceClasses = new Map();
for (const r of load('SkillLineAbility')) {
  const mask = num(r.ClassMask);
  const race = RACE_SKILL[num(r.SkillLine)];
  if (race) {
    // a racial, sometimes in one version per group of classes
    raceOf.set(r.Spell, race);
    const cls = Object.entries(CLASS_BIT).filter(([bit]) => mask > 0 && mask & Number(bit)).map(([, c]) => c);
    if (cls.length) raceClasses.set(r.Spell, cls);
    continue;
  }
  for (const [bit, cls] of Object.entries(CLASS_BIT)) {
    if (mask & Number(bit)) {
      classOf.set(r.Spell, cls);
      if (!skills.has(r.Spell)) skills.set(r.Spell, new Set());
      skills.get(r.Spell).add(num(r.SkillLine));
    }
  }
}
for (const [id, r] of classOpts) {
  const cls = FAMILY[num(r.SpellClassSet)];
  if (cls && !classOf.has(id)) classOf.set(id, cls);
}

// Every class spell, every spell a talent uses, and (one level deep) what
// they trigger or summon.
const wanted = new Set([...classOf.keys(), ...raceOf.keys()].filter((id) => names.has(id)));
for (const d of load('TraitDefinition')) if (names.has(d.SpellID)) wanted.add(d.SpellID);
for (const id of [...wanted]) {
  for (const e of effects.get(id) || []) {
    const t = e.EffectTriggerSpell;
    if (t && t !== '0' && names.has(t)) wanted.add(t);
  }
}

const seconds = (ms) => round(num(ms) / 1000, 3);
function record(id) {
  const m = misc.get(id) || {};
  const out = { id: Number(id), name: names.get(id) };
  const cls = classOf.get(id);
  if (cls) out.class = cls;
  const race = raceOf.get(id);
  if (race) out.race = race;
  if (raceClasses.has(id)) out.classes = raceClasses.get(id);
  const sk = skills.get(id);
  if (sk) out.skill = [...sk].sort((a, b) => a - b);
  const lv = levels.get(id);
  if (lv) {
    if (num(lv.BaseLevel)) out.level = num(lv.BaseLevel);
    if (num(lv.MaxLevel)) out.maxLevel = num(lv.MaxLevel);
    if (num(lv.SpellLevel)) out.spellLevel = num(lv.SpellLevel);
  }
  if (num(m.SchoolMask)) out.school = num(m.SchoolMask);
  const icon = iconNames.get(m.SpellIconFileDataID);
  if (icon) out.icon = icon;
  const ct = castTimes.get(m.CastingTimeIndex);
  if (ct && num(ct.Base)) out.cast = seconds(ct.Base);
  const du = durations.get(m.DurationIndex);
  if (du && num(du.Duration)) out.duration = num(du.Duration) < 0 ? -1 : seconds(du.Duration);
  const rg = ranges.get(m.RangeIndex);
  if (rg && (num(rg.RangeMax_0) || num(rg.RangeMin_0))) out.range = [num(rg.RangeMin_0), num(rg.RangeMax_0)];
  if (num(m.Speed)) out.speed = num(m.Speed);
  const cd = cooldowns.get(id);
  if (cd) {
    if (num(cd.RecoveryTime)) out.cooldown = seconds(cd.RecoveryTime);
    if (num(cd.CategoryRecoveryTime)) out.categoryCooldown = seconds(cd.CategoryRecoveryTime);
    out.gcd = seconds(cd.StartRecoveryTime);
  }
  const cat = categories.get(id);
  if (cat) {
    if (num(cat.Category)) out.category = num(cat.Category);
    if (num(cat.Mechanic)) out.mechanic = num(cat.Mechanic);
    if (num(cat.DispelType)) out.dispel = num(cat.DispelType);
    if (num(cat.DiminishType)) out.diminish = num(cat.DiminishType);
  }
  const costs = (power.get(id) || []).sort((a, b) => num(a.OrderIndex) - num(b.OrderIndex)).map((p) => {
    const c = { power: POWER[p.PowerType] ?? Number(p.PowerType) };
    if (num(p.ManaCost)) c.amount = num(p.ManaCost);
    if (num(p.PowerCostPct)) c.pctOfBase = round(num(p.PowerCostPct));
    if (num(p.ManaCostPerLevel)) c.perLevel = num(p.ManaCostPerLevel);
    if (num(p.ManaPerSecond)) c.perSecond = num(p.ManaPerSecond);
    return c;
  }).filter((c) => c.amount || c.pctOfBase || c.perSecond);
  if (costs.length) out.cost = costs;
  const tr = targets.get(id);
  if (tr) {
    if (num(tr.MaxTargets)) out.maxTargets = num(tr.MaxTargets);
    if (num(tr.MaxTargetLevel)) out.maxTargetLevel = num(tr.MaxTargetLevel);
    if (num(tr.ConeDegrees)) out.cone = num(tr.ConeDegrees);
  }
  const ao = auraOpts.get(id);
  if (ao) {
    const p = {};
    if (num(ao.ProcChance) && num(ao.ProcChance) < 101) p.chance = num(ao.ProcChance) / 100;
    if (num(ao.ProcCharges)) p.charges = num(ao.ProcCharges);
    if (num(ao.CumulativeAura)) out.maxStacks = num(ao.CumulativeAura);
    const pr = ppm.get(ao.SpellProcsPerMinuteID);
    if (pr) p.ppm = num(pr.BaseProcRate);
    if (num(ao.ProcCategoryRecovery)) p.icd = seconds(ao.ProcCategoryRecovery);
    if (Object.keys(p).length) out.proc = p;
  }
  const co = classOpts.get(id);
  if (co) {
    const fam = [0, 1, 2, 3].map((i) => num(co[`SpellClassMask_${i}`]));
    if (fam.some(Boolean)) out.familyFlags = fam;
  }
  const sh = shapes.get(id);
  if (sh && (num(sh.ShapeshiftMask_0) || num(sh.ShapeshiftMask_1))) out.forms = [num(sh.ShapeshiftMask_0), num(sh.ShapeshiftMask_1)];
  const fx = (effects.get(id) || []).sort((a, b) => num(a.EffectIndex) - num(b.EffectIndex)).map((e) => {
    const t = num(e.Effect);
    const f = { i: num(e.EffectIndex), type: EFFECT[t] || t };
    const aura = num(e.EffectAura);
    if (aura) f.aura = AURA[aura] || aura;
    const base = num(e.EffectBasePointsF);
    if (base) f.base = round(base, 3);
    if (num(e.Variance)) f.variance = round(num(e.Variance));
    if (num(e.EffectRealPointsPerLevel)) f.perLevel = round(num(e.EffectRealPointsPerLevel));
    if (num(e.EffectBonusCoefficient)) f.coef = round(num(e.EffectBonusCoefficient));
    if (num(e.BonusCoefficientFromAP)) f.apCoef = round(num(e.BonusCoefficientFromAP));
    if (num(e.EffectChainTargets)) f.chain = num(e.EffectChainTargets);
    if (num(e.EffectChainAmplitude) && num(e.EffectChainAmplitude) !== 1) f.chainFactor = round(num(e.EffectChainAmplitude));
    if (num(e.EffectAmplitude)) f.amplitude = round(num(e.EffectAmplitude));
    if (num(e.EffectAuraPeriod)) f.period = seconds(e.EffectAuraPeriod);
    if (num(e.EffectPointsPerResource)) f.perCombo = round(num(e.EffectPointsPerResource));
    if (num(e.EffectTriggerSpell)) f.trigger = num(e.EffectTriggerSpell);
    if (num(e.EffectMechanic)) f.mechanic = num(e.EffectMechanic);
    const r0 = radii.get(e.EffectRadiusIndex_0);
    const r1 = radii.get(e.EffectRadiusIndex_1);
    if ((r0 && num(r0.Radius)) || (r1 && num(r1.Radius))) f.radius = round(Math.max(num(r0?.Radius), num(r1?.Radius)), 2);
    const misc0 = num(e.EffectMiscValue_0);
    const misc1 = num(e.EffectMiscValue_1);
    if (misc0 || misc1) f.misc = [misc0, misc1];
    const tg = [num(e.ImplicitTarget_0), num(e.ImplicitTarget_1)];
    if (tg[0] || tg[1]) f.targets = tg;
    const flags = [0, 1, 2, 3].map((i) => num(e[`EffectSpellClassMask_${i}`]));
    if (flags.some(Boolean)) f.affects = flags;
    if (t === 28) {
      const sp = summonProps.get(String(misc1));
      f.summon = { creature: misc0, slot: sp ? num(sp.Slot) : 0, title: sp ? num(sp.Title) : 0 };
    }
    return f;
  });
  if (fx.length) out.effects = fx;
  return out;
}

// ---------------------------------------------------------------- talents

// Every talent node of the nine class trees: name, spec tree, maximum ranks,
// points needed in the tree before it, prerequisite talents, and the value
// of each of its spell's effects at every rank.
function talentTrees() {
  const defs = byId(load('TraitDefinition'));
  const entries = byId(load('TraitNodeEntry'));
  const subtrees = new Map(); // spec tree names: not in this build's tables
  const nodes = load('TraitNode');
  const nodeEntries = new Map();
  for (const r of load('TraitNodeXTraitNodeEntry')) {
    if (!nodeEntries.has(r.TraitNodeID)) nodeEntries.set(r.TraitNodeID, []);
    nodeEntries.get(r.TraitNodeID).push(r.TraitNodeEntryID);
  }
  const curve = new Map();
  for (const r of load('CurvePoint')) {
    if (!curve.has(r.CurveID)) curve.set(r.CurveID, []);
    curve.get(r.CurveID).push([num(r.OrderIndex), num(r.Pos_0), num(r.Pos_1)]);
  }
  const points = new Map();
  for (const r of load('TraitDefinitionEffectPoints')) {
    if (!points.has(r.TraitDefinitionID)) points.set(r.TraitDefinitionID, []);
    const c = (curve.get(r.CurveID) || []).sort((a, b) => a[0] - b[0]).map(([, , y]) => round(y));
    points.get(r.TraitDefinitionID).push({ effect: num(r.EffectIndex), op: num(r.OperationType), ranks: c });
  }
  // points needed: the largest "spent in tree" condition on the node's groups
  const conds = byId(load('TraitCond'));
  const groupConds = new Map();
  for (const r of load('TraitNodeGroupXTraitCond')) {
    if (!groupConds.has(r.TraitNodeGroupID)) groupConds.set(r.TraitNodeGroupID, []);
    groupConds.get(r.TraitNodeGroupID).push(r.TraitCondID);
  }
  const needs = new Map();
  for (const r of load('TraitNodeGroupXTraitNode')) {
    for (const cid of groupConds.get(r.TraitNodeGroupID) || []) {
      const c = conds.get(cid);
      if (c && num(c.CondType) === 0) needs.set(r.TraitNodeID, Math.max(needs.get(r.TraitNodeID) || 0, num(c.SpentAmountRequired)));
    }
  }
  const edges = new Map();
  for (const r of load('TraitEdge')) {
    // type 0 is a line drawn for looks; 2 (unlocks) and 3 (required) are prerequisites
    if (num(r.Type) === 0) continue;
    if (!edges.has(r.RightTraitNodeID)) edges.set(r.RightTraitNodeID, []);
    edges.get(r.RightTraitNodeID).push(r.LeftTraitNodeID);
  }
  const byNode = new Map();
  for (const n of nodes) {
    const entry = (nodeEntries.get(n.ID) || []).map((e) => entries.get(e)).find(Boolean);
    const def = entry && defs.get(entry.TraitDefinitionID);
    if (!def) continue;
    const spell = def.SpellID;
    byNode.set(n.ID, {
      node: num(n.ID), tree: num(n.TraitTreeID), name: def.OverrideName_lang || names.get(spell) || `spell ${spell}`,
      spell: num(spell), spec: subtrees.get(n.TraitSubTreeID) || subtrees.get(entry.TraitSubTreeID) || '',
      ranks: num(entry.MaxRanks), points: needs.get(n.ID) || 0, x: num(n.PosX), y: num(n.PosY),
      values: points.get(def.ID) || [], requiresNodes: edges.get(n.ID) || [],
    });
  }
  // which class each tree belongs to: the class most of its spells have
  const trees = new Map();
  for (const t of byNode.values()) {
    const c = classOf.get(String(t.spell));
    if (!c) continue;
    if (!trees.has(t.tree)) trees.set(t.tree, {});
    trees.get(t.tree)[c] = (trees.get(t.tree)[c] || 0) + 1;
  }
  const out = {};
  for (const [tree, votes] of trees) {
    const [cls, n] = Object.entries(votes).sort((a, b) => b[1] - a[1])[0];
    const all = [...byNode.values()].filter((t) => t.tree === tree);
    if (n < 10 || all.length < 30) continue; // small trees are not class talent trees
    // an edge is a prerequisite only from a shallower talent to a deeper one
    // (the tables also link some talents both ways, which can't be required)
    out[cls] = all.map(({ requiresNodes, tree: _t, x, y, ...t }) => ({
      ...t,
      requires: requiresNodes.map((r) => byNode.get(r)).filter((p) => p && p.points < t.points).map((p) => p.name),
      linked: requiresNodes.map((r) => byNode.get(r)).filter((p) => p && p.points >= t.points).map((p) => p.name),
    })).sort((a, b) => a.spec.localeCompare(b.spec) || a.points - b.points || a.name.localeCompare(b.name));
  }
  return out;
}
const talents = talentTrees();

const ids = [...wanted].sort((a, b) => Number(a) - Number(b));
const lines = ids.map((id) => `    ${JSON.stringify(String(Number(id)))}: ${JSON.stringify(record(id))}`);
const header = {
  build: BUILD,
  source: `wago.tools DB2 export of WoW: Forever (product wowf), build ${BUILD}`,
  note: 'Game data © Blizzard Entertainment. Regenerate with node tools/forever_db.mjs; checked by `fsim audit`.',
};
mkdirSync(dirname(OUT), { recursive: true });
// written whole and then moved into place, so a reader never sees half a file
const talentLines = Object.entries(talents).sort().map(([cls, list]) =>
  `    ${JSON.stringify(cls)}: [\n${list.map((t) => `      ${JSON.stringify(t)}`).join(',\n')}\n    ]`);
writeFileSync(`${OUT}.tmp`, `{\n${Object.entries(header).map(([k, v]) => `  ${JSON.stringify(k)}: ${JSON.stringify(v)},`).join('\n')}\n  "spells": {\n${lines.join(',\n')}\n  },\n  "talents": {\n${talentLines.join(',\n')}\n  }\n}\n`);
renameSync(`${OUT}.tmp`, OUT);
const perClass = {};
for (const id of ids) { const c = classOf.get(id) || (raceOf.has(id) ? 'racial' : 'triggered'); perClass[c] = (perClass[c] || 0) + 1; }
console.log(`wrote ${OUT}: ${ids.length} spells`, perClass, 'talents:', Object.fromEntries(Object.entries(talents).map(([c, l]) => [c, l.length])));
