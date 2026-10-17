// The scorecard: every test as a row with its scene from the world and each
// build's result, a one-line summary on top, what the last change did, and
// each test's detail opening inside its row.
import * as M from './meta.js';
import { h, fmtMetric, fmtDiff, fmtNum, fmtSec } from './ui.js';
import { S, env, sig, nameOf, slotOf, baselineBuild, scenarioMeta, primaryOf, accuracyOf, presetOf, presetsFor, baseName, buildDiff, diffText, saveLocal } from './state.js';
import { scorecardTests, cellOf, pairOf, duelWin, changesFor, simulateAll } from './runner.js';
import { renderDetail, leaders, metricOf } from './results.js';

const pctOf = (diff, base) => (Math.abs(base) > 1e-9 ? (100 * diff) / Math.abs(base) : null);

function fmtRel(diff, base, m) {
  if (m.kind !== 'num') return fmtDiff(Math.abs(diff), m, false);
  const p = pctOf(diff, base);
  if (p == null) return fmtDiff(Math.abs(diff), m, false);
  const a = Math.abs(p);
  return `${fmtNum(a, a < 10 ? 1 : 0)}%`;
}

// Columns: one per build, in slot order, with where each sits in a row.
function columns(row) {
  const base = baselineBuild();
  return S.builds.map((b, k) => {
    const slot = M.SLOTS[k];
    return { k, b, letter: slot.letter, color: slot.color, name: nameOf(b), cell: cellOf(row, b), isBase: b === base };
  });
}

const badge = (c) => h('span.slot', { style: { '--slot': c.color }, 'aria-hidden': 'true' }, c.letter);

// ------------------------------------------------------------------ summary

function rowWinner(test, row, cols) {
  if (!row || row.error) return undefined;
  const live = cols.filter((c) => c.cell && c.cell.current);
  if (live.length < 2 || live.length < cols.length) return undefined;
  if (test === 'duel') {
    if (row.one) return undefined;
    for (const c of live) {
      const beatsAll = live.every((o) => o === c || (duelWin(row, c.cell.sig, o.cell.sig)?.lo ?? 0) > 0.5);
      if (beatsAll) return c;
    }
    return null;
  }
  const pm = M.METRICS[primaryOf(test)] || {};
  const best = leaders(row.res, live, primaryOf(test), pm.good);
  return best.size === 1 ? live.find((c) => best.has(c.k)) : null;
}

function joinAnd(list) {
  if (list.length <= 1) return list.join('');
  return `${list.slice(0, -1).join(', ')} and ${list[list.length - 1]}`;
}

// "the two world PvP tests", "all four leveling tests", or test names.
function describeTests(tests, all) {
  const groups = {};
  for (const t of tests) (groups[scenarioMeta(t).group] ||= []).push(t);
  const parts = [];
  for (const [g, ts] of Object.entries(groups)) {
    const inGroup = all.filter((t) => scenarioMeta(t).group === g);
    if (ts.length === inGroup.length && ts.length > 1) parts.push(`${ts.length === 2 ? 'both' : `all ${ts.length}`} ${M.GROUP_NAMES[g]} tests`);
    else parts.push(...ts.map((t) => scenarioMeta(t).phrase || scenarioMeta(t).short));
  }
  return joinAnd(parts);
}

function summary(tests) {
  const cols = columns(null);
  if (cols.length < 2) {
    const done = tests.filter((t) => S.rows[t] && !S.rows[t].error);
    return done.length ? h('h2.summary', 'How ', h('span.who', badge(cols[0]), cols[0].name), ` does in ${done.length === 1 ? 'one test' : `${done.length} tests`}.`) : null;
  }
  const decided = [];
  const wins = new Map();
  let ties = 0;
  for (const t of tests) {
    const w = rowWinner(t, S.rows[t], columns(S.rows[t]));
    if (w === undefined) continue;
    decided.push(t);
    if (w === null) ties++;
    else wins.set(w.k, [...(wins.get(w.k) || []), t]);
  }
  if (!decided.length) return null;
  const ranked = cols.filter((c) => wins.has(c.k)).sort((a, b) => wins.get(b.k).length - wins.get(a.k).length);
  const out = h('h2.summary');
  const name = (c) => h('span.who', badge(c), c.name);
  if (!ranked.length) {
    out.append(`Too close to call in ${decided.length === 1 ? 'the one test' : `all ${decided.length} tests`} so far.`);
    return out;
  }
  const top = ranked[0];
  const n = wins.get(top.k).length;
  const tie = ranked[1] && wins.get(ranked[1].k).length === n;
  if (tie) out.append(name(top), ' and ', name(ranked[1]), ` win ${n === 1 ? 'one test' : `${n} tests`} each.`);
  else out.append(name(top), ` comes out ahead in ${n} of ${decided.length} tests.`);
  const rest = h('span.summary-rest');
  for (const c of ranked.slice(tie ? 2 : 1)) rest.append(' ', name(c), ` wins ${describeTests(wins.get(c.k), decided)}.`);
  if (ties) rest.append(` ${ties === 1 ? 'One test is' : `${ties} tests are`} too close to call.`);
  out.append(rest);
  return out;
}

