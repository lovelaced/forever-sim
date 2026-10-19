// One test in detail, opened from its row, in four tabs: a summary (the
// headline number for each build and how they compare with the main build),
// how each build played (its priority list), one fight step by step, and
// every number side by side with where the damage, healing and casts come from.
import { h, withTip, tt, fmtMetric, fmtDiff, fmtNum } from './ui.js';
import { METRICS, GROUPS, UNIT_PLURAL } from './meta.js';
import { S, bus } from './state.js';
import { pairOf, duelWin } from './runner.js';
import { playedPanel, logPanel } from './played.js';

const TABS = [['summary', 'Summary'], ['played', 'How it played'], ['log', 'One fight'], ['numbers', 'Every number']];

const UNIT = {
  kills_per_hour: 'kills per hour', dps: 'damage per second', breakeven: 'attacker damage per second', win: 'of duels won',
  heal_breakeven: 'boss damage per second',
};

const PHRASE = {
  kills_per_hour: (a, up) => `gets ${a} ${up ? 'more' : 'fewer'} kills per hour than`,
  dps: (a, up) => `does ${a} ${up ? 'more' : 'less'} damage per second than`,
  breakeven: (a, up) => `survives attackers doing ${a} ${up ? 'more' : 'less'} damage per second than`,
  heal_breakeven: (a, up) => `heals through ${a} ${up ? 'more' : 'less'} boss damage per second than`,
};

const CHANGE = {
  kills_per_hour: (a, up) => `${up ? 'added' : 'cost'} ${a} kills per hour`,
  dps: (a, up) => `${up ? 'added' : 'cost'} ${a} damage per second`,
  breakeven: (a, up) => `${up ? 'lets it survive' : 'leaves it losing to'} attackers doing ${a} ${up ? 'more' : 'less'} damage per second`,
  heal_breakeven: (a, up) => `${up ? 'lets it heal through' : 'leaves it healing through'} ${a} ${up ? 'more' : 'less'} boss damage per second`,
};

export const rangeTip = () => [
  tt.title('How sure is this?'),
  tt.line('The true answer lands in this range about 19 times in 20.', 'gold'),
  tt.gap('Every fight is decided by dice rolls for hits, crits and procs. The simulator rolled them many times, and the range is how far luck alone could still move the answer. Running the test again replays the same dice, so you get the same numbers; Precise accuracy runs more fights to narrow it.'),
];

const median = (m) => (m && m.kind === 'median' ? { ...m, mean: m.median } : m);

// The smallest difference worth calling one: 0.5% of the value for amounts
// and times, a point for scores and rates, half a point for shares.
export function smdOf(m, base) {
  if (m && m.smd != null) return m.smd;
  const kind = m?.kind;
  if (kind === 'rate') return 0.01;
  if (kind === 'pct') return 0.005;
  return 0.005 * Math.abs(base || 0);
}

// What a paired difference says, in one of four words:
// clear: real (outside luck) and at least the smallest meaningful difference;
// small: real but smaller than that; same: not real, and the whole range
// sits inside that difference either way; unsure: anything else (too few
// fights to tell). `strict` asks for 2.81 standard errors instead of the
// range, for the long list of secondary numbers.
export function judge(p, m, base, strict = false) {
  if (!p || p.diff == null) return 'unsure';
  const d = smdOf(m, base ?? p.base);
  const real = strict && p.se != null ? Math.abs(p.diff) >= 2.81 * p.se : p.significant;
  if (real) return Math.abs(p.diff) >= d ? 'clear' : 'small';
  return p.lo != null && p.hi != null && p.lo >= -d && p.hi <= d ? 'same' : 'unsure';
}
export const metricOf = (res, i, name) => median(res.variants[i]?.metrics.find((m) => m.name === name));

const badge = (c) => h('span.slot', { style: { '--slot': c.color }, 'aria-hidden': 'true' }, c.letter);
const who = (c) => h('span.who', c.letter ? badge(c) : null, h('span.who-name', c.name));

