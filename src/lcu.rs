//! League Client (LCU) and Live Client Data API access.
//!
//! The LCU listens on 127.0.0.1 with a self-signed certificate. The port and
//! password come from the `lockfile` in the League install folder.

use base64::Engine;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::time::Duration;

#[derive(Clone, Debug)]
pub struct Lockfile {
    pub port: u16,
    pub password: String,
}

pub struct Lcu {
    agent: ureq::Agent,
    install_dir: Option<PathBuf>,
    lock: Option<Lockfile>,
}

/// Agent for 127.0.0.1 only. Riot uses a self-signed cert on both local APIs. It starts no
/// thread per request (see `net`): these requests run about three times a second.
fn local_agent() -> ureq::Agent {
    let tls = ureq::tls::TlsConfig::builder()
        .disable_verification(true)
        .build();
    crate::net::agent(
        ureq::Agent::config_builder()
            .tls_config(tls)
            .timeout_global(Some(Duration::from_millis(1500)))
            .build(),
    )
}

impl Lcu {
    pub fn new() -> Self {
        Self {
            agent: local_agent(),
            install_dir: None,
            lock: None,
        }
    }

    pub fn install_dir(&self) -> Option<&Path> {
        self.install_dir.as_deref()
    }

    pub fn connected(&self) -> bool {
        self.lock.is_some()
    }

    /// Find the lockfile. Returns false when the client is not running.
    pub fn refresh_lock(&mut self) -> bool {
        if self.install_dir.is_none() {
            self.install_dir = find_install_dir();
        }
        let Some(dir) = &self.install_dir else {
            self.lock = None;
            return false;
        };
        self.lock = read_lockfile(&dir.join("lockfile"));
        if self.lock.is_none() {
            // The client can move. Search again next time.
            self.install_dir = find_install_dir();
            if let Some(dir) = &self.install_dir {
                self.lock = read_lockfile(&dir.join("lockfile"));
            }
        }
        self.lock.is_some()
    }

    fn auth(lock: &Lockfile) -> String {
        let raw = format!("riot:{}", lock.password);
        format!(
            "Basic {}",
            base64::engine::general_purpose::STANDARD.encode(raw)
        )
    }

    pub fn get(&mut self, path: &str) -> Option<Value> {
        let lock = self.lock.clone()?;
        let url = format!("https://127.0.0.1:{}{}", lock.port, path);
        match self
            .agent
            .get(&url)
            .header("Authorization", Self::auth(&lock))
            .call()
        {
            Ok(mut r) => r.body_mut().read_json::<Value>().ok(),
            // Refused connection: the client closed or restarted. Find it again.
            Err(ureq::Error::ConnectionFailed) => {
                self.lock = None;
                None
            }
            Err(ureq::Error::Io(e)) if e.kind() == std::io::ErrorKind::ConnectionRefused => {
                self.lock = None;
                None
            }
            // A timeout or an HTTP error keeps the connection.
            Err(_) => None,
        }
    }

    pub fn send(&mut self, method: &str, path: &str, body: Option<&Value>) -> Result<(), String> {
        let lock = self.lock.clone().ok_or(NOT_CONNECTED)?;
        let url = format!("https://127.0.0.1:{}{}", lock.port, path);
        let auth = Self::auth(&lock);
        let res = match (method, body) {
            ("POST", Some(b)) => self
                .agent
                .post(&url)
                .header("Authorization", &auth)
                .send_json(b),
            ("POST", None) => self
                .agent
                .post(&url)
                .header("Authorization", &auth)
                .send_empty(),
            ("PUT", Some(b)) => self
                .agent
                .put(&url)
                .header("Authorization", &auth)
                .send_json(b),
            ("PATCH", Some(b)) => self
                .agent
                .patch(&url)
                .header("Authorization", &auth)
                .send_json(b),
            ("DELETE", _) => self
                .agent
                .delete(&url)
                .header("Authorization", &auth)
                .call(),
            _ => return Err(format!("unsupported {method}")),
        };
        res.map(|_| ()).map_err(|e| match e {
            // The text that `failure_of` reads. Not the text of ureq, which can change.
            ureq::Error::StatusCode(code) => format!("{STATUS_PREFIX}{code}"),
            e => e.to_string(),
        })
    }
}

