// Forever Sim front end: build a character, pick a test, read the results.
import { startEngine } from './engine.js';
import * as M from './meta.js';
import { h, svgIcon, art, ability, loadIconManifest, retargetTip, hideTip, withTip, tt, flash, fmtNum, fmtSec, debounce, tipIsPinned, repositionTip } from './ui.js';
import { prepareClass, renderTrees, learnBlock, unlearnBlock, budgetFor, spent } from './talents.js';
import { renderResults, emptyResults } from './results.js';

const STORE_KEY = 'forever-sim/v1';
const MAX_BUILDS = 3;
const DEFAULT_PRESET = 'warrior/vampire_v2.toml';

let engine;
let catalog;
let uidSeq = 1;

const S = {
  builds: [],
  active: 0,
  scenario: 'solo_1',
  accuracy: 'standard',
  opponent: `preset:${DEFAULT_PRESET}`,
  opts: {},
  tab: 'talents',
  tree: null,
  sheets: new Map(),
  run: null,
  result: null,
};

// ------------------------------------------------------------------ lookups

const classOf = (id) => catalog.classes.find((c) => c.id === id);
const raceOf = (id) => catalog.races.find((r) => r.id === id);
const presetOf = (file) => catalog.presets.find((p) => p.file === file);
const presetsFor = (cls) => catalog.presets.filter((p) => p.class === cls);
const baseName = (name) => name.replace(/\s*\([\d/+]+\)\s*$/, '').replace(/\+$/, '+');
const active = () => S.builds[S.active];
const scenarioMeta = (id) => M.SCENARIOS.find((s) => s.id === id) || { id, title: id, unit: 'fight', icon: 'crossed-swords', help: '' };
const accuracy = () => M.ACCURACY.find((a) => a.id === S.accuracy) || M.ACCURACY[1];
const classColor = (cls) => M.CLASS_TEXT[cls] || '#ddd';

function buildFromPreset(p, level) {
  return {
    uid: uidSeq++, cls: p.class, race: p.race, level: level || p.level || 60,
    talents: { ...p.talents }, plan: null, gear: p.gear, rotation: p.rotation,
    preset: p.file, order: p.order || null, popts: p.opts || null, edited: false, customName: null, seq: 0,
  };
}

function emptyBuild(cls, level) {
  const p = presetsFor(cls)[0];
  const c = classOf(cls);
  return {
    uid: uidSeq++, cls, race: p ? p.race : c.races[0], level: level || 60, talents: {}, plan: null,
    gear: p ? p.gear : catalog.gear[0].id, rotation: p ? p.rotation : c.rotations[0].id,
    preset: null, order: null, popts: null, edited: false, customName: null, seq: 0,
  };
}

function autoName(b) {
  const p = b.preset && presetOf(b.preset);
  if (p) return baseName(p.name) + (b.edited ? ' (edited)' : '');
  const prep = prepareClass(classOf(b.cls));
  const split = prep.trees.map((tr) => tr.talents.reduce((a, t) => a + (b.talents[t.id] || 0), 0)).join('/');
  return `${classOf(b.cls).name} ${split}`;
}

function names() {
  const seen = new Map();
  return S.builds.map((b) => {
    let n = (b.customName || autoName(b)).trim() || 'Unnamed build';
    const k = n.toLowerCase();
    const c = (seen.get(k) || 0) + 1;
    seen.set(k, c);
    if (c > 1) n = `${n} #${c}`;
    return n;
  });
}

function apiBuild(b, name, talents = b.plan || b.talents) {
  const out = { name, class: b.cls, race: b.race, level: b.level, talents, gear: b.gear, rotation: b.rotation };
  if (b.order && !b.edited) out.order = b.order;
  if (b.popts) out.opts = b.popts;
  return out;
}

// ------------------------------------------------------------------ character sheet (server-side validation)

async function refreshSheet(b, { announceTrim = false } = {}) {
  const seq = ++b.seq;
  const name = names()[S.builds.indexOf(b)] || 'Build';
  let res;
  try {
    res = await engine.call({ op: 'sheet', build: apiBuild(b, name), opts: S.opts });
  } catch (e) {
    res = { error: e.message };
  }
  if (seq !== b.seq) return;
  S.sheets.set(b.uid, res);
  if (!res.error && res.talents) {
    const before = spent(b.talents);
    const now = spent(res.talents);
    if (JSON.stringify(sorted(res.talents)) !== JSON.stringify(sorted(b.talents))) {
      b.talents = res.talents;
      if (announceTrim && b.plan && now < spent(b.plan) && now < before) {
        flash(`Level ${b.level} has ${budgetFor(b.level)} talent points, so the last ${spent(b.plan) - now} planned points are left out.`, 'info');
      }
    }
    if (b.plan && spent(b.plan) <= spent(b.talents)) b.plan = null;
  }
  render();
}
const sorted = (o) => Object.fromEntries(Object.entries(o).filter(([, v]) => v > 0).sort());
const sheetTimers = new Map();
function refreshLater(b, o) {
  clearTimeout(sheetTimers.get(b.uid));
  sheetTimers.set(b.uid, setTimeout(() => refreshSheet(b, o), 140));
}

