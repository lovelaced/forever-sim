// Runs the simulator (fsim.wasm, the Rust engine compiled to WebAssembly)
// off the main thread so the page stays responsive. The page starts several
// of these, one per spare core, and hands each the compiled module.
// Protocol: in {module} once (optional), then {id, req: string};
// out {id, res: string} or {id, fatal: string}.

let inst = null;
let loading = null;
let given = null;

async function load() {
  let module = given;
  if (!module) {
    // The page could not pass the module over: fetch it ourselves.
    const resp = await fetch(new URL('fsim.wasm', self.location.href));
    if (!resp.ok) throw new Error(`fsim.wasm: HTTP ${resp.status}`);
    module = await WebAssembly.compile(await resp.arrayBuffer());
  }
  inst = (await WebAssembly.instantiate(module, {})).exports;
}

function call(req) {
  const enc = new TextEncoder().encode(req);
  const ptr = inst.fsim_alloc(enc.length);
  new Uint8Array(inst.memory.buffer, ptr, enc.length).set(enc);
  inst.fsim_call(ptr, enc.length);
  // Re-read the buffer: memory may have grown (and detached) during the call.
  const out = new Uint8Array(inst.memory.buffer, inst.fsim_out_ptr(), inst.fsim_out_len());
  return new TextDecoder().decode(out);
}

self.onmessage = async (e) => {
  if (e.data.module) {
    given = e.data.module;
    return;
  }
  const { id, req } = e.data;
  try {
    if (!inst) {
      loading = loading || load();
      await loading;
    }
    // with how much memory it holds (WebAssembly memory only grows)
    const res = call(req);
    self.postMessage({ id, res, mem: inst.memory.buffer.byteLength });
  } catch (err) {
    // A trap leaves the module in an unknown state: start afresh next time.
    inst = null;
    loading = null;
    self.postMessage({ id, fatal: String(err && err.message || err) });
  }
};
