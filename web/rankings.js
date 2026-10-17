// Two bigger runs over every build on the site: a ranked list per test
// ("Class rankings"), and one build dueling every other ("Duels"). Both use
// the same engine pool as the scorecard, one run at a time.
import * as M from './meta.js';
import { h, art, withTip, tt, flash, fmtMetric, fmtNum, fmtSec } from './ui.js';
import { S, env, bus, classOf, presetOf, presetTags, isExperimental, baseName, accuracyOf, scenarioMeta, primaryOf, classColor, nameOf, apiBuild, raceOf, baselineBuild } from './state.js';
import { metricOf, rangeTip } from './results.js';

// The ranking's tests, in the order the switcher shows them: seven for
// everyone, then three for the healers (the raid at level 60 only).
export const RANK_TESTS = ['solo_1', 'solo_2', 'solo_2_hard', 'solo_3', 'boss', 'pressure', 'pressure_2v1', 'heal_dungeon', 'heal_tank', 'heal_raid'];
// Quickest first, so lists appear while the slow three-mob test runs.
const RANK_RUN_ORDER = ['boss', 'solo_1', 'heal_dungeon', 'heal_tank', 'heal_raid', 'solo_2', 'solo_2_hard', 'pressure', 'pressure_2v1', 'solo_3'];
// Rough share of the run each test takes (timed on Standard), for the progress bar.
const RANK_COST = { boss: 1, solo_1: 4, solo_2: 12, solo_2_hard: 11, pressure: 13, pressure_2v1: 30, solo_3: 80, heal_dungeon: 1, heal_tank: 1, heal_raid: 2 };
const RANK_TITLES = {
  solo_1: 'One mob', solo_2: 'Two mobs', solo_2_hard: 'Two tough mobs', solo_3: 'Three mobs', boss: 'Dungeon boss', pressure: 'World PvP', pressure_2v1: 'Two attackers',
  heal_dungeon: 'Dungeon healing', heal_tank: 'Main tank healing', heal_raid: 'Raid healing',
};
const isHealTest = (t) => scenarioMeta(t).group === 'healing';
// The tests a ranking at this level runs.
const rankTests = (level) => RANK_RUN_ORDER.filter((t) => level >= (scenarioMeta(t).minLevel || 1));
// Role filter for the lists.
const ROLES = [['all', 'All'], ['damage', 'Damage'], ['healer', 'Healers'], ['tank', 'Tanks']];
const roleOf = (p) => p.role || 'damage';
const LEVELS = [20, 30, 40, 50, 60];
const STORE = 'forever-sim/rankings-v2';

// ------------------------------------------------------------------ state

export function initRankings() {
  S.rank = { level: 30, acc: 'standard', experiments: true, test: 'solo_1', role: 'all', results: {}, run: null };
  S.duels = { build: null, level: 30, acc: 'standard', experiments: true, result: null, run: null };
  try {
    const saved = JSON.parse(localStorage.getItem(STORE) || 'null');
    if (saved && saved.data === env.catalog.data_build) {
      Object.assign(S.rank, saved.rank || {}, { run: null });
      Object.assign(S.duels, saved.duels || {}, { run: null });
    }
  } catch { /* nothing saved, or storage is off */ }
  if (!S.duels.build || !(presetOf(S.duels.build) || ownBuild(S.duels.build))) S.duels.build = S.builds[0]?.preset || env.catalog.presets[0].file;
}

function save() {
  try {
    const strip = (x) => { const { run, ...rest } = x; return rest; };
    localStorage.setItem(STORE, JSON.stringify({ data: env.catalog.data_build, rank: strip(S.rank), duels: strip(S.duels) }));
  } catch { /* storage full or off: results stay for this visit */ }
}

// The settings a result depends on; a result with another key is out of date.
const settingsKey = (x, extra = '') => JSON.stringify([x.level, x.acc, x.experiments, S.opts, env.catalog.data_build, extra]);