// ------------------------------------------------------------------ state changes

function changed(b, { sheet = true, announceTrim = false } = {}) {
  if (sheet) refreshLater(b, { announceTrim });
  render();
}

function learn(t, viaTouch) {
  const b = active();
  const prep = prepareClass(classOf(b.cls));
  const why = learnBlock(prep, b.talents, t, budgetFor(b.level));
  if (why) { flash(why); return; }
  b.talents = { ...b.talents, [t.id]: (b.talents[t.id] || 0) + 1 };
  b.plan = null; b.edited = true;
  changed(b);
  afterTalentChange(t, viaTouch);
}

function unlearn(t, viaTouch) {
  const b = active();
  const prep = prepareClass(classOf(b.cls));
  const why = unlearnBlock(prep, b.talents, t);
  if (why) { flash(why); return; }
  const r = (b.talents[t.id] || 0) - 1;
  const next = { ...b.talents };
  if (r > 0) next[t.id] = r; else delete next[t.id];
  b.talents = next;
  b.plan = null; b.edited = true;
  changed(b);
  afterTalentChange(t, viaTouch);
}

function afterTalentChange(t, viaTouch) {
  const btn = document.querySelector(`.talent[data-id="${t.id}"]`);
  if (!btn) return;
  btn.classList.add('bump');
  if (viaTouch || btn.matches(':hover')) btn.dispatchEvent(new CustomEvent('showtip', { detail: { pinned: !!viaTouch } }));
  if (document.activeElement === document.body) btn.focus({ preventScroll: true });
}

function pickClass(cls) {
  const b = active();
  if (b.cls === cls) return;
  const p = presetsFor(cls)[0];
  const nb = p ? buildFromPreset(p, b.level) : emptyBuild(cls, b.level);
  nb.uid = b.uid;
  S.builds[S.active] = nb;
  S.tree = null;
  changed(nb);
}

function pickPreset(file) {
  const b = active();
  let nb;
  if (file === '') {
    nb = { ...b, talents: {}, plan: null, preset: null, order: null, popts: null, edited: false, customName: null };
  } else {
    nb = buildFromPreset(presetOf(file), b.level);
    nb.uid = b.uid;
  }
  S.builds[S.active] = nb;
  S.tree = null;
  changed(nb);
}

function setLevel(level) {
  const b = active();
  if (b.level === level) return;
  if (!b.plan) b.plan = { ...b.talents };
  b.level = level;
  changed(b, { announceTrim: true });
}

function addBuild() {
  if (S.builds.length >= MAX_BUILDS) { flash(`You can compare up to ${MAX_BUILDS} builds.`); return; }
  const b = active();
  const copy = structuredClone({ ...b, seq: 0 });
  copy.uid = uidSeq++;
  S.builds.push(copy);
  S.active = S.builds.length - 1;
  flash('Copied. Change something in this build, then simulate to compare.', 'info');
  changed(copy);
}

function removeBuild(i) {
  if (S.builds.length <= 1) return;
  const [gone] = S.builds.splice(i, 1);
  S.sheets.delete(gone.uid);
  if (S.opponent === `build:${gone.uid}`) S.opponent = `preset:${DEFAULT_PRESET}`;
  S.active = Math.min(S.active, S.builds.length - 1);
  render();
}

// ------------------------------------------------------------------ rendering

const $ = (id) => document.getElementById(id);

function render() {
  if (!catalog) return;
  renderBuilder();
  renderTester();
  retargetTip();
  persist();
}

