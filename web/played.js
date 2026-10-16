// "How it played", in a test's detail: the build's priority list in plain
// English with how often each line fired in this test and its share of the
// damage, then a timeline of one fight (the run's first, same dice).
import { h, fmtNum, fmtSec } from './ui.js';
import { S, env, bus, classOf, apiBuild, nameOf, sig, presetOf } from './state.js';
import { metricOf } from './results.js';

const traces = new Map(); // key -> {events, me, leveling} | {error} | 'loading'

export function playedSection(ctx, cols, res) {
  if (!cols.length) return null;
  const pick = cols.find((c) => c.b.uid === S.played) || cols[0];
  const c = pick;
  const rot = classOf(c.b.cls)?.rotations.find((r) => r.id === c.b.rotation);
  if (!rot) return null;
  const v = res.variants[c.cell.i];
  const leveling = ctx.scenario.group === 'leveling';
  const per = (k) => metricOf(res, c.cell.i, `line_per_min:${k}`)?.mean ?? 0;
  const restPer = (k) => metricOf(res, c.cell.i, `rest_per_hour:${k}`)?.mean ?? 0;
  const share = damageShare(v, leveling);

  const head = h('div.played-head',
    h('h4', cols.length > 1 ? 'How each build played' : `How ${c.name} played`),
    cols.length > 1 ? h('div.quick', { role: 'group', 'aria-label': 'Build' }, cols.map((x) => h('button.chip', {
      type: 'button', 'aria-pressed': String(x === c), style: { '--slot': x.color }, onclick: () => { S.played = x.b.uid; bus.render(); },
    }, x.letter ? `${x.letter} · ${x.name}` : x.name))) : null);

  const setup = [];
  let opened = [];
  // only the buffs that apply (Deadly Poison replaces the off-hand Instant
  // Poison once it's learned); results from before this was counted show all
  const pre = rot.precombat?.filter((n, k) => (metricOf(res, c.cell.i, `precombat_used:${k}`)?.mean ?? 1) > 0.005) || [];
  if (pre.length) setup.push(h('p.played-note', h('b', 'Before a fight: '), list(pre), '.'));
  if (rot.opener?.length) {
    // each opener move with how often it came up (one that can't be used,
    // like Charge still on cooldown, is skipped)
    const each = rot.opener.map((l, k) => ({ ...l, rate: metricOf(res, c.cell.i, `opener_per_fight:${k}`)?.mean ?? 0 }));
    opened = each.filter((l) => l.rate > 0.005);
    const unit = leveling ? 'pull' : 'fight';
    setup.push(h('p.played-note', h('b', 'Opens with: '), opened.length
      ? list(opened.map((l) => {
        const dmg = share.get(l.name);
        return `${l.name} (${l.rate > 0.99 ? `every ${unit}` : `${fmtNum(100 * l.rate, 0)}% of ${unit}s`}${dmg >= 0.01 ? `, ${pctText(dmg)} of damage` : ''})`;
      }))
      : 'none of its opening moves came up here', '.'));
  }

  const lines = rot.lines.map((l, k) => ({ ...l, k, rate: per(k), dmg: share.get(l.name) }));
  const used = lines.filter((l) => l.rate > 0.005);
  const all = S.playedAll === `${c.b.uid}|${ctx.test}`;
  const shown = all ? lines : used;
  const prio = h('ol.prio', shown.map((l) => h('li.prio-row', { class: l.rate > 0.005 ? '' : 'unused' },
    h('span.prio-n', String(l.k + 1)),
    h('div.prio-what', h('b', l.name), h('span.prio-when', ruleText(l))),
    h('div.prio-stats',
      l.rate > 0.005 ? h('span', `${fmtRate(l.rate)} a minute`) : h('span.muted', 'not used here'),
      l.dmg ? h('span.muted', `${pctText(l.dmg)} of damage`) : null))));
  const toggle = lines.length > used.length ? h('button.linkish.prio-toggle', {
    type: 'button', onclick: () => { S.playedAll = all ? null : `${c.b.uid}|${ctx.test}`; bus.render(); },
  }, all ? 'Show only the lines that came up' : `Show all ${lines.length} lines, including ${lines.length - used.length} that never came up here`) : null;
  // damage that no line of the list casts: auto attacks, procs, pets, bleeds
  const named = new Set([...used, ...opened].map((l) => l.name));
  const other = [...share].filter(([n, x]) => !named.has(n) && x >= 0.01).sort((a, b) => b[1] - a[1]);
  const otherLine = other.length ? h('p.played-note', h('b', 'Damage from outside the list: '),
    other.map(([n, x]) => `${n === 'melee' ? 'auto attacks' : n} ${pctText(x)}`).join(', '), '.') : null;

  const rest = leveling && rot.rest?.length ? h('div.played-rest', h('h5', 'Between pulls'),
    h('ol.prio', rot.rest.map((l, k) => ({ ...l, k, rate: restPer(k) })).map((l) => h('li.prio-row', { class: l.rate > 0.05 ? '' : 'unused' },
      h('span.prio-n', String(l.k + 1)),
      h('div.prio-what', h('b', l.name), h('span.prio-when', ruleText(l))),
      h('div.prio-stats', l.rate > 0.05 ? h('span', `${fmtNum(l.rate, l.rate < 10 ? 1 : 0)} an hour`) : h('span.muted', 'not used here')))))) : null;

  const stale = !c.cell.current ? h('p.notice', 'This build changed since the run, so these numbers are from its earlier version.') : null;
  return h('section.played',
    head, stale, ...setup,
    h('p.hint', `The simulated player goes down this list every time it can act and uses the first line that applies. ${used.length} of ${lines.length} lines came up in this test${leveling ? '; times a minute count only time spent fighting' : ''}.`),
    prio, toggle, otherLine, rest,
    timeline(ctx, c, leveling));
}

