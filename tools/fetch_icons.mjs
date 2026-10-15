#!/usr/bin/env node
// Collect the game's own icons the website shows (talents, classes, races,
// tests, gear, talent trees) into web/icons/, with a manifest the page reads.
// Icons are Blizzard Entertainment's artwork.
//
// Sources, in order:
//   1. A checkout of github.com/Gethe/wow-ui-textures (the game's interface
//      textures as 64x64 PNGs). Only the ICONS files needed are checked out:
//        git clone --depth 1 --filter=blob:none --sparse --no-checkout \
//          https://github.com/Gethe/wow-ui-textures textures
//      then pass --textures textures; this script does the sparse checkout.
//   2. For anything not found there: Wowhead's image server, then Blizzard's
//      (56x56 JPEGs). Skip with --offline.
//
//   fsim api '{"op":"catalog"}' > catalog.json
//   node tools/fetch_icons.mjs catalog.json --textures textures
//
// Missing icons are not an error: the page draws a stand-in for them.
import { readFileSync, writeFileSync, mkdirSync, existsSync, copyFileSync, readdirSync } from 'node:fs';
import { execFileSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (k) => { const i = args.indexOf(k); return i >= 0 ? args.splice(i, 2)[1] : null; };
const flag = (k) => { const i = args.indexOf(k); return i >= 0 ? (args.splice(i, 1), true) : false; };
const textures = opt('--textures');
const offline = flag('--offline');
const [catalogPath, outDir = join(root, 'web', 'icons')] = args;
if (!catalogPath) {
  console.error('usage: fetch_icons.mjs <catalog.json> [--textures <wow-ui-textures checkout>] [--offline] [out dir]');
  process.exit(2);
}
const catalog = JSON.parse(readFileSync(catalogPath, 'utf8'));
const M = await import(join(root, 'web', 'meta.js'));

const names = new Set(M.extraWowIcons());
for (const c of catalog.classes) {
  if (c.icon) names.add(c.icon);
  for (const t of c.talents) if (t.icon) names.add(t.icon);
}
for (const r of catalog.races) if (r.icon) names.add(r.icon);
for (const n of names) {
  if (!/^[a-z0-9_]+$/.test(n)) { console.error(`bad icon name '${n}'`); process.exit(1); }
}

mkdirSync(outDir, { recursive: true });
const manifestPath = join(outDir, 'manifest.json');
const manifest = existsSync(manifestPath) ? JSON.parse(readFileSync(manifestPath, 'utf8')) : {};
for (const [n, f] of Object.entries(manifest)) if (!names.has(n) || !existsSync(join(outDir, f))) delete manifest[n];
const counts = { textures: 0, web: 0, kept: Object.keys(manifest).length };

// 1. The textures repository (file names there are mixed case).
if (textures) {
  const git = (...a) => execFileSync('git', ['-C', textures, ...a], { encoding: 'utf8', maxBuffer: 64 << 20 });
  const tree = git('ls-tree', '--name-only', 'HEAD', 'ICONS/').trim().split('\n');
  const byName = new Map(tree.map((p) => [p.slice(6).replace(/\.png$/i, '').toLowerCase(), p]));
  const want = [...names].filter((n) => !manifest[n] && byName.has(n)).map((n) => byName.get(n));
  if (want.length) {
    execFileSync('git', ['-C', textures, 'sparse-checkout', 'set', '--no-cone', '--stdin'], { input: want.join('\n') + '\n' });
    git('checkout', 'HEAD');
  }
  for (const n of names) {
    const p = byName.get(n);
    if (manifest[n] || !p || !existsSync(join(textures, p))) continue;
    copyFileSync(join(textures, p), join(outDir, `${n}.png`));
    manifest[n] = `${n}.png`;
    counts.textures++;
  }
}

// 2. Image servers, for whatever is still missing.
const BASES = ['https://wow.zamimg.com/images/wow/icons/large/', 'https://render.worldofwarcraft.com/us/icons/56/'];
const missing = [];
async function fromBase(base, n) {
  for (let attempt = 1; attempt <= 3; attempt++) {
    try {
      const r = await fetch(base + n + '.jpg', { headers: { 'User-Agent': 'forever-sim icon fetch (fan site build)' } });
      if (r.status === 404) return '404';
      if (!r.ok) throw new Error(`HTTP ${r.status}`);
      const buf = Buffer.from(await r.arrayBuffer());
      if (buf.length < 200 || buf[0] !== 0xff || buf[1] !== 0xd8) throw new Error('not a JPEG');
      return buf;
    } catch (e) {
      if (attempt === 3) return e.message;
      await new Promise((ok) => setTimeout(ok, 500 * attempt));
    }
  }
}
const todo = [...names].filter((n) => !manifest[n]).sort();
if (offline) missing.push(...todo);
else {
  const queue = [...todo];
  await Promise.all(Array.from({ length: 6 }, async () => {
    while (queue.length) {
      const n = queue.shift();
      const why = [];
      for (const base of BASES) {
        const r = await fromBase(base, n);
        if (Buffer.isBuffer(r)) {
          writeFileSync(join(outDir, `${n}.jpg`), r);
          manifest[n] = `${n}.jpg`;
          counts.web++;
          why.length = 0;
          break;
        }
        why.push(`${new URL(base).host}: ${r}`);
      }
      if (why.length) missing.push(`${n} (${why.join('; ')})`);
    }
  }));
}

const sorted = Object.fromEntries(Object.entries(manifest).sort());
writeFileSync(manifestPath, JSON.stringify(sorted, null, 0) + '\n');
console.log(`${names.size} icons used: ${counts.textures} from the textures repository, ${counts.web} from image servers, ${counts.kept} already present`);
if (missing.length) console.log(`not found (the page draws a stand-in): ${missing.join(', ')}`);
const stray = readdirSync(outDir).filter((f) => f !== 'manifest.json' && !Object.values(sorted).includes(f));
if (stray.length) console.log(`unused files in ${outDir}: ${stray.length}`);