function renderBuilder() {
  const root = $('builder-body');
  const b = active();
  const c = classOf(b.cls);
  const nm = names();
  const sheet = S.sheets.get(b.uid);
  const prep = prepareClass(c);
  const budget = budgetFor(b.level);
  const used = spent(b.talents);
  const cc = classColor(b.cls);

  // Builds bar: one tab per build, plus "compare"
  const tabs = h('div.build-bar', { role: 'tablist', 'aria-label': 'Builds' },
    S.builds.map((x, i) => h('div.build-tab', {
      role: 'tab', tabindex: '0', 'aria-selected': String(i === S.active), style: { '--cc': M.CLASS_COLORS[x.cls] },
      onclick: () => { S.active = i; S.tree = null; render(); },
      onkeydown: (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); S.active = i; S.tree = null; render(); } },
    },
    h('span.tab-icon', art(M.CLASS_ICONS[x.cls], classOf(x.cls).icon)),
    h('span.tab-name', nm[i]),
    S.builds.length > 1 ? h('span.x', {
      role: 'button', tabindex: '0', 'aria-label': `Remove ${nm[i]}`, title: 'Remove this build',
      onclick: (e) => { e.stopPropagation(); removeBuild(i); },
      onkeydown: (e) => { if (e.key === 'Enter' || e.key === ' ') { e.preventDefault(); e.stopPropagation(); removeBuild(i); } },
    }, '×') : null)),
    S.builds.length < MAX_BUILDS
      ? h('button.add-build', { type: 'button', onclick: addBuild }, 'Compare with another build')
      : null);

  // Hero: portrait, editable name, who they are, points per tree
  const nameInput = h('input.name-input', {
    type: 'text', value: b.customName || autoName(b), 'aria-label': 'Build name', maxlength: '48', spellcheck: 'false',
    oninput: (e) => {
      b.customName = e.target.value.trim() && e.target.value !== autoName(b) ? e.target.value : null;
      const tab = root.querySelector('.build-tab[aria-selected="true"] .tab-name');
      if (tab) tab.textContent = names()[S.active];
      persist();
    },
    onchange: () => render(),
  });
  const race = raceOf(b.race);
  const treeStyle = M.TREE_STYLE[b.cls] || [];
  const spec = h('div.spec', { 'aria-label': 'Talent points per tree' }, prep.trees.map((tr) => {
    const pts = tr.talents.reduce((a, t) => a + (b.talents[t.id] || 0), 0);
    const [, glyph, wow] = treeStyle[tr.index] || [];
    return h('span.spec-chip', { class: pts ? '' : 'empty' }, ability(glyph || 'rune-stone', null, wow), tr.name, h('b', String(pts)));
  }));
  const head = h('div.hero',
    h('div.portrait', { style: { '--cc': M.CLASS_COLORS[b.cls] } },
      h('span.face', art(M.RACE_ICONS[b.race] || 'rune-stone', race?.icon)),
      h('span.crest', art(M.CLASS_ICONS[b.cls], c.icon))),
    h('div.hero-text', nameInput,
      h('p.hero-sub', `Level ${b.level} ${race?.name || ''} `, h('span.cls', { style: { '--cc': cc } }, c.name)),
      spec));

  // Class: icons, with the chosen one named beside them
  const classRow = h('div.field-row', h('div.label', 'Class'),
    h('div.picker',
      h('div.picker-icons', { role: 'group', 'aria-label': 'Class' }, catalog.classes.map((x) => withTip(
        h('button.pick', {
          type: 'button', 'aria-pressed': String(x.id === b.cls), 'aria-label': x.name, style: { '--cc': M.CLASS_COLORS[x.id] },
          onclick: () => pickClass(x.id),
        }, art(M.CLASS_ICONS[x.id], x.icon)),
        () => [tt.title(x.name), tt.line(M.CLASS_BLURB[x.id] || '', 'gold')]))),
      h('div.picker-note', { style: { '--cc': cc } }, h('b', c.name), h('span', M.CLASS_BLURB[b.cls] || ''))));

  // Race: Alliance, then Horde
  const races = catalog.races.filter((r) => c.races.includes(r.id));
  const raceButton = (r) => withTip(
    h('button.pick', {
      type: 'button', 'aria-pressed': String(r.id === b.race), 'aria-label': r.name,
      onclick: () => { if (b.race !== r.id) { b.race = r.id; changed(b); } },
    }, art(M.RACE_ICONS[r.id] || 'rune-stone', r.icon)),
    () => [tt.title(r.name), tt.line(r.faction === 'alliance' ? 'Alliance' : 'Horde', 'grey'),
      ...r.passives.flatMap((p) => [tt.gap(p, 'gold'), M.RACE_BLURB[p] ? tt.line(M.RACE_BLURB[p]) : null])]);
  const alliance = races.filter((r) => r.faction === 'alliance').map(raceButton);
  const horde = races.filter((r) => r.faction === 'horde').map(raceButton);
  const raceNote = race ? race.passives.map((p) => (M.RACE_BLURB[p] ? `${p}: ${M.RACE_BLURB[p].replace(/\.$/, '').toLowerCase()}` : p)).join('. ') + '.' : '';
  const raceRow = h('div.field-row', h('div.label', 'Race'),
    h('div.picker',
      h('div.picker-icons', { role: 'group', 'aria-label': 'Race' }, alliance, alliance.length && horde.length ? h('span.sep', { 'aria-hidden': 'true' }) : null, horde),
      h('div.picker-note', h('b', `${race?.name || ''}`), h('span', `${race?.faction === 'alliance' ? 'Alliance' : 'Horde'}. ${raceNote}`))));

  // Level
  const lvlValue = h('output.level-value', String(b.level));
  const slider = h('input.range', {
    type: 'range', min: '10', max: '60', step: '1', value: String(b.level), 'aria-label': 'Level',
    style: { '--p': `${((b.level - 10) / 50) * 100}%` },
    oninput: (e) => {
      const v = Number(e.target.value);
      lvlValue.textContent = v;
      e.target.style.setProperty('--p', `${((v - 10) / 50) * 100}%`);
    },
    onchange: (e) => setLevel(Number(e.target.value)),
  });
  const levelRow = h('div.field-row', h('div.label', 'Level'),
    h('div.level', lvlValue, slider,
      h('div.quick', [20, 40, 60].map((l) => h('button.chip', { type: 'button', 'aria-pressed': String(b.level === l), onclick: () => setLevel(l) }, String(l))))));

  // Talents to start from
  const presets = presetsFor(b.cls);
  const p = b.preset && presetOf(b.preset);
  const presetRow = h('div.field-row', h('label.label', { for: 'preset' }, 'Start from'),
    h('div.field',
      h('select.select#preset', { onchange: (e) => pickPreset(e.target.value) },
        presets.map((x) => h('option', { value: x.file, selected: x.file === b.preset }, baseName(x.name))),
        h('option', { value: '', selected: !b.preset }, 'No talents (start from scratch)')),
      h('p.note', p ? (b.edited ? `${p.note} You have changed its talents.` : p.note) : 'Spend your own talent points below.')));

  // Inner tabs
  const tabDefs = [['talents', 'Talents'], ['sheet', 'Character sheet'], ['gear', 'Gear and play style']];
  const inner = h('div.tabs', { role: 'tablist' }, tabDefs.map(([id, label]) => h('button.tab', {
    type: 'button', role: 'tab', 'aria-selected': String(S.tab === id), onclick: () => { S.tab = id; render(); },
  }, label)));

  let panel;
  if (S.tab === 'talents') {
    const left = budget - used;
    const planned = b.plan ? spent(b.plan) : 0;
    panel = h('div.tab-panel',
      h('div.talent-bar',
        h('div.points-left', { class: left === 0 ? 'none' : '' }, h('b', String(left)), ` of ${budget} talent points left`),
        h('div.talent-tools',
          h('span.talent-help.for-mouse', 'Click to learn, right-click to unlearn.'),
          h('span.talent-help.for-touch', 'Tap a talent to learn or unlearn it.'),
          h('button.btn.btn-dark.btn-small', {
            type: 'button', disabled: used === 0,
            onclick: () => { b.talents = {}; b.plan = null; b.edited = !!b.preset; changed(b); },
          }, 'Reset talents'))),
      planned > used ? h('div.notice', `Showing the first ${used} of ${planned} planned points, in levelling order. Raise the level to see the rest. `,
        h('button.linkish', { type: 'button', onclick: () => { b.plan = null; render(); } }, 'Forget the rest')) : null,
      renderTrees({
        prep, talents: b.talents, budget, activeTree: S.tree ?? mainTree(prep, b.talents),
        onLearn: learn, onUnlearn: unlearn, onPickTree: (i) => { S.tree = i; render(); },
      }),
      h('div.talent-legend',
        h('span', h('i.g'), 'Can learn'), h('span', h('i.y'), 'Fully learned'),
        h('span', h('i.k'), 'Not simulated yet (no effect on results)')),
      sheet && sheet.error ? h('div.warn', sheet.error) : null);
  } else if (S.tab === 'sheet') {
    panel = h('div.tab-panel', sheetPanel(b, sheet));
  } else {
    panel = h('div.tab-panel', gearPanel(b));
  }

  const focusedId = document.activeElement?.closest?.('.talent')?.dataset.id;
  const hadNameFocus = document.activeElement === root.querySelector('.name-input');
  root.replaceChildren(...[tabs, head, h('div.fields', classRow, raceRow, levelRow, presetRow), inner, panel].filter(Boolean));
  if (focusedId) root.querySelector(`.talent[data-id="${focusedId}"]`)?.focus({ preventScroll: true });
  if (hadNameFocus) root.querySelector('.name-input')?.focus({ preventScroll: true });
}