/// The raw error of `Lcu::send` when no client runs.
pub const NOT_CONNECTED: &str = "client not connected";

/// The start of the raw error of `Lcu::send` when the client answers with an
/// HTTP error, for example "http status: 404".
const STATUS_PREFIX: &str = "http status: ";

/// Why a call to the client failed, in the terms of a player. The raw error is
/// English and technical, so a message names only this.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CallFailure {
    /// No client runs.
    NotConnected,
    /// The client answered with an error: the swap or the trade is not possible now.
    Refused,
    /// No answer: a timeout, or the connection broke.
    NoAnswer,
}

/// The failure of a raw error from `Lcu::send`.
pub fn failure_of(raw: &str) -> CallFailure {
    if raw == NOT_CONNECTED {
        CallFailure::NotConnected
    } else if raw.starts_with(STATUS_PREFIX) {
        CallFailure::Refused
    } else {
        CallFailure::NoAnswer
    }
}

fn read_lockfile(path: &Path) -> Option<Lockfile> {
    // The client keeps the lockfile open. Read it with shared access.
    let text = std::fs::read_to_string(path).ok()?;
    // Format: LeagueClient:PID:PORT:PASSWORD:https
    let parts: Vec<&str> = text.trim().split(':').collect();
    if parts.len() < 5 {
        return None;
    }
    Some(Lockfile {
        port: parts[2].parse().ok()?,
        password: parts[3].to_string(),
    })
}

/// Look for the League install folder: running process first, then known places.
fn find_install_dir() -> Option<PathBuf> {
    // Test hook: point the app at a mock client folder.
    if let Some(p) = std::env::var_os("HEXSIGHT_LEAGUE_DIR") {
        return Some(PathBuf::from(p));
    }
    if let Some(p) = crate::win::process_dir("LeagueClientUx.exe") {
        return Some(p);
    }
    let settings = r"C:\ProgramData\Riot Games\Metadata\league_of_legends.live\league_of_legends.live.product_settings.yaml";
    if let Ok(text) = std::fs::read_to_string(settings) {
        for line in text.lines() {
            let line = line.trim();
            if let Some(rest) = line.strip_prefix("product_install_full_path:") {
                let p = PathBuf::from(rest.trim().trim_matches('"'));
                if p.exists() {
                    return Some(p);
                }
            }
        }
    }
    let default = PathBuf::from(r"C:\Riot Games\League of Legends");
    default.exists().then_some(default)
}

// ---------------------------------------------------------------------------
// Typed views of the LCU state that the app uses.

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub enum Phase {
    #[default]
    Offline,
    Idle,
    Lobby,
    ChampSelect,
    InGame,
    PostGame,
}

/// A teammate in champ select who has a champion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Mate {
    pub cell: i64,
    pub champion: i64,
}

/// State of a champion trade with a teammate (`trades[].state` in the session).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeState {
    /// You can ask this teammate.
    Available,
    /// The teammate is in another trade.
    Busy,
    /// No trade is possible now.
    Invalid,
    /// The teammate asked you.
    Received,
    /// You asked the teammate.
    Sent,
}

impl TradeState {
    fn parse(s: &str) -> Option<Self> {
        Some(match s {
            "AVAILABLE" => TradeState::Available,
            "BUSY" => TradeState::Busy,
            "INVALID" => TradeState::Invalid,
            "RECEIVED" => TradeState::Received,
            "SENT" => TradeState::Sent,
            _ => return None,
        })
    }
}

/// One entry of `trades` in the champ select session.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Trade {
    pub id: i64,
    /// The cell of the teammate that the trade is with.
    pub cell: i64,
    pub state: TradeState,
}

