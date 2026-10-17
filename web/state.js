// Page state, lookups, build helpers and saving. Every other module reads
// and changes `S`, then calls `bus.render()`.
import * as M from './meta.js';
import { prepareClass } from './talents.js';

export const MAX_BUILDS = 3;
export const DEFAULT_PRESET = 'warrior/arms.toml';
// The beta's level cap: a new visitor starts here.
export const DEFAULT_LEVEL = 30;
const STORE_KEY = 'forever-sim/v1';
const ROWS_KEY = 'forever-sim/rows/v1';

// Set once at boot.
export const env = { engine: null, catalog: null };

// Filled in by app.js so modules can ask for a redraw without importing it.
export const bus = { render: () => {}, progress: () => {} };

let uidSeq = 1;

export const S = {
  builds: [],
  baseline: null, // uid of the main build: the others are compared with it, and its fights are shown first
  editing: null, // uid of the build open in the editor, or null when it is folded
  accuracy: 'standard',
  opponent: null, // one-build duel: 'preset:<file>' or null
  opts: {},
  tab: 'talents',
  tree: null,
  sheets: new Map(), // uid -> character sheet (or {error})
  rows: {}, // test id -> results of its latest run (see runner.js)
  open: null, // test whose detail is open
  run: null, // the run in progress or just finished
  ranAs: new Map(), // uid -> {sig, build, sheet} as of its latest run
  cols: new Map(), // `${uid}|${test}` -> {sig, env, build, values}: per-fight results, for "what your change did"
  changes: new Map(), // `${uid}|${test}` -> {fromSig, toSig, from, diff, lo, hi, significant}
};

// ------------------------------------------------------------------ lookups

export const classOf = (id) => env.catalog.classes.find((c) => c.id === id);
// A class's play styles for its own fights (the healing one is for the healing tests).
export const soloRotations = (c) => c.rotations.filter((r) => !r.healer);
export const raceOf = (id) => env.catalog.races.find((r) => r.id === id);
export const presetOf = (file) => env.catalog.presets.find((p) => p.file === file);
// A class's builds: standard specs first, in talent tree order, then the
// Forever experiments by name.
export const isExperimental = (p) => p && p.kind === 'experimental';
function mainTree(p) {
  const c = classOf(p.class);
  if (!c) return 0;
  const per = c.trees.map((tr) => c.talents.filter((t) => t.tree === tr).reduce((a, t) => a + (p.talents?.[t.id] || 0), 0));
  return per.indexOf(Math.max(...per));
}
export const presetsFor = (cls) => env.catalog.presets.filter((p) => p.class === cls).sort((a, b) =>
  (isExperimental(a) - isExperimental(b)) || (isExperimental(a) ? a.name.localeCompare(b.name) : mainTree(a) - mainTree(b) || a.name.localeCompare(b.name)));
// "Experimental", "Healer" or "Tank", for labels next to a build's name.
export function presetTags(p) {
  if (!p) return [];
  return [isExperimental(p) ? 'Experimental' : null, p.role === 'healer' ? 'Healer' : p.role === 'tank' ? 'Tank' : null].filter(Boolean);
}
export const baseName = (name) => name.replace(/\s*\([\d/+]+\)\s*$/, '').replace(/\+$/, '+');
export const scenarioMeta = (id) => M.SCENARIOS.find((s) => s.id === id) || { id, title: id, short: id, unit: 'fight', per: '', icon: 'crossed-swords', help: '' };
export const primaryOf = (test) => env.catalog.scenarios.find((s) => s.id === test)?.primary;
export const accuracyOf = (id) => M.ACCURACY.find((a) => a.id === id) || M.ACCURACY[1];
export const accuracyRank = (id) => M.ACCURACY.findIndex((a) => a.id === id);
export const classColor = (cls) => M.CLASS_TEXT[cls] || '#ddd';
export const buildOf = (uid) => S.builds.find((b) => b.uid === uid);
export const editingBuild = () => buildOf(S.editing);
export const slotOf = (b) => M.SLOTS[Math.max(0, S.builds.indexOf(b))] || M.SLOTS[0];

export function baselineBuild() {
  return buildOf(S.baseline) || S.builds[0];
}

// The healing tests are for healers: the healer presets, and any build of a
// healing class that the player marks as healing in group tests (`healer`
// set on the build overrides the preset).
export const healRotationOf = (cls) => classOf(cls)?.rotations.find((r) => r.healer) || null;
export function healsGroups(b) {
  if (!b || !healRotationOf(b.cls)) return false;
  if (typeof b.healer === 'boolean') return b.healer;
  return presetOf(b.preset)?.role === 'healer';
}
// Why a build sits a test out, or null when it takes it.
export function sitsOut(b, test) {
  const sc = M.SCENARIOS.find((s) => s.id === test);
  if (sc?.group !== 'healing') return null;
  if (!healsGroups(b)) return 'Not a healer';
  if (b.level < (sc.minLevel || 1)) return `Level ${sc.minLevel} only`;
  return null;
}
export const takesTest = (b, test) => !sitsOut(b, test);

