// Two tabs of a test's detail. "How it played": the build's priority list
// as a spellbook page, with how often each line fired in this test and its
// share of the damage. "One fight": the run's first fight (the same dice as
// the results) step by step, every move with the game's icon, and a chart of
// everyone's health above it. Both show the main build first; small portrait
// buttons switch to another build.
import * as M from './meta.js';
import { h, art, ability, withTip, tt, fmtNum, fmtSec } from './ui.js';
import { S, env, bus, classOf, raceOf, apiBuild, nameOf, sig, baselineBuild } from './state.js';
import { metricOf } from './results.js';

const traces = new Map(); // key -> {events, me, leveling} | {error} | 'loading'

// The build a panel shows: the one picked, else the main build.
function pickCol(cols) {
  const main = baselineBuild();
  return cols.find((c) => c.b.uid === S.played) || cols.find((c) => c.b === main) || cols[0];
}

function portrait(b) {
  return h('span.mini-portrait', art(M.RACE_ICONS[b.race] || 'rune-stone', raceOf(b.race)?.icon));
}

// "Showing Arms (main build)" with buttons for the other builds.
function whoseHead(cols, c, what) {
  const main = baselineBuild();
  const title = h('h4.whose', portrait(c.b), h('span', `${what} `, h('b', c.name)), c.b === main && cols.length > 1 ? h('span.tag.main', 'Main build') : null);
  if (cols.length < 2) return h('div.played-head', title);
  return h('div.played-head', title,
    h('div.whose-pick', { role: 'group', 'aria-label': 'Show another build' }, h('span.muted', 'Show:'),
      cols.map((x) => h('button.whose-btn', {
        type: 'button', 'aria-pressed': String(x === c), style: { '--slot': x.color }, title: x.name,
        onclick: () => { S.played = x.b.uid; bus.detail(); },
      }, h('span.slot', x.letter), portrait(x.b), h('span.whose-name', x.name)))));
}

const spellIcon = (icon, cls = '') => h(`span.ico${cls}`, ability('rune-stone', null, icon));

// ------------------------------------------------------------------ how it played