#[derive(Clone, Debug, Default)]
pub struct ChampSelect {
    pub my_champion: i64,
    pub team: Vec<Mate>,
    pub bench: Vec<i64>,
    pub bench_enabled: bool,
    pub trades: Vec<Trade>,
    /// The champions that the client offers for the first step, while the player has
    /// none yet (`my_champion` is 0). Empty when the client offers none, and once the
    /// player has a champion. Hexsight picks none of them.
    pub choices: Vec<i64>,
}

/// Client phase. None when this one request failed (the caller keeps the
/// last phase for a few polls, so one slow reply does not end a game).
pub fn phase(lcu: &mut Lcu) -> Option<Phase> {
    let v = lcu.get("/lol-gameflow/v1/gameflow-phase")?;
    Some(match v.as_str()? {
        "Lobby" | "Matchmaking" | "ReadyCheck" => Phase::Lobby,
        "ChampSelect" => Phase::ChampSelect,
        "GameStart" | "InProgress" | "Reconnect" => Phase::InGame,
        "WaitingForStats" | "PreEndOfGame" | "EndOfGame" => Phase::PostGame,
        _ => Phase::Idle,
    })
}

/// What the gameflow session says about the game.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct GameSession {
    pub queue: Option<i64>,
    /// The id of the game. The session has 0 until the game exists.
    pub game_id: Option<i64>,
}

/// Queue and game id of the gameflow session. `None` when the request failed.
pub fn game_session(lcu: &mut Lcu) -> Option<GameSession> {
    lcu.get("/lol-gameflow/v1/session")
        .map(|s| parse_session(&s))
}

fn parse_session(s: &Value) -> GameSession {
    GameSession {
        queue: s.pointer("/gameData/queue/id").and_then(|v| v.as_i64()),
        game_id: s
            .pointer("/gameData/gameId")
            .and_then(|v| v.as_i64())
            .filter(|&id| id > 0),
    }
}

/// The id (`puuid`) of the signed-in player. `None` until the client has signed in.
pub fn puuid(lcu: &mut Lcu) -> Option<String> {
    let v = lcu.get("/lol-summoner/v1/current-summoner")?;
    v.get("puuid")?
        .as_str()
        .filter(|p| !p.is_empty())
        .map(str::to_string)
}

/// The newest 20 games of the player. The client can keep an old list for a long time.
pub fn match_list(lcu: &mut Lcu, puuid: &str) -> Option<Value> {
    lcu.get(&format!(
        "/lol-match-history/v1/products/lol/{puuid}/matches?begIndex=0&endIndex=19"
    ))
}

/// One whole game. `None` while the game is not in the match history yet.
pub fn match_game(lcu: &mut Lcu, game_id: i64) -> Option<Value> {
    lcu.get(&format!("/lol-match-history/v1/games/{game_id}"))
}

pub fn champ_select(lcu: &mut Lcu) -> Option<ChampSelect> {
    parse_champ_select(&lcu.get("/lol-champ-select/v1/session")?)
}

/// The route of the champions that the first step of a Mayhem champ select offers.
const SUBSET_LIST: &str = "/lol-lobby-team-builder/champ-select/v1/subset-champion-list";

/// The most champions that count as an offer. A longer list is a pool, not a pick of three.
const MAX_CHOICES: usize = 6;

/// The champions that the client offers for the first step. Empty when the route is
/// missing (an older client, another queue), when it answers with an error, or when
/// the list is empty.
pub fn subset_champions(lcu: &mut Lcu) -> Vec<i64> {
    lcu.get(SUBSET_LIST)
        .map(|v| parse_subset(&v))
        .unwrap_or_default()
}

