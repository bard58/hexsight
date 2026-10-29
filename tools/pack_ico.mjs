// Pack icon-<size>.png files into one .ico (PNG-compressed entries).
// Usage: node pack_ico.mjs <dir> <out.ico>
import fs from "node:fs";
import path from "node:path";
const [dir, out] = process.argv.slice(2);
const sizes = [16, 24, 32, 48, 64, 128, 256];
const pngs = sizes.map((s) => fs.readFileSync(path.join(dir, `icon-${s}.png`)));
const header = Buffer.alloc(6);
header.writeUInt16LE(0, 0); // reserved
header.writeUInt16LE(1, 2); // type: icon
header.writeUInt16LE(sizes.length, 4);
const entries = [];
let offset = 6 + 16 * sizes.length;
sizes.forEach((s, i) => {
  const e = Buffer.alloc(16);
  e.writeUInt8(s >= 256 ? 0 : s, 0); // width (0 = 256)
  e.writeUInt8(s >= 256 ? 0 : s, 1); // height
  e.writeUInt8(0, 2); // palette
  e.writeUInt8(0, 3); // reserved
  e.writeUInt16LE(1, 4); // planes
  e.writeUInt16LE(32, 6); // bits per pixel
  e.writeUInt32LE(pngs[i].length, 8);
  e.writeUInt32LE(offset, 12);
  offset += pngs[i].length;
  entries.push(e);
});
fs.writeFileSync(out, Buffer.concat([header, ...entries, ...pngs]));
console.log(`wrote ${out} (${offset} bytes)`);