// ------------------------------------------------------------------ builds

export function buildFromPreset(p, level) {
  const lv = level || p.level || 60;
  return {
    uid: uidSeq++, cls: p.class, race: p.race, level: lv,
    // below the build's own level, its full talents stay planned for later
    talents: { ...p.talents }, plan: lv < (p.level || 60) ? { ...p.talents } : null, gear: p.gear, rotation: p.rotation,
    preset: p.file, order: p.order || null, popts: p.opts || null, edited: false, customName: null, seq: 0,
  };
}

export function emptyBuild(cls, level) {
  const p = presetsFor(cls)[0];
  const c = classOf(cls);
  return {
    uid: uidSeq++, cls, race: p ? p.race : c.races[0], level: level || 60, talents: {}, plan: null,
    gear: p ? p.gear : env.catalog.gear[0].id, rotation: p ? p.rotation : soloRotations(c)[0].id,
    preset: null, order: null, popts: null, edited: false, customName: null, seq: 0,
  };
}

// After a class or spec change: keep the player's race when the class can
// be it, else a race of the same faction (the spec's own if it's on that
// side), so a new class never switches faction on its own.
export function keepRace(nb, race) {
  const c = classOf(nb.cls);
  if (!race || nb.race === race) return nb;
  if (c.races.includes(race)) return { ...nb, race };
  const side = raceOf(race)?.faction;
  if (raceOf(nb.race)?.faction === side) return nb;
  const same = c.races.find((r) => raceOf(r)?.faction === side);
  return same ? { ...nb, race: same } : nb;
}

export function copyBuild(b) {
  return { ...b, uid: uidSeq++, talents: { ...b.talents }, plan: b.plan ? { ...b.plan } : null, customName: null, seq: 0 };
}

export function autoName(b) {
  const p = b.preset && presetOf(b.preset);
  if (p) return baseName(p.name) + (b.edited ? ' (edited)' : '');
  const prep = prepareClass(classOf(b.cls));
  const split = prep.trees.map((tr) => tr.talents.reduce((a, t) => a + (b.talents[t.id] || 0), 0)).join('/');
  return `${classOf(b.cls).name} ${split}`;
}

// Display names, made unique ("Frost", "Frost #2").
export function names() {
  const seen = new Map();
  return S.builds.map((b) => {
    let n = (b.customName || autoName(b)).trim() || 'Unnamed build';
    const k = n.toLowerCase();
    const c = (seen.get(k) || 0) + 1;
    seen.set(k, c);
    if (c > 1) n = `${n} #${c}`;
    return n;
  });
}
export const nameOf = (b) => names()[S.builds.indexOf(b)] || autoName(b);

export function apiBuild(b, name, talents = b.plan || b.talents) {
  const out = { name, class: b.cls, race: b.race, level: b.level, talents, gear: b.gear, rotation: b.rotation };
  if (b.order && !b.edited) out.order = b.order;
  if (b.popts) out.opts = b.popts;
  return out;
}

// What makes a build simulate the way it does (everything but its name):
// two builds with the same signature give identical results.
export function sig(b) {
  const { name, ...rest } = apiBuild(b, '');
  const talents = Object.fromEntries(Object.entries(rest.talents).filter(([, v]) => v > 0).sort());
  return JSON.stringify({ ...rest, talents });
}

// Settings every test shares: results from a different setting are out of date.
export function envKey() {
  const opts = Object.fromEntries(Object.entries(S.opts).sort());
  return JSON.stringify({ opts, data: env.catalog.data_build });
}

// ------------------------------------------------------------------ saving and sharing

export function serial() {
  const bl = Math.max(0, S.builds.indexOf(baselineBuild()));
  const ed = S.builds.findIndex((b) => b.uid === S.editing);
  return {
    v: 1,
    b: S.builds.map((b) => ({
      c: b.cls, r: b.race, l: b.level, t: b.plan || b.talents, p: b.preset || undefined, e: b.edited ? 1 : undefined,
      g: b.gear, ro: b.rotation, n: b.customName || undefined, h: typeof b.healer === 'boolean' ? Number(b.healer) : undefined,
    })),
    // `a` and `s` are what older versions of the page read: the build shown and the test picked.
    a: ed >= 0 ? ed : bl, s: S.open || 'solo_1', acc: S.accuracy,
    o: S.opponent || undefined, op: Object.keys(S.opts).length ? S.opts : undefined,
    bl: bl || undefined, f: S.open || undefined,
  };
}