/// The champion ids of the answer of `SUBSET_LIST`. The route gives a JSON array of
/// champion ids (`[51, 103, 157]`); the field `subsetChampionIds` of the inventory draft
/// has the same shape. Hexsight also takes a list of objects with `championId` (or `id`),
/// and an object that holds such a list in `subsetChampionIds` or `championIds`, because
/// no one has published the answer of a live client. An id that is not a positive number
/// and a repeated id are dropped. A list of more than `MAX_CHOICES` is no offer.
fn parse_subset(v: &Value) -> Vec<i64> {
    let list = match v {
        Value::Array(a) => a.as_slice(),
        Value::Object(o) => match o.get("subsetChampionIds").or_else(|| o.get("championIds")) {
            Some(Value::Array(a)) => a.as_slice(),
            _ => return Vec::new(),
        },
        _ => return Vec::new(),
    };
    let mut out: Vec<i64> = Vec::new();
    for item in list {
        let id = item
            .as_i64()
            .or_else(|| item.get("championId").or_else(|| item.get("id"))?.as_i64());
        if let Some(id) = id.filter(|&id| id > 0 && !out.contains(&id)) {
            out.push(id);
        }
    }
    if out.len() > MAX_CHOICES {
        Vec::new()
    } else {
        out
    }
}

/// The parts of the champ select session that the app uses. `trades` is a list
/// of `{"id": 1, "cellId": 3, "state": "AVAILABLE"}`. An entry with a state that
/// Hexsight does not know is dropped, and a session without `trades` has none.
fn parse_champ_select(s: &Value) -> Option<ChampSelect> {
    let me = s.get("localPlayerCellId")?.as_i64()?;
    let mut out = ChampSelect::default();
    for p in s.get("myTeam")?.as_array()? {
        let cid = p.get("championId").and_then(|v| v.as_i64()).unwrap_or(0);
        let cell = p.get("cellId").and_then(|v| v.as_i64());
        if cell == Some(me) {
            out.my_champion = cid;
        } else if let (Some(cell), true) = (cell, cid > 0) {
            out.team.push(Mate {
                cell,
                champion: cid,
            });
        }
    }
    out.bench_enabled = s
        .get("benchEnabled")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    if let Some(b) = s.get("benchChampions").and_then(|v| v.as_array()) {
        out.bench = b
            .iter()
            .filter_map(|c| c.get("championId")?.as_i64())
            .collect();
    }
    if let Some(t) = s.get("trades").and_then(|v| v.as_array()) {
        out.trades = t
            .iter()
            .filter_map(|t| {
                Some(Trade {
                    id: t.get("id")?.as_i64()?,
                    cell: t.get("cellId")?.as_i64()?,
                    state: TradeState::parse(t.get("state")?.as_str()?)?,
                })
            })
            .collect();
    }
    Some(out)
}

pub fn locale(lcu: &mut Lcu) -> Option<String> {
    let v = lcu.get("/riotclient/region-locale")?;
    v.get("locale")
        .and_then(|l| l.as_str())
        .map(|s| s.to_string())
}

/// Swap my champion with a bench champion (ARAM bench).
pub fn bench_swap(lcu: &mut Lcu, champion_id: i64) -> Result<(), String> {
    lcu.send(
        "POST",
        &format!("/lol-champ-select/v1/session/bench/swap/{champion_id}"),
        None,
    )
}

/// The four calls on a trade.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TradeCall {
    /// Ask a teammate (state Available).
    Request,
    /// Take the request of a teammate (state Received).
    Accept,
    /// Refuse the request of a teammate (state Received).
    Decline,
    /// Take back your own request (state Sent).
    Cancel,
}

impl TradeCall {
    /// The last part of the route.
    fn word(self) -> &'static str {
        match self {
            TradeCall::Request => "request",
            TradeCall::Accept => "accept",
            TradeCall::Decline => "decline",
            TradeCall::Cancel => "cancel",
        }
    }

    /// The call for its route word. For the test hook `HEXSIGHT_TRADE`.
    pub fn parse(word: &str) -> Option<Self> {
        [
            TradeCall::Request,
            TradeCall::Accept,
            TradeCall::Decline,
            TradeCall::Cancel,
        ]
        .into_iter()
        .find(|c| c.word() == word)
    }
}