export function playedPanel(ctx, cols, res) {
  if (!cols.length) return null;
  const c = pickCol(cols);
  const rot = classOf(c.b.cls)?.rotations.find((r) => r.id === c.b.rotation);
  if (!rot) return h('p.hint', 'This build has no play style to show.');
  const v = res.variants[c.cell.i];
  const leveling = ctx.scenario.group === 'leveling';
  const per = (k) => metricOf(res, c.cell.i, `line_per_min:${k}`)?.mean ?? 0;
  const restPer = (k) => metricOf(res, c.cell.i, `rest_per_hour:${k}`)?.mean ?? 0;
  const share = damageShare(v, leveling);

  const setup = [];
  let opened = [];
  // only the buffs that apply (Deadly Poison replaces the off-hand Instant
  // Poison once it's learned); results from before this was counted show all
  const pre = (rot.precombat || []).map((n, k) => ({ name: n, icon: rot.precombat_icons?.[k] }))
    .filter((x, k) => (metricOf(res, c.cell.i, `precombat_used:${k}`)?.mean ?? 1) > 0.005);
  if (pre.length) {
    setup.push(h('div.setup-line', h('span.setup-label', 'Before a fight'),
      h('span.setup-items', pre.map((x) => h('span.setup-item', spellIcon(x.icon, '.s'), x.name)))));
  }
  if (rot.opener?.length) {
    // each opener move with how often it came up (one that can't be used,
    // like Charge still on cooldown, is skipped)
    const each = rot.opener.map((l, k) => ({ ...l, rate: metricOf(res, c.cell.i, `opener_per_fight:${k}`)?.mean ?? 0 }));
    opened = each.filter((l) => l.rate > 0.005);
    const unit = leveling ? 'pull' : 'fight';
    setup.push(h('div.setup-line', h('span.setup-label', 'Opens with'),
      h('span.setup-items', opened.length ? opened.map((l) => {
        const dmg = share.get(l.name);
        return h('span.setup-item', spellIcon(l.icon, '.s'), h('span', l.name,
          h('span.muted', ` ${l.rate > 0.99 ? `every ${unit}` : `${fmtNum(100 * l.rate, 0)}% of ${unit}s`}${dmg >= 0.01 ? `, ${pctText(dmg)} of damage` : ''}`)));
      }) : h('span.muted', 'none of its opening moves came up here'))));
  }

  const lines = rot.lines.map((l, k) => ({ ...l, k, rate: per(k), dmg: share.get(l.name) }));
  const used = lines.filter((l) => l.rate > 0.005);
  const all = S.playedAll === `${c.b.uid}|${ctx.test}`;
  const shown = all ? lines : used;
  const maxRate = Math.max(...used.map((l) => l.rate), 1e-9);
  const prio = h('ol.prio', shown.map((l) => h('li.prio-row', { class: l.rate > 0.005 ? '' : 'unused' },
    h('span.prio-n', String(l.k + 1)),
    spellIcon(l.icon),
    h('div.prio-what', h('b', l.name), h('span.prio-when', ruleText(l))),
    h('div.prio-stats',
      l.rate > 0.005 ? h('span', `${fmtRate(l.rate)} a minute`) : h('span.muted', 'not used here'),
      l.dmg ? h('span.muted', `${pctText(l.dmg)} of damage`) : null,
      l.rate > 0.005 ? h('span.prio-bar', h('i', { style: { width: `${(100 * l.rate) / maxRate}%` } })) : null))));
  const toggle = lines.length > used.length ? h('button.linkish.prio-toggle', {
    type: 'button', onclick: () => { S.playedAll = all ? null : `${c.b.uid}|${ctx.test}`; bus.detail(); },
  }, all ? 'Show only the lines that came up' : `Show all ${lines.length} lines, including ${lines.length - used.length} that never came up here`) : null;
  // damage that no line of the list casts: auto attacks, procs, pets, bleeds
  const named = new Set([...used, ...opened].map((l) => l.name));
  const other = [...share].filter(([n, x]) => !named.has(n) && x >= 0.01).sort((a, b) => b[1] - a[1]);
  const otherLine = other.length ? h('p.played-note', h('b', 'Damage from outside the list: '),
    other.map(([n, x]) => `${n === 'melee' ? 'auto attacks' : n} ${pctText(x)}`).join(', '), '.') : null;

  const rest = leveling && rot.rest?.length ? h('div.played-rest', h('h5', 'Between pulls'),
    h('ol.prio', rot.rest.map((l, k) => ({ ...l, k, rate: restPer(k) })).map((l) => h('li.prio-row', { class: l.rate > 0.05 ? '' : 'unused' },
      h('span.prio-n', String(l.k + 1)),
      spellIcon(l.icon),
      h('div.prio-what', h('b', l.name), h('span.prio-when', ruleText(l))),
      h('div.prio-stats', l.rate > 0.05 ? h('span', `${fmtNum(l.rate, l.rate < 10 ? 1 : 0)} an hour`) : h('span.muted', 'not used here')))))) : null;

  const stale = !c.cell.current ? h('p.notice', 'This build changed since the run, so these numbers are from its earlier version.') : null;
  return h('section.played.spellbook',
    whoseHead(cols, c, 'How it played:'), stale, ...setup,
    h('p.hint', `Whenever it can act, it casts the first line that applies. ${used.length} of ${lines.length} came up here.`),
    prio, toggle, otherLine, rest);
}

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

// ------------------------------------------------------------------ one fight

function traceKey(ctx, b) {
  return JSON.stringify([ctx.test, sig(b), S.opts, ctx.test === 'duel' ? S.opponent : null]);
}

