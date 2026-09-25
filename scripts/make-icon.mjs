/**
 * Generates the mdex app icon (src-tauri/app-icon.png) without any image
 * dependencies: a rounded-square indigo/violet gradient with a bold white M.
 * Output is a valid 1024x1024 RGBA PNG built by hand with zlib.
 */

import { deflateSync } from "node:zlib";
import { writeFileSync, mkdirSync } from "node:fs";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const SIZE = 1024;
const RADIUS = 176;

// Colors
const TOP = [79, 70, 229]; // indigo-600
const BOTTOM = [124, 58, 237]; // violet-600
const WHITE = [255, 255, 255];

// --- PNG plumbing -----------------------------------------------------------

function crc32(buf) {
  let c = ~0;
  for (let i = 0; i < buf.length; i++) {
    c ^= buf[i];
    for (let k = 0; k < 8; k++) c = (c >>> 1) ^ (0xedb88320 & -(c & 1));
  }
  return ~c >>> 0;
}

function chunk(type, data) {
  const len = Buffer.alloc(4);
  len.writeUInt32BE(data.length, 0);
  const body = Buffer.concat([Buffer.from(type, "ascii"), data]);
  const crc = Buffer.alloc(4);
  crc.writeUInt32BE(crc32(body), 0);
  return Buffer.concat([len, body, crc]);
}

// --- Geometry ----------------------------------------------------------------

function insideRoundedSquare(x, y) {
  const min = 0;
  const max = SIZE - 1;
  const r = RADIUS;
  if (x < min || x > max || y < min || y > max) return false;
  const dx = Math.max(r - x, 0, x - (max - r));
  const dy = Math.max(r - y, 0, y - (max - r));
  return dx * dx + dy * dy <= r * r;
}

function distToSegment(px, py, ax, ay, bx, by) {
  const abx = bx - ax;
  const aby = by - ay;
  const t = Math.max(0, Math.min(1, ((px - ax) * abx + (py - ay) * aby) / (abx * abx + aby * aby)));
  const cx = ax + t * abx;
  const cy = ay + t * aby;
  return Math.hypot(px - cx, py - cy);
}

/** Bold "M" defined in normalized icon coordinates. */
function insideM(nx, ny) {
  const y0 = 0.2;
  const y1 = 0.82;
  // Stems
  if (ny >= y0 && ny <= y1 && ((nx >= 0.18 && nx <= 0.31) || (nx >= 0.69 && nx <= 0.82))) {
    return true;
  }
  // Diagonals meeting at the middle vertex
  const s = SIZE;
  const d1 = distToSegment(nx * s, ny * s, 0.245 * s, y0 * s, 0.5 * s, 0.6 * s);
  const d2 = distToSegment(nx * s, ny * s, 0.755 * s, y0 * s, 0.5 * s, 0.6 * s);
  return d1 <= 0.068 * s || d2 <= 0.068 * s;
}

// --- Render with 3x3 supersampling -------------------------------------------

const raw = Buffer.alloc(SIZE * (SIZE * 4 + 1));

for (let y = 0; y < SIZE; y++) {
  const rowStart = y * (SIZE * 4 + 1);
  raw[rowStart] = 0; // filter: none
  for (let x = 0; x < SIZE; x++) {
    let insideCount = 0;
    let mCount = 0;
    for (let sy = 0; sy < 3; sy++) {
      for (let sx = 0; sx < 3; sx++) {
        const px = x + (sx + 0.5) / 3;
        const py = y + (sy + 0.5) / 3;
        if (insideRoundedSquare(px, py)) {
          insideCount++;
          if (insideM(px / SIZE, py / SIZE)) mCount++;
        }
      }
    }
    const total = 9;
    const alpha = (insideCount / total) * 255;
    let r, g, b;
    if (mCount > 0) {
      const mix = mCount / total;
      // gradient underneath blended toward white for the glyph
      const t = (x + y) / (2 * SIZE);
      const gr = TOP[0] + (BOTTOM[0] - TOP[0]) * t;
      const gg = TOP[1] + (BOTTOM[1] - TOP[1]) * t;
      const gb = TOP[2] + (BOTTOM[2] - TOP[2]) * t;
      r = Math.round(gr + (WHITE[0] - gr) * mix);
      g = Math.round(gg + (WHITE[1] - gg) * mix);
      b = Math.round(gb + (WHITE[2] - gb) * mix);
    } else {
      const t = (x + y) / (2 * SIZE);
      r = Math.round(TOP[0] + (BOTTOM[0] - TOP[0]) * t);
      g = Math.round(TOP[1] + (BOTTOM[1] - TOP[1]) * t);
      b = Math.round(TOP[2] + (BOTTOM[2] - TOP[2]) * t);
    }
    const o = rowStart + 1 + x * 4;
    raw[o] = r;
    raw[o + 1] = g;
    raw[o + 2] = b;
    raw[o + 3] = Math.round(alpha);
  }
}

const ihdr = Buffer.alloc(13);
ihdr.writeUInt32BE(SIZE, 0);
ihdr.writeUInt32BE(SIZE, 4);
ihdr[8] = 8; // bit depth
ihdr[9] = 6; // RGBA
ihdr[10] = 0;
ihdr[11] = 0;
ihdr[12] = 0;

const png = Buffer.concat([
  Buffer.from([0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a]),
  chunk("IHDR", ihdr),
  chunk("IDAT", deflateSync(raw, { level: 9 })),
  chunk("IEND", Buffer.alloc(0)),
]);

const outPath = resolve(dirname(fileURLToPath(import.meta.url)), "../src-tauri/app-icon.png");
mkdirSync(dirname(outPath), { recursive: true });
writeFileSync(outPath, png);
console.log(`Wrote ${outPath} (${png.length} bytes)`);
