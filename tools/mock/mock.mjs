// Mock League Client (LCU) + Live Client Data API for local tests.
// Usage: node mock.mjs [phase]   phase: ChampSelect | InProgress | Lobby | None
// Then start the app with HEXSIGHT_LEAGUE_DIR=<this folder>/league
// Change phase at runtime: curl -k -u riot:testpass https://127.0.0.1:52811/mock/phase/InProgress
// Own champion at runtime: /mock/me/<champion id> (an id without stats shows the no-data paths).
// Bench at runtime: /mock/bench/<0|1>. Off is a champ select without a bench (not ARAM).
// The first step of a Mayhem champ select: /mock/me/0 gives a player without a champion (championId 0 in the
// session). The client then offers champions on GET /lol-lobby-team-builder/champ-select/v1/subset-champion-list,
// a JSON array of champion ids (the mock offers [164, 235, 12] first). /mock/subset/<id,id,id> sets the offer,
// /mock/subset/empty makes the list [], and /mock/subset/missing makes the route answer 404 (an older client).
// The route answers 404 outside a champ select. /mock/me/<id> with an id above 0 is a player who has picked.
// League window mode at runtime: /mock/window/<0 Fullscreen | 1 Windowed | 2 Borderless>.
// The mock writes league/Config/game.cfg for it. The default is 2.
// Champion trades (champ select session "trades", ids 100 + cell): /mock/trade/<cell>/<AVAILABLE|BUSY|INVALID|RECEIVED|SENT>
// sets the state of the trade with the teammate in that cell (0, 1, 3 or 4). /mock/trades shows the state of the
// mock (champions, trades, route). POST .../session/<trades|champion-swaps>/<id>/<request|accept|decline|cancel>
// work as in the client: request needs AVAILABLE (then SENT), accept needs RECEIVED (the champions swap),
// decline needs RECEIVED, and cancel needs SENT (all three then AVAILABLE). /mock/route/<trades|swaps> picks the
// route that exists. The other one answers 404, as an older client would, so the fall back of the app shows.
// Game settings, as the client keeps them: GET /lol-game-settings/v1/game-settings, PATCH the same route with the
// changed settings (it merges, and the file waits), and POST /lol-game-settings/v1/save writes game.cfg. The app uses
// them for the button "Fix" next to the Fullscreen note.
// Match history, in the shape of the real LCU: /lol-summoner/v1/current-summoner,
// /lol-match-history/v1/products/lol/<puuid>/matches (the newest games of the player), and
// /lol-match-history/v1/games/<id> (one whole game). The game of the gameflow session (id 7000100)
// is not in the list, as with a stale list in the real client. /mock/late/<n> lets that game answer
// 404 for the next n requests, so the retries of the app can be seen.
import https from "node:https";
import fs from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const here = path.dirname(fileURLToPath(import.meta.url));
const tls = { key: fs.readFileSync(path.join(here, "key.pem")), cert: fs.readFileSync(path.join(here, "cert.pem")) };
const PORT = 52811;
const PASS = "testpass";
const AUTH = "Basic " + Buffer.from("riot:" + PASS).toString("base64");

fs.mkdirSync(path.join(here, "league"), { recursive: true });
fs.writeFileSync(path.join(here, "league", "lockfile"), `LeagueClient:4242:${PORT}:${PASS}:https`);

// The settings of the client (a small part of the real object). `writeWindowMode` keeps WindowMode in step.
const gameSettings = { General: { WindowMode: 2, Height: 1080, Width: 1920 }, Audio: { MasterVolume: 0.5 } };
// A PATCH waits here until the POST on /save writes it to game.cfg.
let pendingMode = null;

// The app reads WindowMode from Config/game.cfg in the League folder.
function writeWindowMode(mode) {
  gameSettings.General.WindowMode = Number(mode);
  fs.mkdirSync(path.join(here, "league", "Config"), { recursive: true });
  fs.writeFileSync(path.join(here, "league", "Config", "game.cfg"), `\n[General]\nHeight=1080\nWindowMode=${mode}\nWidth=1920\n`);
}
writeWindowMode(2);

let phase = process.argv[2] || "ChampSelect";
const state = {
  me: 51, bench: [103, 157, 222], team: [1, 89, 412, 99], benchOn: true, late: 0,
  // The champions of the first step (Camille, Senna, Alistar), or null for a client without the route.
  subset: [164, 235, 12],
  // Trades by teammate cell. The teammates sit in the cells 0, 1, 3 and 4 (the player is in cell 2).
  trades: { 0: "AVAILABLE", 1: "AVAILABLE", 3: "BUSY", 4: "INVALID" },
  route: "trades",
};
const TEAM_INDEX = { 0: 0, 1: 1, 3: 2, 4: 3 }; // cell -> index in state.team
const STATES = ["AVAILABLE", "BUSY", "INVALID", "RECEIVED", "SENT"];