/// The two routes of one trade call, in the order to try them. The `trades`
/// route is the current one. Some clients call `champion-swaps`, the name in
/// some client versions.
fn trade_routes(id: i64, call: TradeCall) -> [String; 2] {
    let word = call.word();
    [
        format!("/lol-champ-select/v1/session/trades/{id}/{word}"),
        format!("/lol-champ-select/v1/session/champion-swaps/{id}/{word}"),
    ]
}

/// Send to the first route that works. When all fail, the error of the first
/// route counts, because it is the current route.
fn first_that_works(
    routes: &[String],
    mut send: impl FnMut(&str) -> Result<(), String>,
) -> Result<(), String> {
    let mut first_error = None;
    for route in routes {
        match send(route) {
            Ok(()) => return Ok(()),
            Err(e) => {
                first_error.get_or_insert(e);
            }
        }
    }
    Err(first_error.unwrap_or_else(|| "no route".into()))
}

/// Ask for, accept, decline, or cancel the trade with this id (`trades[].id`).
pub fn trade(lcu: &mut Lcu, id: i64, call: TradeCall) -> Result<(), String> {
    first_that_works(&trade_routes(id, call), |route| {
        lcu.send("POST", route, None)
    })
}

/// The route of the game settings of the client.
const GAME_SETTINGS: &str = "/lol-game-settings/v1/game-settings";

/// The route that makes the client write its game settings to `game.cfg`.
const GAME_SETTINGS_SAVE: &str = "/lol-game-settings/v1/save";

/// `General.WindowMode` of the client for Borderless (`game.cfg` has the same value).
const BORDERLESS: i64 = 2;

/// The body for the PATCH that sets Borderless: the settings of the client as they
/// are, with `General.WindowMode` set to 2. Nothing else changes. `None` when the
/// answer is not an object.
pub fn borderless_patch(settings: &Value) -> Option<Value> {
    let mut body = settings.as_object()?.clone();
    let general = body
        .entry("General")
        .or_insert_with(|| Value::Object(Default::default()));
    // A `General` that is no object (a strange answer) is replaced: the key needs it.
    if !general.is_object() {
        *general = Value::Object(Default::default());
    }
    general["WindowMode"] = Value::from(BORDERLESS);
    Some(Value::Object(body))
}

/// Set League to Borderless through the client: read the game settings, set
/// `General.WindowMode` to 2, send them back, and ask the client to save. Only a
/// click of the player starts it. The error is a raw error for `failure_of`.
pub fn set_borderless(lcu: &mut Lcu) -> Result<(), String> {
    if !lcu.connected() {
        return Err(NOT_CONNECTED.into());
    }
    // No answer to the read: the client is busy or went away. `get` is silent about why.
    let settings = lcu.get(GAME_SETTINGS).ok_or("no answer")?;
    let body = borderless_patch(&settings).ok_or("game settings in an unknown shape")?;
    lcu.send("PATCH", GAME_SETTINGS, Some(&body))?;
    lcu.send("POST", GAME_SETTINGS_SAVE, None)
}

// ---------------------------------------------------------------------------
// Live Client Data API (in game, port 2999, no auth).

pub struct LiveClient {
    agent: ureq::Agent,
}

impl LiveClient {
    pub fn new() -> Self {
        Self {
            agent: local_agent(),
        }
    }

