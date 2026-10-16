// Forever Sim front end: set up to three builds, run them through every
// test, and read how they compare. Results come first; the build editor
// folds away and opens from a build card.
import { startEngine } from './engine.js';
import * as M from './meta.js';
import { h, svgIcon, loadIconManifest, retargetTip, hideTip, flash, fmtSec, debounce, tipIsPinned, repositionTip } from './ui.js';
import {
  S, env, bus, MAX_BUILDS, DEFAULT_PRESET, DEFAULT_LEVEL, presetOf, buildOf, nameOf, baselineBuild, copyBuild, buildFromPreset,
  scenarioMeta, accuracyOf, sig, serial, restore, loadLocal, saveLocal, loadRows, encode, decode,
} from './state.js';
import { renderCards } from './cards.js';
import { renderEditor, refreshSheet, refreshLater } from './editor.js';
import { renderScorecard } from './scorecard.js';
import { simulateAll, cancelRun, scorecardTests } from './runner.js';

const $ = (id) => document.getElementById(id);
const reducedMotion = () => matchMedia('(prefers-reduced-motion: reduce)').matches;
const narrow = () => matchMedia('(max-width: 760px)').matches;

const persist = debounce(saveLocal, 300);

function render() {
  if (!env.catalog) return;
  renderCards($('builds'), cardActions);
  renderRunbar();
  renderEditor($('editor'), editorActions);
  renderScorecard($('results'), { onRender: render });
  retargetTip();
  persist();
}

bus.render = render;
bus.cards = () => renderCards($('builds'), cardActions);
bus.progress = () => updateProgress();

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
  makeBaseline: (b) => { S.baseline = b.uid; render(); },
};

const editorActions = {
  done: () => closeEditor(),
  simulate: () => simulate(),
};

// ------------------------------------------------------------------ running

async function simulate() {
  if (S.run && !S.run.finished) return;
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
  if (!run.finished) return [h('b', scenarioMeta(run.test).title), ` · ${Math.min(run.i + 1, run.jobs.length)} of ${run.jobs.length}`];
  if (run.nothing) return 'Up to date';
  if (run.stopped) return `Stopped after ${run.i} of ${run.jobs.length}`;
  return `Done in ${fmtSec(run.ms / 1000)}`;
}

function updateProgress() {
  const run = S.run;
  const bar = document.querySelector('.run-progress');
  const status = document.querySelectorAll('.run-status');
  if (!run || !bar || !status.length) return;
  const running = !run.finished;
  bar.classList.toggle('on', running);
  bar.firstChild.style.width = `${running ? Math.min(99, Math.round(100 * run.frac)) : 100}%`;
  for (const el of status) el.replaceChildren(...[statusText()].flat());
}

function renderRunbar() {
  const run = S.run;
  const running = !!run && !run.finished;
  const n = S.builds.length;
  const tests = scorecardTests().length;
  const changedN = Object.keys(S.opts).length;
  const go = running
    ? h('button.btn.run-cancel', { type: 'button', onclick: cancelRun }, 'Cancel')
    : h('button.primary.run-go#simulate', { type: 'button', onclick: simulate },
      svgIcon('rolling-dices'), h('span', n > 1 ? `Simulate ${n} builds` : 'Simulate'), h('span.go-sub', `in ${tests} tests`));
  // Accuracy sits right above the Simulate button; assumptions sit to the
  // left in the run bar on wide screens and above the results on phones.
  const accuracy = h('div.segmented.acc', { role: 'group', 'aria-label': 'Accuracy' }, M.ACCURACY.map((a) => h('button', {
    type: 'button', 'aria-pressed': String(a.id === S.accuracy), disabled: running, onclick: () => { S.accuracy = a.id; render(); },
  }, a.title, h('span', a.short))));
  const assume = () => (relevantOptions().length ? h('button.quiet.assume-btn', { type: 'button', onclick: openAssumptions },
    changedN ? `Assumptions (${changedN} changed)` : 'Assumptions') : null);
  $('run-options').replaceChildren(...[assume()].filter(Boolean));
  $('runbar').classList.toggle('running', running);
  $('runbar').replaceChildren(
    h('div.runbar-inner',
      h('div.run-left', assume(), h('div.run-status', { role: 'status' }, statusText())),
      h('div.go-stack', accuracy, h('div.run-status.mini', { 'aria-hidden': 'true' }, statusText()), go)),
    h('div.run-progress', { class: running ? 'on' : '', 'aria-hidden': 'true' }, h('i', { style: { width: `${running ? Math.round(100 * run.frac) : 0}%` } })));
}