// The tree with the most points: shown first on small screens.
function mainTree(prep, talents) {
  let best = 0;
  let most = -1;
  for (const tr of prep.trees) {
    const n = tr.talents.reduce((a, t) => a + (talents[t.id] || 0), 0);
    if (n > most) { most = n; best = tr.index; }
  }
  return best;
}

function sheetPanel(b, sheet) {
  if (!sheet) return h('p.hint', 'Working out the character sheet…');
  if (sheet.error) return h('div.warn', sheet.error);
  const rows = Object.fromEntries(sheet.rows.map((r) => [r.stat, r]));
  const fmtStat = (k, v) => {
    if (k.endsWith(' x')) return `${v >= 1 ? '+' : '−'}${fmtNum(Math.abs(v - 1) * 100, 0)}%`;
    if (k.endsWith('%')) return `${fmtNum(v, 1)}%`;
    if (k.endsWith('speed')) return `${fmtNum(v, 2)} sec`;
    if (k.endsWith('dps')) return fmtNum(v, 1);
    return fmtNum(v, 0);
  };
  const boxes = [];
  const usesMana = classOf(b.cls).resources.includes('mana');
  const hasRanged = (rows['ranged dps']?.base || 0) > 0;
  for (const [title, keys] of M.SHEET) {
    if (title === 'Spells' && !usesMana) continue;
    if (title === 'Ranged' && !hasRanged) continue;
    const shown = keys.filter((k) => {
      const r = rows[k];
      if (!r) return false;
      if (k.endsWith(' x')) return Math.abs(r.base - 1) > 1e-6 || Math.abs(r.buffed - 1) > 1e-6;
      return Math.abs(r.base) > 1e-6 || Math.abs(r.buffed) > 1e-6;
    });
    if (!shown.length) continue;
    boxes.push(h('div.sheet-box', h('h4', title), shown.map((k) => {
      const r = rows[k];
      const diff = r.buffed - r.base;
      const worseIfUp = k === 'damage taken x' || k.endsWith('speed');
      const cls = Math.abs(diff) < 1e-6 ? '' : (diff > 0) !== worseIfUp ? 'up' : 'down';
      const v = h('span.v', { class: cls }, fmtStat(k, r.buffed));
      if (cls) withTip(v, () => [tt.title(M.STAT_LABELS[k] || k), tt.line(`${fmtStat(k, r.base)} before self-buffs`, 'gold'), tt.line(`Self-buffs: ${sheet.auras.join(', ')}`, 'grey')]);
      return h('div.stat-row', h('span.k', M.STAT_LABELS[k] || k), v);
    })));
  }
  const race = raceOf(b.race);
  return [
    h('div.sheet', boxes),
    h('div.section', h('h3', 'Racial traits'),
      h('div.chips-list', race.passives.map((p) => h('span.pill', M.RACE_BLURB[p] ? `${p}: ${M.RACE_BLURB[p]}` : p)))),
    sheet.auras.length ? h('div.section', h('h3', 'Self-buffs before a fight'), h('div.chips-list', sheet.auras.map((a) => h('span.pill', a)))) : null,
    sheet.unmodelled.length ? h('div.section', h('h3', 'Talents the simulator ignores'),
      h('p.hint', 'These have points in them but are not simulated yet, so they make no difference to the results.'),
      h('div.chips-list', sheet.unmodelled.map((a) => h('span.pill.grey', a)))) : null,
    h('p.note', { style: { marginTop: '18px' } }, 'Green numbers include buffs the character casts on itself before a fight. Hover over them to see the value without.'),
  ];
}