    pub fn get(&self, path: &str) -> Option<Value> {
        let url = format!("https://127.0.0.1:2999/liveclientdata/{path}");
        let mut r = self.agent.get(&url).call().ok()?;
        r.body_mut().read_json::<Value>().ok()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_session_has_the_queue_and_the_game_id() {
        let s: Value = serde_json::from_str(
            r#"{"phase":"InProgress","gameData":{"gameId":7000100,"queue":{"id":2400,"gameMode":"KIWI"},"teamOne":[]}}"#,
        )
        .unwrap();
        assert_eq!(
            parse_session(&s),
            GameSession {
                queue: Some(2400),
                game_id: Some(7000100)
            }
        );
        // Before the game exists, the id is 0.
        let early: Value =
            serde_json::from_str(r#"{"gameData":{"gameId":0,"queue":{"id":2400}}}"#).unwrap();
        assert_eq!(
            parse_session(&early),
            GameSession {
                queue: Some(2400),
                game_id: None
            }
        );
        // A lobby without game data.
        assert_eq!(parse_session(&Value::Null), GameSession::default());
    }

    /// A champ select session in the shape of the client: five cells, the bench, and `trades`.
    const SESSION: &str = r#"{"localPlayerCellId":2,"benchEnabled":true,
        "benchChampions":[{"championId":103,"isPriority":false},{"championId":157}],
        "myTeam":[{"cellId":0,"championId":1},{"cellId":1,"championId":89},{"cellId":2,"championId":51},
                  {"cellId":3,"championId":0},{"cellId":4,"championId":99}],
        "trades":[{"id":100,"cellId":0,"state":"AVAILABLE"},{"id":101,"cellId":1,"state":"RECEIVED"},
                  {"id":103,"cellId":3,"state":"BUSY"},{"id":104,"cellId":4,"state":"SENT"}]}"#;

    #[test]
    fn the_session_has_the_team_the_bench_and_the_trades() {
        let cs = parse_champ_select(&serde_json::from_str(SESSION).unwrap()).unwrap();
        assert_eq!(cs.my_champion, 51);
        assert!(cs.bench_enabled);
        assert_eq!(cs.bench, vec![103, 157]);
        // A teammate without a champion (cell 3) is not listed. The cells stay with the champions.
        assert_eq!(
            cs.team,
            vec![
                Mate {
                    cell: 0,
                    champion: 1
                },
                Mate {
                    cell: 1,
                    champion: 89
                },
                Mate {
                    cell: 4,
                    champion: 99
                }
            ]
        );
        assert_eq!(
            cs.trades,
            vec![
                Trade {
                    id: 100,
                    cell: 0,
                    state: TradeState::Available
                },
                Trade {
                    id: 101,
                    cell: 1,
                    state: TradeState::Received
                },
                Trade {
                    id: 103,
                    cell: 3,
                    state: TradeState::Busy
                },
                Trade {
                    id: 104,
                    cell: 4,
                    state: TradeState::Sent
                },
            ]
        );
    }

    #[test]
    fn the_subset_list_is_an_array_of_champion_ids() {
        assert_eq!(
            parse_subset(&serde_json::json!([51, 103, 157])),
            vec![51, 103, 157]
        );
        // Other shapes of the same list.
        assert_eq!(
            parse_subset(&serde_json::json!([{"championId": 51}, {"id": 103}, 157])),
            vec![51, 103, 157]
        );
        assert_eq!(
            parse_subset(&serde_json::json!({"subsetChampionIds": [51, 103]})),
            vec![51, 103]
        );
        assert_eq!(
            parse_subset(&serde_json::json!({"championIds": [51]})),
            vec![51]
        );
        // Zero, negative, text, and repeated ids are dropped. The order stays.
        assert_eq!(
            parse_subset(&serde_json::json!([157, 0, -1, "x", null, 51, 157, 1.5])),
            vec![157, 51]
        );
    }

    #[test]
    fn an_empty_or_strange_subset_answer_gives_no_choices() {
        for none in [
            serde_json::json!([]),
            Value::Null,
            serde_json::json!({}),
            serde_json::json!("51"),
            serde_json::json!(51),
            serde_json::json!({"subsetChampionIds": 5}),
        ] {
            assert!(parse_subset(&none).is_empty(), "{none}");
        }
        // A pool of dozens is no offer of three.
        let pool: Vec<i64> = (1..=40).collect();
        assert!(parse_subset(&serde_json::json!(pool)).is_empty());
        assert_eq!(
            parse_subset(&serde_json::json!(
                (1..=MAX_CHOICES as i64).collect::<Vec<_>>()
            ))
            .len(),
            MAX_CHOICES
        );
        // The route is missing when the client is closed: no request, no choices.
        assert!(subset_champions(&mut Lcu::new()).is_empty());
    }

    #[test]
    fn the_session_of_a_player_without_a_champion_has_champion_zero_and_no_choices() {
        let s: Value = serde_json::from_str(
            r#"{"localPlayerCellId":2,"benchEnabled":false,
                "myTeam":[{"cellId":0,"championId":1},{"cellId":2,"championId":0},{"cellId":3,"championId":0}]}"#,
        )
        .unwrap();
        let cs = parse_champ_select(&s).unwrap();
        assert_eq!(cs.my_champion, 0);
        assert!(cs.choices.is_empty());
    }