const list = (names) => (names.length < 2 ? names.join('') : `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`);
const pctText = (x) => `${fmtNum(100 * x, x < 0.1 ? 1 : 0)}%`;

// The line's own condition plus what the spell itself needs.
function ruleText(l) {
  if (l.when && l.needs && l.needs.includes(l.when)) return `when ${l.needs}`;
  if (l.when && l.needs && l.when.includes(l.needs)) return `when ${l.when}`;
  if (l.when && l.needs) return `when ${l.when}, and ${l.needs}`;
  if (l.when || l.needs) return `when ${l.when || l.needs}`;
  return 'whenever it’s ready';
}
const fmtRate = (x) => fmtNum(x, x < 1 ? 2 : 1);

// Damage share per ability name: the boss and ambushes report damage per
// second by ability, leveling reports shares.
function damageShare(v, leveling) {
  const out = new Map();
  const ms = v.metrics || [];
  if (leveling) {
    for (const m of ms) if (m.name.startsWith('dmg_share:')) out.set(m.name.slice(10), m.mean);
    return out;
  }
  const dps = ms.filter((m) => m.name.startsWith('dps:'));
  const total = dps.reduce((a, m) => a + m.mean, 0);
  if (total > 0) for (const m of dps) out.set(m.name.slice(4), m.mean / total);
  return out;
}

// ------------------------------------------------------------------ timeline

function traceKey(ctx, b) {
  return JSON.stringify([ctx.test, sig(b), S.opts, ctx.test === 'duel' ? S.opponent : null]);
}

function timeline(ctx, c, leveling) {
  const key = traceKey(ctx, c.b);
  const tr = traces.get(key);
  const box = h('div.timeline-box', h('h5', leveling ? 'One pull, step by step' : 'The first 30 seconds, step by step'));
  if (!tr) {
    traces.set(key, 'loading');
    env.engine.call({ op: 'trace', scenario: ctx.test, build: apiBuild(c.b, nameOf(c.b)), opts: S.opts })
      .then((r) => { traces.set(key, r); bus.render(); })
      .catch((e) => { traces.set(key, { error: e.message || String(e) }); bus.render(); });
  }
  if (!tr || tr === 'loading') {
    box.append(h('p.hint', 'Playing one fight…'));
    return box;
  }
  if (tr.error) {
    box.append(h('div.warn', tr.error));
    return box;
  }
  box.append(h('p.hint', leveling
    ? 'The first fight of the run, from the pull to the rest after it. Your actions are on the left; damage you take is on the right.'
    : 'The first fight of the run, on the same dice as the results above. Your actions are on the left; damage you take is on the right.'));
  box.append(h('ol.timeline', rows(tr).map((r) => h('li.tl-row', { class: r.cls || '' },
    h('span.tl-t', `${fmtNum(r.t, 1)}s`),
    h('span.tl-mine', r.mine || ''),
    h('span.tl-theirs', r.theirs || '')))));
  return box;
}