const pool = (withExperiments) => env.catalog.presets
  .filter((p) => withExperiments || !isExperimental(p))
  .sort((a, b) => a.class.localeCompare(b.class) || a.name.localeCompare(b.name));

function presetApi(p, level) {
  const { file, note, kind, role, ...rest } = p;
  return { ...rest, name: baseName(p.name), level };
}

const anyRunning = () => (S.run && !S.run.finished) || (S.rank.run && !S.rank.run.finished) || (S.duels.run && !S.duels.run.finished);

// ------------------------------------------------------------------ running

// Replications a session will need, from how precise it is so far (the
// half-width shrinks with the square root of the replications).
function estimateReps(st, est) {
  const worst = Math.max(0, ...(st.precision || []).filter((x) => x != null));
  if (st.reps >= 32 && worst > 0 && st.target) return Math.min(st.max_reps, Math.max(st.reps, st.reps * (worst / st.target) ** 2));
  return Math.max(est, st.reps);
}

export async function runRankings() {
  if (anyRunning()) { flash('Another simulation is still running. Wait for it or stop it first.'); return; }
  const st = S.rank;
  const everyone = pool(st.experiments);
  const a = accuracyOf(st.acc);
  const key = settingsKey(st);
  const tests = rankTests(st.level);
  const total = tests.reduce((a, t) => a + RANK_COST[t], 0);
  let done = 0;
  const run = { i: 0, frac: 0, test: tests[0], t0: performance.now(), cancelled: false, finished: false, n: tests.length };
  st.run = run;
  bus.render();
  for (const test of tests) {
    if (run.cancelled) break;
    run.test = test;
    bus.render();
    // the healing tests are for the healer builds
    const presets = isHealTest(test) ? everyone.filter((p) => p.role === 'healer') : everyone;
    const builds = presets.map((p) => presetApi(p, st.level));
    const req = { op: 'start', scenario: test, builds, precision: a.precision, max_reps: a.max_reps, min_reps: 32 };
    if (Object.keys(S.opts).length) req.opts = { ...S.opts };
    const t0 = performance.now();
    try {
      let est = a.max_reps;
      const res = await env.engine.simulate(req, (p) => {
        est = estimateReps(p, est);
        run.frac = (done + RANK_COST[test] * Math.min(0.97, p.reps / Math.max(est, 1))) / total;
        bus.progress();
      }, () => run.cancelled, { columns: [] });
      if (!res) break;
      // keep only what the list shows, so the result fits in local storage
      const keep = new Set([primaryOf(test), 'deaths_per_hour']);
      const slim = { reps: res.reps, variants: res.variants.map((v) => ({ metrics: v.metrics.filter((m) => keep.has(m.name)) })) };
      st.results[test] = { key, files: presets.map((p) => p.file), res: slim, ms: performance.now() - t0, level: st.level, acc: st.acc };
      // show the first list that's ready if the chosen one has nothing current yet
      if (st.results[st.test]?.key !== key) st.test = test;
    } catch (e) {
      st.results[test] = { key, error: e.message || String(e) };
    }
    run.i++;
    done += RANK_COST[test];
    run.frac = done / total;
    bus.render();
  }
  run.finished = true;
  run.ms = performance.now() - run.t0;
  run.stopped = run.cancelled;
  save();
  bus.render();
}

