#!/usr/bin/env node
// Make the site's pixel art: each piece in art/pieces.json is drawn by the
// codex CLI's image tool, then cleaned into true pixel art (one color per
// grid cell, a fixed palette, no background) by pixfix, and saved at its
// real size in web/art/.
//
//   node tools/art.mjs gen  [ids or groups] [--force] [--jobs 6]   draw (raw images in art/raw/, gitignored)
//   node tools/art.mjs fix  [ids or groups]                        clean raw images into web/art/
//   node tools/art.mjs list [ids or groups]                        pieces and their state
//
// Without ids it works on every piece. `gen` skips pieces that already have
// a raw image unless --force. Needs `codex` (logged in) and `pixfix`
// (github.com/lovelaced/pixfix; set PIXFIX to its binary). The art is
// original fan art: prompts ask for World of Warcraft's places, classes and
// armor, never logos, text or named characters.
import { readFileSync, writeFileSync, mkdirSync, existsSync, readdirSync, statSync, copyFileSync, rmSync } from 'node:fs';
import { spawn, execFileSync } from 'node:child_process';
import { inflateSync, deflateSync } from 'node:zlib';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';
import { homedir } from 'node:os';

const root = join(dirname(fileURLToPath(import.meta.url)), '..');
const args = process.argv.slice(2);
const opt = (k, d) => { const i = args.indexOf(k); return i >= 0 ? args.splice(i, 2)[1] : d; };
const flag = (k) => { const i = args.indexOf(k); return i >= 0 ? (args.splice(i, 1), true) : false; };
const force = flag('--force');
const jobs = Number(opt('--jobs', '6'));
const model = opt('--model', process.env.ART_MODEL || 'gpt-6-luna');
const PIXFIX = process.env.PIXFIX || join(homedir(), 'git/normalize-pixelart/target/release/pixfix');
const CODEX = process.env.CODEX || 'codex';
const [cmd = 'list', ...sel] = args;

const spec = JSON.parse(readFileSync(join(root, 'art/pieces.json'), 'utf8'));
const RAW = join(root, 'art/raw');
const OUT = join(root, 'web/art');
mkdirSync(RAW, { recursive: true });
mkdirSync(OUT, { recursive: true });

// one hero sprite per race and class pair, all on one scale
const H = spec.heroes;
const heroes = H.pairs.map((pair) => {
  const [race, cls] = pair.split('/');
  const r = H.races[race];
  const c = H.classes[cls];
  if (!r || !c) throw new Error(`hero ${pair}: no description`);
  return {
    id: `hero-${race}-${cls}`, group: 'heroes', style: 'sprite',
    prompt: `${r.desc}, as ${c}, in the look of classic World of Warcraft. The character's height is about ${r.height}% of the canvas height (all heroes share one scale: a tauren nearly fills the canvas, a gnome reaches less than half).`,
  };
});
const all = [...spec.pieces, ...heroes];
const pieces = all.filter((p) => !sel.length || sel.includes(p.id) || sel.includes(p.group));
if (!pieces.length) {
  console.error('no pieces match', sel.join(' '));
  process.exit(2);
}

// The words every prompt of a kind shares, so the pieces look like one set.
function promptFor(p) {
  const st = spec.styles[p.style];
  if (!st) throw new Error(`${p.id}: unknown style ${p.style}`);
  const lines = [
    `Use case: stylized-concept`,
    `Asset type: ${st.asset}`,
    `Primary request: ${p.prompt}`,
    `Style/medium: ${st.medium}`,
    `Composition/framing: ${p.framing || st.framing}`,
    `Scene/backdrop: ${p.backdrop || st.backdrop}`,
    // a piece can replace the shared constraints (a scene with a named boss)
    `Constraints: ${p.constraints || spec.constraints}${st.constraints ? ' ' + st.constraints : ''}`,
  ];
  return lines.join('\n');
}

function instructions(p, file) {
  const shape = spec.aspects[p.aspect || spec.styles[p.style].aspect];
  return `Use the built-in image_gen tool to generate exactly one ${shape} image, passing it the image prompt below word for word. Do not write code, do not run commands, and do not copy, resize, crop, recolor or otherwise touch the image afterwards. Then reply with only the word done.\n\nImage prompt:\n${promptFor(p)}`;
}

function run(bin, argv, { cwd, input } = {}) {
  return new Promise((resolve) => {
    const ch = spawn(bin, argv, { cwd, stdio: ['ignore', 'pipe', 'pipe'] });
    let out = '', err = '';
    ch.stdout.on('data', (d) => (out += d));
    ch.stderr.on('data', (d) => (err += d));
    ch.on('close', (code) => resolve({ code, out, err }));
  });
}