function gearPanel(b) {
  const c = classOf(b.cls);
  const g = catalog.gear.find((x) => x.id === b.gear) || catalog.gear[0];
  const rot = c.rotations.find((r) => r.id === b.rotation) || c.rotations[0];
  const [rTitle, rDetail] = M.splitDesc(rot.desc);
  const gearTitle = (x) => M.GEAR_TITLES[x.id] || M.splitDesc(x.desc)[0];
  const gearSel = h('select.select', { 'aria-label': 'Gear', onchange: (e) => { b.gear = e.target.value; changed(b); } },
    catalog.gear.map((x) => h('option', { value: x.id, selected: x.id === b.gear }, gearTitle(x))));
  const rotSel = h('select.select', { 'aria-label': 'Play style', onchange: (e) => { b.rotation = e.target.value; changed(b); } },
    c.rotations.map((r) => h('option', { value: r.id, selected: r.id === b.rotation }, M.splitDesc(r.desc)[0])));
  return [
    h('div.section', h('h3', 'Gear'),
      h('div.choice-card', ability(M.gearIcon(g.id), ['#d9c49a', '#2d2416'], M.gearWowIcon(g.id)),
        h('div.grow', gearSel, h('p.note', `${g.desc} Stats are a typical set for the character's level.`)))),
    h('div.section', h('h3', 'Play style'),
      h('div.choice-card', ability(M.CLASS_ICONS[b.cls], ['#d9c49a', '#2d2416'], c.icon),
        h('div.grow', rotSel, h('p.note', rDetail ? `In order of priority: ${rDetail.replace(/\.$/, '')}.` : rTitle)))),
    h('p.note', { style: { marginTop: '18px' } }, 'The simulated player follows the play style perfectly, with the reaction time of a good human player.'),
  ];
}

// ------------------------------------------------------------------ test panel

function renderTester() {
  const root = $('tester-body');
  const sc = scenarioMeta(S.scenario);
  const nm = names();

  const cards = h('div.scenarios', { role: 'radiogroup', 'aria-label': 'Test' }, M.SCENARIOS
    .filter((s) => catalog.scenarios.some((x) => x.id === s.id))
    .map((s) => h('button.scenario', {
      type: 'button', role: 'radio', 'aria-checked': String(s.id === S.scenario),
      onclick: () => { S.scenario = s.id; render(); },
    }, ability(s.icon, null, s.wow), h('b', s.title), h('span', s.help))));

  let opponent = null;
  if (S.scenario === 'duel') {
    const groups = catalog.classes.map((c) => h('optgroup', { label: c.name },
      presetsFor(c.id).map((p) => h('option', { value: `preset:${p.file}`, selected: S.opponent === `preset:${p.file}` }, baseName(p.name)))));
    const yours = h('optgroup', { label: 'Your builds' }, S.builds.map((b, i) =>
      h('option', { value: `build:${b.uid}`, selected: S.opponent === `build:${b.uid}` }, nm[i])));
    opponent = h('div.section', h('h3', 'Opponent'),
      h('select.select', { 'aria-label': 'Opponent', onchange: (e) => { S.opponent = e.target.value; render(); } }, yours, groups),
      h('p.note', 'The opponent fights at the same level as your first build.'));
  }

  const acc = h('div.section', h('h3', 'Accuracy'),
    h('div.segmented', { role: 'group', 'aria-label': 'Accuracy' }, M.ACCURACY.map((a) => h('button', {
      type: 'button', 'aria-pressed': String(a.id === S.accuracy), onclick: () => { S.accuracy = a.id; render(); },
    }, h('b', a.title), h('span', a.note)))),
    h('p.note', 'More accurate runs simulate more fights and take longer.'));

  const n = S.builds.length;
  const running = !!S.run && !S.run.finished;
  const levels = [...new Set(S.builds.map((b) => b.level))];
  const btn = h('button.primary#simulate', { type: 'button', disabled: running, onclick: simulate },
    svgIcon('rolling-dices'), running ? 'Simulating…' : n > 1 ? `Simulate ${n} builds` : 'Simulate');
  const summary = h('p.run-summary', n > 1
    ? `Compares ${nm.join(n > 2 ? ', ' : ' and ').replace(/, ([^,]*)$/, ' and $1')} on the same dice rolls.`
    : `Runs ${nm[0]} through “${sc.title}”.`,
  levels.length > 1 ? ' Your builds are at different levels.' : '');

  const runArea = h('div.run-area', btn, summary, castbar());

  let results;
  if (S.result) {
    const stale = S.result.sig !== runSignature();
    results = renderResults(S.result.res, { ...S.result.ctx, stale, onPrecise: () => { S.accuracy = 'precise'; simulate(); } });
  } else if (S.runError) {
    results = h('div.warn', S.runError);
  } else {
    results = emptyResults();
  }

  const resultsBox = $('results-box');
  root.replaceChildren(...[cards, opponent, acc, assumptions(), runArea].filter(Boolean));
  if (!running || !resultsBox.firstChild) resultsBox.replaceChildren(results);
}

