// Small DOM helpers shared by the page.
import { ICONS } from './icons.js';
import { iconColors } from './meta.js';

const SVG = 'http://www.w3.org/2000/svg';

// h('div.cls1.cls2', {attrs}, children...)
export function h(sel, attrs, ...kids) {
  const parts = sel.split(/(?=[.#])/);
  const tag = parts[0] && !/^[.#]/.test(parts[0]) ? parts.shift() : 'div';
  const el = document.createElement(tag);
  const cls = parts.filter((x) => x[0] === '.').map((x) => x.slice(1));
  const id = parts.find((x) => x[0] === '#');
  if (cls.length) el.className = cls.join(' ');
  if (id) el.id = id.slice(1);
  if (attrs != null && (typeof attrs !== 'object' || attrs instanceof Node || Array.isArray(attrs))) {
    kids.unshift(attrs);
    attrs = null;
  }
  for (const [k, v] of Object.entries(attrs || {})) {
    if (v == null || v === false) continue;
    if (k.startsWith('on')) el.addEventListener(k.slice(2).toLowerCase(), v);
    else if (k === 'style' && typeof v === 'object') {
      for (const [sk, sv] of Object.entries(v)) {
        if (sv == null) continue;
        if (sk.startsWith('--')) el.style.setProperty(sk, sv);
        else el.style[sk] = sv;
      }
    } else if (k === 'class') el.className += (el.className ? ' ' : '') + v;
    else if (k === 'html') el.innerHTML = v;
    else if (k in el && typeof v !== 'string') el[k] = v;
    else el.setAttribute(k, v === true ? '' : v);
  }
  append(el, kids);
  return el;
}

function append(el, kids) {
  for (const k of kids) {
    if (k == null || k === false) continue;
    if (Array.isArray(k)) append(el, k);
    else el.append(k instanceof Node ? k : document.createTextNode(String(k)));
  }
}

export function svgIcon(name, cls) {
  const s = document.createElementNS(SVG, 'svg');
  s.setAttribute('viewBox', '0 0 512 512');
  s.setAttribute('aria-hidden', 'true');
  if (cls) s.setAttribute('class', cls);
  const p = document.createElementNS(SVG, 'path');
  p.setAttribute('d', ICONS[name] || ICONS['rune-stone'] || '');
  s.append(p);
  return s;
}

// Game icons (Blizzard's art) are served from icons/, listed in
// icons/manifest.json ({name: file}). The deploy builds both with
// tools/fetch_icons.mjs. Icons not in the manifest are drawn instead
// (game-icons.net glyphs), so the page never asks for a file that is not there.
export const WOW_ICON_DIR = 'icons/';
let iconFiles = null;
const missingArt = new Set();

export async function loadIconManifest() {
  try {
    const r = await fetch(`${WOW_ICON_DIR}manifest.json`, { cache: 'no-cache' });
    iconFiles = r.ok ? await r.json() : {};
  } catch {
    iconFiles = {};
  }
  return Object.keys(iconFiles).length;
}

function iconUrl(wow) {
  if (!wow || missingArt.has(wow) || !iconFiles || !iconFiles[wow]) return null;
  return WOW_ICON_DIR + iconFiles[wow];
}

// The game icon `wow` if there is one, else the drawn glyph. The returned
// element swaps itself for the glyph if the image still fails to load.
export function art(glyph, wow, onFallback) {
  const url = iconUrl(wow);
  if (!url) return svgIcon(glyph);
  const img = h('img.game-art', { src: url, alt: '', decoding: 'async', draggable: 'false' });
  img.addEventListener('error', () => {
    missingArt.add(wow);
    img.replaceWith(svgIcon(glyph));
    if (onFallback) onFallback();
  }, { once: true });
  return img;
}

// A square ability icon: the game's own when available, else a painted glyph.
export function ability(name, colors, wow) {
  const [c1, c2] = colors || iconColors(name);
  const el = h('span.ability', { style: { '--c1': c1, '--c2': c2 } });
  if (iconUrl(wow)) el.classList.add('game');
  el.append(art(name, wow, () => el.classList.remove('game')));
  return el;
}

// ------------------------------------------------------------- tooltip

const tipEl = () => document.getElementById('tooltip');
let tipAnchor = null;

export function showTip(anchor, content, { pinned = false } = {}) {
  const el = tipEl();
  el.replaceChildren(...[content].flat());
  el.classList.toggle('pinned', pinned);
  el.hidden = false;
  tipAnchor = anchor;
  placeTip(anchor);
}

export function hideTip(anchor) {
  if (anchor && anchor !== tipAnchor) return;
  const el = tipEl();
  if (el) el.hidden = true;
  tipAnchor = null;
}

// After a re-render: if the element a tooltip belongs to was replaced, show
// the tooltip again on its replacement (same data-id), or hide it.
export function retargetTip() {
  const el = tipEl();
  if (!el || el.hidden || !tipAnchor || tipAnchor.isConnected) return;
  const id = tipAnchor.dataset && tipAnchor.dataset.id;
  const pinned = el.classList.contains('pinned');
  const next = id && document.querySelector(`.talent[data-id="${id}"]`);
  hideTip();
  if (next) next.dispatchEvent(new CustomEvent('showtip', { detail: { pinned } }));
}

export function repositionTip() {
  const el = tipEl();
  if (el && !el.hidden && tipAnchor && tipAnchor.isConnected) placeTip(tipAnchor);
  else hideTip();
}

export function tipIsPinned() {
  const el = tipEl();
  return el && !el.hidden && el.classList.contains('pinned');
}

function placeTip(anchor) {
  const el = tipEl();
  const r = anchor.getBoundingClientRect();
  const vw = document.documentElement.clientWidth;
  const vh = window.innerHeight;
  el.style.left = '0px';
  el.style.top = '0px';
  const w = el.offsetWidth;
  const ht = el.offsetHeight;
  let x = r.right + 12;
  if (x + w > vw - 8) x = r.left - w - 12;
  if (x < 8) x = Math.min(Math.max(8, r.left + r.width / 2 - w / 2), vw - w - 8);
  let y = r.top - 4;
  if (x === r.left - w - 12 || x === r.right + 12) {
    if (y + ht > vh - 8) y = vh - ht - 8;
  } else {
    y = r.bottom + 10;
    if (y + ht > vh - 8) y = r.top - ht - 10;
  }
  el.style.left = `${Math.round(x)}px`;
  el.style.top = `${Math.round(Math.max(8, y))}px`;
}

// Hover and keyboard-focus tooltips for simple elements.
export function withTip(el, content) {
  const show = () => showTip(el, typeof content === 'function' ? content() : content);
  el.addEventListener('mouseenter', show);
  el.addEventListener('focus', show);
  el.addEventListener('mouseleave', () => hideTip(el));
  el.addEventListener('blur', () => hideTip(el));
  return el;
}

export const tt = {
  title: (t) => h('div.tt-title', t),
  line: (t, color) => h(`div${color ? '.tt-' + color : ''}`, t),
  gap: (t, color) => h(`div.tt-gap${color ? '.tt-' + color : ''}`, t),
};

// ------------------------------------------------------------- red error text

export function flash(msg, kind) {
  const box = document.getElementById('errors');
  const d = h('div', { class: kind || '' }, msg);
  box.append(d);
  while (box.children.length > 3) box.firstChild.remove();
  setTimeout(() => d.remove(), 3300);
}

// ------------------------------------------------------------- numbers

export function fmtNum(x, digits = 0) {
  if (x == null || !isFinite(x)) return '–';
  return x.toLocaleString('en-GB', { minimumFractionDigits: digits, maximumFractionDigits: digits });
}

export function fmtSec(s) {
  if (s == null || !isFinite(s)) return '–';
  if (s < 90) return `${fmtNum(s, s < 10 ? 1 : 0)} sec`;
  const m = Math.floor(s / 60);
  const r = Math.round(s - m * 60);
  return `${m} min ${r} sec`;
}

// Format a metric value by kind. pct/rate values are 0..1.
export function fmtMetric(x, m) {
  if (!m) return fmtNum(x, 1);
  switch (m.kind) {
    case 'pct':
    case 'rate': {
      const v = x * 100;
      if (Math.abs(v) < 1e-9) return '0%';
      if (v < 0.1) return 'under 0.1%';
      return `${fmtNum(v, v < 10 ? 1 : 0)}%`;
    }
    case 'sec': return fmtSec(x);
    default: return fmtNum(x, m.digits ?? 1);
  }
}

// Differences: percentage points for shares and chances.
export function fmtDiff(x, m, signed = true) {
  const sign = signed ? (x > 0 ? '+' : x < 0 ? '−' : '') : '';
  const a = Math.abs(x);
  if (!m) return sign + fmtNum(a, 1);
  switch (m.kind) {
    case 'pct':
    case 'rate': return `${sign}${fmtNum(a * 100, a * 100 < 10 ? 1 : 0)} pts`;
    case 'sec': return sign + fmtSec(a);
    default: return sign + fmtNum(a, m.digits ?? 1);
  }
}

export function debounce(fn, ms) {
  let t = 0;
  return (...a) => {
    clearTimeout(t);
    t = setTimeout(() => fn(...a), ms);
  };
}