// ------------------------------------------------------------------ assumptions

// Where an assumption's default comes from (options.toml `source`).
const SOURCE = {
  game: 'From Forever’s game data', community: 'From players’ reports', classic: 'The original game’s value', estimate: 'Our estimate',
};

function relevantOptions() {
  const tags = new Set();
  for (const b of S.builds) { tags.add(b.cls); tags.add(b.race); }
  const opp = S.opponent && presetOf(S.opponent.slice(7));
  if (opp) { tags.add(opp.class); tags.add(opp.race); }
  return env.catalog.options.filter((o) => !o.applies.length || o.applies.some((t) => tags.has(t)) || o.name in S.opts);
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

function renderAssumptions() {
  const list = relevantOptions();
  const fmtDefault = (o, ui) => ui.kind === 'toggle' ? (o.default ? 'on' : 'off') : ui.kind === 'pct' ? `${+(o.default * 100).toFixed(4)}%` : `${o.default}${ui.unit && ui.unit !== '%' ? ` ${ui.unit}` : ui.unit || ''}`;
  $('assumptions-body').replaceChildren(
    h('p.note', 'Some numbers the game does not publish. These are the simulator’s best guesses; change one to see how much it matters. Only the ones that affect your builds are listed. Changing one puts the results out of date until you simulate again.'),
    h('div.opt-list', list.map((o) => {
      const ui = o;
      const v = o.name in S.opts ? S.opts[o.name] : o.default;
      const isChanged = o.name in S.opts;
      const id = `opt-${o.name}`;
      let input;
      if (ui.kind === 'toggle') {
        input = h('input.check', { id, type: 'checkbox', checked: v >= 0.5, onchange: (e) => setOpt(o, e.target.checked ? 1 : 0) });
      } else {
        const shown = ui.kind === 'pct' ? +(v * 100).toFixed(4) : v;
        input = h('span.numwrap',
          h('input.numbox.num', { id, type: 'number', step: 'any', min: '0', value: String(shown), onchange: (e) => setOpt(o, Number(e.target.value) / (ui.kind === 'pct' ? 100 : 1)) }),
          ui.kind === 'pct' || ui.unit === '%' ? h('span.unit', '%') : ui.unit ? h('span.unit', ui.unit) : null);
      }
      return h('div.opt',
        h('label.desc', { for: id, class: isChanged ? 'changed' : '' }, ui.label, ui.help ? h('span.opt-help', ui.help) : null,
          h('span.opt-source', { class: `src-${o.source}` }, SOURCE[o.source] || '', o.cite ? `: ${o.cite}` : '')),
        input,
        isChanged ? h('button.linkish.reset', { type: 'button', onclick: () => { delete S.opts[o.name]; S.builds.forEach((b) => refreshLater(b)); renderAssumptions(); render(); } }, `Back to the default (${fmtDefault(o, ui)})`) : null);
    })));
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
  // Builds already in the saved results count as run, for "since your last run".
  for (const b of S.builds) {
    if (Object.values(S.rows).some((r) => r.roles?.some((x) => x.sig === sig(b)))) S.ranAs.set(b.uid, { sig: sig(b), build: structuredClone(b), sheet: null });
  }
  // A first visit on a wide screen opens the editor on the starting build; a
  // shared link, a return visit or a phone starts with the results.
  if (!from && !narrow()) S.editing = S.builds[0].uid;
  render();
  for (const b of S.builds) refreshSheet(b);
}

boot();