export function logPanel(ctx, cols) {
  if (!cols.length) return null;
  const c = pickCol(cols);
  const leveling = ctx.scenario.group === 'leveling';
  const key = traceKey(ctx, c.b);
  const tr = traces.get(key);
  const box = h('section.played.fight', whoseHead(cols, c, leveling ? 'One pull:' : 'The first 30 seconds:'));
  if (!tr) {
    traces.set(key, 'loading');
    env.engine.call({ op: 'trace', scenario: ctx.test, build: apiBuild(c.b, nameOf(c.b)), opts: S.opts })
      .then((r) => { traces.set(key, r); bus.detail(); })
      .catch((e) => { traces.set(key, { error: e.message || String(e) }); bus.detail(); });
  }
  if (!tr || tr === 'loading') {
    box.append(h('p.hint.loading', 'Playing one fight…'));
    return box;
  }
  if (tr.error) {
    box.append(h('div.warn', tr.error));
    return box;
  }
  const t0 = tr.events[0]?.t || 0;
  const data = rows(tr, ctx.test);
  // the two sides speak for themselves; say so only when one is empty
  if (!data.rows.some((r) => r.theirs.length)) {
    box.append(h('p.hint', ctx.test === 'boss' ? 'The tank holds the boss, so nothing attacks you.' : 'Nothing attacked you in this fight.'));
  }
  box.append(healthChart(tr, t0, data.end));
  const LIMIT = 36;
  const showAll = S.logAll === key;
  const shown = showAll ? data.rows : data.rows.slice(0, LIMIT);
  box.append(h('ol.log', shown.map((r) => h('li.log-row', { class: r.cls || '', 'data-t': String(r.t) },
    h('span.log-t', clock(r.t)),
    h('div.log-mine', r.mine),
    h('div.log-theirs', r.theirs)))));
  if (data.rows.length > LIMIT) {
    box.append(h('button.linkish.log-more', { type: 'button', onclick: () => { S.logAll = showAll ? null : key; bus.detail(); } },
      showAll ? 'Show less' : `Show the rest of the fight (${data.rows.length - LIMIT} more moments)`));
  }
  return box;
}

const clock = (t) => (t < 60 ? `${fmtNum(t, 1)}s` : `${Math.floor(t / 60)}:${fmtNum(t % 60, 1).padStart(4, '0')}`);

// enemies: a generic portrait by kind
function foeFace(name) {
  const f = /^Boss|^Add/.test(name) ? 'face-boss' : /^Attacker/.test(name) ? 'face-attacker' : 'face-mob';
  return h('img.foe.px', { src: `art/${f}.png`, alt: '' });
}

const amount = (e, cls = '') => h(`span.amt${cls}`, {
  class: [e.crit ? 'crit' : '', e.tick ? 'tick' : '', `sch-${e.school || 'physical'}`].join(' '),
}, fmtNum(e.amount), e.crit ? h('span.crit-mark', ' crit') : null);

