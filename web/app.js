// Forever Sim front end: set up to three builds, run them through every
// test, and read how they compare. Results come first; the build editor
// folds away and opens from a build card.
import { startEngine } from './engine.js';
import * as M from './meta.js';
import { h, loadIconManifest, loadGlyphs, setArtScale, retargetTip, hideTip, flash, fmtSec, debounce, tipIsPinned, repositionTip } from './ui.js';
import {
  S, env, bus, MAX_BUILDS, DEFAULT_PRESET, DEFAULT_LEVEL, presetOf, raceOf, baseName, buildOf, nameOf, baselineBuild, copyBuild, buildFromPreset,
  scenarioMeta, accuracyOf, sig, serial, restore, loadLocal, saveLocal, loadRows, encode, decode,
} from './state.js';
import { renderCards } from './cards.js';
import { renderEditor, refreshSheet, refreshLater } from './editor.js';
import { renderScorecard } from './scorecard.js';
import { simulateAll, cancelRun, scorecardTests } from './runner.js';
import { initRankings, renderRankings, renderDuels, updateBigProgress } from './rankings.js';

const $ = (id) => document.getElementById(id);
const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
const narrow = () => matchMedia('(max-width: 760px)').matches;

const persist = debounce(saveLocal, 300);

// The page's three views: your builds compared, every spec ranked, and one
// build dueling every other.
const VIEWS = [['compare', 'Compare your builds'], ['rankings', 'Class rankings'], ['duels', 'Duels']];
const VIEW_KEY = 'forever-sim/view';
let view = 'compare';
try { view = VIEWS.some(([v]) => v === localStorage.getItem(VIEW_KEY)) ? localStorage.getItem(VIEW_KEY) : 'compare'; } catch { /* storage off */ }

function setView(v) {
  view = v;
  try { localStorage.setItem(VIEW_KEY, v); } catch { /* storage off */ }
  render();
  window.scrollTo({ top: 0, behavior: 'auto' });
}

function renderViews() {
  $('views').replaceChildren(h('div.view-tabs-inner', { role: 'tablist' }, VIEWS.map(([v, label]) => h('button.view-tab', {
    type: 'button', role: 'tab', 'aria-selected': String(view === v), onclick: () => setView(v),
  }, label))));
  const compare = view === 'compare';
  for (const id of ['builds', 'run-options', 'runbar', 'results']) $(id).hidden = !compare;
  $('rankings').hidden = view !== 'rankings';
  $('duels').hidden = view !== 'duels';
  // the header shows the main build's side of the world
  const main = baselineBuild();
  const faction = raceOf(main?.race)?.faction;
  $('mast').classList.toggle('horde', faction === 'horde');
  for (const img of document.querySelectorAll('.mast .endcap')) {
    const src = M.sideArt(faction).endcap;
    if (img.getAttribute('src') !== src) img.setAttribute('src', src);
  }
  document.body.dataset.view = view;
}

function render() {
  if (!env.catalog) return;
  renderViews();
  if (view === 'compare') {
    renderCards($('builds'), cardActions);
    renderRunbar();
    renderEditor($('editor'), editorActions);
    renderScorecard($('results'), { onRender: render });
  } else {
    $('editor').hidden = true;
    document.body.classList.remove('editor-open');
    if (view === 'rankings') renderRankings($('rankings'));
    else renderDuels($('duels'));
  }
  retargetTip();
  persist();
}

bus.render = render;
bus.cards = () => renderCards($('builds'), cardActions);
// From the rankings and duels lists: put that build in the Compare tab. It
// takes a free slot, or the last slot that isn't the main build.
bus.comparePreset = (file, level) => {
  const p = presetOf(file);
  if (!p) return;
  const nb = buildFromPreset(p, level);
  if (S.builds.length < MAX_BUILDS) {
    S.builds.push(nb);
    flash(`Added ${baseName(p.name)} to Compare your builds.`, 'info');
  } else {
    const k = [...S.builds.keys()].reverse().find((i) => S.builds[i].uid !== S.baseline) ?? S.builds.length - 1; // never the main build
    S.builds[k] = nb;
    flash(`Compare holds ${MAX_BUILDS} builds, so this one took the place of the last.`, 'info');
  }
  refreshSheet(nb);
  setView('compare');
};
bus.progress = () => { updateProgress(); updateBigProgress(); };

// ------------------------------------------------------------------ builds

