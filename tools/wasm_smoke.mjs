#!/usr/bin/env node
// Load the WebAssembly build exactly as web/worker.js does and run one small
// simulation through it, so a broken build fails CI instead of the website.
//   node tools/wasm_smoke.mjs target/wasm32-unknown-unknown/release/forever_sim.wasm
import { readFileSync } from 'node:fs';

const path = process.argv[2];
const bytes = readFileSync(path);
const module = await WebAssembly.compile(bytes);
const imports = WebAssembly.Module.imports(module);
if (imports.length) {
  console.error('fsim.wasm must not need imports (the page provides none):');
  for (const i of imports) console.error(`  ${i.module}.${i.name} (${i.kind})`);
  process.exit(1);
}
const { exports: ex } = await WebAssembly.instantiate(module, {});
for (const f of ['memory', 'fsim_alloc', 'fsim_call', 'fsim_out_ptr', 'fsim_out_len']) {
  if (!(f in ex)) { console.error(`missing export ${f}`); process.exit(1); }
}

function call(req) {
  const enc = new TextEncoder().encode(JSON.stringify(req));
  const ptr = ex.fsim_alloc(enc.length);
  new Uint8Array(ex.memory.buffer, ptr, enc.length).set(enc);
  ex.fsim_call(ptr, enc.length);
  const out = JSON.parse(new TextDecoder().decode(new Uint8Array(ex.memory.buffer, ex.fsim_out_ptr(), ex.fsim_out_len())));
  if (out.error && req.op !== 'nope') throw new Error(`${req.op}: ${out.error}`);
  return out;
}

const fail = (msg) => { console.error(msg); process.exit(1); };
const t0 = performance.now();
const cat = call({ op: 'catalog' });
if (cat.classes.length !== 9) fail(`expected 9 classes, got ${cat.classes.length}`);
const preset = cat.presets.find((p) => p.file === 'warrior/vampire_v2.toml') || cat.presets[0];
const sheet = call({ op: 'sheet', build: preset });
if (!sheet.rows.length) fail('empty character sheet');
if (!call({ op: 'nope' }).error) fail('bad requests should come back as errors');

for (const scenario of ['boss', 'solo_2', 'pressure']) {
  const { id } = call({ op: 'start', scenario, builds: [preset, cat.presets[1]], precision: 0.05, max_reps: 200 });
  let st;
  const t = performance.now();
  do st = call({ op: 'step', id, reps: 16 }); while (!st.finished);
  const res = call({ op: 'result', id });
  call({ op: 'drop', id });
  const m = res.variants[0].metrics.find((x) => x.name === res.scenario.primary);
  if (!Number.isFinite(m.mean)) fail(`${scenario}: no result`);
  console.log(`${scenario.padEnd(9)} ${String(res.reps).padStart(4)} reps  ${res.scenario.primary} ${m.mean.toFixed(1)}  ${(performance.now() - t).toFixed(0)} ms`);
}
// The page splits a run between workers: ranges of fights delivered in any
// order must give exactly what stepping through on one engine gives.
{
  const req = { op: 'start', scenario: 'pressure', builds: [preset, cat.presets[1]], precision: 0.05, max_reps: 200 };
  const one = call(req).id;
  while (!call({ op: 'step', id: one, reps: 7 }).finished);
  const expect = JSON.stringify(call({ op: 'result', id: one }));
  const keeper = call(req).id;
  const worker = call(req).id;
  const chunks = [];
  for (let from = 0, i = 0; from < 200; i++) {
    const count = [3, 1, 9, 2, 16][i % 5];
    chunks.push(call({ op: 'run', id: worker, from, count }));
    from += count;
  }
  let st;
  for (const chunk of chunks.reverse()) st = call({ op: 'add', id: keeper, chunk });
  if (!st.finished) fail('split run did not finish');
  if (JSON.stringify(call({ op: 'result', id: keeper })) !== expect) fail('split run differs from one engine stepping through');
  for (const id of [one, keeper, worker]) call({ op: 'drop', id });
  console.log(`split     ${String(st.reps).padStart(4)} reps  same result as one engine`);
}
console.log(`ok: ${(bytes.length / 1024).toFixed(0)} KiB, ${cat.presets.length} builds, data ${cat.data_build}, ${(performance.now() - t0).toFixed(0)} ms in all`);