// Turns the event list into rows: a cast and what it did read as one item,
// auto attacks are quieter, buffs and debuffs come and go as small icons.
function rows(tr, test) {
  const me = tr.me;
  const pets = new Set(tr.pets || []);
  // who an enemy's attack landed on: you, or your pet
  const onWhom = (t) => (t === me ? 'you' : pets.has(t) ? `your ${t.toLowerCase()}` : null);
  const out = [];
  const t0 = tr.events[0]?.t || 0;
  let end = 0;
  const row = (t, cls = '') => {
    const last = out[out.length - 1];
    if (last && Math.abs(last.t - (t - t0)) < 0.005 && last.cls === cls) return last;
    const r = { t: t - t0, cls, mine: [], theirs: [], casts: [] };
    out.push(r);
    return r;
  };
  const item = (...kids) => h('span.log-item', ...kids);
  for (const e of tr.events) {
    end = Math.max(end, e.t - t0);
    const mine = e.team === 0;
    if (e.kind === 'pull') { out.push({ t: e.t - t0, cls: 'mark', mine: [item(h('b', e.stacks > 1 ? `Pulls ${fmtNum(e.stacks)} mobs` : 'Pulls a mob'))], theirs: [] }); continue; }
    if (e.kind === 'rest') { out.push({ t: e.t - t0, cls: 'mark', mine: [item(h('b', 'Fight over: resting'))], theirs: [] }); continue; }
    if (e.kind === 'eat') { out.push({ t: e.t - t0, cls: 'mark', mine: [item(`${e.name[0].toUpperCase()}${e.name.slice(1)}s for ${fmtSec(e.amount)}`)], theirs: [] }); continue; }
    if (e.kind === 'death') {
      const r = row(e.t);
      if (e.who === me) r.theirs.push(item(h('b.death', 'You die')));
      else r.mine.push(item(h('b.death', `${e.who} dies`)));
      continue;
    }
    const r = row(e.t);
    if (!mine) {
      if (e.kind === 'gain' && e.target === me) {
        // a debuff you put on an enemy; a shared one (a stun, a slow) takes
        // the icon of the spell you just cast
        const generic = !e.key || !e.key.includes('.');
        const icon = r.casts.find((x) => x.name === e.name)?.icon || (generic && r.casts.at(-1)?.icon) || e.icon;
        r.mine.push(item(spellIcon(icon, '.s.debuff'), h('span.aura', `${e.name} on ${e.who}${e.stacks > 1 ? ` (${fmtNum(e.stacks)})` : ''}`)));
      } else if (e.kind === 'gain' && e.who === me) {
        r.theirs.push(item(spellIcon(e.icon, '.s.debuff'), h('span.aura', `You’re hit by ${e.name}`)));
      } else if (e.kind === 'hit' && onWhom(e.target)) {
        const whom = onWhom(e.target);
        const name = e.name === 'melee' ? `hits ${whom}` : `${e.name} hits ${whom}`;
        if (e.amount < 0.5 && e.absorbed > 0) {
          r.theirs.push(item(foeFace(e.who), h('span', `${e.who} ${name}, `), h('span.absorb', `${e.via || 'a shield'} takes ${fmtNum(e.absorbed)}`)));
        } else {
          r.theirs.push(item(foeFace(e.who), h('span', `${e.who} ${name} `), amount(e, '.hurt'),
            e.absorbed > 0 ? h('span.absorb', ` (${fmtNum(e.absorbed)} absorbed)`) : null));
        }
      } else if ((e.kind === 'miss' || e.kind === 'dodge' || e.kind === 'parry') && onWhom(e.target)) {
        const whom = onWhom(e.target);
        const verb = e.kind === 'miss' ? (whom === 'you' ? 'avoid' : 'avoids') : whom === 'you' ? e.kind : `${e.kind}s`;
        r.theirs.push(item(foeFace(e.who), h('span.muted', `${whom[0].toUpperCase()}${whom.slice(1)} ${verb} ${e.who}’s attack`)));
      }
      continue;
    }
    const pet = e.pet ? `${e.who}: ` : '';
    // a cast and what it did at the same moment read as one item
    const cast = r.cast && r.cast.who === e.who ? r.cast : null;
    if (cast && e.kind === 'hit' && e.name === cast.name) {
      cast.el.append(' ', amount(e));
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
        const el = h('span', h('b', pet + e.name));
        const it = item(spellIcon(e.icon), el);
        attachTip(it, e);
        r.mine.push(it);
        r.cast = { el, name: e.name, who: e.who };
        r.casts.push({ name: e.name, icon: e.icon });
        break;
      }
      case 'hit': {
        const auto = /^melee|^auto shot|^wand/.test(e.name);
        const label = auto ? `${pet}${e.name === 'melee (off-hand)' ? 'Off-hand' : e.name === 'auto shot' ? 'Auto Shot' : 'Auto attack'}` : pet + e.name;
        r.mine.push(item({ class: auto ? 'auto' : '' }, spellIcon(e.icon, auto ? '.s' : '.s'), h('span', label, ' '), amount(e), e.tick ? h('span.muted', ' tick') : null));
        break;
      }
      case 'heal': r.mine.push(item(spellIcon(e.icon, '.s.buff'), h('span.heal', `${pet}${e.name} heals ${e.target && e.target !== me && e.target !== e.who ? `${e.target} ` : ''}`, h('b', `+${fmtNum(e.amount)}`), e.crit ? ' crit' : ''))); break;
      case 'miss': case 'dodge': case 'parry': r.mine.push(item(h('span.muted', `${pet}${e.kind === 'miss' ? 'a swing misses' : e.kind === 'dodge' ? 'a swing is dodged' : 'a swing is parried'}`))); break;
      case 'gain': r.mine.push(item(spellIcon(e.icon, '.s.buff'), h('span.aura', `${e.name}${e.stacks > 1 ? ` (${fmtNum(e.stacks)})` : ''}`))); break;
      case 'lose': r.mine.push(item({ class: 'faded' }, spellIcon(e.icon, '.s.off'), h('span.aura.off', `${e.name} ends`))); break;
      default: break;
    }
  }
  return { rows: out.filter((r) => r.mine.length || r.theirs.length), end };
}

