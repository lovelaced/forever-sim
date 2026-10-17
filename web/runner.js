// Running the tests: what to run, running it, and keeping the results.
//
// A row (S.rows[test]) holds the latest run of one test:
//   {acc, env, roles: [{sig, uids}], res, ms}                 for most tests
//   {acc, env, duel: [{opp: role, roles: [role], res}], ms}   for head to head
// `roles[i]` says which builds variant i of the result stands for. A build's
// cell is current while the row has a variant with the build's signature and
// the shared settings (`env`) are unchanged; the engine is deterministic, so
// that variant is exactly what a new run would give.
import * as M from './meta.js';
import { S, env, bus, sig, envKey, apiBuild, nameOf, presetOf, primaryOf, accuracyOf, accuracyRank, buildOf, saveRows, takesTest } from './state.js';

// Tests in the scorecard, in display order. Head to head needs two builds,
// or an opponent picked for a single build; the healing tests need a healer
// (the raid, one at level 60).
export function scorecardTests() {
  return M.SCENARIOS.map((s) => s.id).filter((id) =>
    env.catalog.scenarios.some((x) => x.id === id) && (id !== 'duel' || S.builds.length > 1 || !!S.opponent)
    && S.builds.some((b) => takesTest(b, id)));
}

// Distinct builds by signature: copies run once and share the result.
function rolesOf(builds) {
  const roles = [];
  for (const b of builds) {
    const s = sig(b);
    const r = roles.find((x) => x.sig === s);
    if (r) r.uids.push(b.uid);
    else roles.push({ sig: s, uids: [b.uid], build: b });
  }
  return roles;
}

const plain = ({ sig: s, uids }) => ({ sig: s, uids: [...uids] });

function opponentApi(level) {
  const p = S.opponent && presetOf(S.opponent.slice(7));
  if (!p) return null;
  const { file, note, ...rest } = p;
  return { ...rest, level };
}

// Every request one test needs, fixed at the moment Simulate is pressed.
function planTest(test, acc) {
  const a = accuracyOf(acc);
  const base = { op: 'start', scenario: test, precision: a.precision, max_reps: a.max_reps, min_reps: 32 };
  if (Object.keys(S.opts).length) base.opts = { ...S.opts };
  const roles = rolesOf(S.builds.filter((b) => takesTest(b, test)));
  const api = (r) => apiBuild(r.build, nameOf(r.build));
  if (test !== 'duel') {
    return { test, acc, env: envKey(), roles: roles.map(plain), sessions: [{ req: { ...base, builds: roles.map(api) } }] };
  }
  // Head to head, both ways round: whoever opens the duel strikes first, so
  // each build challenges every other one and the two directions are averaged.
  // A single build fights the opponent picked in the duel's detail instead.
  const sides = roles.map((r) => ({ role: plain(r), api: api(r) }));
  let key = envKey();
  if (roles.length < 2) {
    const opp = opponentApi(roles[0].build.level);
    sides.push({ role: { sig: S.opponent, uids: [] }, api: opp });
    key = `${key}|${S.opponent}`;
  }
  const sessions = sides.map((opp) => {
    const others = sides.filter((x) => x !== opp);
    return { opp: opp.role, challengers: others.map((x) => x.role), req: { ...base, builds: others.map((x) => x.api), opponent: opp.api } };
  });
  return { test, acc, env: key, one: roles.length < 2, roles: roles.map(plain), sessions };
}

// A row needs no new run when every build is already in it, with the same
// settings, at this accuracy or better.
export function upToDate(test, acc) {
  const row = S.rows[test];
  if (!row || row.error || row.running) return false;
  const want = planTest(test, acc);
  if (row.env !== want.env || accuracyRank(row.acc) < accuracyRank(acc)) return false;
  return want.roles.every((r) => row.roles.some((x) => x.sig === r.sig));
}

const estimate = (st, est) => {
  const worst = Math.max(0, ...st.precision.filter((x) => x != null));
  if (st.reps >= 32 && worst > 0) return Math.min(st.max_reps, Math.max(st.reps, st.reps * (worst / st.target) ** 2));
  return Math.max(est, st.reps);
};

