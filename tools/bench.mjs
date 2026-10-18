#!/usr/bin/env node
// Time the WebAssembly engine on one thread, as one of the website's workers
// runs it: milliseconds per build and fight (replication) for each test.
//   node tools/bench.mjs target/wasm32-unknown-unknown/release/forever_sim.wasm [reps]
// Each test runs a fixed number of fights (no early stop), so two engine
// versions are timed on exactly the same work.
import { readFileSync } from 'node:fs';

const [path, repsArg] = process.argv.slice(2);
if (!path) { console.error('usage: bench.mjs <forever_sim.wasm> [reps]'); process.exit(2); }
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
  ['solo_1', mixed, [20, 40, 60]], ['solo_2', mixed, [20, 40, 60]], ['solo_3', mixed, [40]],
  ['boss', mixed, [20, 40, 60]], ['pressure', mixed, [20, 40, 60]], ['duel', mixed, [20, 40, 60]],
  ['heal_dungeon', healers, [20, 40, 60]], ['heal_raid', healers, [60]],
];
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
  console.log(`${scenario.padEnd(13)} ${(ms / n).toFixed(2).padStart(7)} ms per build-fight`);
}
console.log(`total ${(total / 1000).toFixed(1)} s`);