// One build against every other, both ways round: whoever opens a duel
// strikes first, so each matchup runs with each side challenging and the two
// are averaged (as the scorecard's duel row does).
export async function runDuels() {
  if (anyRunning()) { flash('Another simulation is still running. Wait for it or stop it first.'); return; }
  const st = S.duels;
  const me = duelistOf(st.build);
  if (!me) return;
  const others = pool(st.experiments).filter((p) => p.file !== st.build);
  const level = me.own ? me.level : st.level;
  const a = accuracyOf(st.acc);
  const base = { op: 'start', scenario: 'duel', precision: a.precision, max_reps: a.max_reps, min_reps: 32 };
  if (Object.keys(S.opts).length) base.opts = { ...S.opts };
  const mine = me.api(level);
  // session 0: everyone challenges me; sessions 1..n: I challenge each of them
  const sessions = [{ ...base, builds: others.map((p) => presetApi(p, level)), opponent: mine }]
    .concat(others.map((p) => ({ ...base, builds: [mine], opponent: presetApi(p, level) })));
  const run = { i: 0, frac: 0, t0: performance.now(), cancelled: false, finished: false, n: sessions.length };
  st.run = run;
  bus.render();
  const results = [];
  try {
    for (const req of sessions) {
      if (run.cancelled) break;
      const res = await env.engine.simulate(req, (p) => {
        run.frac = (run.i + Math.min(0.95, p.reps / Math.max(p.max_reps || a.max_reps, 1))) / run.n;
        bus.progress();
      }, () => run.cancelled, { columns: [] });
      if (!res) break;
      results.push(res);
      run.i++;
      run.frac = run.i / run.n;
      bus.progress();
    }
    if (!run.cancelled) {
      st.result = {
        key: settingsKey(st, st.build), build: st.build, me: { name: me.name, cls: me.cls, race: me.race }, files: others.map((p) => p.file),
        level, acc: st.acc, ms: performance.now() - run.t0, ...duelTable(results, others.length),
      };
    }
  } catch (e) {
    st.result = { key: settingsKey(st, st.build), build: st.build, error: e.message || String(e) };
  }
  run.finished = true;
  run.ms = performance.now() - run.t0;
  run.stopped = run.cancelled;
  save();
  bus.render();
}

// Each opponent: my win rate when they open, when I open, and the average.
function duelTable(results, n) {
  const metric = (res, i, name) => res.variants[i]?.metrics.find((m) => m.name === name)?.mean;
  const rows = [];
  for (let i = 0; i < n; i++) {
    const theyOpen = metric(results[0], i, 'loss'); // they challenge me and lose
    const iOpen = metric(results[i + 1], 0, 'win');
    if (theyOpen == null || iOpen == null) continue;
    const n1 = results[0].reps, n2 = results[i + 1].reps;
    const mean = (theyOpen + iOpen) / 2;
    const se = Math.sqrt(theyOpen * (1 - theyOpen) / n1 + iOpen * (1 - iOpen) / n2) / 2;
    const duration = ((metric(results[0], i, 'duration') || 0) + (metric(results[i + 1], 0, 'duration') || 0)) / 2;
    rows.push({ i, mean, lo: Math.max(0, mean - 1.96 * se), hi: Math.min(1, mean + 1.96 * se), iOpen, theyOpen, duration });
  }
  return { rows };
}

// Moves the progress bar and clock of a big run without redrawing the view.
export function updateBigProgress() {
  const run = [S.rank.run, S.duels.run].find((r) => r && !r.finished);
  const el = document.querySelector('.big-progress');
  if (!run || !el) return;
  el.querySelector('.castbar i').style.width = `${Math.min(99, Math.round(100 * run.frac))}%`;
  el.querySelector('.run-elapsed').textContent = fmtSec((performance.now() - run.t0) / 1000);
}

export function cancelBigRun() {
  for (const r of [S.rank.run, S.duels.run]) if (r && !r.finished) r.cancelled = true;
}

// ------------------------------------------------------------------ shared bits

const levelPicker = (st, disabled) => h('div.quick', { role: 'group', 'aria-label': 'Level' },
  LEVELS.map((l) => h('button.chip', { type: 'button', disabled, 'aria-pressed': String(st.level === l), onclick: () => { st.level = l; bus.render(); } }, String(l))));

