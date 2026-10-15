// Turning Monte Carlo summaries into plain English.
import { h, svgIcon, art, withTip, tt, fmtMetric, fmtDiff, fmtNum } from './ui.js';
import { METRICS, GROUPS, PRIMARY_HELP, UNIT_PLURAL } from './meta.js';

const UNIT = {
  kills_per_hour: 'kills per hour', dps: 'damage per second', breakeven: 'attacker damage per second', win: 'of duels won',
};

const PHRASE = {
  kills_per_hour: (a, up) => `gets ${a} ${up ? 'more' : 'fewer'} kills per hour than`,
  dps: (a, up) => `does ${a} ${up ? 'more' : 'less'} damage per second than`,
  breakeven: (a, up) => `can beat attackers doing ${a} ${up ? 'more' : 'less'} damage per second than`,
  win: (a, up) => `wins ${a.replace(' pts', ' percentage points')} ${up ? 'more' : 'less'} often than`,
};

const rangeTip = () => [
  tt.title('How sure is this?'),
  tt.line('This is a 95% confidence interval.', 'gold'),
  tt.gap('Every fight is decided by dice rolls for hits, crits and procs. The simulator rolled them many times; if you ran the whole test again, the average would almost always land inside this range.'),
];

// ctx: {scenario: {title, unit}, builds: [{name, color, barColor}], accuracy, stale, onPrecise}
export function renderResults(res, ctx) {
  const primary = res.scenario.primary;
  const pm = METRICS[primary] || { label: primary, kind: 'num', digits: 1 };
  const vars = res.variants.map((v, i) => {
    // Medians (e.g. the PvP breakeven) are reported by their median, which is
    // what their interval is for.
    const byName = Object.fromEntries(v.metrics.map((m) => [m.name, m.kind === 'median' ? { ...m, mean: m.median } : m]));
    return { ...v, i, byName, b: ctx.builds[i] || { name: v.build.name, color: '#ccc', barColor: '#ccc' } };
  });
  const unitWord = UNIT_PLURAL[ctx.scenario.unit] || 'runs';
  const lvl = vars[0].build.level;

  const out = h('div.results', { 'aria-live': 'polite' });
  out.append(h('div.results-head',
    h('h3', ctx.scenario.title),
    h('p.hint', `Level ${lvl}, ${fmtNum(res.reps)} simulated ${unitWord}${vars.length > 1 ? ' for each build' : ''}.`)));
  if (ctx.stale) out.append(h('div.notice', 'Your builds have changed since this run. Simulate again to see the effect.'));

  // ---------------------------------------------------------------- headline
  const head = h('div.headline',
    h('div.metric-name', pm.label),
    PRIMARY_HELP[primary] ? h('p.metric-help', PRIMARY_HELP[primary]) : null);
  if (vars.length === 1) {
    const s = vars[0].byName[primary];
    head.append(
      h('div.big-number.num', fmtMetric(s.mean, pm), UNIT[primary] && pm.kind === 'num' ? h('small', UNIT[primary]) : null),
      h('p.range-line', withTip(h('span.q', { tabindex: '0' }, 'Likely between'), rangeTip), ` ${fmtMetric(s.lo, pm)} and ${fmtMetric(s.hi, pm)}.`));
  } else {
    const max = Math.max(...vars.map((v) => v.byName[primary]?.hi ?? 0)) * 1.04 || 1;
    const sorted = [...vars].sort((a, b) => (b.byName[primary]?.mean ?? 0) - (a.byName[primary]?.mean ?? 0));
    if (pm.good === 'low') sorted.reverse();
    const rows = h('div.rank-rows');
    sorted.forEach((v, k) => {
      const s = v.byName[primary];
      const tied = k === 0 && sorted.length > 1 && overlapsBest(res, vars, sorted, primary);
      rows.append(h('div.rank-row', { style: { '--cc': v.b.color } },
        h('div.who', v.b.glyph ? h('span.who-icon', art(v.b.glyph, v.b.icon)) : null, h('span.who-name', v.b.name),
          k === 0 ? h('span.best', { class: tied ? 'close' : '' }, tied ? 'Ahead, but close' : 'Best') : null),
        h('div.val', fmtMetric(s.mean, pm), h('small', `±${fmtDiff((s.hi - s.lo) / 2, pm, false)}`)),
        h('div.bar', { style: { '--cc': v.b.barColor } },
          h('i', { style: { width: `${(100 * Math.max(0, s.mean)) / max}%` } }),
          h('s', { style: { left: `${(100 * Math.max(0, s.lo)) / max}%`, width: `${(100 * (s.hi - s.lo)) / max}%` } }))));
    });
    head.append(rows);
    head.append(h('p.hint', { style: { marginTop: '10px' } },
      'The thin marks on each bar show the range the true average very likely sits in. ',
      withTip(h('span.q', { tabindex: '0', style: { borderBottom: '1px dotted' } }, 'What does that mean?'), rangeTip)));
  }
  out.append(head);

  // ---------------------------------------------------------------- verdicts
  if (vars.length > 1) {
    const box = h('div.verdicts');
    for (const p of res.paired.filter((p) => p.metric === primary)) {
      const vi = vars.findIndex((v) => v.build.name === p.variant);
      const v = vars[vi > 0 ? vi : 1];
      const base = vars[0];
      box.append(verdict(p, pm, primary, v, base, ctx));
    }
    out.append(box);
  }

  // ---------------------------------------------------------------- details table
  const names = Object.keys(METRICS).filter((k) => k !== primary && !METRICS[k].hidden && vars[0].byName[k]);
  const shown = names.filter((k) => !(METRICS[k].hideZero && vars.every((v) => Math.abs(v.byName[k]?.mean ?? 0) < 1e-9)));
  if (shown.length) {
    const table = h('table.stats-table',
      h('thead', h('tr', h('th', 'Measure'), vars.map((v) => h('th', { style: { '--cc': v.b.color } }, vars.length > 1 ? v.b.name : 'Average')))),
      h('tbody', [primary, ...shown].map((k) => {
        const m = METRICS[k] || { label: k };
        return h('tr', h('td', m.label), vars.map((v) => {
          const s = v.byName[k];
          const cell = h('td', s ? fmtMetric(s.mean, m) : '–');
          if (v.i > 0 && s) {
            const p = res.paired.find((p) => p.metric === k && p.variant === v.build.name);
            if (p) {
              const better = m.good ? (p.diff > 0) === (m.good === 'high') : null;
              const cls = !p.significant || better === null ? '' : better ? 'good' : 'bad';
              cell.append(h('span.d', { class: cls }, p.significant ? fmtDiff(p.diff, m) : 'about the same'));
            }
          }
          return cell;
        }));
      })));
    out.append(h('div.details', h('h4', 'All measures'), h('div', { style: { overflowX: 'auto' } }, table)));
  }

  // ---------------------------------------------------------------- breakdown meters
  const groups = Object.keys(GROUPS).filter((g) => vars.some((v) => v.metrics.some((m) => m.name.startsWith(g + ':'))));
  if (groups.length) {
    const wrap = h('div.breakdown', h('h4', 'Breakdown'));
    let pick = 0;
    const body = h('div');
    const draw = () => {
      body.replaceChildren(...meterGroups(vars[pick], groups));
      tabs?.querySelectorAll('button').forEach((b, i) => b.setAttribute('aria-pressed', String(i === pick)));
    };
    const tabs = vars.length > 1 ? h('div.mini-tabs', vars.map((v, i) =>
      h('button', { type: 'button', style: { '--cc': v.b.color }, onclick: () => { pick = i; draw(); } }, v.b.name))) : null;
    if (tabs) wrap.append(tabs);
    wrap.append(body);
    draw();
    out.append(wrap);
  }

  // ---------------------------------------------------------------- fine print
  const fp = h('p.fine-print',
    vars.length > 1
      ? `Every build faced exactly the same dice rolls, so the differences come from the builds, not from luck. `
      : '',
    res.converged
      ? `The simulation kept going until the headline number was pinned down to within about ±${fmtNum(ctx.accuracy.precision * 100, 1).replace(/\.0$/, '')}%. `
      : `The simulation hit its limit of ${fmtNum(res.reps)} ${unitWord} before reaching the accuracy you asked for, so the ranges are wider than usual. `,
    `Ranges are 95% confidence intervals. Game data build ${res.data_build}, random seed ${res.seed}.`);
  out.append(fp);
  return out;
}

