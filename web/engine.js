// One interface to the simulator, wherever it runs:
//  * in the browser, from fsim.wasm in Web Workers (the GitHub Pages build),
//    one per spare core, each simulating its own share of the fights;
//  * or, for local development, through `fsim serve`, which answers the same
//    JSON requests at POST /api.
// Either way `simulate` gives the same result for the same request: the
// engine checks convergence at fixed fight counts, however the work is split.

// How long one worker's share of fights should take before it reports back.
const CHUNK_MS = 120;
// How long a worker may take to answer before it's taken as stuck.
const ANSWER_MS = 60000;

// One Web Worker running fsim.wasm. `text` sends a JSON string and resolves
// with the JSON string it answers. A worker that fails to load or stops
// answering is `dead`: it's stopped, and the engine starts a new one.
class Thread {
  constructor(module) {
    this.worker = new Worker(new URL('./worker.js', import.meta.url));
    this.next = 1;
    this.waiting = new Map();
    this.dead = false;
    this.mem = 0;
    this.worker.onmessage = (e) => {
      const { id, res, fatal, mem } = e.data;
      if (mem) this.mem = mem;
      const p = this.waiting.get(id);
      if (!p) return;
      this.waiting.delete(id);
      if (fatal) p.reject(new Error(fatal));
      else p.resolve(res);
    };
    this.worker.onerror = (e) => this.kill(e.message || 'a worker failed to start');
    // Browsers that cannot pass a compiled module along make the worker fetch it.
    try { this.worker.postMessage({ module }); } catch { /* fetched by the worker */ }
  }
  text(req, ms = ANSWER_MS) {
    if (this.dead) return Promise.reject(new Error('a worker stopped'));
    return new Promise((resolve, reject) => {
      const id = this.next++;
      const timer = setTimeout(() => this.kill('a worker stopped answering'), ms);
      this.waiting.set(id, {
        resolve: (v) => { clearTimeout(timer); resolve(v); },
        reject: (e) => { clearTimeout(timer); reject(e); },
      });
      this.worker.postMessage({ id, req });
    });
  }
  async call(req) {
    return JSON.parse(await this.text(JSON.stringify(req)));
  }
  kill(why) {
    this.dead = true;
    this.worker.terminate();
    for (const p of this.waiting.values()) p.reject(new Error(why));
    this.waiting.clear();
  }
  terminate() {
    this.kill('stopped');
  }
}

// Resolves with CANCEL once `cancelled()` turns true, so no wait outlasts
// the Cancel button.
const CANCEL = Symbol('cancel');
function whenCancelled(cancelled) {
  let timer;
  const p = new Promise((resolve) => {
    timer = setInterval(() => { if (cancelled()) resolve(CANCEL); }, 100);
  });
  return { p, stop: () => clearInterval(timer) };
}

async function compileEngine() {
  // the published page starts compiling the engine before any script runs
  if (typeof window !== 'undefined' && window.fsimModule) {
    const early = await window.fsimModule;
    if (early) return early;
  }
  const resp = await fetch(new URL('fsim.wasm', import.meta.url));
  if (!resp.ok) throw new Error(`fsim.wasm: HTTP ${resp.status}`);
  const type = resp.headers.get('content-type') || '';
  if (WebAssembly.compileStreaming && type.startsWith('application/wasm')) return WebAssembly.compileStreaming(resp);
  return WebAssembly.compile(await resp.arrayBuffer());
}

// Bytes the result-keeping worker may hold before it's replaced.
const KEEPER_MAX_MEM = 256 * 1024 * 1024;

// Workers that simulate fights. The first Thread keeps the results, so it
// is not counted; the main thread stays free for the page.
function poolSize() {
  const cores = navigator.hardwareConcurrency || 2;
  return Math.max(1, Math.min(32, cores - 1));
}

// One session stepped through on one engine: the local server, or a
// browser with too few cores to split the work.
export async function stepThrough(engine, req, onProgress, cancelled, resultOpts = {}) {
  const s = await engine.call(req);
  if (s.error) throw new Error(s.error);
  try {
    let batch = 4;
    for (;;) {
      if (cancelled()) return null;
      const t = performance.now();
      const st = await engine.call({ op: 'step', id: s.id, reps: batch });
      if (st.error) throw new Error(st.error);
      onProgress(st);
      if (st.finished) break;
      const per = (performance.now() - t) / batch;
      batch = Math.max(1, Math.min(4000, Math.round(160 / Math.max(per, 0.01))));
    }
    const res = await engine.call({ ...resultOpts, op: 'result', id: s.id });
    if (res.error) throw new Error(res.error);
    return res;
  } finally {
    engine.call({ op: 'drop', id: s.id }).catch(() => {});
  }
}

