#!/usr/bin/env node
// Put the published site together from web/ and the engine:
//   node tools/assemble_site.mjs target/wasm32-unknown-unknown/dist/forever_sim.wasm [out=_site]
// The code, the engine and its catalog go in v/<hash>/, a folder named by
// their content, so a browser can keep them for good and a new version is a
// new folder. The page preloads them, starts compiling the engine before
// any script runs, and draws from catalog.json while the engine starts.
// Art, icons, fonts and styles stay where they are (the styles with a
// ?v=<hash> on their links). Ends with a size report.
import { readFileSync, writeFileSync, mkdirSync, readdirSync, statSync, copyFileSync, rmSync, existsSync } from 'node:fs';
import { join, dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { createHash } from 'node:crypto';
import { gzipSync } from 'node:zlib';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const [wasmPath, outArg] = process.argv.slice(2);
if (!wasmPath) { console.error('usage: assemble_site.mjs <forever_sim.wasm> [out]'); process.exit(2); }
const web = join(root, 'web');
const out = resolve(root, outArg || '_site');

// the catalog, from this very engine
const wasm = readFileSync(wasmPath);
const { exports: ex } = await WebAssembly.instantiate(await WebAssembly.compile(wasm), {});
function call(req) {
  const enc = new TextEncoder().encode(JSON.stringify(req));
  const ptr = ex.fsim_alloc(enc.length);
  new Uint8Array(ex.memory.buffer, ptr, enc.length).set(enc);
  ex.fsim_call(ptr, enc.length);
  return new TextDecoder().decode(new Uint8Array(ex.memory.buffer, ex.fsim_out_ptr(), ex.fsim_out_len()));
}
const catalog = call({ op: 'catalog' });
if (catalog.startsWith('{"error"')) throw new Error(catalog);

const files = (dir) => readdirSync(dir).flatMap((f) => {
  const p = join(dir, f);
  return statSync(p).isDirectory() ? files(p) : [p];
});
const all = files(web);
const code = all.filter((p) => dirname(p) === web && p.endsWith('.js')).sort();

const hash = createHash('sha256');
for (const p of code) hash.update(relative(web, p)).update(readFileSync(p));
hash.update(wasm).update(catalog);
const v = hash.digest('hex').slice(0, 10);
const vdir = `v/${v}`;

rmSync(out, { recursive: true, force: true });
mkdirSync(join(out, vdir), { recursive: true });
for (const p of all) {
  const rel = relative(web, p);
  if (code.includes(p) || rel === 'index.html') continue;
  mkdirSync(dirname(join(out, rel)), { recursive: true });
  copyFileSync(p, join(out, rel));
}
for (const p of code) copyFileSync(p, join(out, vdir, relative(web, p)));
copyFileSync(wasmPath, join(out, vdir, 'fsim.wasm'));
writeFileSync(join(out, vdir, 'catalog.json'), catalog);

// the page: versioned code, preloads, the engine compiling from the start
let html = readFileSync(join(web, 'index.html'), 'utf8');
const preload = [
  `<link rel="preload" href="${vdir}/fsim.wasm" as="fetch" type="application/wasm" crossorigin>`,
  `<link rel="preload" href="${vdir}/catalog.json" as="fetch" type="application/json" crossorigin>`,
  ...code.map((p) => `<link rel="modulepreload" href="${vdir}/${relative(web, p)}">`),
  // start compiling the engine before any script runs; the page picks it up
  `<script>window.fsimVersion = ${JSON.stringify(v)}; window.fsimModule = 'compileStreaming' in WebAssembly ? WebAssembly.compileStreaming(fetch(${JSON.stringify(`${vdir}/fsim.wasm`)})).catch(() => null) : null;</script>`,
].join('\n  ');
if (!html.includes('<script type="module" src="app.js"></script>')) throw new Error('index.html: no app.js script tag to version');
html = html.replace('<script type="module" src="app.js"></script>', `${preload}\n  <script type="module" src="${vdir}/app.js"></script>`);
html = html.replace(/href="(css\/[^"]+\.css)"/g, `href="$1?v=${v}"`);
writeFileSync(join(out, 'index.html'), html);

// size report
const kb = (n) => `${(n / 1024).toFixed(0)} KiB`;
const sizeOf = (paths) => {
  let raw = 0, gz = 0;
  for (const p of paths) {
    const b = readFileSync(p);
    raw += b.length;
    gz += gzipSync(b, { level: 9 }).length;
  }
  return `${kb(raw)} (${kb(gz)} gzipped)`;
};
const outFiles = files(out);
const pick = (f) => outFiles.filter(f);
console.log(`site in ${relative(root, out)}/, version ${v}`);
console.log(`  engine   ${sizeOf([join(out, vdir, 'fsim.wasm')])}`);
console.log(`  catalog  ${sizeOf([join(out, vdir, 'catalog.json')])}`);
console.log(`  code     ${sizeOf(pick((p) => p.includes(`/${vdir}/`) && p.endsWith('.js')))}`);
console.log(`  styles   ${sizeOf(pick((p) => p.endsWith('.css')))}`);
console.log(`  art      ${sizeOf(pick((p) => p.includes('/art/')))}`);
console.log(`  icons    ${sizeOf(pick((p) => p.includes('/icons/')))}`);
console.log(`  fonts    ${sizeOf(pick((p) => p.includes('/fonts/')))}`);
if (!existsSync(join(out, 'icons', 'manifest.json'))) console.log('  note: no icons/manifest.json (run tools/fetch_icons.mjs first)');