function openEditor(b) {
  S.editing = b.uid;
  S.tree = null;
  render();
  $('editor-title')?.focus({ preventScroll: true });
  if (!narrow()) {
    const r = $('editor').getBoundingClientRect();
    if (r.top > innerHeight * 0.6 || r.top < 0) $('editor').scrollIntoView({ behavior: reducedMotion() ? 'auto' : 'smooth', block: 'start' });
  }
}

function closeEditor() {
  const uid = S.editing;
  S.editing = null;
  render();
  document.querySelector(`[data-edit="${uid}"]`)?.focus({ preventScroll: narrow() });
}

function addCopyOf(b) {
  if (S.builds.length >= MAX_BUILDS) { flash(`You can compare up to ${MAX_BUILDS} builds.`); return; }
  const copy = copyBuild(b);
  S.builds.push(copy);
  refreshSheet(copy);
  flash('Copied. Change something in the copy, then simulate to compare.', 'info');
  openEditor(copy);
}

const cardActions = {
  edit: (b) => (S.editing === b.uid ? closeEditor() : openEditor(b)),
  copy: (b) => addCopyOf(b),
  add: () => addCopyOf(buildOf(S.editing) || baselineBuild()),
  remove: (b) => {
    if (S.builds.length <= 1) return;
    const name = nameOf(b);
    S.builds = S.builds.filter((x) => x !== b);
    S.sheets.delete(b.uid);
    S.ranAs.delete(b.uid);
    if (S.baseline === b.uid) S.baseline = S.builds[0].uid;
    if (S.editing === b.uid) S.editing = null;
    flash(`Removed ${name}.`, 'info');
    render();
  },
  makeMain: (b) => { S.baseline = b.uid; S.played = null; render(); },
};

const editorActions = {
  done: () => closeEditor(),
  simulate: () => simulate(),
};

// ------------------------------------------------------------------ running

async function simulate() {
  if (S.run && !S.run.finished) return;
  if ([S.rank?.run, S.duels?.run].some((r) => r && !r.finished)) { flash('A ranking or duel run is still going. Wait for it or stop it first.'); return; }
  hideTip();
  S.editing = null;
  const started = simulateAll();
  render();
  // Bring the scorecard into view, just under the run bar.
  requestAnimationFrame(() => {
    const r = $('results').getBoundingClientRect();
    if (r.top < 0 || r.top > innerHeight * 0.5) $('results').scrollIntoView({ behavior: reducedMotion() ? 'auto' : 'smooth', block: 'start' });
  });
  await started;
  if (S.run?.nothing) flash('These results are already up to date.', 'info');
}

function statusText() {
  const run = S.run;
  if (!run) return '';
  if (!run.finished) return `${scenarioMeta(run.test).title}, ${Math.min(run.i + 1, run.jobs.length)} of ${run.jobs.length} tests`;
  if (run.nothing) return 'Up to date';
  if (run.stopped) return `Stopped after ${run.i} of ${run.jobs.length} tests`;
  return `Done in ${fmtSec(run.ms / 1000)}`;
}

function updateProgress() {
  const run = S.run;
  if (!run) return;
  const bar = document.querySelector('.runbar .castbar');
  if (bar && !run.finished) {
    const pct = Math.min(99, Math.round(100 * run.frac));
    bar.querySelector('i').style.width = `${pct}%`;
    bar.querySelector('.castbar-text').textContent = statusText();
    bar.setAttribute('aria-valuenow', String(pct));
  }
  for (const el of document.querySelectorAll('.run-status')) el.textContent = run.finished ? statusText() : '';
}