function castbar() {
  const r = S.run;
  if (!r) return null;
  const cls = r.failed ? 'failed' : r.finished ? 'done' : '';
  const pct = r.failed ? 100 : r.finished ? 100 : Math.min(99, Math.round(100 * r.frac));
  const unit = M.UNIT_PLURAL[scenarioMeta(r.scenario).unit] || 'fights';
  const left = r.failed ? 'Stopped' : r.finished ? 'Done' : scenarioMeta(r.scenario).title;
  const right = r.finished && !r.failed
    ? `${fmtNum(r.reps)} ${unit} in ${fmtSec(r.ms / 1000)}`
    : `${fmtNum(r.reps)} ${unit}`;
  return h('div.castbar-wrap',
    h('div.progress-label', h('b', left), h('span.num', right)),
    h('div.castbar', { class: cls, role: 'progressbar', 'aria-valuemin': '0', 'aria-valuemax': '100', 'aria-valuenow': String(pct), 'aria-label': 'Simulation progress' },
      h('div.fill', { style: { width: `${pct}%` } })),
    !r.finished ? h('div.cancel-row', h('button.btn.btn-small', { type: 'button', onclick: cancel }, 'Cancel')) : null);
}

function relevantOptions() {
  const tags = new Set();
  for (const b of S.builds) { tags.add(b.cls); tags.add(b.race); }
  if (S.scenario === 'duel') {
    const o = opponentBuildRef();
    if (o) { tags.add(o.class || o.cls); tags.add(o.race); }
  }
  return catalog.options.filter((o) => {
    const scope = M.OPTION_SCOPE[o.name];
    return !scope || scope.some((t) => tags.has(t)) || o.name in S.opts;
  });
}

function setOpt(o, x) {
  if (!isFinite(x)) return;
  if (Math.abs(x - o.default) < 1e-12) delete S.opts[o.name]; else S.opts[o.name] = x;
  S.builds.forEach((b) => refreshLater(b));
  render();
}

function assumptions() {
  const list = relevantOptions();
  if (!list.length) return null;
  const changedN = list.filter((o) => o.name in S.opts).length;
  const fmtDefault = (o, ui) => ui.kind === 'toggle' ? (o.default ? 'on' : 'off') : ui.kind === 'pct' ? `${+(o.default * 100).toFixed(4)}%` : `${o.default}${ui.unit && ui.unit !== '%' ? ` ${ui.unit}` : ui.unit || ''}`;
  return h('details.assumptions', { open: S.optsOpen || null, ontoggle: (e) => { S.optsOpen = e.target.open; } },
    h('summary', `Assumptions${changedN ? ` (${changedN} changed)` : ''}`),
    h('p.note', 'Some numbers the game does not publish. These are the simulator’s best guesses; change one to see how much it matters. Only the ones that affect your builds are listed.'),
    h('div.opt-list', list.map((o) => {
      const ui = M.OPTION_UI[o.name] || { kind: 'num', label: o.desc };
      const v = o.name in S.opts ? S.opts[o.name] : o.default;
      const isChanged = o.name in S.opts;
      const id = `opt-${o.name}`;
      let input;
      if (ui.kind === 'toggle') {
        input = h('input.check', { id, type: 'checkbox', checked: v >= 0.5, onchange: (e) => setOpt(o, e.target.checked ? 1 : 0) });
      } else {
        const shown = ui.kind === 'pct' ? +(v * 100).toFixed(4) : v;
        input = h('span.numwrap',
          h('input.numbox.num', {
            id, type: 'number', step: 'any', min: '0', value: String(shown),
            onchange: (e) => setOpt(o, Number(e.target.value) / (ui.kind === 'pct' ? 100 : 1)),
          }),
          ui.kind === 'pct' || ui.unit === '%' ? h('span.unit', '%') : ui.unit ? h('span.unit', ui.unit) : null);
      }
      return h('div.opt',
        h('label.desc', { for: id, class: isChanged ? 'changed' : '' }, ui.label,
          ui.help ? h('span.opt-help', ui.help) : null),
        input,
        isChanged ? h('button.linkish.reset', { type: 'button', onclick: () => { delete S.opts[o.name]; S.builds.forEach((b) => refreshLater(b)); render(); } }, `Back to the default (${fmtDefault(o, ui)})`) : null);
    })));
}