// ---- Match history. The numbers are made up. The field names are the ones of the real LCU.
const PUUID = "mock-puuid-0001";
const RUNNING_GAME = 7000100; // the game of the gameflow session
const T0 = 1792747231762; // 2026-10-23 09:20:31 UTC
const DAY = 86_400_000;

// A whole game: ten players, the local player is participant 3. `augments` are the playerAugment1..6 slots.
function wholeGame({ id, queue = 2400, ago, champ, win, k, d, a, augments, duration = 1186 }) {
  const champs = [1, 89, champ, 412, 99, 51, 103, 222, 234, 18];
  const participants = champs.map((championId, i) => {
    const participantId = i + 1;
    const teamId = participantId <= 5 ? 100 : 200;
    const mine = participantId === 3;
    const stats = mine
      ? { win, kills: k, deaths: d, assists: a, champLevel: 18, item0: 3031, item1: 3006, item2: 3046, item3: 3072, item4: 3095, item5: 3036, item6: 3340 }
      : { win: teamId === 100 ? win : !win, kills: 3 + (participantId % 6), deaths: 2 + (participantId % 5), assists: 5 + (participantId % 4), champLevel: 17 };
    if (mine) for (let n = 1; n <= 6; n++) stats[`playerAugment${n}`] = augments[n - 1] ?? 0;
    return { participantId, teamId, championId, spell1Id: 4, spell2Id: 32, stats };
  });
  const participantIdentities = champs.map((_, i) => ({
    participantId: i + 1,
    player: i === 2
      ? { puuid: PUUID, gameName: "Me", tagLine: "TR1", summonerName: "Me", profileIcon: 29 }
      : { puuid: `mock-puuid-${i + 1}`, gameName: `Player${i + 1}`, tagLine: "TR1", summonerName: `Player${i + 1}`, profileIcon: 28 },
  }));
  return {
    gameId: id, queueId: queue, gameMode: queue === 2400 ? "KIWI" : "ARAM", gameType: "MATCHED_GAME",
    gameCreation: T0 - ago, gameCreationDate: new Date(T0 - ago).toISOString(), gameDuration: duration,
    gameVersion: "16.21.829.8620", mapId: 12, platformId: "TR1", seasonId: 16, participants, participantIdentities, teams: [],
  };
}
// The match list holds the own participant only.
const listEntry = (g) => ({
  ...g,
  participants: g.participants.filter((p) => p.participantId === 3),
  participantIdentities: g.participantIdentities.filter((x) => x.participantId === 3),
});
const HISTORY = [
  wholeGame({ id: 7000099, ago: 1 * DAY, champ: 157, win: true, k: 12, d: 5, a: 9, augments: [1058, 3078, 1206, 1337] }),
  wholeGame({ id: 7000098, ago: 2 * DAY, champ: 157, win: false, k: 7, d: 11, a: 4, augments: [1058, 3116, 1205, 1337] }),
  wholeGame({ id: 7000097, queue: 450, ago: 2 * DAY + 3_600_000, champ: 103, win: true, k: 9, d: 4, a: 12, augments: [] }),
  wholeGame({ id: 7000096, ago: 3 * DAY, champ: 222, win: true, k: 15, d: 6, a: 8, augments: [3078, 1058, 1080, 1001] }),
];
const RUNNING = wholeGame({ id: RUNNING_GAME, ago: 0, champ: 157, win: true, k: 14, d: 3, a: 10, augments: [1058, 3078, 1206, 1337, 0, 0] });
const GAMES = new Map([...HISTORY, RUNNING].map((g) => [g.gameId, g]));
const log = (...a) => console.log(new Date().toISOString().slice(11, 19), ...a);

function send(res, code, body) {
  res.writeHead(code, { "content-type": "application/json" });
  res.end(body === undefined ? "" : JSON.stringify(body));
}