async function gen(p) {
  const raw = join(RAW, `${p.id}.png`);
  if (existsSync(raw) && !force) return `${p.id}: kept`;
  const work = join(RAW, '.work', p.id);
  rmSync(work, { recursive: true, force: true });
  mkdirSync(work, { recursive: true });
  const file = `${p.id}-raw.png`;
  const text = instructions(p, file);
  writeFileSync(join(RAW, `${p.id}.prompt.txt`), text);
  const t0 = Date.now();
  const r = await run(CODEX, ['exec', '--skip-git-repo-check', '--ephemeral', '-s', 'workspace-write', '-C', work,
    '-m', model, '-c', 'model_reasoning_effort="low"', '--json', '-o', join(work, 'last.txt'), text], { cwd: work });
  writeFileSync(join(work, 'events.jsonl'), r.out);
  // The image comes from the image tool's folder for this run's own thread:
  // with several runs at once, the copy codex makes can be another run's
  // newest image, so it is never trusted.
  const thread = (r.out.match(/"thread_id"\s*:\s*"([^"]+)"/) || [])[1];
  const dir = thread && join(homedir(), '.codex/generated_images', thread);
  const pngs = dir && existsSync(dir) ? readdirSync(dir).filter((f) => f.endsWith('.png')).map((f) => join(dir, f)) : [];
  const src = pngs.sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
  if (!src || !existsSync(src)) return `${p.id}: FAILED (codex exit ${r.code}) ${(r.err || r.out).slice(-300).replace(/\s+/g, ' ')}`;
  copyFileSync(src, raw);
  return `${p.id}: drawn in ${Math.round((Date.now() - t0) / 1000)} s`;
}

// Decode a non-interlaced 8-bit PNG (what the image tool writes) into RGBA.
function readPng(path) {
  const buf = readFileSync(path);
  let pos = 8, w = 0, h = 0, type = 0, depth = 0, palette = null;
  const idat = [];
  while (pos < buf.length) {
    const len = buf.readUInt32BE(pos), tag = buf.toString('ascii', pos + 4, pos + 8), data = buf.subarray(pos + 8, pos + 8 + len);
    if (tag === 'IHDR') { w = data.readUInt32BE(0); h = data.readUInt32BE(4); depth = data[8]; type = data[9]; }
    else if (tag === 'PLTE') palette = data;
    else if (tag === 'IDAT') idat.push(data);
    pos += 12 + len;
  }
  if (depth !== 8) throw new Error(`${path}: ${depth}-bit PNG not supported`);
  const ch = { 0: 1, 2: 3, 3: 1, 4: 2, 6: 4 }[type];
  const raw = inflateSync(Buffer.concat(idat));
  const stride = w * ch, out = Buffer.alloc(w * h * 4), cur = Buffer.alloc(stride), prev = Buffer.alloc(stride);
  for (let y = 0; y < h; y++) {
    const ft = raw[y * (stride + 1)], line = raw.subarray(y * (stride + 1) + 1, (y + 1) * (stride + 1));
    for (let i = 0; i < stride; i++) {
      const a = i >= ch ? cur[i - ch] : 0, b = prev[i], c = i >= ch ? prev[i - ch] : 0;
      const pr = ft === 4 ? (() => { const p0 = a + b - c, pa = Math.abs(p0 - a), pb = Math.abs(p0 - b), pc = Math.abs(p0 - c); return pa <= pb && pa <= pc ? a : pb <= pc ? b : c; })() : 0;
      cur[i] = (line[i] + [0, a, b, (a + b) >> 1, pr][ft]) & 255;
    }
    for (let x = 0; x < w; x++) {
      const o = (y * w + x) * 4, i = x * ch;
      if (type === 3) { const k = cur[i] * 3; out[o] = palette[k]; out[o + 1] = palette[k + 1]; out[o + 2] = palette[k + 2]; out[o + 3] = 255; }
      else if (ch >= 3) { out[o] = cur[i]; out[o + 1] = cur[i + 1]; out[o + 2] = cur[i + 2]; out[o + 3] = ch === 4 ? cur[i + 3] : 255; }
      else { out[o] = out[o + 1] = out[o + 2] = cur[i]; out[o + 3] = ch === 2 ? cur[i + 1] : 255; }
    }
    cur.copy(prev);
  }
  return { w, h, px: out };
}

