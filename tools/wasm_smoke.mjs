#!/usr/bin/env node
// Load the WebAssembly build exactly as web/worker.js does and run one small
// simulation through it, so a broken build fails CI instead of the website.
//   node tools/wasm_smoke.mjs target/wasm32-unknown-unknown/dist/forever_sim.wasm [catalog.json]
// With a catalog (the command line's `fsim api '{"op":"catalog"}'`), the
// engine's own must be the same.
import { readFileSync } from 'node:fs';

const [path, catalogPath] = process.argv.slice(2);
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
const version = call({ op: 'version' });
if (!version.build_id || version.build_id !== cat.build_id) fail(`version ${version.build_id} does not match the catalog's ${cat.build_id}`);
if (catalogPath && JSON.stringify(JSON.parse(readFileSync(catalogPath, 'utf8'))) !== JSON.stringify(cat)) fail(`${catalogPath} is not this engine's catalog`);

const healers = cat.presets.filter((p) => p.role === 'healer').slice(0, 2);
for (const scenario of ['boss', 'solo_2', 'pressure', 'heal_dungeon']) {
  const builds = scenario.startsWith('heal_') ? healers : [preset, cat.presets[1]];
  const { id } = call({ op: 'start', scenario, builds, precision: 0.05, max_reps: 200 });
  let st;
  const t = performance.now();
  do st = call({ op: 'step', id, reps: 16 }); while (!st.finished);
  const res = call({ op: 'result', id });
  call({ op: 'drop', id });
  const m = res.variants[0].metrics.find((x) => x.name === res.scenario.primary);
  if (!Number.isFinite(m.mean)) fail(`${scenario}: no result`);
  console.log(`${scenario.padEnd(12)} ${String(res.reps).padStart(4)} reps  ${res.scenario.primary} ${m.mean.toFixed(1)}  ${(performance.now() - t).toFixed(0)} ms`);
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
  const swapped = call({ op: 'result', id: keeper, pairs: [[1, 0]] }).paired.filter((p) => p.metric === 'breakeven');
  if (swapped.length !== 1 || swapped[0].a !== 1 || swapped[0].b !== 0) fail('result ignored the pairs it was asked for');
  const all = call({ op: 'result', id: keeper, pairs: 'all', columns: ['breakeven'] });
  const ab = all.paired.find((p) => p.metric === 'breakeven' && p.a === 0 && p.b === 1);
  const kept = call({ op: 'pair', metric: 'breakeven', a: all.variants[0].columns.breakeven, b: all.variants[1].columns.breakeven });
  if (kept.diff !== ab.diff || kept.lo !== ab.lo || kept.hi !== ab.hi) fail('pairing stored values differs from comparing in one run');
  for (const id of [one, keeper, worker]) call({ op: 'drop', id });
  console.log(`split     ${String(st.reps).padStart(4)} reps  same result as one engine`);
}
// Each build stopping on its own (`independent`), the workers running only
// the builds still going: the same result as one engine.
{
  const req = { op: 'start', scenario: 'solo_1', builds: [preset, cat.presets[1]], precision: 0.004, max_reps: 400, independent: true, keep: ['kills_per_hour', 'deaths_per_hour'] };
  const one = call(req).id;
  while (!call({ op: 'step', id: one, reps: 5 }).finished);
  const expect = JSON.stringify(call({ op: 'result', id: one, pairs: [] }));
  const keeper = call(req).id;
  const worker = call(req).id;
  let st = null;
  for (let from = 0, i = 0; !(st && st.finished); i++) {
    const count = [4, 9, 2][i % 3];
    const only = st ? st.active : undefined;
    st = call({ op: 'add', id: keeper, chunk: call({ op: 'run', id: worker, from, count, ...(only ? { only } : {}) }) });
    from += count;
  }
  if (JSON.stringify(call({ op: 'result', id: keeper, pairs: [] })) !== expect) fail('independent split run differs from one engine');
  for (const id of [one, keeper, worker]) call({ op: 'drop', id });
  console.log(`own stop  ${String(st.each).padStart(9)} reps  same result as one engine`);
}
console.log(`ok: ${(bytes.length / 1024).toFixed(0)} KiB, ${cat.presets.length} builds, data ${cat.data_build}, ${(performance.now() - t0).toFixed(0)} ms in all`);