const accuracyPicker = (st, disabled) => h('div.hourglasses', { role: 'group', 'aria-label': 'Accuracy' }, M.ACCURACY.map((a) => h('button', {
  type: 'button', disabled, 'aria-pressed': String(a.id === st.acc), onclick: () => { st.acc = a.id; bus.render(); }, title: `${a.note} of the true answer`,
}, h('img.px', { src: `art/hourglass-${a.id}.png`, alt: '' }), h('span', a.title), h('small', a.short))));

const experimentsToggle = (st, disabled) => h('label.toggle-line', h('input.check', {
  type: 'checkbox', checked: st.experiments, disabled, onchange: (e) => { st.experiments = e.target.checked; bus.render(); },
}), 'Include the Forever experiments');

function progressBlock(run, label) {
  const secs = (performance.now() - run.t0) / 1000;
  const pct = Math.min(99, Math.round(100 * run.frac));
  return h('div.big-progress', { role: 'status' },
    h('div.castbar', h('i', { style: { width: `${pct}%` } }), h('span.castbar-text', label)),
    h('span.run-elapsed.muted', fmtSec(secs)));
}

function controls(st, { go, goLabel, running, stopLabel, extra, frame, noLevel }) {
  return h(`div.big-controls.${frame}`,
    extra || null,
    noLevel ? null : h('div.field-row', h('span.label', 'Level'), levelPicker(st, running)),
    h('div.field-row', h('span.label', 'Accuracy'), accuracyPicker(st, running)),
    experimentsToggle(st, running),
    running
      ? h('button.btn.run-cancel', { type: 'button', onclick: () => { cancelBigRun(); bus.render(); } }, stopLabel)
      : h('button.primary.small', { type: 'button', onclick: go }, goLabel));
}

// The tab's picture across the top, with its title over it.
const banner = (img, title, frame) => h(`div.view-banner.${frame}`,
  h('img.px', { src: `art/${img}.png`, alt: '' }),
  h('h2.view-title', title));

const tagsOf = (p) => presetTags(p).map((t) => h('span.card-tag', t));
// Opens the build in the Compare tab at the list's level (app.js does the adding).
const compareButton = (p, level) => h('button.linkish.row-compare', {
  type: 'button', 'aria-label': `Compare ${baseName(p.name)} ${classOf(p.class)?.name || ''}`, onclick: () => bus.comparePreset(p.file, level),
}, 'Compare');
const classIcon = (p) => h('span.rank-icon', art(M.CLASS_ICONS[p.class], classOf(p.class)?.icon));
// a podium hero, at twice the scenes' size
const heroOf = (race, cls) => {
  const img = h('img.podium-hero.px', { src: M.heroArt(race, cls), alt: '' });
  img.addEventListener('load', () => { img.style.width = `calc(${2 * img.naturalWidth} * var(--art))`; }, { once: true });
  img.addEventListener('error', () => img.remove(), { once: true });
  return img;
};

// ------------------------------------------------------------------ rankings view: the notice board at the inn