// The run bar: assumptions and the last run on the left; accuracy (three
// hourglasses: more sand, more fights) and Simulate on the right. While a
// run goes, a cast bar takes the button's place.
let wasRunning = false;
function renderRunbar() {
  const run = S.run;
  const running = !!run && !run.finished;
  const n = S.builds.length;
  const tests = scorecardTests().length;
  const changedN = Object.keys(S.opts).length;
  const justDone = wasRunning && !running && run && !run.stopped;
  wasRunning = running;
  const pct = running ? Math.min(99, Math.round(100 * run.frac)) : 0;
  const go = running
    ? h('div.cast-wrap',
      h('div.castbar', { role: 'progressbar', 'aria-label': 'Simulating', 'aria-valuemin': '0', 'aria-valuemax': '100', 'aria-valuenow': String(pct) },
        h('i', { style: { width: `${pct}%` } }), h('span.castbar-text', statusText())),
      h('button.btn.btn-small.run-cancel', { type: 'button', onclick: cancelRun }, 'Cancel'))
    : h('button.primary.run-go#simulate', { type: 'button', onclick: simulate },
      h('span', n > 1 ? `Simulate ${n} builds` : 'Simulate'), h('span.sub', `${tests} tests at ${accuracyOf(S.accuracy).title} accuracy`));
  const accuracy = h('div.hourglasses', { role: 'group', 'aria-label': 'Accuracy' }, M.ACCURACY.map((a) => h('button', {
    type: 'button', 'aria-pressed': String(a.id === S.accuracy), disabled: running, onclick: () => { S.accuracy = a.id; render(); },
    title: `${a.note} of the true answer`,
  }, h('img.px', { src: `art/hourglass-${a.id}.png`, alt: '' }), h('span', a.title), h('small', a.short))));
  const assume = relevantOptions().length ? h('button.btn.btn-small.assume-btn', { type: 'button', onclick: openAssumptions },
    changedN ? `Settings (${changedN} changed)` : 'Settings') : null;
  // on phones the run bar is at the bottom, so the settings sit under the cards
  const assumePhone = assume ? assume.cloneNode(true) : null;
  if (assumePhone) assumePhone.addEventListener('click', openAssumptions);
  $('run-options').replaceChildren(...[assumePhone].filter(Boolean));
  $('runbar').classList.toggle('running', running);
  $('runbar').replaceChildren(h('div.runbar-inner',
    h('div.run-left', assume, h('p.run-status', { role: 'status', class: justDone ? 'flash' : run?.stopped ? 'stopped' : '' }, running ? '' : statusText())),
    h('div.run-right', accuracy, go)));
}

// ------------------------------------------------------------------ assumptions

// Where an assumption's default comes from (options.toml `source`).
const SOURCE = {
  game: 'From Forever’s game data', community: 'From players’ reports', classic: 'The original game’s value', estimate: 'Our estimate', choice: 'Your choice',
};

function relevantOptions() {
  const tags = new Set();
  for (const b of S.builds) { tags.add(b.cls); tags.add(b.race); }
  const opp = S.opponent && presetOf(S.opponent.slice(7));
  if (opp) { tags.add(opp.class); tags.add(opp.race); }
  return env.catalog.options.filter((o) => o.kind !== 'internal' && (!o.applies.length || o.applies.some((t) => tags.has(t)) || o.name in S.opts));
}

function setOpt(o, x) {
  if (!isFinite(x)) return;
  if (Math.abs(x - o.default) < 1e-12) delete S.opts[o.name]; else S.opts[o.name] = x;
  S.builds.forEach((b) => refreshLater(b));
  renderAssumptions();
  render();
}

function openAssumptions() {
  renderAssumptions();
  $('assumptions').showModal();
}

// The Assumptions panel's sections (options.toml `group`), in order.
const OPT_GROUPS = [
  ['play', 'How you play'],
  ['party', 'Your party in the boss test'],
  ['world', 'The leveling world'],
  ['', 'Game numbers'],
];

function renderAssumptions() {
  const list = relevantOptions();
  const fmtDefault = (o, ui) => ui.kind === 'toggle' ? (o.default ? 'on' : 'off') : ui.kind === 'choice' ? (o.choices[o.default] || o.default) : ui.kind === 'pct' ? `${+(o.default * 100).toFixed(4)}%` : `${o.default}${ui.unit && ui.unit !== '%' ? ` ${ui.unit}` : ui.unit || ''}`;
  const section = ([g, title]) => {
    const opts = list.filter((o) => (o.group || '') === g);
    return opts.length ? [h('h3.opt-group', title), h('div.opt-list', opts.map(row))] : [];
  };
  $('assumptions-body').replaceChildren(
    h('p.note', 'How you play, who you play with, and numbers the game doesn’t publish. Change one to see how much it matters.'),
    ...OPT_GROUPS.flatMap(section));
  function row(o) {
      const ui = o;
      const v = o.name in S.opts ? S.opts[o.name] : o.default;
      const isChanged = o.name in S.opts;
      const id = `opt-${o.name}`;
      let input;
      if (ui.kind === 'toggle') {
        input = h('input.check', { id, type: 'checkbox', checked: v >= 0.5, onchange: (e) => setOpt(o, e.target.checked ? 1 : 0) });
      } else if (ui.kind === 'choice') {
        input = h('div.opt-seg', { id, role: 'group', 'aria-label': ui.label },
          o.choices.map((c, i) => h('button', { type: 'button', 'aria-pressed': String(Math.round(v) === i), onclick: () => setOpt(o, i) }, c)));
      } else {
        const shown = ui.kind === 'pct' ? +(v * 100).toFixed(4) : v;
        input = h('span.numwrap',
          h('input.numbox.num', { id, type: 'number', step: 'any', min: '0', value: String(shown), onchange: (e) => setOpt(o, Number(e.target.value) / (ui.kind === 'pct' ? 100 : 1)) }),
          ui.kind === 'pct' || ui.unit === '%' ? h('span.unit', '%') : ui.unit ? h('span.unit', ui.unit) : null);
      }
      // a wax seal for where the number comes from
      const seal = o.source === 'game' ? 'game' : o.source === 'community' ? 'community' : 'estimate';
      return h('div.opt',
        h('img.opt-seal.px', { src: `art/seal-${seal}.png`, alt: '', title: SOURCE[o.source] || '' }),
        h('label.desc', { for: id, class: isChanged ? 'changed' : '' }, ui.label, ui.help ? h('span.opt-help', ui.help) : null,
          h('span.opt-source', { class: `src-${o.source}` }, SOURCE[o.source] || '', o.cite ? `: ${o.cite}` : '')),
        input,
        isChanged ? h('button.linkish.reset', { type: 'button', onclick: () => { delete S.opts[o.name]; S.builds.forEach((b) => refreshLater(b)); renderAssumptions(); render(); } }, `Back to the default (${fmtDefault(o, ui)})`) : null);
  }
}

