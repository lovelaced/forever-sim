// The build cards along the top: who each build is, how it differs from the
// baseline, what its last change did, and the buttons to edit, copy, remove
// or make it the baseline.
import * as M from './meta.js';
import { h, art, withTip, tt } from './ui.js';
import { gearLines } from './editor.js';
import { S, MAX_BUILDS, classOf, raceOf, nameOf, classColor, baselineBuild, buildDiff, diffText, presetOf, presetTags } from './state.js';
import { prepareClass } from './talents.js';
import { changesFor } from './runner.js';

// actions: {edit(b), copy(b), remove(b), makeBaseline(b), add()}
export function renderCards(root, actions) {
  const base = baselineBuild();
  const cards = S.builds.map((b, k) => card(b, k, b === base, base, actions));
  if (S.builds.length < MAX_BUILDS) {
    const from = S.builds.find((b) => b.uid === S.editing) || base;
    cards.push(h('button.card.card-add', { type: 'button', onclick: actions.add },
      h('span.plus', { 'aria-hidden': 'true' }, '+'),
      h('span.add-text', h('b', 'Add a build to compare'), h('span', `Starts as a copy of ${nameOf(from)}`))));
  }
  root.replaceChildren(h('div.cards', { style: { '--n': String(cards.length) } }, cards));
}

// "Typical level 30 gear: caster cloth with a staff and a wand", with the
// kit's stats and weapons on hover or tap.
function gearLine(b) {
  const title = M.GEAR_TITLES[b.gear] || b.gear;
  const line = h('p.card-gear', { tabindex: '0' }, `Typical level ${b.level} gear: ${title.charAt(0).toLowerCase()}${title.slice(1)}`);
  const sheet = S.sheets.get(b.uid);
  if (sheet && sheet.gear) {
    const { stats, weapons } = gearLines(sheet.gear);
    withTip(line, () => [tt.title(`${title}, level ${sheet.gear.level}`), tt.line(stats.join(', '), 'gold'), ...weapons.map((w) => tt.line(w)), tt.line('The same totals for every build with this kit.', 'grey')]);
  }
  return line;
}

function card(b, k, isBase, base, actions) {
  const slot = M.SLOTS[k];
  const c = classOf(b.cls);
  const race = raceOf(b.race);
  const prep = prepareClass(c);
  const editing = S.editing === b.uid;
  const split = prep.trees.map((tr) => [tr, tr.talents.reduce((a, t) => a + (b.talents[t.id] || 0), 0)]);
  const style = M.TREE_STYLE[b.cls] || [];

  let diff = null;
  if (!isBase && S.builds.length > 1) {
    const parts = buildDiff(base, b);
    const baseSlot = M.SLOTS[S.builds.indexOf(base)];
    diff = h('p.card-diff', h('span.vs', `vs ${baseSlot.letter}: `), parts.length ? diffText(parts, 3) : 'same as the baseline');
  }

  // "Last change: better in 2 tests, worse in 1"
  let change = null;
  const ch = changesFor(b).filter((x) => S.rows[x.test]);
  if (ch.length) {
    const better = ch.filter((x) => x.significant && x.diff > 0).length;
    const worse = ch.filter((x) => x.significant && x.diff < 0).length;
    const parts = [];
    if (better) parts.push(h('span.good', `better in ${better}`));
    if (worse) parts.push(h('span.bad', `worse in ${worse}`));
    const same = ch.length - better - worse;
    if (same) parts.push(`${better || worse ? 'same in' : 'no clear difference in'} ${same}`);
    change = h('p.card-change', 'Last change: ', parts.flatMap((p, i) => (i ? [', ', p] : [p])), ch.length === 1 ? ' test' : ' tests');
  }

  const menu = h('details.menu',
    h('summary', { 'aria-label': `More for ${nameOf(b)}` }, '•••'),
    h('div.menu-list', { role: 'menu' },
      !isBase ? h('button', { type: 'button', role: 'menuitem', onclick: (e) => { e.target.closest('details').open = false; actions.makeBaseline(b); } }, 'Make this the baseline') : null,
      S.builds.length < MAX_BUILDS ? h('button', { type: 'button', role: 'menuitem', onclick: (e) => { e.target.closest('details').open = false; actions.copy(b); } }, 'Copy to compare') : null,
      S.builds.length > 1 ? h('button.danger', { type: 'button', role: 'menuitem', onclick: (e) => { e.target.closest('details').open = false; actions.remove(b); } }, 'Remove') : null));

  return h('article.card', { class: editing ? 'editing' : '', style: { '--slot': slot.color, '--cc': M.CLASS_COLORS[b.cls] }, 'aria-label': `Build ${slot.letter}: ${nameOf(b)}` },
    h('div.card-top',
      h('span.slot.big', { 'aria-hidden': 'true' }, slot.letter),
      h('span.card-portrait', h('span.face', art(M.RACE_ICONS[b.race] || 'rune-stone', race?.icon)), h('span.crest', art(M.CLASS_ICONS[b.cls], c.icon))),
      h('div.card-id',
        h('h3.card-name', nameOf(b)),
        h('p.card-sub', `Level ${b.level} ${race?.name || ''} `, h('span', { style: { color: classColor(b.cls) } }, c.name),
          presetTags(b.preset && presetOf(b.preset)).map((t) => h('span.card-tag', t))))),
    h('p.card-split', split.map(([tr, n], i) => h('span', { class: n ? '' : 'zero', title: tr.name }, art(style[i]?.[1] || 'rune-stone', style[i]?.[2]), `${n}`))),
    gearLine(b),
    isBase && S.builds.length > 1 ? h('p.card-diff.base', 'Baseline: the others are compared with this build.') : diff,
    change,
    h('div.card-actions',
      h('button.btn.btn-small', {
        type: 'button', class: editing ? 'on' : '', 'aria-expanded': String(editing), 'aria-controls': 'editor',
        'data-edit': String(b.uid), onclick: () => actions.edit(b),
      }, editing ? 'Close editor' : 'Edit talents and gear'),
      menu));
}
