// The page shows each build its own numbers, from the engine that made them,
// and reads shared links and saved data safely.
import { test } from 'node:test';
import assert from 'node:assert/strict';
import { S, env, engine, resetPage } from './harness.mjs';
import {
  presetOf, buildFromPreset, claimUids, sig, serial, restore, saveLocal, loadLocal, saveRows, loadRows, envKey, validRow, requestOpts,
} from '../../web/state.js';
import { simulateAll, cellOf, planTest, upToDate } from '../../web/runner.js';
import { duelTable } from '../../web/rankings.js';

const preset = (file, level = 40) => buildFromPreset(presetOf(file), level);

// Two builds run through one test, saved the way the page saves them.
async function runTwo() {
  resetPage();
  S.builds = [preset('mage/frost.toml'), preset('mage/arcane.toml')];
  claimUids();
  await simulateAll({ only: 'solo_1', acc: 'quick' });
  saveLocal();
  saveRows();
  return S.builds.map((b) => ({ uid: b.uid, sig: sig(b) }));
}

// What a reload does: the page starts empty and reads back what was saved.
function reload() {
  const saved = localStorage.getItem('forever-sim/v1');
  Object.assign(S, { builds: [], rows: {}, pool: {} });
  loadRows();
  assert.ok(restore(JSON.parse(saved)));
  claimUids();
}

test('after a reload, an edited build reads its own old numbers, not another build’s', async () => {
  const before = await runTwo();
  reload();
  const row = S.rows.solo_1;
  // edit the second build
  S.builds[1].race = S.builds[1].race === 'gnome' ? 'human' : 'gnome';
  const c = cellOf(row, S.builds[1]);
  assert.ok(c, 'the edited build still finds its old numbers');
  assert.equal(c.current, false);
  assert.equal(row.roles[c.i].sig, before[1].sig, 'and they are its own');
});

test('builds from a shared link never pick up saved numbers of other builds', async () => {
  await runTwo();
  const link = serial();
  assert.ok(link.b.every((x) => x.u === undefined), 'shared links carry no build ids');
  // someone else's link: one build, edited, opened next to the saved rows
  link.b = [{ ...link.b[1], r: link.b[1].r === 'gnome' ? 'human' : 'gnome' }];
  Object.assign(S, { builds: [] });
  loadRows();
  assert.ok(restore(link));
  claimUids();
  assert.equal(cellOf(S.rows.solo_1, S.builds[0]), null);
});

test('results from another engine build are out of date and not loaded', async () => {
  await runTwo();
  assert.ok(upToDate('solo_1', 'quick'));
  const keep = env.catalog.build_id;
  try {
    env.catalog.build_id = 'another-engine';
    assert.notEqual(envKey(), JSON.stringify({ opts: {}, data: keep }));
    assert.equal(upToDate('solo_1', 'quick'), false);
    S.rows = {};
    loadRows();
    assert.deepEqual(S.rows, {}, 'saved rows from another engine are dropped');
  } finally {
    env.catalog.build_id = keep;
  }
});

test('in a one-build duel, the opponent’s play style applies to the opponent', async () => {
  resetPage();
  S.builds = [preset('rogue/combat.toml', 60)];
  claimUids();
  S.opponent = 'preset:rogue/combat.toml';
  const firstShare = async () => {
    const job = planTest('duel', 'quick');
    const req = job.sessions[0].req;
    assert.equal(req.builds.length, 1);
    assert.equal(req.builds[0].name, S.builds[0].name ?? req.builds[0].name);
    const res = await engine.simulate({ ...req, max_reps: 400, precision: 0.04 }, () => {}, () => false, {});
    return res.variants[0].metrics.find((m) => m.name === 'first').mean;
  };
  const even = await firstShare();
  S.opts = { opp_play_like: 2 }; // expert opponent
  const job = planTest('duel', 'quick');
  assert.equal(job.sessions[0].req.opts.opp_play_like, 2);
  const vsExpert = await firstShare();
  assert.ok(vsExpert < even - 0.1, `you react first less often against an expert (${vsExpert.toFixed(2)} vs ${even.toFixed(2)})`);
  // and it leaves every other test alone
  assert.equal(requestOpts(), null);
  assert.equal(planTest('solo_1', 'quick').sessions[0].req.opts, undefined);
});

test('the Duels view reads each opponent as a variant, with your build as the build', async () => {
  const mine = { ...presetOf('mage/frost.toml'), level: 40 };
  const foes = ['priest/shadow.toml', 'warrior/arms.toml'].map((f) => ({ ...presetOf(f), level: 40 }));
  const res = await engine.simulate({ op: 'start', scenario: 'duel', builds: [mine], opponents: foes, precision: 0.05, max_reps: 200, min_reps: 32 }, () => {}, () => false, { columns: [] });
  assert.equal(res.variants.length, 2);
  const { rows } = duelTable([res], 2);
  assert.equal(rows.length, 2);
  for (const r of rows) assert.ok(r.mean >= 0 && r.mean <= 1 && r.lo <= r.mean && r.mean <= r.hi);
});

test('shared links are read safely', () => {
  resetPage();
  const ok = restore({
    v: 1,
    b: [
      { c: 'warrior', r: 'orc', l: 33.7, g: 'no-such-gear', p: 'mage/frost.toml', t: { mortal_strike: '1', improved_rend: 99 } },
      { c: 'no-such-class' },
      { c: 'mage', l: 40 }, { c: 'rogue', l: 40 }, { c: 'priest', l: 40 },
    ],
    op: { skirmish_pool: 1, play_like: 7, opp_play_like: 2, potions: 'yes' },
  });
  assert.ok(ok);
  claimUids();
  const [w] = S.builds;
  assert.equal(S.builds.length, 3, 'three builds at most');
  assert.ok(S.restoreNote && S.restoreNote.includes('Left out'));
  assert.equal(w.level, 34, 'levels are whole numbers');
  assert.equal(w.preset, null, 'another class’s preset is ignored');
  assert.notEqual(w.gear, 'cloth_battlemage', 'unknown gear falls back to the class’s own kit');
  assert.equal(w.talents.mortal_strike, 1);
  const ir = env.catalog.classes.find((c) => c.id === 'warrior').talents.find((t) => t.id === 'improved_rend');
  if (ir) assert.equal(w.talents.improved_rend, ir.ranks, 'talents are clamped to their ranks');
  assert.equal(S.opts.skirmish_pool, undefined, 'internal settings are ignored');
  assert.equal(S.opts.play_like, undefined, 'out-of-range choices are ignored');
  assert.equal(S.opts.opp_play_like, 2);
  assert.equal(S.opts.potions, undefined);
  assert.equal(new Set(S.builds.map((b) => b.uid)).size, 3, 'every build has its own id');
});

test('malformed saved rows are dropped instead of breaking the page', async () => {
  await runTwo();
  const saved = JSON.parse(localStorage.getItem('forever-sim/rows/v1'));
  saved.rows.solo_2 = null;
  saved.rows.boss = { roles: 'nope' };
  saved.rows.pressure = { roles: [{ sig: 'x', uids: [1] }] };
  localStorage.setItem('forever-sim/rows/v1', JSON.stringify(saved));
  S.rows = {};
  loadRows();
  assert.deepEqual(Object.keys(S.rows), ['solo_1']);
  assert.ok(validRow(S.rows.solo_1));
  assert.equal(validRow(null), false);
  assert.ok(loadLocal());
});