function overlapsBest(res, vars, sorted, primary) {
  const a = sorted[0].byName[primary];
  const b = sorted[1].byName[primary];
  // Use the paired test when one of them is the baseline; otherwise the plain intervals.
  const p = res.paired.find((p) => p.metric === primary && ((p.variant === sorted[0].build.name && sorted[1].i === 0) || (p.variant === sorted[1].build.name && sorted[0].i === 0)));
  if (p) return !p.significant;
  return a.lo <= b.hi;
}

function verdict(p, pm, primary, v, base, ctx) {
  const up = p.diff > 0;
  const better = pm.good === 'low' ? !up : up;
  const name = (x) => h('span.who', { style: { color: x.b.color } }, x.b.name);
  const vm = v.byName[primary].mean;
  const bm = base.byName[primary].mean;
  if (!p.significant) {
    return h('div.verdict.tie',
      h('div', name(v), ' and ', name(base), ' are too close to call.'),
      h('div.sub', `The gap (${fmtDiff(p.diff, pm)}) is smaller than the simulation's margin of error. `,
        ctx.accuracy.id !== 'precise' ? h('button.linkish', { type: 'button', onclick: ctx.onPrecise }, 'Run again with Precise accuracy') : 'Treat them as equal.',
        ctx.accuracy.id !== 'precise' ? ' to look closer.' : ''));
  }
  const a = fmtDiff(Math.abs(p.diff), pm, false);
  const phrase = (PHRASE[primary] || ((a, up) => `scores ${a} ${up ? 'higher' : 'lower'} on ${pm.label.toLowerCase()} than`))(a, up);
  const rel = bm !== 0 && pm.kind === 'num' ? ` (${fmtNum(Math.abs(100 * p.diff / bm), 0)}% ${up ? 'higher' : 'lower'})` : '';
  const lo = Math.min(Math.abs(p.lo), Math.abs(p.hi));
  const hi = Math.max(Math.abs(p.lo), Math.abs(p.hi));
  return h(`div.verdict.${better ? 'better' : 'worse'}`,
    h('div', name(v), ` ${phrase} `, name(base), rel, '.'),
    h('div.sub', `${fmtMetric(vm, pm)} against ${fmtMetric(bm, pm)}. The real gap is very likely between ${fmtDiff(lo, pm, false)} and ${fmtDiff(hi, pm, false)}.`));
}

const cap = (s) => s.charAt(0).toUpperCase() + s.slice(1);

function meterGroups(v, groups) {
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
      h('div.meter', rows.slice(0, 10).map((r, k) => h('div.meter-row', { style: { '--cc': v.b.barColor } },
        h('i', { style: { width: `${(100 * r.mean) / max}%` } }),
        h('span', h('b', { style: { fontWeight: 500 } }, `${k + 1}. ${r.name}`),
          h('em', kind === 'pct' ? `${fmtNum(100 * r.mean, 1)}%` : g.startsWith('dps') || g.startsWith('heal')
            ? `${fmtNum(r.mean, r.mean < 10 ? 1 : 0)} (${fmtNum(100 * r.mean / total, 0)}%)` : fmtNum(r.mean, r.mean < 10 ? 2 : 1))))))));
  }
  if (!out.length) out.push(h('p.hint', 'No breakdown for this build.'));
  return out;
}

export function emptyResults() {
  return h('div.empty-results', svgIcon('rolling-dices'), h('p', 'Pick a test and press Simulate. Results appear here.'));
}