// A spell's tooltip, like the game's: its name, and what it did here.
function attachTip(el, e) {
  el.tabIndex = 0;
  withTip(el, () => [tt.title(e.name), e.target ? tt.line(`on ${e.target}`, 'grey') : null, e.school && e.school !== 'physical' ? tt.line(`${e.school[0].toUpperCase()}${e.school.slice(1)} spell`, 'gold') : null].filter(Boolean));
}

// Everyone's health across the fight, from the health after each hit and
// heal. Clicking a point scrolls the log to that moment.
function healthChart(tr, t0, end) {
  const me = tr.me;
  const pets = new Set(tr.pets || []);
  const series = new Map([[me, [[0, 1]]]]); // name -> [[t, hp]]; you are always drawn
  for (const e of tr.events) {
    if (e.hp == null || !e.target) continue;
    if (!series.has(e.target)) series.set(e.target, [[0, 1]]);
    series.get(e.target).push([e.t - t0, e.hp]);
  }
  if (end <= 0) return null;
  const W = 600, H = 72;
  const x = (t) => (W * t) / end;
  const y = (v) => 4 + (H - 8) * (1 - v);
  // you in green, your pets in teal, enemies in reds and oranges
  const FOES = ['#ff7a64', '#ffb35a', '#e6739f'];
  let foeN = 0;
  const who = [me, ...[...series.keys()].filter((n) => pets.has(n)), ...[...series.keys()].filter((n) => n !== me && !pets.has(n))];
  const color = new Map(who.map((n) => [n, n === me ? '#7fd36a' : pets.has(n) ? '#5fc9b4' : FOES[foeN++ % FOES.length]]));
  const label = (n) => (n === me ? 'You' : pets.has(n) ? `Your ${n.toLowerCase()}` : n);
  const lines = who.map((n) => {
    let d = `M${x(0).toFixed(1)},${y(1).toFixed(1)}`;
    for (const [t, v] of series.get(n)) d += ` H${x(t).toFixed(1)} V${y(v).toFixed(1)}`;
    // still standing at the end: carry the line to the end of the fight
    if (series.get(n).at(-1)[1] > 0) d += ` H${x(end).toFixed(1)}`;
    const p = document.createElementNS('http://www.w3.org/2000/svg', 'path');
    p.setAttribute('d', d);
    p.setAttribute('stroke', color.get(n));
    return p;
  });
  const svg = document.createElementNS('http://www.w3.org/2000/svg', 'svg');
  svg.setAttribute('viewBox', `0 0 ${W} ${H}`);
  svg.setAttribute('preserveAspectRatio', 'none');
  svg.setAttribute('class', 'hp-lines');
  svg.setAttribute('aria-hidden', 'true');
  svg.append(...lines);
  const legend = h('div.hp-legend', who.map((n) => h('span', { style: { '--c': color.get(n) } }, label(n))));
  const chart = h('div.hp-chart', { title: 'Click to jump to that moment' }, svg);
  chart.addEventListener('click', (ev) => {
    const r = chart.getBoundingClientRect();
    const t = ((ev.clientX - r.left) / r.width) * end;
    const rowsEl = [...chart.closest('.fight').querySelectorAll('.log-row')];
    const target = rowsEl.find((el) => Number(el.dataset.t) >= t - 0.05) || rowsEl.at(-1);
    target?.scrollIntoView({ block: 'center', behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' });
    target?.classList.add('ping');
    setTimeout(() => target?.classList.remove('ping'), 900);
  });
  return h('div.hp-box', h('div.hp-head', h('span.hp-title', 'Health'), legend, h('span.muted.hp-end', clock(end))), chart);
}
