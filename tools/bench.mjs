#!/usr/bin/env node
// Time the WebAssembly engine on one thread, as one of the website's workers
// runs it: milliseconds per build and fight (replication) for each test.
//   node tools/bench.mjs target/wasm32-unknown-unknown/release/forever_sim.wasm [reps] [--check | --write]
// Each test runs a fixed number of fights (no early stop), so two engine
// versions are timed on exactly the same work. --write saves the times as
// the budgets (tools/bench_budgets.json); --check fails when a test runs
// more than 10% over its budget. The website's TEST_MS (web/meta.js) should
// follow the budgets.
import { readFileSync, writeFileSync } from 'node:fs';
import { dirname, join } from 'node:path';
import { fileURLToPath } from 'node:url';

const flags = process.argv.slice(2).filter((a) => a.startsWith('--'));
const [path, repsArg] = process.argv.slice(2).filter((a) => !a.startsWith('--'));
const budgetsPath = join(dirname(fileURLToPath(import.meta.url)), 'bench_budgets.json');
if (!path) { console.error('usage: bench.mjs <forever_sim.wasm> [reps] [--check | --write]'); process.exit(2); }
const reps = Number(repsArg || 48);
const { exports: ex } = await WebAssembly.instantiate(await WebAssembly.compile(readFileSync(path)), {});
function call(req) {
  const enc = new TextEncoder().encode(JSON.stringify(req));
  const ptr = ex.fsim_alloc(enc.length);
  new Uint8Array(ex.memory.buffer, ptr, enc.length).set(enc);
  ex.fsim_call(ptr, enc.length);
  const out = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, ex.fsim_out_ptr(), ex.fsim_out_len())));
  if (out.error) throw new Error(`${req.op}: ${out.error}`);
  return out;
}

const cat = call({ op: 'catalog' });
const pick = (files) => files.map((f) => cat.presets.find((p) => p.file === f)).filter(Boolean);
const mixed = pick(['warrior/arms.toml', 'mage/frost.toml', 'rogue/combat.toml', 'warlock/affliction.toml', 'hunter/beast_mastery.toml', 'shaman/enhancement.toml']);
const healers = cat.presets.filter((p) => p.role === 'healer');
const tests = [
  ['solo_1', mixed, [20, 40, 60]], ['solo_2', mixed, [20, 40, 60]], ['solo_2_hard', mixed, [40, 60]], ['solo_3', mixed, [40]],
  ['boss', mixed, [20, 40, 60]], ['pressure', mixed, [20, 40, 60]], ['pressure_2v1', mixed, [40, 60]], ['duel', mixed, [20, 40, 60]],
  ['strike_first', mixed, [20, 40, 60]], ['struck_first', mixed, [20, 40, 60]],
  ['heal_dungeon', healers, [20, 40, 60]], ['heal_tank', healers, [40, 60]], ['heal_raid', healers, [60]],
];
const times = {};
let total = 0;
for (const [scenario, builds, levels] of tests) {
  let ms = 0, n = 0;
  for (const level of levels) {
    const bs = builds.map((p) => ({ ...p, level }));
    const req = { op: 'start', scenario, builds: bs, precision: 1e-9, min_reps: reps, max_reps: reps };
    if (scenario === 'duel') req.opponent = { ...builds[0], level };
    const t0 = performance.now();
    const { id } = call(req);
    while (!call({ op: 'step', id, reps: 16 }).finished);
    call({ op: 'drop', id });
    ms += performance.now() - t0;
    n += bs.length * reps;
  }
  total += ms;
  times[scenario] = +(ms / n).toFixed(2);
  console.log(`${scenario.padEnd(13)} ${(ms / n).toFixed(2).padStart(7)} ms per build-fight`);
}
console.log(`total ${(total / 1000).toFixed(1)} s`);
if (flags.includes('--write')) {
  writeFileSync(budgetsPath, JSON.stringify({ note: 'ms per build and fight on one thread (tools/bench.mjs); the website\'s TEST_MS follows these', ms: times }, null, 2) + '\n');
  console.log(`wrote ${budgetsPath}`);
}
if (flags.includes('--check')) {
  const budgets = JSON.parse(readFileSync(budgetsPath, 'utf8')).ms;
  const over = Object.entries(times).filter(([t, ms]) => budgets[t] && ms > budgets[t] * 1.1);
  for (const [t, ms] of over) console.log(`over budget: ${t} ${ms} ms (budget ${budgets[t]})`);
  if (over.length) process.exit(1);
  console.log('within budget');
}