export function renderRankings(root) {
  const st = S.rank;
  const running = !!st.run && !st.run.finished;
  const count = pool(st.experiments).length;
  const healers = pool(st.experiments).filter((p) => p.role === 'healer').length;
  const out = [
    banner(M.sideArt(raceOf(baselineBuild()?.race)?.faction).inn, 'Every spec, ranked', 'fr-oak'),
    h('p.lede', `All ${count} builds through seven tests, and the ${healers} healers through the healing tests (the raid at level 60). About four minutes on Standard on a fast laptop, longer on a phone; each list shows up as its test finishes.`),
    controls(st, { go: runRankings, goLabel: `Rank ${count} builds`, running, stopLabel: 'Stop', frame: 'fr-oak' }),
  ];
  if (running) out.push(progressBlock(st.run, `${RANK_TITLES[st.run.test]}, ${Math.min(st.run.i + 1, st.run.n)} of ${st.run.n} tests`));
  else if (st.run?.stopped) out.push(h('p.hint', 'Stopped. The tests that finished are below.'));

  const have = RANK_TESTS.filter((t) => st.results[t]);
  if (have.length) {
    out.push(h('div.rank-tests', { role: 'tablist', 'aria-label': 'Test' }, RANK_TESTS.map((t) => h('button.rank-test', {
      type: 'button', role: 'tab', 'aria-selected': String(st.test === t), disabled: !st.results[t],
      onclick: () => { st.test = t; save(); bus.render(); },
    }, h('span.thumb', h('img.px', { src: M.sceneFor(t, raceOf(baselineBuild()?.race)?.faction, st.level).file, alt: '' })), h('b', RANK_TITLES[t]), h('span', scenarioMeta(t).per)))));
    out.push(h('div.quick.role-chips', { role: 'group', 'aria-label': 'Role' }, ROLES.map(([id, label]) => h('button.chip', {
      type: 'button', 'aria-pressed': String((st.role || 'all') === id), onclick: () => { st.role = id; save(); bus.render(); },
    }, label))));
    out.push(rankList(st, st.results[st.test] || st.results[have[0]], st.results[st.test] ? st.test : have[0]));
  } else if (!running) {
    out.push(h('div.empty-big', h('img.px', { src: 'art/board-empty.png', alt: '' }), h('p', 'Nothing on the board yet. Pick a level and press the button.')));
  }
  root.replaceChildren(...out.filter(Boolean));
}

function rankList(st, r, test) {
  if (r.error) return h('div.warn', r.error);
  const primary = primaryOf(test);
  const pm = M.METRICS[primary];
  const rows = r.files.map((file, i) => ({ p: presetOf(file), m: metricOf(r.res, i, primary), deaths: metricOf(r.res, i, 'deaths_per_hour') }))
    .filter((x) => x.p && x.m && ((st.role || 'all') === 'all' || roleOf(x.p) === st.role))
    .sort((a, b) => b.m.mean - a.m.mean);
  const max = Math.max(...rows.map((x) => x.m.hi), 1e-9);
  // a build whose range overlaps the first build of the group above shares
  // its rank (measured against that first build, so ties don't chain)
  let lead = null;
  rows.forEach((x, k) => {
    x.tied = !!lead && x.m.hi >= lead.m.lo;
    if (!x.tied) { lead = x; x.rank = k + 1; } else x.rank = lead.rank;
  });
  const stale = r.key !== settingsKey(st);
  const meta = scenarioMeta(test);
  const extra = (x) => ((test === 'solo_2' || test === 'solo_2_hard' || test === 'solo_3') && x.deaths ? `${fmtNum(x.deaths.mean, 1)} deaths/h` : `±${fmtMetric((x.m.hi - x.m.lo) / 2, pm)}`);
  const medals = ['gold', 'silver', 'bronze'];
  // the top three on the podium, in hero form
  const podium = h('ol.podium', rows.slice(0, 3).map((x, k) => h('li.podium-step', { class: `p${k + 1}`, style: { '--cc': M.CLASS_COLORS[x.p.class] } },
    heroOf(x.p.race, x.p.class),
    h('div.podium-plate',
      h('img.medal.px', { src: `art/medal-${medals[k]}.png`, alt: x.tied ? `tied ${x.rank}` : `rank ${x.rank}` }),
      h('b', baseName(x.p.name)), h('span.who-class', { style: { color: classColor(x.p.class) } }, classOf(x.p.class)?.name),
      h('span.podium-val', fmtMetric(x.m.mean, pm)), h('small.muted', extra(x)), tagsOf(x.p), compareButton(x.p, r.level)))));
  return h('section.rank-board', { class: stale ? 'stale' : '' },
    h('div.rank-head',
      h('h3', `${meta.title}, level ${r.level}`),
      h('p.hint', `${accuracyOf(r.acc).title} accuracy, ${fmtSec(r.ms / 1000)}.`),
      stale ? h('p.notice', 'The settings above changed since this ranking. Press the button to run it again.') : null),
    podium,
    // everyone, like raid frames: a bar in the class's colour with the name on it
    h('ol.raid-rows', rows.map((x) => h('li.raid-row', { style: { '--cc': M.CLASS_COLORS[x.p.class], '--w': `${(100 * Math.max(0, x.m.mean)) / max}%` } },
      h('span.rank-n', { 'aria-label': x.tied ? `tied ${x.rank}` : `rank ${x.rank}` }, x.tied ? '=' : String(x.rank)),
      classIcon(x.p),
      h('div.raid-bar',
        h('i'), h('s', { style: { left: `${(100 * Math.max(0, x.m.lo)) / max}%`, width: `${(100 * (x.m.hi - x.m.lo)) / max}%` } }),
        h('span.raid-name', h('b', baseName(x.p.name)), h('span', classOf(x.p.class)?.name), tagsOf(x.p)),
        h('span.raid-val', fmtMetric(x.m.mean, pm), h('small', extra(x)))),
      compareButton(x.p, r.level)))),
    h('p.hint.bar-hint', withTip(h('span.q', { tabindex: '0' }, '= means tied'), () => [tt.title('Tied'), tt.line('Within the margin of error of the build above with that rank.', 'gold'), ...rangeTip().slice(1)]),
      test.startsWith('pressure') ? ' · A very high number means the attacker can’t get through at all.' : ''));
}