    #[test]
    fn all_five_trade_states_are_known() {
        for (word, state) in [
            ("AVAILABLE", TradeState::Available),
            ("BUSY", TradeState::Busy),
            ("INVALID", TradeState::Invalid),
            ("RECEIVED", TradeState::Received),
            ("SENT", TradeState::Sent),
        ] {
            assert_eq!(TradeState::parse(word), Some(state));
        }
        // The client writes capitals. Another word is no state.
        assert_eq!(TradeState::parse("available"), None);
        assert_eq!(TradeState::parse("PENDING"), None);
        assert_eq!(TradeState::parse(""), None);
    }

    #[test]
    fn a_bad_trade_entry_is_dropped_and_the_rest_stays() {
        let s: Value = serde_json::from_str(
            r#"{"localPlayerCellId":0,"myTeam":[{"cellId":0,"championId":5},{"cellId":1,"championId":6}],
                "trades":[{"id":1,"cellId":1,"state":"PENDING"},{"cellId":1,"state":"SENT"},
                          {"id":3,"state":"SENT"},{"id":4,"cellId":1},{"id":5,"cellId":1,"state":"INVALID"},"x"]}"#,
        )
        .unwrap();
        let cs = parse_champ_select(&s).unwrap();
        assert_eq!(
            cs.trades,
            vec![Trade {
                id: 5,
                cell: 1,
                state: TradeState::Invalid
            }]
        );
        // No `trades` (a draft, or an old client) gives an empty list. The rest still reads.
        let s: Value = serde_json::from_str(
            r#"{"localPlayerCellId":0,"myTeam":[{"cellId":0,"championId":5}]}"#,
        )
        .unwrap();
        let cs = parse_champ_select(&s).unwrap();
        assert!(cs.trades.is_empty() && cs.bench.is_empty() && !cs.bench_enabled);
        assert_eq!(cs.my_champion, 5);
        // Without the own cell, or without the team, there is no champ select to show.
        assert!(parse_champ_select(&Value::Null).is_none());
        assert!(parse_champ_select(&serde_json::from_str(r#"{"myTeam":[]}"#).unwrap()).is_none());
        assert!(
            parse_champ_select(&serde_json::from_str(r#"{"localPlayerCellId":0}"#).unwrap())
                .is_none()
        );
    }

    #[test]
    fn a_trade_call_tries_trades_first_and_champion_swaps_after() {
        let [first, second] = trade_routes(101, TradeCall::Accept);
        assert_eq!(first, "/lol-champ-select/v1/session/trades/101/accept");
        assert_eq!(
            second,
            "/lol-champ-select/v1/session/champion-swaps/101/accept"
        );
        for (call, word) in [
            (TradeCall::Request, "request"),
            (TradeCall::Accept, "accept"),
            (TradeCall::Decline, "decline"),
            (TradeCall::Cancel, "cancel"),
        ] {
            assert!(trade_routes(7, call)[0].ends_with(&format!("/trades/7/{word}")));
            assert!(trade_routes(7, call)[1].ends_with(&format!("/champion-swaps/7/{word}")));
            assert_eq!(TradeCall::parse(word), Some(call));
        }
        assert_eq!(TradeCall::parse("swap"), None);
    }

    #[test]
    fn the_second_route_runs_only_when_the_first_fails() {
        let routes = trade_routes(1, TradeCall::Request);
        let mut seen = Vec::new();
        // The first route works: one call.
        assert_eq!(
            first_that_works(&routes, |r| {
                seen.push(r.to_string());
                Ok(())
            }),
            Ok(())
        );
        assert_eq!(seen, vec![routes[0].clone()]);
        // The first route fails (an older client): the second one decides.
        seen.clear();
        let r = first_that_works(&routes, |r| {
            seen.push(r.to_string());
            if r.contains("/trades/") {
                Err("http status: 404".into())
            } else {
                Ok(())
            }
        });
        assert_eq!(r, Ok(()));
        assert_eq!(seen, routes.to_vec());
        // Both fail: the error of the first route counts.
        let r = first_that_works(&routes, |r| {
            Err(if r.contains("/trades/") {
                "first".into()
            } else {
                "second".to_string()
            })
        });
        assert_eq!(r, Err("first".to_string()));
        // No route at all is an error too.
        assert!(first_that_works(&[], |_| Ok(())).is_err());
    }

    #[test]
    fn the_borderless_patch_sets_the_window_mode_and_keeps_the_rest() {
        let settings: Value = serde_json::from_str(
            r#"{"General":{"WindowMode":0,"Height":1080,"Width":1920,"CfgVersion":"16.21"},"Audio":{"MasterVolume":0.5}}"#,
        )
        .unwrap();
        let body = borderless_patch(&settings).unwrap();
        assert_eq!(body["General"]["WindowMode"], 2);
        assert_eq!(body["General"]["Height"], 1080);
        assert_eq!(body["General"]["CfgVersion"], "16.21");
        assert_eq!(body["Audio"], settings["Audio"]);
        // The answer of the client is not changed in place.
        assert_eq!(settings["General"]["WindowMode"], 0);
        // Only the window mode differs.
        let mut same = body.clone();
        same["General"]["WindowMode"] = Value::from(0);
        assert_eq!(same, settings);
    }

    #[test]
    fn the_borderless_patch_makes_the_general_section_when_it_is_missing() {
        let body = borderless_patch(&serde_json::json!({ "Audio": {} })).unwrap();
        assert_eq!(
            body,
            serde_json::json!({ "Audio": {}, "General": { "WindowMode": 2 } })
        );
        assert_eq!(
            borderless_patch(&serde_json::json!({})).unwrap(),
            serde_json::json!({ "General": { "WindowMode": 2 } })
        );
        // A General that is no object is replaced.
        assert_eq!(
            borderless_patch(&serde_json::json!({ "General": 5 })).unwrap()["General"]["WindowMode"],
            2
        );
        // An answer that is no object gives nothing to send.
        for bad in [Value::Null, serde_json::json!([]), serde_json::json!("x")] {
            assert!(borderless_patch(&bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn the_fix_does_nothing_when_the_client_is_closed() {
        // No lockfile: the call fails before any request, and the raw error says so.
        let mut lcu = Lcu::new();
        assert_eq!(set_borderless(&mut lcu), Err(NOT_CONNECTED.to_string()));
        assert_eq!(failure_of(NOT_CONNECTED), CallFailure::NotConnected);
    }

    #[test]
    fn a_raw_client_error_has_a_failure_for_the_player() {
        assert_eq!(failure_of(NOT_CONNECTED), CallFailure::NotConnected);
        for code in [400, 403, 404, 409, 500] {
            assert_eq!(
                failure_of(&format!("{STATUS_PREFIX}{code}")),
                CallFailure::Refused,
                "{code}"
            );
        }
        // A timeout, a broken connection, or any other text: the client did not answer.
        for raw in [
            "timeout: global",
            "connection failed",
            "io: An existing connection was forcibly closed",
            "",
        ] {
            assert_eq!(failure_of(raw), CallFailure::NoAnswer, "{raw}");
        }
        // The status text of ureq is the same as the one that `Lcu::send` writes.
        assert_eq!(
            ureq::Error::StatusCode(404).to_string(),
            format!("{STATUS_PREFIX}404")
        );
    }
}
