// One test in detail, opened from its row, in four tabs: a summary (the
// headline number for each build and how they compare with the main build),
// how each build played (its priority list), one fight step by step, and
// every number side by side with where the damage, healing and casts come from.
import { h, withTip, tt, fmtMetric, fmtDiff, fmtNum } from './ui.js';
import { METRICS, GROUPS, PRIMARY_HELP, UNIT_PLURAL } from './meta.js';
import { S, bus } from './state.js';
import { pairOf, duelWin } from './runner.js';
import { playedPanel, logPanel } from './played.js';

const TABS = [['summary', 'Summary'], ['played', 'How it played'], ['log', 'One fight'], ['numbers', 'Every number']];

const UNIT = {
  kills_per_hour: 'kills per hour', dps: 'damage per second', breakeven: 'attacker damage per second', win: 'of duels won',
};

const PHRASE = {
  kills_per_hour: (a, up) => `gets ${a} ${up ? 'more' : 'fewer'} kills per hour than`,
  dps: (a, up) => `does ${a} ${up ? 'more' : 'less'} damage per second than`,
  breakeven: (a, up) => `survives attackers doing ${a} ${up ? 'more' : 'less'} damage per second than`,
};

const CHANGE = {
  kills_per_hour: (a, up) => `${up ? 'added' : 'cost'} ${a} kills per hour`,
  dps: (a, up) => `${up ? 'added' : 'cost'} ${a} damage per second`,
  breakeven: (a, up) => `${up ? 'lets it survive' : 'leaves it losing to'} attackers doing ${a} ${up ? 'more' : 'less'} damage per second`,
};

export const rangeTip = () => [
  tt.title('How sure is this?'),
  tt.line('This is a 95% confidence interval.', 'gold'),
  tt.gap('Every fight is decided by dice rolls for hits, crits and procs. The simulator rolled them many times; if you ran the whole test again, the average would almost always land inside this range.'),
];

const median = (m) => (m && m.kind === 'median' ? { ...m, mean: m.median } : m);
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
    if (p && !p.significant) out.add(c.k);
  }
  return out;
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
  const tabs = h('div.detail-tabs', { role: 'tablist', 'aria-label': `${ctx.scenario.title} in detail` }, TABS.map(([t, label]) => h('button.detail-tab', {
    type: 'button', role: 'tab', 'aria-selected': String(t === tab), onclick: () => { S.detailTab = t; bus.detail(); },
  }, label)));
  const out = h('div.detail', tabs);
  if (tab === 'played') { out.append(h('div.tab-panel', { role: 'tabpanel' }, playedPanel(ctx, cols, res))); return out; }
  if (tab === 'log') { out.append(h('div.tab-panel', { role: 'tabpanel' }, logPanel(ctx, cols, res))); return out; }
  if (tab === 'numbers') { out.append(h('div.tab-panel', { role: 'tabpanel' }, numbersPanel(ctx, cols, res, primary), finePrint(res, cols.length, ctx, unitWord))); return out; }
  const panel = h('div.tab-panel', { role: 'tabpanel' });
  out.append(panel);

  panel.append(h('p.detail-meta',
    `Level ${res.variants[0].build.level}. ${fmtNum(res.reps)} simulated ${unitWord}${cols.length > 1 ? ' for each build' : ''}, at ${ctx.accLabel} accuracy. `,
    PRIMARY_HELP[primary] || ''));

  // ---------------------------------------------------------------- headline
  if (cols.length === 1) {
    const s = metricOf(res, cols[0].cell.i, primary);
    panel.append(h('div.headline',
      h('div.big-number.num', fmtMetric(s.mean, pm), UNIT[primary] ? h('small', UNIT[primary]) : null),
      h('p.range-line', withTip(h('span.q', { tabindex: '0' }, 'Likely between'), rangeTip), ` ${fmtMetric(s.lo, pm)} and ${fmtMetric(s.hi, pm)}.`)));
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
        h('div.val', fmtMetric(s.mean, pm), h('small', `±${fmtDiff((s.hi - s.lo) / 2, pm, false)}`)),
        h('div.bar', h('i', { style: { width: `${(100 * Math.max(0, s.mean)) / max}%` } }),
          h('s', { style: { left: `${(100 * Math.max(0, s.lo)) / max}%`, width: `${(100 * (s.hi - s.lo)) / max}%` } })));
    })));
    panel.append(h('p.hint.bar-hint', 'The thin marks show the range the true average very likely sits in. ',
      withTip(h('span.q', { tabindex: '0' }, 'What does that mean?'), rangeTip)));
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
  panel.append(h('p.hint.more-tabs', 'Open ', h('button.linkish', { type: 'button', onclick: () => { S.detailTab = 'played'; bus.detail(); } }, 'How it played'),
    ' to see each build’s priority list, or ', h('button.linkish', { type: 'button', onclick: () => { S.detailTab = 'log'; bus.detail(); } }, 'One fight'),
    ' to watch a fight step by step.'));
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
            if (p) cell.append(delta(p, m, ctx.base));
          }
          return cell;
        }));
      })));
    out.push(h('div.details', h('h4', 'Every measure'), h('div.table-scroll', table)));
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

