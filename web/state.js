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
const POOL_KEY = 'forever-sim/skirmish-v1';

// Set once at boot.
// `ready` resolves with the engine once it answers; until then `engine`
// stands in and waits for it.
export const env = { engine: null, catalog: null, ready: null };

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
  mode: 'pve', // the Compare page's tab: 'pve' or 'pvp'
  tree: null,
  sheets: new Map(), // uid -> character sheet (or {error})
  rows: {}, // test id -> results of its latest run (see runner.js)
  pool: {}, // open-world PvP test id -> its run against every standard build
  versus: {}, // open-world PvP test id -> which opponent's fight the log shows
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

// The healing tests are for every build of a class that can heal: it heals
// the class's way, with its own talents and gear.
export const healRotationOf = (cls) => classOf(cls)?.rotations.find((r) => r.healer) || null;
export function healsGroups(b) {
  return !!(b && healRotationOf(b.cls));
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
    preset: p.file, order: p.order || null, popts: p.opts || null, edited: false, customName: null, seq: 0, gender: 'm',
  };
}

export function emptyBuild(cls, level) {
  const p = presetsFor(cls)[0];
  const c = classOf(cls);
  return {
    uid: uidSeq++, cls, race: p ? p.race : c.races[0], level: level || 60, talents: {}, plan: null,
    gear: p ? p.gear : env.catalog.gear[0].id, rotation: p ? p.rotation : soloRotations(c)[0].id,
    preset: null, order: null, popts: null, edited: false, customName: null, seq: 0, gender: 'm',
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

// Settings every test shares, and the engine build that made a result:
// results from a different setting or engine are out of date. How the duel
// opponent plays only matters against a picked opponent, so it's left out
// unless `withPlay`.
export function envKey(withPlay = false) {
  const opts = Object.fromEntries(Object.entries(S.opts).filter(([k]) => withPlay || k !== 'opp_play_like').sort());
  return JSON.stringify({ opts, data: engineStamp() });
}

// Which engine made a result: its build id (a hash of its source and data).
export const engineStamp = () => env.catalog.build_id || env.catalog.data_build;

// The settings to send with a request (the opponent's play style only for
// fights against a picked opponent).
export function requestOpts(withPlay = false) {
  const o = Object.fromEntries(Object.entries(S.opts).filter(([k]) => withPlay || k !== 'opp_play_like'));
  return Object.keys(o).length ? o : null;
}

// ------------------------------------------------------------------ saving and sharing

// `local`: for this browser's own save, which also keeps each build's id so
// its saved results stay its own after a reload. Shared links leave it out.
export function serial({ local = false } = {}) {
  const bl = Math.max(0, S.builds.indexOf(baselineBuild()));
  const ed = S.builds.findIndex((b) => b.uid === S.editing);
  return {
    v: 1,
    b: S.builds.map((b) => ({
      c: b.cls, r: b.race, l: b.level, t: b.plan || b.talents, p: b.preset || undefined, e: b.edited ? 1 : undefined,
      g: b.gear, ro: b.rotation, n: b.customName || undefined, x: b.gender === 'f' ? 'f' : undefined, u: local ? b.uid : undefined,
    })),
    // `a` and `s` are what older versions of the page read: the build shown and the test picked.
    a: ed >= 0 ? ed : bl, s: S.open || 'solo_1', acc: S.accuracy,
    o: S.opponent || undefined, op: Object.keys(S.opts).length ? S.opts : undefined,
    bl: bl || undefined, f: S.open || undefined, md: S.mode === 'pvp' ? 'pvp' : undefined,
  };
}

// A setting's value from a link or a save, or null when it isn't one the
// player can set (or isn't a value it can take).
function optValue(name, v) {
  const o = env.catalog.options.find((x) => x.name === name);
  if (!o || o.kind === 'internal' || v == null || typeof v === 'boolean' || !isFinite(v)) return null;
  const x = Number(v);
  if (o.kind === 'choice') return Number.isInteger(x) && x >= 0 && x < o.choices.length ? x : null;
  if (o.kind === 'toggle') return x === 0 || x === 1 ? x : null;
  return x >= 0 ? x : null;
}

// Restores builds and settings from a save or a shared link. Anything it
// can't use is left out: `S.restoreNote` says what, for the page to tell.
// Builds keep the ids a local save gave them; the rest get theirs from
// claimUids(), once the saved results are loaded.
export function restore(d) {
  if (!d || d.v !== 1 || !Array.isArray(d.b) || !d.b.length) return false;
  const builds = [];
  const dropped = [];
  const ids = new Set();
  for (const x of d.b) {
    const c = x && classOf(x.c);
    if (!c) { dropped.push('a build of a class this page doesn’t know'); continue; }
    if (builds.length >= MAX_BUILDS) { dropped.push(`a ${c.name} build`); continue; }
    const p0 = x.p && presetOf(x.p);
    const p = p0 && p0.class === c.id ? p0 : null;
    const talents = {};
    for (const [k, v] of Object.entries(x.t && typeof x.t === 'object' ? x.t : {})) {
      const t = c.talents.find((tt) => tt.id === k);
      const n = Math.min(t ? t.ranks : 0, Math.floor(Number(v)));
      if (t && n > 0) talents[k] = n;
    }
    const uid = Number.isInteger(x.u) && x.u > 0 && !ids.has(x.u) ? x.u : null;
    if (uid) ids.add(uid);
    builds.push({
      uid, cls: c.id, race: c.races.includes(x.r) ? x.r : c.races[0],
      level: Math.min(60, Math.max(10, Math.round(Number(x.l)) || 60)), talents, plan: { ...talents },
      gear: env.catalog.gear.some((g) => g.id === x.g) ? x.g : (p || presetsFor(c.id)[0])?.gear || env.catalog.gear[0].id,
      rotation: soloRotations(c).some((r) => r.id === x.ro) ? x.ro : soloRotations(c)[0].id,
      preset: p ? p.file : null, order: p && !x.e ? p.order || null : null, popts: p ? p.opts || null : null,
      edited: !!x.e, customName: typeof x.n === 'string' ? x.n.slice(0, 48) : null, seq: 0, gender: x.x === 'f' ? 'f' : 'm',
    });
  }
  if (!builds.length) return false;
  S.restoreNote = dropped.length ? `This link had ${d.b.length} builds. Left out: ${[...new Set(dropped)].join(', ')}.` : null;
  S.builds = builds;
  S.baselineIndex = Math.min(Math.max(0, d.bl | 0), builds.length - 1);
  S.open = M.SCENARIOS.some((s) => s.id === d.f) ? d.f : null;
  S.mode = S.open ? M.modeOf(S.open) : d.md === 'pvp' ? 'pvp' : 'pve';
  if (M.ACCURACY.some((a) => a.id === d.acc)) S.accuracy = d.acc;
  if (typeof d.o === 'string' && d.o.startsWith('preset:') && presetOf(d.o.slice(7))) S.opponent = d.o;
  if (d.op && typeof d.op === 'object') {
    for (const [k, v] of Object.entries(d.op)) {
      const x = optValue(k, v);
      if (x != null) S.opts[k] = x;
    }
  }
  return true;
}

// Gives builds without an id (from a shared link, or an older save) fresh
// ids above every id the saved results mention, so no build can pick up
// another's numbers. `seen`: more ids in use (the Duels view's).
export function claimUids(seen = []) {
  let max = Math.max(0, ...seen, ...S.builds.map((b) => b.uid || 0));
  for (const row of [...Object.values(S.rows), ...Object.values(S.pool)]) {
    for (const r of row.roles || []) max = Math.max(max, ...r.uids.filter(Number.isInteger));
  }
  uidSeq = Math.max(uidSeq, max + 1);
  for (const b of S.builds) if (!b.uid) b.uid = uidSeq++;
  if (S.baselineIndex != null || !buildOf(S.baseline)) S.baseline = (S.builds[S.baselineIndex ?? 0] || S.builds[0])?.uid ?? null;
  delete S.baselineIndex;
}

export function saveLocal() {
  try { localStorage.setItem(STORE_KEY, JSON.stringify(serial({ local: true }))); } catch { /* private mode: fine */ }
}

export function loadLocal() {
  try { return restore(JSON.parse(localStorage.getItem(STORE_KEY) || 'null')); } catch { return false; }
}

// Results are kept too, so a reload shows the last scorecard. Rows refer to
// builds by signature, so they stay valid for builds that have not changed;
// rows from another engine build are dropped. A failed run's note isn't kept.
const unsaved = (k, v) => (k === 'columns' || k === 'failure' ? undefined : v);
export function saveRows() {
  try {
    // per-fight values are only needed for the next run's comparison: not kept
    const rows = JSON.parse(JSON.stringify(S.rows, unsaved));
    localStorage.setItem(ROWS_KEY, JSON.stringify({ data: engineStamp(), rows }));
  } catch { /* too big or private mode: fine */ }
}

// A saved row the page can read: which builds it stands for, and a result,
// an error or duels.
export function validRow(r) {
  if (!r || typeof r !== 'object' || !Array.isArray(r.roles)) return false;
  if (!r.roles.every((x) => x && typeof x.sig === 'string' && Array.isArray(x.uids))) return false;
  if (typeof r.error === 'string') return true;
  if (Array.isArray(r.duel)) return r.duel.every((d) => d && d.opp && Array.isArray(d.challengers));
  return !!(r.res && Array.isArray(r.res.variants) && r.res.scenario);
}

const validRows = (rows) => Object.fromEntries(Object.entries(rows && typeof rows === 'object' ? rows : {}).filter(([, r]) => validRow(r)));

export function loadRows() {
  try {
    const d = JSON.parse(localStorage.getItem(ROWS_KEY) || 'null');
    if (d && d.data === engineStamp()) S.rows = validRows(d.rows);
  } catch { /* ignore */ }
  try {
    const d = JSON.parse(localStorage.getItem(POOL_KEY) || 'null');
    if (d && d.data === engineStamp()) S.pool = validRows(d.pool);
  } catch { /* ignore */ }
}

// The open-world PvP runs against every standard build: kept on their own,
// since they're big and a full storage shouldn't cost the scorecard.
export function savePool() {
  try {
    const pool = JSON.parse(JSON.stringify(S.pool, unsaved));
    localStorage.setItem(POOL_KEY, JSON.stringify({ data: engineStamp(), pool }));
  } catch { /* too big or private mode: fine */ }
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