// Write RGBA pixels as a PNG.
function writePng(path, w, h, px, pal) {
  const crcTable = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
  const crc = (b) => { let c = 0xffffffff; for (const x of b) c = crcTable[(c ^ x) & 255] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
  const chunk = (tag, data) => {
    const len = Buffer.alloc(4); len.writeUInt32BE(data.length);
    const td = Buffer.concat([Buffer.from(tag, 'ascii'), data]);
    const c = Buffer.alloc(4); c.writeUInt32BE(crc(td));
    return Buffer.concat([len, td, c]);
  };
  const ihdr = Buffer.alloc(13);
  const bpp = pal ? 1 : 4;
  ihdr.writeUInt32BE(w, 0); ihdr.writeUInt32BE(h, 4); ihdr[8] = 8; ihdr[9] = pal ? 3 : 6;
  const raw = Buffer.alloc(h * (w * bpp + 1));
  for (let y = 0; y < h; y++) px.copy(raw, y * (w * bpp + 1) + 1, y * w * bpp, (y + 1) * w * bpp);
  const extra = pal ? [chunk('PLTE', pal.plte), chunk('tRNS', pal.trns)] : [];
  writeFileSync(path, Buffer.concat([Buffer.from([137, 80, 78, 71, 13, 10, 26, 10]), chunk('IHDR', ihdr), ...extra, chunk('IDAT', deflateSync(raw, { level: 9 })), chunk('IEND', Buffer.alloc(0))]));
}

// Rewrite a PNG with a colour palette when it has 256 colours or fewer:
// the same pixels, a fraction of the size.
function indexPng(path) {
  const { w, h, px } = readPng(path);
  const index = new Map();
  const idx = Buffer.alloc(w * h);
  for (let i = 0; i < w * h; i++) {
    const a = px[i * 4 + 3];
    const key = a === 0 ? 0 : ((px[i * 4] << 24) | (px[i * 4 + 1] << 16) | (px[i * 4 + 2] << 8) | a) >>> 0;
    let k = index.get(key);
    if (k === undefined) {
      if (index.size >= 256) return false;
      k = index.size;
      index.set(key, k);
    }
    idx[i] = k;
  }
  const keys = [...index.keys()];
  const plte = Buffer.alloc(keys.length * 3), trns = Buffer.alloc(keys.length);
  keys.forEach((key, k) => { plte[k * 3] = key >>> 24; plte[k * 3 + 1] = (key >>> 16) & 255; plte[k * 3 + 2] = (key >>> 8) & 255; trns[k] = key & 255; });
  writePng(path, w, h, idx, { plte, trns });
  return true;
}

// Crop a PNG to what isn't transparent, so frames slice at their edges.
function trim(path) {
  const { w, h, px } = readPng(path);
  let x0 = w, y0 = h, x1 = -1, y1 = -1;
  for (let y = 0; y < h; y++) for (let x = 0; x < w; x++) if (px[(y * w + x) * 4 + 3] > 0) { x0 = Math.min(x0, x); x1 = Math.max(x1, x); y0 = Math.min(y0, y); y1 = Math.max(y1, y); }
  if (x1 < 0 || (x0 === 0 && y0 === 0 && x1 === w - 1 && y1 === h - 1)) return [w, h];
  const cw = x1 - x0 + 1, chh = y1 - y0 + 1, out = Buffer.alloc(cw * chh * 4);
  for (let y = 0; y < chh; y++) px.copy(out, y * cw * 4, ((y + y0) * w + x0) * 4, ((y + y0) * w + x1 + 1) * 4);
  writePng(path, cw, chh, out);
  return [cw, chh];
}

// Height in raw pixels of what stands on the flat magenta background.
function subjectHeight(path) {
  const { w, h, px } = readPng(path);
  const bg = [px[0], px[1], px[2]];
  const isBg = (o) => px[o + 3] < 128 || Math.abs(px[o] - bg[0]) + Math.abs(px[o + 1] - bg[1]) + Math.abs(px[o + 2] - bg[2]) < 90;
  let top = h, bottom = -1;
  for (let y = 0; y < h; y++) {
    let n = 0;
    for (let x = 0; x < w; x++) if (!isBg((y * w + x) * 4)) n++;
    if (n > w * 0.004) { top = Math.min(top, y); bottom = y; }
  }
  return bottom >= top ? bottom - top + 1 : h;
}

async function fix(p) {
  const raw = join(RAW, `${p.id}.png`);
  if (!existsSync(raw)) return { id: p.id, error: 'no raw image (run gen first)' };
  const f = { ...spec.fix[p.style], ...(p.fix || {}) };
  if (f.hero_height) {
    // heroes share one scale: a race's height in art pixels, whatever size
    // the image tool drew it
    const race = p.id.split('-')[1];
    const target = f.hero_height * (spec.heroes.races[race]?.height ?? 80) / 100;
    f.grid = +(subjectHeight(raw) / target).toFixed(3);
  }
  const out = join(OUT, `${p.id}.png`);
  const argv = ['--json', '--no-config', 'process', raw, out, '--overwrite', '--seed', '1'];
  if (f.grid) argv.push('--grid-size', String(f.grid));
  else argv.push('--min-confidence', String(f.min_confidence ?? 0.3));
  if (f.coarsen) argv.push('--coarsen', String(f.coarsen));
  if (f.palette) argv.push('--palette-file', join(root, 'art/palettes', `${f.palette}.hex`));
  else argv.push('--colors', String(f.colors ?? 24));
  if (f.bg) argv.push('--remove-bg');
  // a background enclosed by the subject (the inside of a ring) needs a key
  if (f.chroma) argv.push('--chroma-key', f.chroma, '--chroma-tolerance', String(f.tolerance ?? 0.15));
  argv.push('--downscale-mode', 'majority-vote', '--logical-size');
  const r = await run(PIXFIX, argv);
  let d;
  try { d = JSON.parse(r.out); } catch { return { id: p.id, error: (r.err || r.out).slice(-300) }; }
  if (d.error) return { id: p.id, error: d.error };
  // everything with a cut-out background is cropped to its content
  const size = f.bg || f.chroma ? trim(out) : d.logical_size;
  indexPng(out);
  return {
    id: p.id, size, colors: d.colors_after, grid: d.grid && [d.grid.pitch_x, d.grid.pitch_y].map((x) => +(+x).toFixed(2)),
    contested: d.total_blocks ? +(d.low_confidence_blocks / d.total_blocks).toFixed(3) : null, bg_removed: d.bg_pixels_removed ?? null,
  };
}

async function pool(items, n, fn) {
  const res = [];
  let i = 0;
  await Promise.all(Array.from({ length: Math.min(n, items.length) }, async () => {
    while (i < items.length) {
      const k = i++;
      res[k] = await fn(items[k]);
      if (typeof res[k] === 'string') console.log(res[k]);
    }
  }));
  return res;
}

if (cmd === 'gen') {
  try { execFileSync(CODEX, ['--version'], { stdio: 'ignore' }); } catch { console.error('codex not found (set CODEX)'); process.exit(1); }
  await pool(pieces, jobs, gen);
} else if (cmd === 'fix') {
  if (!existsSync(PIXFIX)) { console.error(`pixfix not found at ${PIXFIX} (set PIXFIX)`); process.exit(1); }
  const res = await pool(pieces, 8, fix);
  const reportPath = join(root, 'art/report.json');
  const report = existsSync(reportPath) ? JSON.parse(readFileSync(reportPath, 'utf8')) : {};
  for (const r of res) {
    if (r.error) console.log(`${r.id}: ${r.error}`);
    else {
      report[r.id] = r;
      if (!r.size || !r.grid) {
        console.log(`${r.id}: NOT SNAPPED (no grid found; give it a "grid" in its fix)`);
        continue;
      }
      const warn = r.contested > 0.12 ? '  (grid unsure: check it by eye)' : '';
      console.log(`${r.id}: ${r.size.join('x')} px, ${r.colors} colors, grid ${r.grid.join('x')}, ${(100 * r.contested).toFixed(1)}% contested${warn}`);
    }
  }
  writeFileSync(reportPath, JSON.stringify(report, null, 1) + '\n');
} else if (cmd === 'list') {
  for (const p of pieces) {
    const raw = existsSync(join(RAW, `${p.id}.png`)) ? 'drawn' : '-';
    const out = existsSync(join(OUT, `${p.id}.png`)) ? 'cleaned' : '-';
    console.log(`${p.group.padEnd(10)} ${p.id.padEnd(28)} ${raw.padEnd(6)} ${out}`);
  }
} else if (cmd === 'prompt') {
  for (const p of pieces) console.log(`== ${p.id}\n${instructions(p, `${p.id}-raw.png`)}\n`);
} else {
  console.error('usage: art.mjs gen|fix|list|prompt [ids or groups] [--force] [--jobs N] [--model M]');
  process.exit(2);
}