// ------------------------------------------------------------------ duels view: the arena

// "Your build" can be one of your own builds from Compare (it fights at its
// own level) or any build on the site.
const ownBuild = (key) => (key && key.startsWith('build:') ? S.builds.find((b) => `build:${b.uid}` === key) : null);
const duelistOf = (key) => {
  const b = ownBuild(key);
  if (b) return { name: nameOf(b), cls: b.cls, race: b.race, level: b.level, api: (lvl) => apiBuild(b, nameOf(b)), own: true };
  const p = presetOf(key);
  return p ? { name: baseName(p.name), cls: p.class, race: p.race, file: p.file, api: (lvl) => presetApi(p, lvl) } : null;
};

export function renderDuels(root) {
  const st = S.duels;
  const running = !!st.run && !st.run.finished;
  const me = duelistOf(st.build);
  const others = pool(st.experiments).filter((p) => p.file !== st.build);
  const picker = h('div.field-row', h('label.label', { for: 'duel-build' }, 'Your build'),
    h('select.select#duel-build', { disabled: running, onchange: (e) => { st.build = e.target.value; bus.render(); } },
      h('optgroup', { label: 'Your builds' }, S.builds.map((b) => h('option', { value: `build:${b.uid}`, selected: st.build === `build:${b.uid}` }, `${nameOf(b)} (level ${b.level})`))),
      env.catalog.classes.map((c) => h('optgroup', { label: c.name }, env.catalog.presets.filter((p) => p.class === c.id)
        .sort((a, b) => isExperimental(a) - isExperimental(b) || a.name.localeCompare(b.name))
        .map((p) => h('option', { value: p.file, selected: p.file === st.build }, `${baseName(p.name)}${isExperimental(p) ? ' (experimental)' : ''}`))))));
  const out = [
    banner('banner-arena', 'One build against every other', 'fr-iron'),
    h('p.lede', `Your build against each of the other ${others.length}, both ways round. About ten seconds on a fast laptop, minutes on a phone.`),
    controls(st, { go: runDuels, goLabel: `Duel ${others.length} builds`, running, stopLabel: 'Stop', extra: picker, frame: 'fr-iron', noLevel: me?.own }),
  ];
  if (me?.own) out.push(h('p.hint', `Everyone fights at your build’s level, ${me.level}.`));
  if (running) out.push(progressBlock(st.run, `Duel ${Math.min(st.run.i + 1, st.run.n)} of ${st.run.n}`));
  else if (st.run?.stopped) out.push(h('p.hint', 'Stopped before every duel finished, so there are no results yet.'));
  const r = st.result;
  if (r && (r.me || presetOf(r.build))) out.push(duelBoard(st, r));
  else if (!running && me) out.push(h('div.empty-big', h('img.px', { src: 'art/dummies.png', alt: '' }), h('p', `Nothing run yet for ${me.name}. Pick a level and press the button.`)));
  root.replaceChildren(...out.filter(Boolean));
}