// ------------------------------------------------------------------ sharing

async function share() {
  const url = `${location.origin}${location.pathname}#b=${encode(serial())}`;
  try {
    await navigator.clipboard.writeText(url);
    flash('Link copied. Anyone who opens it sees these builds.', 'info');
  } catch {
    window.prompt('Copy this link:', url);
  }
}

// ------------------------------------------------------------------ boot

async function boot() {
  setArtScale();
  const tips = M.LOADING_TIPS;
  $('boot-tip').textContent = `Tip: ${tips[Math.floor(Math.random() * tips.length)]}`;
  $('about-btn').addEventListener('click', () => $('about').showModal());
  $('about-link').addEventListener('click', () => $('about').showModal());
  $('share-btn').addEventListener('click', share);
  $('about-close').addEventListener('click', () => $('about').close());
  $('assumptions-close').addEventListener('click', () => $('assumptions').close());
  // the regular save waits for typing to settle: make sure nothing is lost on leaving
  addEventListener('pagehide', () => { if (env.catalog) saveLocal(); });
  document.addEventListener('pointerdown', (e) => {
    if (tipIsPinned() && !e.target.closest('#tooltip') && !e.target.closest('.talent')) hideTip();
  });
  addEventListener('scroll', () => { if (tipIsPinned()) repositionTip(); else hideTip(); }, { passive: true });
  $('editor').addEventListener('scroll', () => { if (tipIsPinned()) repositionTip(); else hideTip(); }, { passive: true });
  addEventListener('keydown', (e) => {
    if (e.key !== 'Escape') return;
    if (!$('tooltip').hidden) { hideTip(); return; }
    if (document.querySelector('dialog[open]')) return;
    if (S.editing) closeEditor();
  });

  try {
    ({ engine: env.engine, catalog: env.catalog } = await startEngine());
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
  $('data-build').textContent = env.catalog.data_build;
  $('engine-kind').textContent = env.engine.kind === 'webassembly' ? 'in your browser' : 'on the local test server';

  let from = null;
  if (location.hash.startsWith('#b=')) {
    try { from = restore(decode(location.hash.slice(3))) ? 'link' : null; } catch { from = null; }
    if (from) history.replaceState(null, '', location.pathname + location.search);
    else flash('That shared link could not be read.');
  }
  if (!from && loadLocal()) from = 'local';
  if (!from) S.builds = [buildFromPreset(presetOf(DEFAULT_PRESET) || env.catalog.presets[0], DEFAULT_LEVEL)];
  if (!buildOf(S.baseline)) S.baseline = S.builds[0].uid;
  loadRows();
  initRankings();
  // Builds already in the saved results count as run, for "since your last run".
  for (const b of S.builds) {
    if (Object.values(S.rows).some((r) => r.roles?.some((x) => x.sig === sig(b)))) S.ranAs.set(b.uid, { sig: sig(b), build: structuredClone(b), sheet: null });
  }
  // A first visit on a wide screen opens the editor on the starting build; a
  // shared link, a return visit or a phone starts with the results.
  if (!from && !narrow()) S.editing = S.builds[0].uid;
  render();
  for (const b of S.builds) refreshSheet(b);
  // the drawn stand-ins for missing game icons come last
  loadGlyphs().then(render);
}

boot();