// ------------------------------------------------------------------ running

function opponentBuildRef() {
  if (S.opponent.startsWith('build:')) {
    const b = S.builds.find((x) => `build:${x.uid}` === S.opponent);
    if (b) return b;
  }
  return presetOf(S.opponent.replace('preset:', '')) || presetOf(DEFAULT_PRESET);
}

function opponentApi() {
  const level = S.builds[0].level;
  if (S.opponent.startsWith('build:')) {
    const i = S.builds.findIndex((x) => `build:${x.uid}` === S.opponent);
    if (i >= 0) return { ...apiBuild(S.builds[i], `${names()[i]} (opponent)`), level };
  }
  const p = presetOf(S.opponent.replace('preset:', '')) || presetOf(DEFAULT_PRESET);
  const { file, note, ...rest } = p;
  return { ...rest, level };
}

function runRequest() {
  const nm = names();
  const a = accuracy();
  const req = {
    op: 'start', scenario: S.scenario, builds: S.builds.map((b, i) => apiBuild(b, nm[i])),
    precision: a.precision, max_reps: a.max_reps, min_reps: 32,
  };
  if (S.scenario === 'duel') req.opponent = opponentApi();
  if (Object.keys(S.opts).length) req.opts = { ...S.opts };
  return req;
}

function runSignature() {
  const { op, ...rest } = runRequest();
  return JSON.stringify(rest);
}

async function simulate() {
  if (S.run && !S.run.finished) return;
  hideTip();
  const req = runRequest();
  const sig = runSignature();
  const nm = names();
  const ctx = {
    scenario: scenarioMeta(S.scenario), accuracy: accuracy(),
    builds: S.builds.map((b, i) => ({
      name: nm[i], color: classColor(b.cls), barColor: b.cls === 'priest' ? '#bcbcbc' : M.CLASS_COLORS[b.cls],
      glyph: M.CLASS_ICONS[b.cls], icon: classOf(b.cls).icon,
    })),
  };
  S.runError = null;
  const run = { scenario: S.scenario, reps: 0, frac: 0, finished: false, failed: false, cancelled: false, t0: performance.now(), ms: 0 };
  S.run = run;
  render();
  try {
    let est = req.max_reps;
    const res = await engine.simulate(req, (st) => {
      run.reps = st.reps;
      const worst = Math.max(0, ...st.precision.filter((x) => x != null));
      if (st.reps >= 32 && worst > 0) est = Math.min(st.max_reps, Math.max(st.reps, st.reps * (worst / st.target) ** 2));
      else est = Math.max(est, st.reps);
      run.frac = Math.max(run.frac, st.reps / Math.max(st.reps, Math.min(est, st.max_reps)));
      updateCastbar();
    }, () => run.cancelled);
    if (!res) {
      run.failed = true;
      run.finished = true;
      render();
      return;
    }
    run.finished = true;
    run.ms = performance.now() - run.t0;
    S.result = { res, ctx, sig };
    render();
    if (window.innerWidth < 1240) $('results-box').scrollIntoView({ behavior: 'smooth', block: 'start' });
  } catch (e) {
    run.finished = true;
    run.failed = true;
    S.runError = `The simulation stopped with an error: ${e.message}`;
    S.result = null;
    render();
  }
}

function updateCastbar() {
  const wrap = document.querySelector('.castbar-wrap');
  const fresh = castbar();
  if (wrap && fresh) wrap.replaceWith(fresh);
}

function cancel() {
  if (S.run && !S.run.finished) S.run.cancelled = true;
}

// ------------------------------------------------------------------ saving and sharing

function serial() {
  return {
    v: 1,
    b: S.builds.map((b) => ({
      c: b.cls, r: b.race, l: b.level, t: b.plan || b.talents, p: b.preset || undefined, e: b.edited ? 1 : undefined,
      g: b.gear, ro: b.rotation, n: b.customName || undefined,
    })),
    a: S.active, s: S.scenario, acc: S.accuracy, o: S.opponent, op: Object.keys(S.opts).length ? S.opts : undefined,
  };
}