https.createServer(tls, (req, res) => {
  if (req.headers.authorization !== AUTH) return send(res, 401, { message: "bad auth" });
  const u = req.url;
  log("LCU", req.method, u);
  let m;
  if ((m = u.match(/^\/mock\/phase\/(\w+)/))) { phase = m[1]; return send(res, 200, { phase }); }
  if ((m = u.match(/^\/mock\/me\/(\d+)/))) { state.me = Number(m[1]); return send(res, 200, { me: state.me }); }
  if ((m = u.match(/^\/mock\/subset\/(empty|missing|\d+(?:,\d+)*)$/))) {
    state.subset = m[1] === "missing" ? null : m[1] === "empty" ? [] : m[1].split(",").map(Number);
    return send(res, 200, { subset: state.subset });
  }
  if (u === "/lol-lobby-team-builder/champ-select/v1/subset-champion-list") {
    if (phase !== "ChampSelect" || state.subset === null) return send(res, 404, { errorCode: "RPC_ERROR", httpStatus: 404, message: "Unknown resource" });
    return send(res, 200, state.subset);
  }
  if ((m = u.match(/^\/mock\/bench\/([01])/))) { state.benchOn = m[1] === "1"; return send(res, 200, { bench: state.benchOn }); }
  if ((m = u.match(/^\/mock\/window\/([012])/))) { writeWindowMode(m[1]); return send(res, 200, { windowMode: Number(m[1]) }); }
  if ((m = u.match(/^\/mock\/late\/(\d+)/))) { state.late = Number(m[1]); return send(res, 200, { late: state.late }); }
  if ((m = u.match(/^\/mock\/trade\/(\d)\/([A-Z]+)$/))) {
    if (!(m[1] in TEAM_INDEX) || !STATES.includes(m[2])) return send(res, 400, { message: "cell 0, 1, 3 or 4, and a known state" });
    state.trades[m[1]] = m[2];
    return send(res, 200, { trades: state.trades });
  }
  if ((m = u.match(/^\/mock\/route\/(trades|swaps)$/))) { state.route = m[1]; return send(res, 200, { route: state.route }); }
  if (u === "/mock/trades") return send(res, 200, { me: state.me, team: state.team, bench: state.bench, trades: state.trades, route: state.route });
  if (u === "/lol-game-settings/v1/game-settings" && req.method === "GET") return send(res, 200, gameSettings);
  if (u === "/lol-game-settings/v1/game-settings" && req.method === "PATCH") {
    let body = "";
    req.on("data", (c) => (body += c));
    req.on("end", () => {
      try {
        const patch = JSON.parse(body);
        const mode = patch?.General?.WindowMode;
        if (![0, 1, 2].includes(mode)) return send(res, 400, { message: "General.WindowMode must be 0, 1 or 2" });
        pendingMode = mode;
        send(res, 200, gameSettings);
      } catch {
        send(res, 400, { message: "not JSON" });
      }
    });
    return;
  }
  if (u === "/lol-game-settings/v1/save" && req.method === "POST") {
    if (pendingMode !== null) writeWindowMode(pendingMode);
    pendingMode = null;
    return send(res, 204);
  }
  if (u === "/lol-summoner/v1/current-summoner")
    return send(res, 200, { accountId: 2000001, displayName: "Me", gameName: "Me", tagLine: "TR1", puuid: PUUID, summonerId: 3000001, summonerLevel: 30, profileIconId: 29 });
  if ((m = u.match(/^\/lol-match-history\/v1\/products\/lol\/([^/]+)\/matches/))) {
    if (m[1] !== PUUID && m[1] !== "current-summoner") return send(res, 404, { message: "unknown player" });
    // The real list holds 20 games at most, whatever begIndex and endIndex say.
    const games = HISTORY.map(listEntry);
    return send(res, 200, { accountId: 2000001, platformId: "TR1", games: { gameBeginDate: "2026-10-01", gameCount: games.length, gameIndexBegin: 0, gameIndexEnd: games.length, games } });
  }
  if ((m = u.match(/^\/lol-match-history\/v1\/games\/(\d+)$/))) {
    const id = Number(m[1]);
    if (id === RUNNING_GAME && state.late > 0) { state.late -= 1; return send(res, 404, { message: "game not found yet" }); }
    return GAMES.has(id) ? send(res, 200, GAMES.get(id)) : send(res, 404, { message: "no such game" });
  }
  if (u === "/lol-gameflow/v1/gameflow-phase") return send(res, 200, phase);
  if (u === "/lol-gameflow/v1/session") return send(res, 200, { phase, gameData: { gameId: RUNNING_GAME, queue: { id: 2400, gameMode: "KIWI" } } });
  // MOCK_LOCALE=en_US renders the app in English (for the README shots).
  if (u === "/riotclient/region-locale") return send(res, 200, { locale: process.env.MOCK_LOCALE || "tr_TR", region: "TR" });
  if (u === "/lol-champ-select/v1/session") {
    if (phase !== "ChampSelect") return send(res, 404, { message: "no session" });
    return send(res, 200, {
      localPlayerCellId: 2,
      benchEnabled: state.benchOn,
      benchChampions: state.benchOn ? state.bench.map((championId) => ({ championId, isPriority: false })) : [],
      myTeam: [
        { cellId: 0, championId: state.team[0] },
        { cellId: 1, championId: state.team[1] },
        { cellId: 2, championId: state.me },
        { cellId: 3, championId: state.team[2] },
        { cellId: 4, championId: state.team[3] },
      ],
      trades: Object.entries(state.trades).map(([cell, st]) => ({ id: 100 + Number(cell), cellId: Number(cell), state: st })),
    });
  }
  // A call on a trade. Only the route of state.route exists: the other one is unknown, as in an older client.
  if ((m = u.match(/^\/lol-champ-select\/v1\/session\/(trades|champion-swaps)\/(\d+)\/(request|accept|decline|cancel)$/)) && req.method === "POST") {
    if ((m[1] === "trades") !== (state.route === "trades")) return send(res, 404, { errorCode: "RPC_ERROR", httpStatus: 404, message: "Unknown resource" });
    const cell = Number(m[2]) - 100;
    const call = m[3];
    const need = { request: "AVAILABLE", accept: "RECEIVED", decline: "RECEIVED", cancel: "SENT" }[call];
    if (!(cell in TEAM_INDEX) || state.trades[cell] !== need) {
      return send(res, 400, { errorCode: "RPC_ERROR", httpStatus: 400, message: `Trade ${m[2]} is not ${need}` });
    }
    if (call === "request") state.trades[cell] = "SENT";
    else {
      if (call === "accept") [state.me, state.team[TEAM_INDEX[cell]]] = [state.team[TEAM_INDEX[cell]], state.me];
      state.trades[cell] = "AVAILABLE";
    }
    return send(res, 204);
  }
  if ((m = u.match(/^\/lol-champ-select\/v1\/session\/bench\/swap\/(\d+)$/)) && req.method === "POST") {
    const id = Number(m[1]);
    const i = state.bench.indexOf(id);
    if (i < 0) return send(res, 400, { message: "not on bench" });
    state.bench[i] = state.me;
    state.me = id;
    return send(res, 204);
  }
  send(res, 404, { message: "not mocked" });
}).listen(PORT, "127.0.0.1", () => log("LCU mock on", PORT, "phase", phase));