// Builds tied with the leader (no clear difference on the same dice) share
// first place. Returns the set of column indexes in first place.
export function leaders(res, cols, metric, good = 'high') {
  const withVal = cols.filter((c) => c.cell && metricOf(res, c.cell.i, metric));
  if (withVal.length < 2) return new Set();
  const sign = good === 'low' ? -1 : 1;
  const top = withVal.reduce((a, c) => (sign * metricOf(res, c.cell.i, metric).mean > sign * metricOf(res, a.cell.i, metric).mean ? c : a));
  const out = new Set([top.k]);
  for (const c of withVal) {
    if (c === top) continue;
    const p = pairOf(res, metric, top.cell.i, c.cell.i);
    if (p && judge(p, METRICS[metric], metricOf(res, top.cell.i, metric)?.mean) !== 'clear') out.add(c.k);
  }
  return out;
}

function tabStrip(ctx, tabs, tab) {
  return h('div.detail-tabs', { role: 'tablist', 'aria-label': `${ctx.scenario.title} in detail` }, tabs.map(([t, label]) => h('button.detail-tab', {
    type: 'button', role: 'tab', 'aria-selected': String(t === tab), onclick: () => { S.detailTab = t; bus.detail(); },
  }, label)));
}

// ctx: {test, scenario (meta), row, cols: [{k, b, letter, color, name, cell}], base (col),
//       changes: Map uid -> change for this test, onPrecise, onOpponent, opponentPicker}
export function renderDetail(ctx) {
  const { row } = ctx;
  if (ctx.test === 'duel') return renderDuel(ctx);
  const res = row.res;
  const primary = res.scenario.primary;
  const pm = METRICS[primary] || { label: primary, kind: 'num', digits: 1 };
  const cols = ctx.cols.filter((c) => c.cell);
  const unitWord = UNIT_PLURAL[ctx.scenario.unit] || 'runs';
  const tab = TABS.some(([t]) => t === S.detailTab) ? S.detailTab : 'summary';
  const out = h('div.detail', tabStrip(ctx, TABS, tab));
  if (tab === 'played') { out.append(h('div.tab-panel', { role: 'tabpanel' }, playedPanel(ctx, cols, res))); return out; }
  if (tab === 'log') { out.append(h('div.tab-panel', { role: 'tabpanel' }, logPanel(ctx, cols, res))); return out; }
  if (tab === 'numbers') { out.append(h('div.tab-panel', { role: 'tabpanel' }, numbersPanel(ctx, cols, res, primary))); return out; }
  const panel = h('div.tab-panel', { role: 'tabpanel' });
  out.append(panel);

  panel.append(h('p.detail-meta', `${ctx.scenario.help} Level ${res.variants[0].build.level}.`));

  // ---------------------------------------------------------------- headline
  if (cols.length === 1) {
    const s = metricOf(res, cols[0].cell.i, primary);
    panel.append(h('div.headline',
      h('div.big-number.num', fmtMetric(s.mean, pm), UNIT[primary] ? h('small', UNIT[primary]) : null),
      h('p.range-line', withTip(h('span.q', { tabindex: '0' }, `${fmtMetric(s.lo, pm)} to ${fmtMetric(s.hi, pm)}`), rangeTip))));
  } else {
    const best = leaders(res, cols, primary, pm.good);
    const max = Math.max(...cols.map((c) => metricOf(res, c.cell.i, primary)?.hi ?? 0)) * 1.04 || 1;
    const sorted = [...cols].sort((a, b) => metricOf(res, b.cell.i, primary).mean - metricOf(res, a.cell.i, primary).mean);
    if (pm.good === 'low') sorted.reverse();
    panel.append(h('div.rank-rows', sorted.map((c) => {
      const s = metricOf(res, c.cell.i, primary);
      return h('div.rank-row', { style: { '--slot': c.color }, class: c.cell.current ? '' : 'stale' },
        h('div.who-line', who(c), c === ctx.base ? h('span.tag.main', 'Main build') : null,
          best.has(c.k) ? h('span.best', best.size > 1 ? 'Tied best' : 'Best') : null),
        h('div.val', fmtMetric(s.mean, pm), withTip(h('small.q', { tabindex: '0' }, `±${fmtDiff((s.hi - s.lo) / 2, pm, false)}`), rangeTip)),
        h('div.bar', h('i', { style: { width: `${(100 * Math.max(0, s.mean)) / max}%` } }),
          h('s', { style: { left: `${(100 * Math.max(0, s.lo)) / max}%`, width: `${(100 * (s.hi - s.lo)) / max}%` } })));
    })));
  }

  // ---------------------------------------------------------------- verdicts
  const verdicts = [];
  for (const c of cols) {
    if (c === ctx.base || !ctx.base?.cell) continue;
    const p = pairOf(res, primary, ctx.base.cell.i, c.cell.i);
    if (p) verdicts.push(verdict(p, pm, primary, c, ctx.base, res, ctx));
  }
  for (const c of cols) {
    const ch = ctx.changes.get(c.b.uid);
    if (ch) verdicts.push(changeLine(ch, pm, primary, c));
  }
  if (verdicts.length) panel.append(h('div.verdicts', verdicts));
  panel.append(finePrint(res, cols.length, ctx, unitWord));
  return out;
}