export function restore(d) {
  if (!d || d.v !== 1 || !Array.isArray(d.b) || !d.b.length) return false;
  const builds = [];
  for (const x of d.b.slice(0, MAX_BUILDS)) {
    const c = classOf(x.c);
    if (!c) continue;
    const p = x.p && presetOf(x.p);
    const talents = Object.fromEntries(Object.entries(x.t || {}).filter(([k, v]) => c.talents.some((t) => t.id === k) && v > 0));
    builds.push({
      uid: uidSeq++, cls: c.id, race: c.races.includes(x.r) ? x.r : c.races[0],
      level: Math.min(60, Math.max(10, Number(x.l) || 60)), talents, plan: { ...talents },
      gear: env.catalog.gear.some((g) => g.id === x.g) ? x.g : (p ? p.gear : env.catalog.gear[0].id),
      rotation: soloRotations(c).some((r) => r.id === x.ro) ? x.ro : soloRotations(c)[0].id,
      preset: p ? p.file : null, order: p && !x.e ? p.order || null : null, popts: p ? p.opts || null : null,
      edited: !!x.e, customName: typeof x.n === 'string' ? x.n.slice(0, 48) : null, seq: 0,
      healer: x.h === 0 || x.h === 1 ? x.h === 1 : undefined,
    });
  }
  if (!builds.length) return false;
  S.builds = builds;
  S.baseline = (builds[Math.min(Math.max(0, d.bl | 0), builds.length - 1)] || builds[0]).uid;
  S.open = M.SCENARIOS.some((s) => s.id === d.f) ? d.f : null;
  if (M.ACCURACY.some((a) => a.id === d.acc)) S.accuracy = d.acc;
  if (typeof d.o === 'string' && d.o.startsWith('preset:') && presetOf(d.o.slice(7))) S.opponent = d.o;
  if (d.op && typeof d.op === 'object') {
    for (const [k, v] of Object.entries(d.op)) if (env.catalog.options.some((o) => o.name === k) && isFinite(v)) S.opts[k] = Number(v);
  }
  return true;
}

export function saveLocal() {
  try { localStorage.setItem(STORE_KEY, JSON.stringify(serial())); } catch { /* private mode: fine */ }
}

export function loadLocal() {
  try { return restore(JSON.parse(localStorage.getItem(STORE_KEY) || 'null')); } catch { return false; }
}

// Results are kept too, so a reload shows the last scorecard. Rows refer to
// builds by signature, so they stay valid for builds that have not changed.
export function saveRows() {
  try {
    // per-fight values are only needed for the next run's comparison: not kept
    const rows = JSON.parse(JSON.stringify(S.rows, (k, v) => (k === 'columns' ? undefined : v)));
    localStorage.setItem(ROWS_KEY, JSON.stringify({ data: env.catalog.data_build, rows }));
  } catch { /* too big or private mode: fine */ }
}

export function loadRows() {
  try {
    const d = JSON.parse(localStorage.getItem(ROWS_KEY) || 'null');
    if (d && d.data === env.catalog.data_build && d.rows) S.rows = d.rows;
  } catch { /* ignore */ }
}

export function encode(obj) {
  const bytes = new TextEncoder().encode(JSON.stringify(obj));
  let s = '';
  bytes.forEach((b) => { s += String.fromCharCode(b); });
  return btoa(s).replace(/\+/g, '-').replace(/\//g, '_').replace(/=+$/, '');
}

export function decode(str) {
  const s = atob(str.replace(/-/g, '+').replace(/_/g, '/'));
  return JSON.parse(new TextDecoder().decode(Uint8Array.from(s, (c) => c.charCodeAt(0))));
}

// What changed from build `a` to build `b`, as short phrases:
// ["+5 Cruelty", "−5 Gore Drinker", "Orc instead of Undead", ...].
export function buildDiff(a, b) {
  if (!a || !b) return [];
  if (a.cls !== b.cls) return [`${classOf(b.cls).name} instead of ${classOf(a.cls).name}`];
  const out = [];
  if (a.level !== b.level) out.push(`level ${b.level} instead of ${a.level}`);
  if (a.race !== b.race) out.push(`${raceOf(b.race)?.name} instead of ${raceOf(a.race)?.name}`);
  const c = classOf(b.cls);
  const ta = a.plan || a.talents;
  const tb = b.plan || b.talents;
  const moves = c.talents
    .map((t) => ({ t, d: (tb[t.id] || 0) - (ta[t.id] || 0) }))
    .filter((x) => x.d !== 0)
    .sort((x, y) => Math.abs(y.d) - Math.abs(x.d) || y.d - x.d);
  for (const { t, d } of moves) out.push(`${d > 0 ? '+' : '−'}${Math.abs(d)} ${t.name}`);
  if (a.gear !== b.gear) out.push('different gear');
  if (a.rotation !== b.rotation) out.push('different play style');
  return out;
}

// "+5 Cruelty, −5 Gore Drinker and 3 more"
export function diffText(parts, max = 3) {
  if (!parts.length) return '';
  const shown = parts.slice(0, max).join(', ');
  return parts.length > max ? `${shown} and ${parts.length - max} more` : shown;
}