// ------------------------------------------------------------------ what the last change did

function changeStrip(cols) {
  const boxes = [];
  for (const c of cols) {
    const list = changesFor(c.b).filter((x) => S.rows[x.test]);
    if (!list.length) continue;
    const from = list.find((x) => x.from)?.from;
    const what = diffText(buildDiff(from, c.b), 4);
    const clear = list.filter((x) => x.significant);
    const items = clear.map((x) => {
      const m = M.METRICS[primaryOf(x.test)] || {};
      const up = x.diff > 0;
      const better = m.good === 'low' ? !up : up;
      return h('li', { class: better ? 'good' : 'bad' }, h('b', scenarioMeta(x.test).short), ` ${up ? '▲' : '▼'} ${fmtDiff(Math.abs(x.diff), m, false)} ${m.kind === 'num' ? scenarioMeta(x.test).abbr : ''}`,
        h('span.sr-only', ` ${scenarioMeta(x.test).per}, ${better ? 'better' : 'worse'}`));
    });
    const quiet = list.length - clear.length;
    boxes.push(h('div.change-box', { style: { '--slot': c.color } },
      h('p', 'Your last change to ', h('span.who', badge(c), c.name), what ? h('span.change-what', ` (${what})`) : null,
        clear.length ? ':' : ' made no clear difference in any test.'),
      clear.length ? h('ul.change-list', items, quiet ? h('li.quiet', `no clear difference in ${quiet === 1 ? 'one other test' : `${quiet} others`}`) : null) : null));
  }
  return boxes.length ? h('div.changes', { 'aria-label': 'What your last change did' }, boxes) : null;
}

// ------------------------------------------------------------------ the trials

// A test's scene, with the main build's hero standing in it. Scenes and
// heroes are drawn on one pixel scale, so they're shown at whole art pixels.
function scene(test) {
  const sc = M.SCENES[test];
  if (!sc) return h('span.scene.empty');
  const main = baselineBuild();
  const hero = artImg('img.hero.px', M.heroArt(main.race, main.cls), { '--ground': String(sc.ground) });
  hero.addEventListener('error', () => hero.remove(), { once: true });
  return h('span.scene', { 'aria-hidden': 'true' }, artImg('img.bg.px', `art/scene-${test}.png`, { '--top': String(sc.top) }), hero);
}

// Pixel art at its own size in art pixels (whole screen pixels each).
function artImg(sel, src, style) {
  const img = h(sel, { src, alt: '', decoding: 'async', style });
  const size = () => { img.style.width = `calc(${img.naturalWidth} * var(--art))`; img.style.height = `calc(${img.naturalHeight} * var(--art))`; };
  if (img.complete && img.naturalWidth) size(); else img.addEventListener('load', size, { once: true });
  return img;
}

function trialHead(test, open) {
  const meta = scenarioMeta(test);
  const sc = M.SCENES[test];
  return h('button.trial-btn', {
    type: 'button', 'aria-expanded': String(open), 'aria-controls': `detail-${test}`,
    onclick: () => { S.open = open ? null : test; saveLocal(); render(); },
  },
  scene(test),
  h('span.trial-text',
    h('span.trial-title', meta.title),
    h('span.trial-per', meta.per, sc ? h('span.trial-zone', ` · ${sc.zone}`) : null)),
  h('span.chev', { 'aria-hidden': 'true' }, open ? '−' : '+'));
}

