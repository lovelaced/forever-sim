// Talent trees: layout, the game's spending rules, and drawing.
import { h, ability, svgIcon, showTip, hideTip, tt } from './ui.js';
import { talentIcon, iconColors, TREE_STYLE } from './meta.js';

export const CELL = 40;
export const GAP = 14;
export const ROW_GAP = 20;

const prepared = new Map();

// Group a class's talents into trees with a row and column for each.
export function prepareClass(cls) {
  if (prepared.has(cls.id)) return prepared.get(cls.id);
  const byId = {};
  const trees = cls.trees.map((name, index) => ({ name, index, talents: [], rows: 0, used: {} }));
  for (const t0 of cls.talents) {
    const tree = trees.find((x) => x.name === t0.tree);
    const { icon: glyph, school } = talentIcon(cls.id, t0, tree.used);
    const t = { ...t0, tree: tree.index, treeName: tree.name, row: Math.round(t0.points / 5), col: 0, glyph, wow: t0.icon || null, colors: iconColors(glyph, school) };
    tree.talents.push(t);
    byId[t.id] = t;
  }
  for (const tree of trees) {
    const rows = new Map();
    for (const t of tree.talents) {
      if (!rows.has(t.row)) rows.set(t.row, []);
      rows.get(t.row).push(t);
    }
    for (const row of [...rows.keys()].sort((a, b) => a - b)) {
      const items = rows.get(row);
      const used = new Set();
      for (const t of items) {
        const p = t.requires && byId[t.requires];
        if (p && p.tree === t.tree && !used.has(p.col)) {
          t.col = p.col;
          t.placed = true;
          used.add(p.col);
        }
      }
      const pref = items.length <= 2 ? [1, 2, 0, 3] : [0, 1, 2, 3];
      for (const t of items) {
        if (t.placed) continue;
        t.col = pref.find((c) => !used.has(c)) ?? 0;
        used.add(t.col);
      }
    }
    tree.rows = Math.max(7, ...tree.talents.map((t) => t.row + 1));
  }
  const out = { id: cls.id, trees, byId };
  prepared.set(cls.id, out);
  return out;
}

export const budgetFor = (level) => Math.max(0, level - 9);
export const spent = (talents) => Object.values(talents).reduce((a, b) => a + b, 0);

function treePoints(prep, talents, tree, belowRow = Infinity) {
  let n = 0;
  for (const t of prep.trees[tree].talents) if (t.row < belowRow) n += talents[t.id] || 0;
  return n;
}

export function splitOf(prep, talents) {
  return prep.trees.map((tr) => treePoints(prep, talents, tr.index)).join('/');
}

// Why a point can't go into t (or null if it can).
export function learnBlock(prep, talents, t, budget) {
  const r = talents[t.id] || 0;
  if (r >= t.ranks) return 'Already at its highest rank.';
  const above = treePoints(prep, talents, t.tree, t.row);
  if (above < t.points) return `Requires ${t.points} points in ${t.treeName} talents.`;
  const p = t.requires && prep.byId[t.requires];
  if (p && (talents[p.id] || 0) < p.ranks) return `Requires ${p.ranks} ${p.ranks === 1 ? 'point' : 'points'} in ${p.name}.`;
  if (spent(talents) >= budget) return budget === 0 ? 'Talents start at level 10.' : 'No talent points left. Raise the level to learn more.';
  return null;
}

// Why a point can't come out of t (or null if it can).
export function unlearnBlock(prep, talents, t) {
  const r = talents[t.id] || 0;
  if (r <= 0) return 'No points to remove.';
  for (const u of prep.trees[t.tree].talents) {
    if ((talents[u.id] || 0) > 0 && u.requires === t.id) return `${u.name} depends on it.`;
  }
  const after = { ...talents, [t.id]: r - 1 };
  for (const u of prep.trees[t.tree].talents) {
    if ((after[u.id] || 0) > 0 && u.row > t.row && treePoints(prep, after, t.tree, u.row) < u.points) {
      return `${u.name} needs ${u.points} points above it.`;
    }
  }
  return null;
}

export function talentTooltip(prep, talents, t, budget, { touch = false, onLearn, onUnlearn } = {}) {
  const r = talents[t.id] || 0;
  const lb = learnBlock(prep, talents, t, budget);
  const ub = unlearnBlock(prep, talents, t);
  const out = [tt.title(t.name), tt.line(`Rank ${r}/${t.ranks}`)];
  const above = treePoints(prep, talents, t.tree, t.row);
  if (above < t.points) out.push(tt.line(`Requires ${t.points} points in ${t.treeName} talents`, 'red'));
  const p = t.requires && prep.byId[t.requires];
  if (p && (talents[p.id] || 0) < p.ranks) out.push(tt.line(`Requires ${p.ranks} ${p.ranks === 1 ? 'point' : 'points'} in ${p.name}`, 'red'));
  const text = t.text || 'No description.';
  out.push(tt.gap(t.ranks > 1 ? `At rank ${t.ranks}: ${text}` : text, 'gold'));
  if (!t.modelled) out.push(tt.gap('Not simulated yet: points here do not change the results.', 'grey'));
  if (touch) {
    out.push(h('div.tt-actions',
      h('button.btn', { type: 'button', disabled: !!lb, onclick: onLearn }, 'Learn'),
      h('button.btn.btn-dark', { type: 'button', disabled: !!ub, onclick: onUnlearn }, 'Unlearn')));
    if (lb && r < t.ranks) out.push(tt.gap(lb, 'red'));
  } else {
    if (!lb) out.push(tt.gap('Click to learn', 'green'));
    else if (r < t.ranks && !(above < t.points) && !(p && (talents[p.id] || 0) < p.ranks)) out.push(tt.gap(lb, 'red'));
    if (!ub) out.push(tt.line('Right-click to unlearn', 'green'));
  }
  return out;
}

