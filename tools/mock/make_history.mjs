// Write a fake history.json for tests, so the My games tab has many rows.
// Usage: node make_history.mjs <data folder> [games]
// The data folder is the Hexsight folder of a test run, for example <isolated LOCALAPPDATA>\Hexsight.
// The tool refuses the real folder %LOCALAPPDATA%\Hexsight unless you add --force.
import fs from "node:fs";
import path from "node:path";

const args = process.argv.slice(2).filter((a) => a !== "--force");
const force = process.argv.includes("--force");
const dir = args[0];
if (!dir) {
  console.error("Usage: node make_history.mjs <data folder> [games] [--force]");
  process.exit(1);
}
const real = path.resolve(process.env.LOCALAPPDATA || "", "Hexsight").toLowerCase();
if (path.resolve(dir).toLowerCase() === real && !force) {
  console.error(`Refused: ${dir} is the real data folder of Hexsight. Use an isolated LOCALAPPDATA.`);
  process.exit(1);
}
const count = Number(args[1] || 40);

// A small random generator with a fixed start, so each run gives the same file.
let seed = 20261023;
const rand = () => ((seed = (seed * 1664525 + 1013904223) >>> 0) / 2 ** 32);
const pick = (list) => list[Math.floor(rand() * list.length)];

// Real Mayhem augment ids (patch 16.21). The first ones come up more often.
const augments = [1058, 3078, 1337, 1206, 3116, 1205, 1080, 1001, 1041, 1006, 1030, 1195, 1002, 1103, 1013, 1022, 1007, 1044, 1138, 1181, 1105, 1038, 1116, 1134, 1011, 1019, 2010, 2087, 1310, 1323];
// Champion ids: Yasuo, Caitlyn, Ahri, Jinx, Viego, Annie, Lux, Thresh.
const champions = [157, 157, 157, 157, 51, 51, 103, 103, 222, 234, 1, 99, 412];

const T0 = 1792747231762;
const games = [];
for (let i = 0; i < count; i++) {
  const picks = [];
  while (picks.length < 4) {
    // The index is skewed to the start of the list.
    const a = augments[Math.floor(rand() ** 2 * augments.length)];
    if (!picks.includes(a)) picks.push(a);
  }
  const champion = pick(champions);
  const win = rand() < (champion === 157 ? 0.62 : 0.5);
  const date = new Date(T0 - i * 7 * 3_600_000 - Math.floor(rand() * 3_600_000)).toISOString().replace(/\.\d+Z$/, "Z");
  games.push({
    id: 7000099 - i,
    date,
    queue: 2400,
    champion,
    augments: picks,
    win,
    kills: Math.floor(rand() * 16),
    deaths: Math.floor(rand() * 12),
    assists: Math.floor(rand() * 18),
  });
}
fs.mkdirSync(dir, { recursive: true });
const text = `{"version":1,"games":[\n${games.map((g) => JSON.stringify(g)).join(",\n")}\n]}\n`;
fs.writeFileSync(path.join(dir, "history.json"), text);
console.log(`wrote ${games.length} fake games to ${path.join(dir, "history.json")}`);