// Every measure side by side, and where the damage, healing and casts come from.
function numbersPanel(ctx, cols, res, primary) {
  const out = [];
  const names = Object.keys(METRICS).filter((k) => !METRICS[k].hidden && cols.some((c) => metricOf(res, c.cell.i, k)));
  const shown = names.filter((k) => !(METRICS[k].hideZero && cols.every((c) => Math.abs(metricOf(res, c.cell.i, k)?.mean ?? 0) < 1e-9)));
  const ordered = [primary, ...shown.filter((k) => k !== primary)];
  if (ordered.length > 1) {
    const table = h('table.stats-table',
      h('thead', h('tr', h('th', { scope: 'col' }, 'Measure'), cols.map((c) => h('th', { scope: 'col', style: { '--slot': c.color } }, cols.length > 1 ? who(c) : 'Average')))),
      h('tbody', ordered.map((k) => {
        const m = METRICS[k] || { label: k };
        return h('tr', h('th', { scope: 'row' }, m.label), cols.map((c) => {
          const s = metricOf(res, c.cell.i, k);
          const cell = h('td', s ? fmtMetric(s.mean, m) : '–');
          if (s && c !== ctx.base && ctx.base?.cell) {
            const p = pairOf(res, k, ctx.base.cell.i, c.cell.i);
            if (p) cell.append(delta(p, m, ctx.base, metricOf(res, ctx.base.cell.i, k)?.mean));
          }
          return cell;
        }));
      })));
    out.push(h('div.details', h('h4', 'Every measure'),
      ctx.scenario.group === 'healing' ? h('p.hint', 'Apart from the first line, these come from the hardest fight each build held.') : null,
      h('div.table-scroll', table)));
  }

  // ---------------------------------------------------------------- breakdowns, side by side
  const groups = Object.keys(GROUPS).filter((g) => cols.some((c) => res.variants[c.cell.i].metrics.some((m) => m.name.startsWith(g + ':'))));
  if (groups.length) {
    out.push(h('div.breakdown', h('h4', 'Where it comes from'),
      h('div.breakdown-cols', { style: { '--n': String(cols.length) } }, cols.map((c) =>
        h('div.breakdown-col', cols.length > 1 ? h('div.breakdown-who', who(c)) : null, meterGroups(res.variants[c.cell.i], groups, c))))));
  }
  return out;
}

function delta(p, m, base, baseVal) {
  // the long list asks more of a difference before it gets an arrow
  const clear = judge(p, m, baseVal, true) === 'clear';
  const better = m.good ? (p.diff > 0) === (m.good === 'high') : null;
  const cls = !clear || better === null ? '' : better ? 'good' : 'bad';
  const text = clear ? `${p.diff > 0 ? '▲' : '▼'} ${fmtDiff(Math.abs(p.diff), m, false)}` : 'about the same';
  return h('span.d', { class: cls }, text, h('span.sr-only', ` compared with ${base.name}`));
}