// One build's result in a test: its letter, the number, a crown for the
// best, and how it compares with the main build.
function plaque(test, row, c, cols, best) {
  const el = h('div.plaque', { style: { '--slot': c.color } }, badge(c), h('span.sr-only', `${c.name}: `));
  if (!c.cell) { el.append(h('span.v.none', '—')); return el; }
  if (!c.cell.current) el.classList.add('stale');
  const base = cols.find((x) => x.isBase);
  if (test === 'duel') return duelPlaque(el, row, c, base);
  const primary = row.res.scenario.primary;
  const m = M.METRICS[primary] || { kind: 'num', digits: 1 };
  const s = metricOf(row.res, c.cell.i, primary);
  if (!s) { el.append(h('span.v.none', '—')); return el; }
  const isBest = best.has(c.k);
  el.append(h('span.v', fmtMetric(s.mean, m),
    isBest ? h('img.crown.px', { src: 'art/crown.png', alt: best.size > 1 ? 'Tied for best' : 'Best', title: best.size > 1 ? 'Tied for best' : 'Best' }) : null));
  if (!c.isBase && base?.cell) {
    const p = pairOf(row.res, primary, base.cell.i, c.cell.i);
    const bs = metricOf(row.res, base.cell.i, primary);
    if (p && bs) {
      const up = p.diff > 0;
      const better = m.good === 'low' ? !up : up;
      el.append(p.significant
        ? h('span.d', { class: better ? 'good' : 'bad' }, `${up ? '▲' : '▼'} ${fmtRel(p.diff, bs.mean, m)}`,
          h('span.sr-only', ` ${up ? 'more' : 'less'} than ${base.name}`))
        : h('span.d', 'about the same', h('span.sr-only', ` as ${base.name}`)));
    }
  } else if (c.isBase && cols.length > 1) {
    el.append(h('span.d.main', 'main build'));
  }
  return el;
}

function duelPlaque(el, row, c, base) {
  if (row.one) {
    const w = S.opponent && duelWin(row, c.cell.sig, S.opponent);
    if (!w) { el.append(h('span.v.none', '—')); return el; }
    el.append(h('span.v', `${fmtNum(100 * w.mean, 0)}%`), h('span.d', { class: w.lo > 0.5 ? 'good' : w.hi < 0.5 ? 'bad' : '' }, w.lo > 0.5 ? 'wins' : w.hi < 0.5 ? 'loses' : 'even'));
    return el;
  }
  if (c.isBase || !base?.cell) { el.append(h('span.v.none', '—'), c.isBase ? h('span.d.main', 'main build') : null); return el; }
  const w = duelWin(row, c.cell.sig, base.cell.sig);
  if (!w) { el.append(h('span.v.none', '—')); return el; }
  const cls = w.lo > 0.5 ? 'good' : w.hi < 0.5 ? 'bad' : '';
  el.append(h('span.v', `${fmtNum(100 * w.mean, 0)}%`),
    h('span.d', { class: cls }, w.lo > 0.5 ? `beats ${base.letter}` : w.hi < 0.5 ? `loses to ${base.letter}` : `even with ${base.letter}`,
      h('span.sr-only', ` (${base.name})`)));
  return el;
}

let render = () => {};

