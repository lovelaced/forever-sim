// Loads the website's modules in Node with the real engine (fsim.wasm), so
// the page's logic can be tested without a browser. Set FSIM_WASM to test
// another build of the engine.
import { existsSync, readFileSync, statSync } from 'node:fs';

// What the page's modules expect from a browser, as far as these tests need.
globalThis.addEventListener ??= () => {};
const store = new Map();
globalThis.localStorage = {
  getItem: (k) => (store.has(k) ? store.get(k) : null),
  setItem: (k, v) => { store.set(k, String(v)); },
  removeItem: (k) => { store.delete(k); },
  clear: () => store.clear(),
  key: (i) => [...store.keys()][i] ?? null,
  get length() { return store.size; },
};

const { env, S } = await import('../../web/state.js');
const { stepThrough } = await import('../../web/engine.js');

// FSIM_WASM, or the newer of the published (dist) and release builds
const built = ['dist', 'release'].map((p) => new URL(`../../target/wasm32-unknown-unknown/${p}/forever_sim.wasm`, import.meta.url)).filter((u) => existsSync(u));
built.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs);
const path = process.env.FSIM_WASM || built[0];
const inst = (await WebAssembly.instantiate(readFileSync(path), {})).instance.exports;

// The same calling convention as web/worker.js.
function callText(req) {
  const enc = new TextEncoder().encode(req);
  const ptr = inst.fsim_alloc(enc.length);
  new Uint8Array(inst.memory.buffer, ptr, enc.length).set(enc);
  inst.fsim_call(ptr, enc.length);
  return new TextDecoder().decode(new Uint8Array(inst.memory.buffer, inst.fsim_out_ptr(), inst.fsim_out_len()));
}

export const engine = {
  kind: 'test',
  call: async (req) => JSON.parse(callText(JSON.stringify(req))),
  simulate: (req, onProgress, cancelled, resultOpts) => stepThrough(engine, req, onProgress, cancelled, resultOpts),
};
env.engine = engine;
env.catalog = await engine.call({ op: 'catalog' });

// A fresh page: no builds, results or settings, and nothing saved.
export function resetPage() {
  store.clear();
  Object.assign(S, { builds: [], baseline: null, editing: null, opts: {}, rows: {}, pool: {}, open: null, run: null, opponent: null, restoreNote: null });
  S.ranAs.clear();
  S.cols.clear();
  S.changes.clear();
}

export { env, S };