function verdict(p, pm, primary, c, base, res, ctx) {
  const up = p.diff > 0;
  const better = pm.good === 'low' ? !up : up;
  const bm = metricOf(res, base.cell.i, primary).mean;
  const j = judge(p, pm, bm);
  if (j === 'same' || j === 'small') {
    return h('div.verdict.tie', { style: { '--slot': c.color } },
      h('div', who(c), ' and ', who(base), ' are about the same here.'));
  }
  if (j !== 'clear') {
    return h('div.verdict.tie', { style: { '--slot': c.color } },
      h('div', who(c), ' and ', who(base), ' are too close to call.'),
      ctx.acc !== 'precise' ? h('div.sub', h('button.linkish', { type: 'button', onclick: ctx.onPrecise }, 'Look closer at Precise accuracy')) : null);
  }
  const a = fmtDiff(Math.abs(p.diff), pm, false);
  const phrase = (PHRASE[primary] || ((x, u) => `scores ${x} ${u ? 'higher' : 'lower'} on ${pm.label.toLowerCase()} than`))(a, up);
  const rel = bm !== 0 && pm.kind === 'num' ? ` (${fmtNum(Math.abs(100 * p.diff / bm), 0)}% ${up ? 'more' : 'less'})` : '';
  return h(`div.verdict.${better ? 'better' : 'worse'}`, { style: { '--slot': c.color } },
    h('div', who(c), ` ${phrase} `, who(base), rel, '.'));
}

// "Your last change to B added 4.2 damage per second" on the same fights.
function changeLine(ch, pm, primary, c) {
  const up = ch.diff > 0;
  if (judge(ch, pm) !== 'clear') {
    return h('div.verdict.change.tie', { style: { '--slot': c.color } },
      h('div', 'Your last change to ', who(c), ' made no clear difference here.'));
  }
  const better = pm.good === 'low' ? !up : up;
  const phrase = (CHANGE[primary] || ((a, u) => `moved ${pm.label.toLowerCase()} ${u ? 'up' : 'down'} by ${a}`))(fmtDiff(Math.abs(ch.diff), pm, false), up);
  return h(`div.verdict.change.${better ? 'better' : 'worse'}`, { style: { '--slot': c.color } },
    h('div', 'Your last change to ', who(c), ` ${phrase}.`));
}

// Only what the page doesn't already show: a run that hit its limit.
function finePrint(res, n, ctx, unitWord) {
  const note = res.converged ? null : h('p.fine-print', `Stopped at the limit of ${fmtNum(res.reps)} ${unitWord}, so the ranges are a little wider.`);
  const precise = ctx.acc !== 'precise' ? h('button.btn.btn-small', { type: 'button', onclick: ctx.onPrecise }, 'Run at Precise accuracy') : null;
  return note || precise ? h('div.detail-foot', note || h('span'), precise) : null;
}

const cap = (s) => s.charAt(0).toUpperCase() + s.slice(1);

function meterGroups(v, groups, c) {
  const out = [];
  for (const g of groups) {
    const rows = v.metrics.filter((m) => m.name.startsWith(g + ':')).map((m) => ({ name: cap(m.name.slice(g.length + 1)), mean: m.mean }))
      .filter((r) => r.mean > 1e-9).sort((a, b) => b.mean - a.mean);
    if (!rows.length) continue;
    const max = rows[0].mean;
    const kind = GROUPS[g].kind;
    const total = rows.reduce((a, r) => a + r.mean, 0);
    out.push(h('div.meter-group',
      h('h5', GROUPS[g].title),
      h('div.meter', rows.slice(0, 10).map((r) => h('div.meter-row', { style: { '--slot': c.color } },
        h('i', { style: { width: `${(100 * r.mean) / max}%` } }),
        h('span', h('b', r.name),
          h('em', kind === 'pct' ? `${fmtNum(100 * r.mean, 1)}%` : g.startsWith('dps') || g.startsWith('heal') || g === 'hps'
            ? `${fmtNum(r.mean, r.mean < 10 ? 1 : 0)} (${fmtNum(100 * r.mean / total, 0)}%)` : fmtNum(r.mean, r.mean < 10 ? 2 : 1))))))));
  }
  if (!out.length) out.push(h('p.hint', 'Nothing to break down for this build.'));
  return out;
}