https.createServer(tls, (req, res) => {
  log("LIVE", req.method, req.url);
  if (phase !== "InProgress") { res.destroy(); return; }
  if (req.url === "/liveclientdata/activeplayer") return send(res, 200, { riotId: "Me#TR1", level: 7, currentGold: 1234.5 });
  // Ten players in the shape of the Live Client: side, items (slot 6 is the trinket, a potion is a consumable),
  // and the score line. The names are made up. Soraka is a healer by name, and Zed is fed (9/2/4).
  if (req.url === "/liveclientdata/playerlist") {
    const item = (itemID, slot, consumable = false) => ({ itemID, slot, consumable, count: 1, canUse: false, price: 0 });
    const player = (riotId, champ, team, items, k = 0, d = 0, a = 0) => ({
      riotId, summonerName: riotId, championName: champ, rawChampionName: "game_character_displayname_" + champ, team, level: 7,
      isDead: false, items: [...items, item(3340, 6)], scores: { kills: k, deaths: d, assists: a, creepScore: 10, wardScore: 0 },
    });
    return send(res, 200, [
      player("Other#TR1", "Annie", "ORDER", [item(1056, 0), item(2003, 1, true)], 1, 1, 2),
      player("Me#TR1", "Yasuo", "ORDER", [item(1038, 0), item(1037, 1)], 2, 1, 3),
      player("Mate3#TR1", "Garen", "ORDER", [item(3075, 0)], 1, 2, 1),
      player("Mate4#TR1", "Lux", "ORDER", [item(1026, 0)], 0, 1, 4),
      player("Mate5#TR1", "Jinx", "ORDER", [item(1055, 0)], 3, 2, 1),
      player("Foe1#TR1", "Zed", "CHAOS", [item(3071, 0), item(3142, 1)], 9, 2, 4),
      player("Foe2#TR1", "Soraka", "CHAOS", [item(2065, 0)], 0, 3, 8),
      player("Foe3#TR1", "Malphite", "CHAOS", [item(3076, 0), item(2504, 1)], 2, 3, 3),
      player("Foe4#TR1", "Ahri", "CHAOS", [item(6655, 0)], 4, 2, 2),
      player("Foe5#TR1", "Ashe", "CHAOS", [item(3031, 0)], 3, 4, 1),
    ]);
  }
  send(res, 404, {});
}).listen(2999, "127.0.0.1", () => log("Live mock on 2999"));