function duelBoard(st, r) {
  if (r.error) return h('div.warn', r.error);
  const me = r.me || (() => { const p = presetOf(r.build); return { name: baseName(p.name), cls: p.class, race: p.race }; })();
  const rows = r.rows.map((x) => ({ ...x, p: presetOf(r.files[x.i]) })).filter((x) => x.p).sort((a, b) => b.mean - a.mean);
  const wins = rows.filter((x) => x.lo > 0.5).length;
  const losses = rows.filter((x) => x.hi < 0.5).length;
  const even = rows.length - wins - losses;
  const stale = r.key !== settingsKey(st, st.build);
  const name = me.name;
  const best = rows[0];
  const worst = rows[rows.length - 1];
  const pct = (x) => `${fmtNum(100 * x, 0)}%`;
  const face = (race, cls) => h('span.duel-face', art(M.CLASS_ICONS[cls], classOf(cls)?.icon));
  return h('section.rank-board.duel-board', { class: stale ? 'stale' : '' },
    h('div.rank-head',
      h('h3', `${name} (${classOf(me.cls)?.name}), level ${r.level}`),
      h('p.summary-line', `Beats ${wins} of ${rows.length} builds, loses to ${losses}${even ? `, and ${even} ${even === 1 ? 'is' : 'are'} too close to call` : ''}. `,
        best ? `Best matchup: ${baseName(best.p.name)} ${classOf(best.p.class)?.name} (wins ${pct(best.mean)}). ` : '',
        worst ? `Hardest: ${baseName(worst.p.name)} ${classOf(worst.p.class)?.name} (wins ${pct(worst.mean)}).` : ''),
      h('p.hint', `${accuracyOf(r.acc).title} accuracy, ${fmtSec(r.ms / 1000)}. No movement yet, so builds that kite do worse here.`),
      stale ? h('p.notice', 'The settings above changed since these duels. Press the button to run them again.') : null),
    // each matchup as a forecast: you on the left, them on the right, the
    // bar pushed toward whoever wins more often
    h('ol.forecasts', rows.map((x) => h('li.forecast', { class: x.lo > 0.5 ? 'win' : x.hi < 0.5 ? 'loss' : 'even', style: { '--cc': M.CLASS_COLORS[x.p.class], '--me': M.CLASS_COLORS[me.cls] } },
      face(me.race, me.cls),
      withTip(h('div.tug', { tabindex: '0' },
        h('span.tug-me', { style: { width: `${100 * x.mean}%` } }), h('span.tug-them'), h('s', { style: { left: `${100 * x.lo}%`, width: `${100 * (x.hi - x.lo)}%` } }),
        h('span.tug-val', `${pct(x.mean)} won`)), () => [
        tt.title(`${name} against ${baseName(x.p.name)}`),
        tt.line(`Wins ${pct(x.mean)} of duels (${pct(x.lo)} to ${pct(x.hi)})`, 'gold'),
        tt.line(`${pct(x.iOpen)} when ${name} opens, ${pct(x.theyOpen)} when ${baseName(x.p.name)} opens`),
        tt.line(`Duels last ${fmtSec(x.duration)} on average`, 'grey')]),
      face(x.p.race, x.p.class),
      h('span.foe-name', h('b', baseName(x.p.name)), h('span', { style: { color: classColor(x.p.class) } }, classOf(x.p.class)?.name), tagsOf(x.p)),
      compareButton(x.p, r.level)))),
    h('p.hint.bar-hint', 'The left side of each bar is your share of duels won.'));
}