// Runs the scorecard (or just `only`). Rows fill in as their tests finish.
export async function simulateAll({ only = null, acc = S.accuracy } = {}) {
  if (S.run && !S.run.finished) return;
  const order = only ? [only] : [S.open, ...M.RUN_ORDER].filter((t, i, a) => t && a.indexOf(t) === i && scorecardTests().includes(t));
  const jobs = order.filter((t) => only || !upToDate(t, acc)).map((t) => planTest(t, acc));
  const run = { jobs: jobs.map((j) => j.test), i: 0, frac: 0, test: null, acc, cancelled: false, finished: false, stopped: false, t0: performance.now(), ms: 0, full: !only };
  S.run = run;
  if (!jobs.length) {
    run.finished = true;
    run.nothing = true;
    bus.render();
    return;
  }
  bus.render();
  for (const job of jobs) {
    if (run.cancelled) break;
    run.test = job.test;
    run.jobFrac = 0;
    bus.render();
    const t0 = performance.now();
    const primary = primaryOf(job.test);
    const results = [];
    try {
      for (let k = 0; k < job.sessions.length; k++) {
        let est = job.sessions[k].req.max_reps;
        const res = await env.engine.simulate(job.sessions[k].req, (st) => {
          est = estimate(st, est);
          const f = st.reps / Math.max(st.reps, Math.min(est, st.max_reps));
          run.jobFrac = (k + f) / job.sessions.length;
          run.frac = (run.i + run.jobFrac) / jobs.length;
          bus.progress();
        }, () => run.cancelled, { pairs: 'all', columns: [primary] });
        if (!res) break;
        results.push(res);
      }
    } catch (e) {
      S.rows[job.test] = { acc: job.acc, env: job.env, roles: job.roles, error: e.message || String(e) };
      run.i++;
      bus.render();
      continue;
    }
    if (run.cancelled) break;
    await commit(job, results, performance.now() - t0);
    run.i++;
    run.frac = run.i / jobs.length;
    bus.render();
  }
  run.finished = true;
  run.ms = performance.now() - run.t0;
  run.stopped = run.cancelled;
  if (!run.cancelled) {
    for (const b of S.builds) S.ranAs.set(b.uid, { sig: sig(b), build: structuredClone(b), sheet: S.sheets.get(b.uid) || null });
  }
  saveRows();
  bus.render();
}

export function cancelRun() {
  if (S.run && !S.run.finished) S.run.cancelled = true;
}

// Store a finished test, and pair each build's per-fight results with the
// ones from its previous version on the same fights: "what your change did".
async function commit(job, results, ms) {
  const primary = primaryOf(job.test);
  if (job.test === 'duel') {
    S.rows.duel = {
      acc: job.acc, env: job.env, roles: job.roles, one: job.one, ms,
      duel: job.sessions.map((s, k) => ({ opp: s.opp, challengers: s.challengers, res: results[k] })),
    };
    return;
  }
  const res = results[0];
  S.rows[job.test] = { acc: job.acc, env: job.env, roles: job.roles, res, ms };
  for (let i = 0; i < job.roles.length; i++) {
    const values = res.variants[i]?.columns?.[primary];
    if (!values) continue;
    for (const uid of job.roles[i].uids) {
      const key = `${uid}|${job.test}`;
      const old = S.cols.get(key);
      const b = buildOf(uid);
      S.cols.set(key, { sig: job.roles[i].sig, env: job.env, build: b ? structuredClone(b) : null, values });
      if (!old || old.sig === job.roles[i].sig || old.env !== job.env) continue;
      try {
        const p = await env.engine.call({ op: 'pair', metric: primary, a: old.values, b: values });
        if (!p.error) S.changes.set(key, { ...p, fromSig: old.sig, toSig: job.roles[i].sig, from: old.build });
      } catch { /* no change line for this test */ }
    }
  }
}

// ------------------------------------------------------------------ reading rows

// Where build `b` sits in a row: {i, current} or null. A copy with the same
// signature reads its source's numbers; an edited build reads its old ones,
// marked out of date.
export function cellOf(row, b) {
  if (!row || !row.roles) return null;
  const s = sig(b);
  let i = row.roles.findIndex((r) => r.sig === s);
  if (i >= 0) return { i, sig: s, current: row.env === rowEnv(row) };
  i = row.roles.findIndex((r) => r.uids.includes(b.uid));
  return i >= 0 ? { i, sig: row.roles[i].sig, current: false } : null;
}

function rowEnv(row) {
  return row.one ? `${envKey()}|${S.opponent}` : envKey();
}

// Paired comparison of variant b against variant a for `metric`.
export function pairOf(res, metric, a, b) {
  return res?.paired?.find((p) => p.metric === metric && p.a === a && p.b === b) || null;
}

// Head to head: how often the build with signature sx beats the one with sy,
// both ways round. {mean, lo, hi, first, second, duration}: first = x opening
// the duel, second = y opening.
export function duelWin(row, sx, sy) {
  if (!row?.duel) return null;
  const side = (oppSig, chSig, metric) => {
    const s = row.duel.find((d) => d.opp?.sig === oppSig);
    if (!s || !s.res) return null;
    const i = s.challengers.findIndex((c) => c.sig === chSig);
    const ms = i >= 0 && s.res.variants[i]?.metrics;
    const m = ms && ms.find((q) => q.name === metric);
    return m ? { p: m.mean, n: s.res.reps, duration: ms.find((q) => q.name === 'duration')?.mean } : null;
  };
  const first = side(sy, sx, 'win'); // x challenges y
  const second = side(sx, sy, 'loss'); // y challenges x and loses
  if (!first || !second) return null;
  const mean = (first.p + second.p) / 2;
  const se = Math.sqrt(first.p * (1 - first.p) / first.n + second.p * (1 - second.p) / second.n) / 2;
  return {
    mean, lo: Math.max(0, mean - 1.96 * se), hi: Math.min(1, mean + 1.96 * se),
    first: first.p, second: second.p, n: first.n + second.n, duration: (first.duration + second.duration) / 2,
  };
}

// Changes on the fights both runs share, for builds as they are now.
export function changesFor(b) {
  const s = sig(b);
  const out = [];
  for (const [key, c] of S.changes) {
    const [uid, test] = key.split('|');
    if (Number(uid) === b.uid && c.toSig === s) out.push({ test, ...c });
  }
  return out;
}