function delta(p, m, base) {
  const better = m.good ? (p.diff > 0) === (m.good === 'high') : null;
  const cls = !p.significant || better === null ? '' : better ? 'good' : 'bad';
  const text = p.significant ? `${p.diff > 0 ? '▲' : '▼'} ${fmtDiff(Math.abs(p.diff), m, false)}` : 'about the same';
  return h('span.d', { class: cls }, text, h('span.sr-only', ` compared with ${base.name}`));
}

function verdict(p, pm, primary, c, base, res, ctx) {
  const up = p.diff > 0;
  const better = pm.good === 'low' ? !up : up;
  const vm = metricOf(res, c.cell.i, primary).mean;
  const bm = metricOf(res, base.cell.i, primary).mean;
  if (!p.significant) {
    return h('div.verdict.tie', { style: { '--slot': c.color } },
      h('div', who(c), ' and ', who(base), ' are too close to call.'),
      h('div.sub', `The gap (${fmtDiff(p.diff, pm)}) is smaller than the margin of error. `,
        ctx.acc !== 'precise' ? [h('button.linkish', { type: 'button', onclick: ctx.onPrecise }, 'Run this test at Precise accuracy'), ' to look closer.'] : 'Treat them as equal.'));
  }
  const a = fmtDiff(Math.abs(p.diff), pm, false);
  const phrase = (PHRASE[primary] || ((x, u) => `scores ${x} ${u ? 'higher' : 'lower'} on ${pm.label.toLowerCase()} than`))(a, up);
  const rel = bm !== 0 && pm.kind === 'num' ? ` (${fmtNum(Math.abs(100 * p.diff / bm), 0)}% ${up ? 'more' : 'less'})` : '';
  const [lo, hi] = [Math.abs(p.lo), Math.abs(p.hi)].sort((x, y) => x - y);
  return h(`div.verdict.${better ? 'better' : 'worse'}`, { style: { '--slot': c.color } },
    h('div', who(c), ` ${phrase} `, who(base), rel, '.'),
    h('div.sub', `${fmtMetric(vm, pm)} against ${fmtMetric(bm, pm)}. The real gap is very likely between ${fmtDiff(lo, pm, false)} and ${fmtDiff(hi, pm, false)}.`));
}

// "Your last change to B added 4.2 damage per second" on the same fights.
function changeLine(ch, pm, primary, c) {
  const up = ch.diff > 0;
  const range = `likely between ${fmtDiff(ch.lo, pm)} and ${fmtDiff(ch.hi, pm)}`;
  if (!ch.significant) {
    return h('div.verdict.change.tie', { style: { '--slot': c.color } },
      h('div', 'Your last change to ', who(c), ' made no clear difference here.'),
      h('div.sub', `${fmtDiff(ch.diff, pm)} on the same ${fmtNum(ch.n)} fights, ${range}.`));
  }
  const better = pm.good === 'low' ? !up : up;
  const phrase = (CHANGE[primary] || ((a, u) => `moved ${pm.label.toLowerCase()} ${u ? 'up' : 'down'} by ${a}`))(fmtDiff(Math.abs(ch.diff), pm, false), up);
  return h(`div.verdict.change.${better ? 'better' : 'worse'}`, { style: { '--slot': c.color } },
    h('div', 'Your last change to ', who(c), ` ${phrase}.`),
    h('div.sub', `Measured on the same ${fmtNum(ch.n)} fights as before: ${range}.`));
}