export function renderScorecard(root, { onRender }) {
  render = onRender;
  const tests = scorecardTests();
  const showDuelPicker = S.builds.length === 1 && !S.opponent;
  const listed = showDuelPicker ? [...tests, 'duel'] : tests;
  const anyRow = listed.some((t) => S.rows[t]);
  const run = S.run;
  const running = run && !run.finished;
  const levels = new Set(S.builds.map((b) => b.level)).size;

  const out = [];
  const sum = anyRow ? summary(tests) : null;
  out.push(sum || h('h2.summary.empty', S.builds.length > 1 ? 'How do these builds compare?' : 'How does this build do?'));

  if (!anyRow && !running) {
    out.push(h('p.lede', `Press Simulate to put ${S.builds.length > 1 ? 'these builds' : 'this build'} through ${tests.length === 1 ? 'one test' : `all ${tests.length} tests`} below. Each test plays out hundreds of times, so it takes a few seconds.`));
  }

  // notices
  const notes = [];
  const staleBuilds = S.builds.filter((b) => listed.some((t) => { const c = cellOf(S.rows[t], b); return S.rows[t] && !S.rows[t].error && (!c || !c.current); }));
  if (anyRow && staleBuilds.length && !running) {
    notes.push(h('div.notice', `${joinAnd(staleBuilds.map((b) => nameOf(b)))} ${staleBuilds.length === 1 ? 'has' : 'have'} changed since ${staleBuilds.length === 1 ? 'its' : 'their'} last run. Faded numbers are from before; press Simulate to update them.`));
  }
  if (levels > 1) notes.push(h('div.notice', 'Your builds are at different levels, so they fight different mobs. Compare them with that in mind.'));
  if (run?.finished && run.stopped) notes.push(h('div.notice', `Stopped after ${run.i} of ${run.jobs.length} tests. Press Simulate to run the rest.`));
  out.push(...notes);

  const strip = changeStrip(columns(null));
  if (strip) out.push(strip);

  const list = h('ol.trials', { 'aria-busy': running ? 'true' : 'false', 'aria-label': 'Tests' });
  listed.forEach((test, n) => {
    const row = S.rows[test];
    const cols = columns(row);
    const isRunning = running && run.test === test;
    const queued = running && run.jobs.includes(test) && run.jobs.indexOf(test) > run.jobs.indexOf(run.test);
    const open = S.open === test;
    const li = h('li.trial.fr-small', { class: [isRunning ? 'running' : '', queued ? 'queued' : '', open ? 'open' : ''].join(' '), style: { '--n': String(n) } });
    const vals = h('div.trial-vals', { class: `n${cols.length}` });
    if (test === 'duel' && showDuelPicker) {
      vals.append(h('button.linkish.pick-opp', { type: 'button', onclick: () => { S.open = 'duel'; render(); } }, 'Pick an opponent'));
    } else if (row?.error) {
      vals.append(h('p.error-cell', `This test failed: ${row.error}`));
    } else if (!row) {
      for (const c of cols) vals.append(h('div.plaque', { style: { '--slot': c.color } }, badge(c), h('span.v.none', isRunning ? '…' : '—')));
    } else {
      const best = test === 'duel' ? new Set() : leaders(row.res, cols.filter((c) => c.cell), row.res.scenario.primary, (M.METRICS[row.res.scenario.primary] || {}).good);
      for (const c of cols) vals.append(plaque(test, row, c, cols, best));
    }
    li.append(h('div.trial-head', trialHead(test, open), vals));
    if (isRunning) li.append(h('span.sr-only', ' (running)'));
    if (open) li.append(h('div.trial-detail', { id: `detail-${test}` }, detailFor(test, row, cols)));
    list.append(li);
  });
  out.push(list);

  if (run?.finished && !run.stopped && run.full && !run.nothing) {
    out.push(h('p.fine-print.run-note', `Last run: ${run.jobs.length === 1 ? 'one test' : `${run.jobs.length} tests`} in ${fmtSec(run.ms / 1000)}, at ${accuracyOf(run.acc).title} accuracy. Every build faces the same dice in each test.`));
  }
  root.replaceChildren(...out.filter(Boolean));
}

function detailFor(test, row, cols) {
  const meta = scenarioMeta(test);
  const base = cols.find((c) => c.isBase);
  const changes = new Map();
  for (const c of cols) {
    const ch = S.changes.get(`${c.b.uid}|${test}`);
    if (ch && ch.toSig === sig(c.b)) changes.set(c.b.uid, ch);
  }
  const opponentPicker = () => {
    const own = S.builds[0];
    const groups = env.catalog.classes.map((cl) => h('optgroup', { label: cl.name },
      presetsFor(cl.id).map((p) => h('option', { value: `preset:${p.file}`, selected: S.opponent === `preset:${p.file}` },
        baseName(p.name) + (p.kind === 'experimental' ? ' (experimental)' : p.role === 'healer' ? ' (healer)' : p.role === 'tank' ? ' (tank)' : '')))));
    return h('div.opponent',
      h('label', { for: 'opponent' }, 'Opponent'),
      h('select.select#opponent', { onchange: (e) => { S.opponent = e.target.value || null; saveLocal(); render(); } },
        h('option', { value: '', selected: !S.opponent }, 'Pick a build to duel'), groups),
      S.opponent ? h('button.btn.btn-small', { type: 'button', onclick: () => simulateAll({ only: 'duel' }) }, 'Run the duel') : null,
      h('p.note', `The opponent fights at level ${own.level}, like your build.`));
  };
  if (!row && test === 'duel') {
    return h('div.detail', h('p.detail-meta', 'Your build duels another build until one of them dies, with a three-minute limit.'), opponentPicker());
  }
  if (!row) return h('div.detail', h('p.hint', 'This test has not been run yet. Press Simulate.'));
  if (row.error) return h('div.detail', h('div.warn', row.error));
  const acc = accuracyOf(row.acc);
  const oppPreset = S.opponent && presetOf(S.opponent.slice(7));
  return renderDetail({
    test, scenario: meta, row, cols, base, changes,
    acc: row.acc, accLabel: acc.title, precision: acc.precision,
    onPrecise: () => simulateAll({ only: test, acc: 'precise' }),
    opponentPicker, opponentSig: S.opponent, opponentName: oppPreset ? baseName(oppPreset.name) : 'the opponent',
  });
}
