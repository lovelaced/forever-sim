// One interface to the simulator, wherever it runs:
//  * in the browser, from fsim.wasm in Web Workers (the GitHub Pages build),
//    one per spare core, each simulating its own share of the fights;
//  * or, for local development, through `fsim serve`, which answers the same
//    JSON requests at POST /api.
// Either way `simulate` gives the same result for the same request: the
// engine checks convergence at fixed fight counts, however the work is split.

// How long one worker's share of fights should take before it reports back.
const CHUNK_MS = 120;

// One Web Worker running fsim.wasm. `text` sends a JSON string and resolves
// with the JSON string it answers.
class Thread {
  constructor(module) {
    this.worker = new Worker(new URL('./worker.js', import.meta.url));
    this.next = 1;
    this.waiting = new Map();
    this.worker.onmessage = (e) => {
      const { id, res, fatal } = e.data;
      const p = this.waiting.get(id);
      if (!p) return;
      this.waiting.delete(id);
      if (fatal) p.reject(new Error(fatal));
      else p.resolve(res);
    };
    this.worker.onerror = (e) => {
      for (const p of this.waiting.values()) p.reject(new Error(e.message || 'worker failed'));
      this.waiting.clear();
    };
    // Browsers that cannot pass a compiled module along make the worker fetch it.
    try { this.worker.postMessage({ module }); } catch { /* fetched by the worker */ }
  }
  text(req) {
    return new Promise((resolve, reject) => {
      const id = this.next++;
      this.waiting.set(id, { resolve, reject });
      this.worker.postMessage({ id, req });
    });
  }
  async call(req) {
    return JSON.parse(await this.text(JSON.stringify(req)));
  }
  terminate() {
    this.worker.terminate();
  }
}

async function compileEngine() {
  const resp = await fetch(new URL('fsim.wasm', import.meta.url));
  if (!resp.ok) throw new Error(`fsim.wasm: HTTP ${resp.status}`);
  const type = resp.headers.get('content-type') || '';
  if (WebAssembly.compileStreaming && type.startsWith('application/wasm')) return WebAssembly.compileStreaming(resp);
  return WebAssembly.compile(await resp.arrayBuffer());
}

// Workers that simulate fights. The first Thread keeps the results, so it
// is not counted; the main thread stays free for the page.
function poolSize() {
  const cores = navigator.hardwareConcurrency || 2;
  return Math.max(1, Math.min(32, cores - 1));
}

// One session stepped through on one engine: the local server, or a
// browser with too few cores to split the work.
async function stepThrough(engine, req, onProgress, cancelled) {
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
    const res = await engine.call({ op: 'result', id: s.id });
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
    return this.main.call(req);
  }
  // Runs `req` (a `start` request) to a result. Every worker opens the same
  // session; each then asks for the next unclaimed range of fights, sized to
  // take about CHUNK_MS, and the main Thread collects them in order until
  // the result is precise enough. Resolves with null when cancelled.
  async simulate(req, onProgress, cancelled) {
    const n = poolSize();
    if (n < 2) return stepThrough(this, req, onProgress, cancelled);
    while (this.pool.length < n) this.pool.push(new Thread(this.module));
    const threads = [this.main, ...this.pool.slice(0, n)];
    const text = JSON.stringify(req);
    const opened = await Promise.all(threads.map((t) => t.text(text).then(JSON.parse)));
    const drop = () => threads.forEach((t, i) => opened[i].id != null && t.text(`{"op":"drop","id":${opened[i].id}}`).catch(() => {}));
    const bad = opened.find((s) => s.error);
    if (bad) {
      drop();
      throw new Error(bad.error);
    }
    const id = opened[0].id;
    const maxReps = opened[0].max_reps;
    let next = 0;
    let last = null;
    let failure = null;
    const stop = () => !!failure || !!(last && last.finished) || cancelled();
    const adds = [];
    const work = async (t, wid) => {
      let count = 1;
      while (!stop() && next < maxReps) {
        const from = next;
        count = Math.min(count, maxReps - from);
        next += count;
        const t0 = performance.now();
        const raw = await t.text(`{"op":"run","id":${wid},"from":${from},"count":${count}}`);
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
        count = Math.max(1, Math.min(2000, Math.round(count * CHUNK_MS / Math.max(dt, 1))));
      }
    };
    await Promise.all(threads.slice(1).map((t, i) => work(t, opened[i + 1].id).catch((e) => { failure = failure || e; })));
    await Promise.all(adds);
    try {
      if (failure) throw failure;
      if (cancelled()) return null;
      const res = await this.main.call({ op: 'result', id });
      if (res.error) throw new Error(res.error);
      return res;
    } finally {
      drop();
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
  simulate(req, onProgress, cancelled) {
    return stepThrough(this, req, onProgress, cancelled);
  }
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