// Draw all three trees. ctx: {prep, talents, budget, activeTree, onLearn(t, el), onUnlearn(t, el), onPickTree(i)}
export function renderTrees(ctx) {
  const { prep, talents, budget } = ctx;
  const style = TREE_STYLE[prep.id] || [];
  const trees = prep.trees.map((tree) => {
    const pts = treePoints(prep, talents, tree.index);
    const [tint, mark, treeIcon] = style[tree.index] || ['#2a2420', 'rune-stone', null];
    const grid = h('div.tree-grid', {
      style: { '--cell': `${CELL}px`, '--gap': `${ROW_GAP}px ${GAP}px`, width: `${4 * CELL + 3 * GAP}px`, margin: '0 auto' },
    });
    grid.append(arrows(prep, talents, tree));
    for (const t of tree.talents) grid.append(talentButton(ctx, t));
    return h('section.tree', { class: tree.index === ctx.activeTree ? 'active' : '', style: { '--tint': tint }, 'aria-label': `${tree.name} talents` },
      svgIcon(mark, 'watermark'),
      h('div.tree-head', h('h4', ability(mark, null, treeIcon), tree.name), h('span.pts', h('b', pts), pts === 1 ? ' point' : ' points')),
      grid);
  });
  const tabs = h('div.tree-tabs', { role: 'tablist' }, prep.trees.map((tree) =>
    h('button.chip', { type: 'button', 'aria-pressed': String(tree.index === ctx.activeTree), onclick: () => ctx.onPickTree(tree.index) },
      `${tree.name} (${treePoints(prep, talents, tree.index)})`)));
  return [tabs, h('div.trees', trees)];
}

let lastPointer = 'mouse';
addEventListener('pointerdown', (e) => { lastPointer = e.pointerType; }, true);

function talentButton(ctx, t) {
  const { prep, talents, budget } = ctx;
  const r = talents[t.id] || 0;
  const lb = learnBlock(prep, talents, t, budget);
  const state = r >= t.ranks ? 'maxed' : r > 0 ? 'partial' : lb && !/No talent points|level 10/.test(lb) ? 'locked' : spent(talents) >= budget ? 'locked' : 'open';
  const btn = h('button.talent', {
    type: 'button', class: `${state}${t.modelled ? '' : ' ghost'}`, 'data-id': t.id,
    style: { gridRow: String(t.row + 1), gridColumn: String(t.col + 1) },
    'aria-label': `${t.name}, rank ${r} of ${t.ranks}`,
  }, ability(t.glyph, t.colors, t.wow), h('span.rank', `${r}/${t.ranks}`));
  const tip = (touch) => showTip(btn, talentTooltip(prep, ctx.talents, t, ctx.budget, {
    touch,
    onLearn: () => ctx.onLearn(t, true),
    onUnlearn: () => ctx.onUnlearn(t, true),
  }), { pinned: touch });
  btn.addEventListener('showtip', (e) => tip(!!e.detail.pinned));
  btn.addEventListener('mouseenter', () => { if (lastPointer !== 'touch') tip(false); });
  btn.addEventListener('focus', () => { if (lastPointer !== 'touch') tip(false); });
  btn.addEventListener('mouseleave', () => { if (lastPointer !== 'touch') hideTip(btn); });
  btn.addEventListener('blur', () => { if (lastPointer !== 'touch') hideTip(btn); });
  btn.addEventListener('click', (e) => {
    if (lastPointer === 'touch') { tip(true); return; }
    if (e.shiftKey) ctx.onUnlearn(t);
    else ctx.onLearn(t);
  });
  btn.addEventListener('contextmenu', (e) => {
    e.preventDefault();
    if (lastPointer === 'touch') { tip(true); return; }
    ctx.onUnlearn(t);
  });
  btn.addEventListener('keydown', (e) => {
    if (e.key === 'Backspace' || e.key === 'Delete' || e.key === '-') { e.preventDefault(); ctx.onUnlearn(t); }
    if (e.key === '+') { e.preventDefault(); ctx.onLearn(t); }
  });
  return btn;
}

function arrows(prep, talents, tree) {
  const NS = 'http://www.w3.org/2000/svg';
  const svg = document.createElementNS(NS, 'svg');
  svg.setAttribute('class', 'arrows');
  svg.setAttribute('aria-hidden', 'true');
  const cx = (c) => c * (CELL + GAP) + CELL / 2;
  const top = (r) => r * (CELL + ROW_GAP);
  for (const t of tree.talents) {
    const p = t.requires && prep.byId[t.requires];
    if (!p || p.tree !== t.tree) continue;
    const on = (talents[p.id] || 0) >= p.ranks;
    const x2 = cx(t.col);
    const yEnd = top(t.row) - 3;
    let d;
    if (p.col === t.col) {
      d = `M${x2} ${top(p.row) + CELL + 2} V${yEnd - 8}`;
    } else {
      const y1 = top(p.row) + CELL / 2;
      const x1 = p.col < t.col ? cx(p.col) + CELL / 2 + 3 : cx(p.col) - CELL / 2 - 3;
      d = `M${x1} ${y1} H${x2} V${yEnd - 8}`;
    }
    const path = document.createElementNS(NS, 'path');
    path.setAttribute('d', d);
    if (on) path.setAttribute('class', 'on');
    const head = document.createElementNS(NS, 'path');
    head.setAttribute('d', `M${x2 - 7} ${yEnd - 9} L${x2 + 7} ${yEnd - 9} L${x2} ${yEnd} Z`);
    head.setAttribute('class', on ? 'head on' : 'head');
    svg.append(path, head);
  }
  return svg;
}