function finePrint(res, n, ctx, unitWord) {
  return h('div.detail-foot',
    h('p.fine-print',
      n > 1 ? 'Every build faced exactly the same dice rolls, so the differences come from the builds, not from luck. ' : '',
      res.converged
        ? `The simulation kept going until the headline number was pinned down to within about ±${fmtNum(ctx.precision * 100, 1).replace(/\.0$/, '')}%. `
        : `The simulation stopped at its limit of ${fmtNum(res.reps)} ${unitWord}, so the ranges are a little wider than asked for. `,
      n > 1 ? 'With this many comparisons, an occasional "clear" difference can still be chance. ' : '',
      `Ranges are 95% confidence intervals. Game data build ${res.data_build}, random seed ${res.seed}.`),
    ctx.acc !== 'precise' ? h('button.btn.btn-small', { type: 'button', onclick: ctx.onPrecise }, 'Run this test at Precise accuracy') : null);
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
          h('em', kind === 'pct' ? `${fmtNum(100 * r.mean, 1)}%` : g.startsWith('dps') || g.startsWith('heal')
            ? `${fmtNum(r.mean, r.mean < 10 ? 1 : 0)} (${fmtNum(100 * r.mean / total, 0)}%)` : fmtNum(r.mean, r.mean < 10 ? 2 : 1))))))));
  }
  if (!out.length) out.push(h('p.hint', 'Nothing to break down for this build.'));
  return out;
}

// ------------------------------------------------------------------ duels

const pct = (x) => `${fmtNum(100 * x, 0)}%`;

function renderDuel(ctx) {
  const { row } = ctx;
  const out = h('div.detail');
  const cols = ctx.cols.filter((c) => c.cell);
  if (row.one || cols.length < 2) {
    out.append(h('p.detail-meta', 'Your build duels another build until one of them dies, with a three-minute limit. The duel is fought both ways round, since whoever opens it strikes first.'));
    out.append(ctx.opponentPicker());
    const c = cols[0];
    const w = c && ctx.opponentSig && duelWin(row, c.cell.sig, ctx.opponentSig);
    if (w) out.append(duelLine(c, { name: ctx.opponentName, color: '#9d9d9d', letter: null }, w));
    return out;
  }
  out.append(h('p.detail-meta', 'Each pair of builds duels until one dies, with a three-minute limit. Every duel is fought both ways round, because whoever opens it strikes first, and the two are averaged.'));
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
  out.append(h('div.verdicts', lines));
  out.append(h('div.detail-foot', h('p.fine-print',
    `${fmtNum(row.duel[0]?.res?.reps || 0)} duels each way for every pair. Win rates are pinned down to a few percentage points; ranges are 95% confidence intervals. Duels that hit the time limit count as neither a win nor a loss.`)));
  return out;
}

function duelLine(c, other, w) {
  const verdictCls = w.lo > 0.5 ? 'better' : w.hi < 0.5 ? 'worse' : 'tie';
  const head = w.lo > 0.5 ? ` beats ` : w.hi < 0.5 ? ` loses to ` : ` is evenly matched with `;
  return h(`div.verdict.${verdictCls}`, { style: { '--slot': c.color } },
    h('div', who(c), head, who(other), `, winning ${pct(w.mean)} of duels.`),
    h('div.sub', `Likely between ${pct(w.lo)} and ${pct(w.hi)}. ${c.name} wins ${pct(w.first)} when it strikes first and ${pct(w.second)} when ${other.name} does. Duels last ${fmtNum(w.duration, 0)} seconds on average.`));
}
