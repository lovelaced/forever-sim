// The editor for one build: class, race, level, starting talents, the talent
// trees, the character sheet, gear and play style. It folds away while the
// results are read and opens again from a build card.
import * as M from './meta.js';
import { h, art, ability, withTip, tt, flash, fmtNum } from './ui.js';
import { prepareClass, renderTrees, learnBlock, unlearnBlock, budgetFor, spent } from './talents.js';
import {
  S, env, bus, classOf, raceOf, presetOf, presetsFor, baseName, classColor, editingBuild, names, nameOf, slotOf, autoName,
  apiBuild, buildFromPreset, emptyBuild, sig, buildDiff, diffText, saveLocal,
} from './state.js';

export async function refreshSheet(b, { announceTrim = false } = {}) {
  const seq = ++b.seq;
  const name = names()[S.builds.indexOf(b)] || 'Build';
  let res;
  try {
    res = await env.engine.call({ op: 'sheet', build: apiBuild(b, name), opts: S.opts });
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
  bus.render();
}
const sorted = (o) => Object.fromEntries(Object.entries(o).filter(([, v]) => v > 0).sort());
const sheetTimers = new Map();
export function refreshLater(b, o) {
  clearTimeout(sheetTimers.get(b.uid));
  sheetTimers.set(b.uid, setTimeout(() => refreshSheet(b, o), 140));
}

// ------------------------------------------------------------------ state changes

function changed(b, { sheet = true, announceTrim = false } = {}) {
  if (sheet) refreshLater(b, { announceTrim });
  bus.render();
}

function learn(t, viaTouch) {
  const b = editingBuild();
  const prep = prepareClass(classOf(b.cls));
  const why = learnBlock(prep, b.talents, t, budgetFor(b.level));
  if (why) { flash(why); return; }
  b.talents = { ...b.talents, [t.id]: (b.talents[t.id] || 0) + 1 };
  b.plan = null; b.edited = true;
  changed(b);
  afterTalentChange(t, viaTouch);
}

function unlearn(t, viaTouch) {
  const b = editingBuild();
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
  const b = editingBuild();
  if (b.cls === cls) return;
  const p = presetsFor(cls)[0];
  const nb = p ? buildFromPreset(p, b.level) : emptyBuild(cls, b.level);
  nb.uid = b.uid;
  S.builds[S.builds.indexOf(b)] = nb;
  S.tree = null;
  changed(nb);
}

function pickPreset(file) {
  const b = editingBuild();
  let nb;
  if (file === '') {
    nb = { ...b, talents: {}, plan: null, preset: null, order: null, popts: null, edited: false, customName: null };
  } else {
    nb = buildFromPreset(presetOf(file), b.level);
    nb.uid = b.uid;
  }
  S.builds[S.builds.indexOf(b)] = nb;
  S.tree = null;
  changed(nb);
}

function setLevel(level) {
  const b = editingBuild();
  if (b.level === level) return;
  if (!b.plan) b.plan = { ...b.talents };
  b.level = level;
  changed(b, { announceTrim: true });
}


export function renderEditor(root, actions) {
  const b = editingBuild();
  root.hidden = !b;
  document.body.classList.toggle('editor-open', !!b);
  if (!b) { root.replaceChildren(); return; }
  const c = classOf(b.cls);
  const sheet = S.sheets.get(b.uid);
  const prep = prepareClass(c);
  const budget = budgetFor(b.level);
  const used = spent(b.talents);
  const cc = classColor(b.cls);

  // Hero: portrait, editable name, who they are, points per tree
  const nameInput = h('input.name-input', {
    type: 'text', value: b.customName || autoName(b), 'aria-label': 'Build name', maxlength: '48', spellcheck: 'false',
    oninput: (e) => {
      b.customName = e.target.value.trim() && e.target.value !== autoName(b) ? e.target.value : null;
      bus.cards();
      saveLocal();
    },
    onchange: () => bus.render(),
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
      h('div.picker-icons', { role: 'group', 'aria-label': 'Class' }, env.catalog.classes.map((x) => withTip(
        h('button.pick', {
          type: 'button', 'aria-pressed': String(x.id === b.cls), 'aria-label': x.name, style: { '--cc': M.CLASS_COLORS[x.id] },
          onclick: () => pickClass(x.id),
        }, art(M.CLASS_ICONS[x.id], x.icon)),
        () => [tt.title(x.name), tt.line(M.CLASS_BLURB[x.id] || '', 'gold')]))),
      h('div.picker-note', { style: { '--cc': cc } }, h('b', c.name), h('span', M.CLASS_BLURB[b.cls] || ''))));

  // Race: Alliance, then Horde
  const races = env.catalog.races.filter((r) => c.races.includes(r.id));
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
    type: 'button', role: 'tab', 'aria-selected': String(S.tab === id), onclick: () => { S.tab = id; bus.render(); },
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
      planned > used ? h('div.notice', `Showing the first ${used} of ${planned} planned points, in leveling order. Raise the level to see the rest. `,
        h('button.linkish', { type: 'button', onclick: () => { b.plan = null; bus.render(); } }, 'Forget the rest')) : null,
      renderTrees({
        prep, talents: b.talents, budget, activeTree: S.tree ?? mainTree(prep, b.talents),
        onLearn: learn, onUnlearn: unlearn, onPickTree: (i) => { S.tree = i; bus.render(); },
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

  const slot = slotOf(b);
  const top = h('header.editor-head',
    h('h2.editor-title', { id: 'editor-title', tabindex: '-1' }, 'Editing ', h('span.slot', { style: { '--slot': slot.color }, 'aria-hidden': 'true' }, slot.letter), ' ', nameOf(b)),
    h('button.btn.btn-small.editor-done', { type: 'button', onclick: actions.done }, 'Done'));
  const foot = h('footer.editor-foot', pendingChanges(b, sheet),
    h('div.editor-foot-actions',
      h('button.btn', { type: 'button', onclick: actions.done }, 'Done'),
      h('button.primary.small', { type: 'button', onclick: actions.simulate }, 'Simulate')));

  const focusedId = document.activeElement?.closest?.('.talent')?.dataset.id;
  const hadNameFocus = document.activeElement === root.querySelector('.name-input');
  root.replaceChildren(top,
    h('div.editor-body',
      h('div.editor-side', head, h('div.fields', classRow, raceRow, levelRow, presetRow)),
      h('div.editor-main', inner, panel)),
    foot);
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
  const g = env.catalog.gear.find((x) => x.id === b.gear) || env.catalog.gear[0];
  const rot = c.rotations.find((r) => r.id === b.rotation) || c.rotations[0];
  const [rTitle, rDetail] = M.splitDesc(rot.desc);
  const gearTitle = (x) => M.GEAR_TITLES[x.id] || M.splitDesc(x.desc)[0];
  const gearSel = h('select.select', { 'aria-label': 'Gear', onchange: (e) => { b.gear = e.target.value; changed(b); } },
    env.catalog.gear.map((x) => h('option', { value: x.id, selected: x.id === b.gear }, gearTitle(x))));
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

// What changed since this build was last simulated, before simulating it
// again: talents and other choices, and the biggest stat changes on the sheet.
function pendingChanges(b, sheet) {
  const ran = S.ranAs.get(b.uid);
  if (!ran) return h('p.pending', 'Not simulated yet.');
  if (ran.sig === sig(b)) {
    if (!ran.sheet && sheet && !sheet.error) ran.sheet = sheet;
    return h('p.pending', 'No changes since your last run.');
  }
  const parts = buildDiff(ran.build, b);
  const out = h('p.pending', h('b', 'Since your last run: '), parts.length ? diffText(parts, 6) : 'small changes');
  const c = classOf(b.cls);
  const moved = c.talents.filter((t) => ((b.plan || b.talents)[t.id] || 0) !== ((ran.build.plan || ran.build.talents)[t.id] || 0));
  if (moved.length && moved.every((t) => !t.modelled) && ran.build.race === b.race && ran.build.level === b.level && ran.build.gear === b.gear && ran.build.rotation === b.rotation) {
    out.append(h('span.pending-note', ' These talents are not simulated yet, so the results will not change.'));
  }
  const stats = statChanges(ran.sheet, sheet);
  if (stats.length) out.append(h('span.pending-stats', ' · ', stats.join(', ')));
  return out;
}

function fmtStat(k, v) {
  if (k.endsWith(' x')) return `${v >= 1 ? '+' : '−'}${fmtNum(Math.abs(v - 1) * 100, 0)}%`;
  if (k.endsWith('%')) return `${fmtNum(v, 1)}%`;
  if (k.endsWith('speed')) return `${fmtNum(v, 2)} sec`;
  if (k.endsWith('dps')) return fmtNum(v, 1);
  return fmtNum(v, 0);
}

function statChanges(before, after) {
  if (!before?.rows || !after?.rows) return [];
  const old = Object.fromEntries(before.rows.map((r) => [r.stat, r.buffed]));
  return after.rows
    .filter((r) => old[r.stat] != null && Math.abs(r.buffed - old[r.stat]) > 1e-6 && M.STAT_LABELS[r.stat])
    .map((r) => ({ r, rel: Math.abs(r.buffed - old[r.stat]) / Math.max(1e-6, Math.abs(old[r.stat])) }))
    .sort((a, b) => b.rel - a.rel)
    .slice(0, 3)
    .map(({ r }) => `${M.STAT_LABELS[r.stat]} ${fmtStat(r.stat, old[r.stat])} → ${fmtStat(r.stat, r.buffed)}`);
}