class WasmEngine {
  constructor(module) {
    this.kind = 'webassembly';
    this.module = module;
    this.main = new Thread(module);
    this.pool = [];
  }
  call(req) {
    if (this.main.dead) this.main = new Thread(this.module);
    return this.main.call(req);
  }
  // Runs `req` (a `start` request) to a result. Every worker opens the same
  // session; each then asks for the next unclaimed range of fights, sized to
  // take about CHUNK_MS, and the main Thread collects them in order until
  // the result is precise enough. A worker that fails leaves the run and its
  // range is run again by another (once). `resultOpts` go with the final
  // `result` request (pairs, columns). Resolves with null when cancelled.
  async simulate(req, onProgress, cancelled, resultOpts = {}) {
    const n = poolSize();
    if (n < 2) return stepThrough(this, req, onProgress, cancelled, resultOpts);
    // workers that failed last time are replaced
    if (this.main.dead) this.main = new Thread(this.module);
    this.pool = this.pool.filter((t) => !t.dead);
    while (this.pool.length < n) this.pool.push(new Thread(this.module));
    const all = [this.main, ...this.pool.slice(0, n)];
    const cancel = whenCancelled(cancelled);
    const text = JSON.stringify(req);
    const settled = await Promise.race([Promise.allSettled(all.map((t) => t.text(text).then(JSON.parse))), cancel.p]);
    if (settled === CANCEL) {
      cancel.stop();
      return null;
    }
    // the ones that opened the session, the keeper (the main Thread) first
    const open = all.map((t, i) => ({ t, s: settled[i].status === 'fulfilled' ? settled[i].value : { error: settled[i].reason?.message || 'a worker failed' } }));
    const drop = () => open.forEach(({ t, s }) => s.id != null && t.text(`{"op":"drop","id":${s.id}}`).catch(() => {}));
    const keeper = open[0];
    const workers = open.slice(1).filter(({ s }) => s.id != null);
    if (keeper.s.error || !workers.length) {
      cancel.stop();
      drop();
      throw new Error(keeper.s.error || open.find(({ s }) => s.error)?.s.error || 'no worker could start');
    }
    const id = keeper.s.id;
    const maxReps = keeper.s.max_reps;
    let next = 0;
    let last = null;
    let failure = null;
    const retry = []; // ranges a failed worker left: [from, count]
    const stop = () => !!failure || !!(last && last.finished) || cancelled();
    const adds = [];
    const work = async ({ t, s }) => {
      let count = 1;
      while (!stop() && (retry.length || next < maxReps)) {
        let range;
        if (retry.length) range = retry.shift();
        else {
          count = Math.min(count, maxReps - next);
          range = [next, count, 0];
          next += count;
        }
        const t0 = performance.now();
        let raw;
        try {
          // only the builds still running (each can stop on its own)
          const only = last && last.active && last.active.length < last.each.length ? `,"only":${JSON.stringify(last.active)}` : '';
          raw = await Promise.race([t.text(`{"op":"run","id":${s.id},"from":${range[0]},"count":${range[1]}${only}}`), cancel.p]);
          if (raw === CANCEL) break;
          if (raw.startsWith('{"error"')) throw new Error(JSON.parse(raw).error);
        } catch (e) {
          // this worker is out of the run; another one runs its range, once
          if (range[2] >= 1) failure = failure || e;
          else retry.push([range[0], range[1], range[2] + 1]);
          return;
        }
        const dt = performance.now() - t0;
        if (stop()) break;
        adds.push(this.main.text(`{"op":"add","id":${id},"chunk":${raw}}`).then((res) => {
          const st = JSON.parse(res);
          if (st.error) throw new Error(st.error);
          if (!last || st.reps >= last.reps) {
            last = st;
            onProgress(st);
          }
        }).catch((e) => { failure = failure || e; }));
        if (!range[2]) count = Math.max(1, Math.min(2000, Math.round(range[1] * CHUNK_MS / Math.max(dt, 1))));
      }
    };
    await Promise.all(workers.map((w) => work(w)));
    try {
      if (await Promise.race([Promise.all(adds), cancel.p]) === CANCEL || cancelled()) return null;
      if (failure) throw failure;
      if (!last?.finished) throw new Error(retry.length ? 'every worker stopped before the fights were done' : 'the run ended early');
      const res = await Promise.race([this.main.call({ ...resultOpts, op: 'result', id }), cancel.p]);
      if (res === CANCEL) return null;
      if (res.error) throw new Error(res.error);
      return res;
    } finally {
      cancel.stop();
      drop();
      // the keeper never gives memory back: after a run that left it large,
      // start a fresh one for the next
      if (this.main.mem > KEEPER_MAX_MEM) {
        this.main.terminate();
        this.main = new Thread(this.module);
      }
    }
  }
  static async create() {
    return new WasmEngine(await compileEngine());
  }
}

class HttpEngine {
  constructor() {
    this.kind = 'local server';
  }
  async call(req) {
    const r = await fetch('api', { method: 'POST', body: JSON.stringify(req), headers: { 'Content-Type': 'application/json' } });
    if (!r.ok) throw new Error(`HTTP ${r.status}`);
    return r.json();
  }
  simulate(req, onProgress, cancelled, resultOpts) {
    return stepThrough(this, req, onProgress, cancelled, resultOpts);
  }
}

// Returns an engine that answers (WebAssembly first, then the local
// server), checked with the cheap `version` request rather than the catalog.
export async function openEngine() {
  const errors = [];
  for (const make of [() => WasmEngine.create(), async () => new HttpEngine()]) {
    let engine = null;
    try {
      engine = await make();
      const v = await engine.call({ op: 'version' });
      if (v && v.build_id) return { engine, version: v };
      errors.push(v && v.error || 'no version');
    } catch (e) {
      errors.push(e.message || String(e));
    }
    if (engine && engine.main) engine.main.terminate();
  }
  throw new Error(errors.join('; '));
}

// Returns {engine, catalog}. Tries WebAssembly first, then the local server.
export async function startEngine() {
  const errors = [];
  for (const make of [() => WasmEngine.create(), async () => new HttpEngine()]) {
    let engine = null;
    try {
      engine = await make();
      const catalog = await engine.call({ op: 'catalog' });
      if (catalog && catalog.classes) return { engine, catalog };
      errors.push(catalog && catalog.error || 'empty catalogue');
    } catch (e) {
      errors.push(e.message || String(e));
    }
    if (engine && engine.main) engine.main.terminate();
  }
  throw new Error(errors.join('; '));
}