// Turns the event list into readable rows: casts merged with what they did,
// auto attacks folded in, buffs and procs as they come and go.
function rows(tr) {
  const me = tr.me;
  const out = [];
  const t0 = tr.events[0]?.t || 0;
  const row = (t, cls) => {
    const last = out[out.length - 1];
    if (last && Math.abs(last.t - (t - t0)) < 0.005 && last.cls === cls) return last;
    const r = { t: t - t0, cls, mine: [], theirs: [] };
    out.push(r);
    return r;
  };
  const add = (arr, x) => { if (arr.length) arr.push(h('span.tl-sep', ' · ')); arr.push(x); };
  for (const e of tr.events) {
    const mine = e.team === 0;
    if (e.kind === 'pull') { out.push({ t: e.t - t0, cls: 'tl-mark', mine: [h('b', e.stacks > 1 ? `Pulls ${fmtNum(e.stacks)} mobs` : 'Pulls a mob')], theirs: [] }); continue; }
    if (e.kind === 'rest') { out.push({ t: e.t - t0, cls: 'tl-mark', mine: [h('b', 'Fight over: resting')], theirs: [] }); continue; }
    if (e.kind === 'eat') { out.push({ t: e.t - t0, cls: 'tl-mark', mine: [`${e.name[0].toUpperCase()}${e.name.slice(1)}s for ${fmtSec(e.amount)}`], theirs: [] }); continue; }
    if (e.kind === 'death') { const r = row(e.t, ''); add(e.who === me ? r.theirs : r.mine, h('b', `${e.who === me ? 'You die' : `${short(e.who)} dies`}`)); continue; }
    const r = row(e.t, '');
    if (!mine) {
      // done to you: hits and heals on you, debuffs others put on you
      if (e.kind === 'gain' && e.target === me) add(r.mine, h('span.tl-aura', `+ ${e.name} on ${short(e.who)}${e.stacks > 1 ? ` (${fmtNum(e.stacks)})` : ''}`));
      else if (e.kind === 'hit') add(r.theirs, h('span', `${short(e.who)} ${e.name === 'melee' ? 'hits' : e.name}`, h('span.tl-num', ` ${fmtNum(e.amount)}${e.crit ? ' crit' : ''}`)));
      else if (e.kind === 'miss' || e.kind === 'dodge' || e.kind === 'parry') add(r.theirs, h('span.muted', `you ${e.kind === 'miss' ? 'avoid' : e.kind} an attack`));
      continue;
    }
    const pet = e.pet ? `${short(e.who)}: ` : '';
    // a cast and what it did at the same moment read as one item
    const cast = r.cast && r.cast.who === e.who ? r.cast : null;
    if (cast && e.kind === 'hit' && e.name === cast.name) {
      cast.el.append(h('span.tl-num', ` ${fmtNum(e.amount)}${e.crit ? ' crit' : ''}`));
      r.cast = null;
      continue;
    }
    if (cast && (e.kind === 'miss' || e.kind === 'dodge' || e.kind === 'parry')) {
      cast.el.append(h('span.muted', e.kind === 'miss' ? ' misses' : e.kind === 'dodge' ? ' is dodged' : ' is parried'));
      r.cast = null;
      continue;
    }
    switch (e.kind) {
      case 'cast': {
        const el = h('b', pet + e.name);
        add(r.mine, el);
        r.cast = { el, name: e.name, who: e.who };
        break;
      }
      case 'hit': {
        const what = e.name === 'melee' || e.name === 'melee (off-hand)' || e.name === 'auto shot' ? h('span.muted', `${pet}${e.name}`) : h('span', pet + e.name);
        add(r.mine, h('span', what, h('span.tl-num', ` ${fmtNum(e.amount)}${e.crit ? ' crit' : ''}${e.tick ? ' (tick)' : ''}`)));
        break;
      }
      case 'heal': add(r.mine, h('span.tl-heal', `${pet}${e.name} heals ${fmtNum(e.amount)}${e.crit ? ' crit' : ''}`)); break;
      case 'miss': case 'dodge': case 'parry': add(r.mine, h('span.muted', `${pet}${e.kind === 'miss' ? 'a swing misses' : e.kind === 'dodge' ? 'a swing is dodged' : 'a swing is parried'}`)); break;
      case 'gain': add(r.mine, h('span.tl-aura', `+ ${e.name}${e.stacks > 1 ? ` (${fmtNum(e.stacks)})` : ''}`)); break;
      case 'lose': add(r.mine, h('span.tl-aura.off', `${e.name} ends`)); break;
      default: break;
    }
  }
  return out.filter((r) => r.mine.length || r.theirs.length);
}

const short = (name) => name.replace(/\s*\(.*\)\s*$/, '');