function restore(d) {
  if (!d || d.v !== 1 || !Array.isArray(d.b) || !d.b.length) return false;
  const builds = [];
  for (const x of d.b.slice(0, MAX_BUILDS)) {
    const c = classOf(x.c);
    if (!c) continue;
    const p = x.p && presetOf(x.p);
    const talents = Object.fromEntries(Object.entries(x.t || {}).filter(([k, v]) => c.talents.some((t) => t.id === k) && v > 0));
    builds.push({
      uid: uidSeq++, cls: c.id, race: c.races.includes(x.r) ? x.r : c.races[0],
      level: Math.min(60, Math.max(10, Number(x.l) || 60)), talents, plan: { ...talents },
      gear: catalog.gear.some((g) => g.id === x.g) ? x.g : (p ? p.gear : catalog.gear[0].id),
      rotation: c.rotations.some((r) => r.id === x.ro) ? x.ro : c.rotations[0].id,
      preset: p ? p.file : null, order: p && !x.e ? p.order || null : null, popts: p ? p.opts || null : null,
      edited: !!x.e, customName: typeof x.n === 'string' ? x.n.slice(0, 48) : null, seq: 0,
    });
  }
  if (!builds.length) return false;
  S.builds = builds;
  S.active = Math.min(Math.max(0, d.a | 0), builds.length - 1);
  if (M.SCENARIOS.some((s) => s.id === d.s)) S.scenario = d.s;
  if (M.ACCURACY.some((a) => a.id === d.acc)) S.accuracy = d.acc;
  if (typeof d.o === 'string') S.opponent = d.o.startsWith('build:') ? `preset:${DEFAULT_PRESET}` : d.o;
  if (d.op && typeof d.op === 'object') {
    for (const [k, v] of Object.entries(d.op)) if (catalog.options.some((o) => o.name === k) && isFinite(v)) S.opts[k] = Number(v);
  }
  return true;
}

const persist = debounce(() => {
  try { localStorage.setItem(STORE_KEY, JSON.stringify(serial())); } catch { /* private mode: fine */ }
}, 300);

function encode(obj) {
  const bytes = new TextEncoder().encode(JSON.stringify(obj));
  let s = '';
  bytes.forEach((b) => { s += String.fromCharCode(b); });
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

function decode(str) {
  const s = atob(str.replace(/-/g, '+').replace(/_/g, '/'));
  return JSON.parse(new TextDecoder().decode(Uint8Array.from(s, (c) => c.charCodeAt(0))));
}

async function share() {
  const url = `${location.origin}${location.pathname}#b=${encode(serial())}`;
  try {
    await navigator.clipboard.writeText(url);
    flash('Link copied. Anyone who opens it sees these builds and this test.', 'info');
  } catch {
    window.prompt('Copy this link:', url);
  }
}

// ------------------------------------------------------------------ boot

// On small screens the Simulate button is far below the talents: offer a
// floating one whenever the real one is out of sight.
function floatingRun() {
  const fab = h('button.primary.float-run', { type: 'button', hidden: true, onclick: () => {
    $('simulate').scrollIntoView({ behavior: 'smooth', block: 'center' });
    simulate();
  } }, svgIcon('rolling-dices'), 'Simulate');
  document.body.append(fab);
  const narrow = matchMedia('(max-width: 1240px)');
  let visible = true;
  const update = () => { fab.hidden = !narrow.matches || visible || !!(S.run && !S.run.finished); };
  new IntersectionObserver((es) => { visible = es[0].isIntersecting; update(); }).observe($('tester'));
  narrow.addEventListener('change', update);
  setInterval(update, 500);
}

async function boot() {
  $('about-btn').addEventListener('click', () => $('about').showModal());
  $('about-link').addEventListener('click', () => $('about').showModal());
  $('share-btn').addEventListener('click', share);
  $('about-close').addEventListener('click', () => $('about').close());
  document.addEventListener('pointerdown', (e) => {
    if (tipIsPinned() && !e.target.closest('#tooltip') && !e.target.closest('.talent')) hideTip();
  });
  addEventListener('scroll', () => { if (tipIsPinned()) repositionTip(); else hideTip(); }, { passive: true });
  addEventListener('keydown', (e) => { if (e.key === 'Escape') hideTip(); });

  try {
    ({ engine, catalog } = await startEngine());
  } catch (e) {
    $('boot').replaceChildren(h('div.frame', h('h2.frame-title', 'Something went wrong'),
      h('p', 'The simulator could not start in this browser.'),
      h('p.note', `Details: ${e.message}`),
      h('p.note', 'Try a recent version of Chrome, Edge, Firefox or Safari. If you opened the file directly from your computer, use the published site instead.')));
    return;
  }
  await loadIconManifest();
  $('boot').hidden = true;
  $('app').hidden = false;
  $('data-build').textContent = catalog.data_build;
  $('engine-kind').textContent = engine.kind === 'webassembly' ? 'in your browser' : 'on the local test server';

  let ok = false;
  if (location.hash.startsWith('#b=')) {
    try { ok = restore(decode(location.hash.slice(3))); } catch { ok = false; }
    if (ok) history.replaceState(null, '', location.pathname + location.search);
    else flash('That shared link could not be read.');
  }
  if (!ok) {
    try { ok = restore(JSON.parse(localStorage.getItem(STORE_KEY) || 'null')); } catch { ok = false; }
  }
  if (!ok) S.builds = [buildFromPreset(presetOf(DEFAULT_PRESET) || catalog.presets[0])];
  render();
  floatingRun();
  for (const b of S.builds) refreshSheet(b);
}

boot();