// ------------------------------------------------------------------ duels

const pct = (x) => `${fmtNum(100 * Math.max(0, x), 0)}%`;

// A duel in two tabs: who beats whom, and one duel step by step.
const DUEL_TABS = [['summary', 'Summary'], ['log', 'One fight']];
function renderDuel(ctx) {
  const { row } = ctx;
  const cols = ctx.cols.filter((c) => c.cell);
  const tab = S.detailTab === 'log' ? 'log' : 'summary';
  const out = h('div.detail', tabStrip(ctx, DUEL_TABS, tab));
  const panel = h('div.tab-panel', { role: 'tabpanel' });
  out.append(panel);
  if (tab === 'log') {
    panel.append(logPanel(ctx, cols));
    return out;
  }
  if (row.one || cols.length < 2) {
    panel.append(h('p.detail-meta', 'To the death, three minutes at most. You start 5 to 10 yards apart, and whoever reacts first strikes first.'));
    panel.append(ctx.opponentPicker());
    panel.append(playLikePicker());
    const c = cols[0];
    const w = c && ctx.opponentSig && duelWin(row, c.cell.sig, ctx.opponentSig);
    if (w) panel.append(duelLine(c, { name: ctx.opponentName, color: '#9d9d9d', letter: null }, w));
    return out;
  }
  panel.append(h('p.detail-meta', 'Each pair duels to the death, three minutes at most. They start 5 to 10 yards apart, and whoever reacts first strikes first.'));
  const lines = [];
  for (const c of cols) {
    if (c === ctx.base) continue;
    const w = duelWin(row, c.cell.sig, ctx.base.cell.sig);
    if (w) lines.push(duelLine(c, ctx.base, w));
  }
  for (let x = 0; x < cols.length; x++) {
    for (let y = x + 1; y < cols.length; y++) {
      if (cols[x] === ctx.base || cols[y] === ctx.base) continue;
      const w = duelWin(row, cols[x].cell.sig, cols[y].cell.sig);
      if (w) lines.push(duelLine(cols[x], cols[y], w));
    }
  }
  panel.append(h('div.verdicts', lines));
  return out;
}

function duelLine(c, other, w) {
  const verdictCls = w.lo > 0.5 ? 'better' : w.hi < 0.5 ? 'worse' : 'tie';
  const head = w.lo > 0.5 ? ` beats ` : w.hi < 0.5 ? ` loses to ` : ` is evenly matched with `;
  const split = w.first != null && w.second != null ? `${pct(w.first)} when it reacts first, ${pct(w.second)} when the other does. ` : '';
  return h(`div.verdict.${verdictCls}`, { style: { '--slot': c.color } },
    h('div', who(c), head, who(other), `, scoring ${pct(w.mean)} (a win counts 1, a draw or a timeout half).`),
    h('div.sub', `${split}About ${fmtNum(w.duration, 0)} seconds a duel.`));
}

// How the duel opponent plays (the assumption opp_play_like).
const PLAY_LIKE = ['Same as you', 'Typical', 'Expert'];
export function playLikePicker() {
  const v = Math.round(S.opts.opp_play_like ?? 0);
  return h('div.field-row.play-like', h('span.label', 'Your opponent plays like'),
    h('div.quick', { role: 'group', 'aria-label': 'Your opponent plays like' }, PLAY_LIKE.map((t, i) => h('button.chip', {
      type: 'button', 'aria-pressed': String(v === i),
      onclick: () => { if (i === 0) delete S.opts.opp_play_like; else S.opts.opp_play_like = i; bus.render(); },
    }, t))));
}
