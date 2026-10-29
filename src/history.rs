//! "My games": the Mayhem games of the local player.
//!
//! After a game, Hexsight reads the game from the League Client match history (a
//! local call on 127.0.0.1) and adds one short record to `history.json` in the
//! data folder. The file stays on this PC. Hexsight never sends it anywhere.
//!
//! The field names come from the match history that the MIT project
//! Yhprum/mayhem-tracker reads (`src/main/lcu.ts` and `src/main/db/payloads.ts`):
//! - `/lol-summoner/v1/current-summoner` has `puuid`, the id of the player.
//! - `/lol-match-history/v1/products/lol/<puuid>/matches?begIndex=0&endIndex=19`
//!   has `games.games[]` with `gameId`, `queueId` and `gameCreation`. It lists
//!   20 games at most, and the client can keep an old list for a long time.
//! - `/lol-match-history/v1/games/<gameId>` is one whole game: `queueId`,
//!   `gameCreation` (milliseconds), `gameDuration` (seconds), `participants[]`
//!   (`championId` and `stats`) and `participantIdentities[].player.puuid`.
//!   The augments are `stats.playerAugment1` to `stats.playerAugment6`, in pick
//!   order (four picks, and up to two bonus slots). `stats` also has `win`,
//!   `kills`, `deaths` and `assists`.

use crate::model::is_mayhem;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// The file keeps this many games. The oldest games go first.
pub const MAX_GAMES: usize = 500;

/// Augment slots of one game: four picks and up to two bonus slots.
const AUGMENT_SLOTS: usize = 6;

/// A game shorter than this (in seconds) is a remake. It counts as no game.
const REMAKE_SECS: i64 = 300;

/// One game of the local player, as the file keeps it.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Game {
    pub id: i64,
    /// Start of the game, in UTC: "2026-10-23T09:20:31Z". Empty when unknown.
    pub date: String,
    pub queue: i64,
    pub champion: i64,
    /// Augment ids in pick order.
    pub augments: Vec<i64>,
    pub win: bool,
    pub kills: u32,
    pub deaths: u32,
    pub assists: u32,
    /// Length of the game in seconds. 0 for a game that an older version saved.
    #[serde(default)]
    pub secs: u32,
}

// ---------------------------------------------------------------------------
// Read the match history. Pure functions, unit tested below.

fn truthy(v: &Value) -> bool {
    match v {
        Value::Bool(b) => *b,
        Value::String(s) => s.eq_ignore_ascii_case("win"),
        Value::Number(n) => n.as_i64().is_some_and(|n| n != 0),
        _ => false,
    }
}

fn count(v: &Value) -> u32 {
    v.as_u64().unwrap_or(0).min(u32::MAX as u64) as u32
}

/// The calendar date (year, month 1 to 12, day 1 to 31) of a day number: the
/// days since 1970-01-01 (the method of Howard Hinnant).
pub fn civil_from_days(days: i64) -> (i64, u32, u32) {
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let day_of_era = z.rem_euclid(146_097);
    let year_of_era =
        (day_of_era - day_of_era / 1460 + day_of_era / 36_524 - day_of_era / 146_096) / 365;
    let day_of_year = day_of_era - (365 * year_of_era + year_of_era / 4 - year_of_era / 100);
    let mp = (5 * day_of_year + 2) / 153;
    let day = day_of_year - (153 * mp + 2) / 5 + 1;
    let month = if mp < 10 { mp + 3 } else { mp - 9 };
    let year = year_of_era + era * 400 + i64::from(month <= 2);
    (year, month as u32, day as u32)
}

/// The day number (days since 1970-01-01) of a calendar date. The inverse of
/// `civil_from_days`.
pub fn days_from_civil(year: i64, month: u32, day: u32) -> i64 {
    let y = year - i64::from(month <= 2);
    let era = y.div_euclid(400);
    let year_of_era = y.rem_euclid(400);
    let mp = i64::from(if month > 2 { month - 3 } else { month + 9 });
    let day_of_year = (153 * mp + 2) / 5 + i64::from(day) - 1;
    let day_of_era = year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year;
    era * 146_097 + day_of_era - 719_468
}

/// The text "2026-10-23T09:20:31Z" for a time in Unix milliseconds. Empty for
/// zero or less.
pub fn iso_date(ms: i64) -> String {
    if ms <= 0 {
        return String::new();
    }
    let secs = ms / 1000;
    let (days, rest) = (secs.div_euclid(86_400), secs.rem_euclid(86_400));
    let (year, month, day) = civil_from_days(days);
    format!(
        "{year:04}-{month:02}-{day:02}T{:02}:{:02}:{:02}Z",
        rest / 3600,
        rest % 3600 / 60,
        rest % 60
    )
}

/// The Unix time in seconds of a text like "2026-10-23T09:20:31Z" (the text of
/// `iso_date`). `None` for an empty text or any other form.
pub fn parse_iso(text: &str) -> Option<i64> {
    let b = text.trim().strip_suffix('Z')?.as_bytes();
    // "YYYY-MM-DDTHH:MM:SS": the marks sit at fixed places, the rest are digits.
    if b.len() != 19
        || [(4, b'-'), (7, b'-'), (10, b'T'), (13, b':'), (16, b':')]
            .iter()
            .any(|&(i, c)| b[i] != c)
    {
        return None;
    }
    let num = |from: usize, to: usize| -> Option<i64> {
        let part = &b[from..to];
        part.iter()
            .all(u8::is_ascii_digit)
            .then(|| part.iter().fold(0, |n, d| n * 10 + i64::from(d - b'0')))
    };
    let (year, month, day) = (num(0, 4)?, num(5, 7)?, num(8, 10)?);
    let (hour, minute, second) = (num(11, 13)?, num(14, 16)?, num(17, 19)?);
    if !(1..=12).contains(&month)
        || !(1..=31).contains(&day)
        || hour > 23
        || minute > 59
        || second > 59
    {
        return None;
    }
    Some(
        days_from_civil(year, month as u32, day as u32) * 86_400
            + hour * 3600
            + minute * 60
            + second,
    )
}

/// The id of a participant: its own `puuid`, else the one of its identity.
/// The identity is the entry with the same `participantId`, else the entry at
/// the same place in the list.
fn puuid_of<'a>(p: &'a Value, at: usize, identities: &'a [Value]) -> Option<&'a str> {
    if let Some(own) = p["puuid"].as_str().filter(|s| !s.is_empty()) {
        return Some(own);
    }
    let pid = p["participantId"].as_i64();
    let identity = identities
        .iter()
        .find(|x| pid.is_some() && x["participantId"].as_i64() == pid)
        .or_else(|| identities.get(at));
    identity.and_then(|x| x["player"]["puuid"].as_str())
}

/// True for a remake: a very short game, or an early surrender in the first
/// ten minutes. Games that still run have no `gameDuration` and count as no remake.
pub fn is_remake(game: &Value) -> bool {
    let secs = game["gameDuration"].as_i64().unwrap_or(i64::MAX);
    let early = game["participants"]
        .as_array()
        .into_iter()
        .flatten()
        .any(|p| {
            let stats = if p["stats"].is_object() {
                &p["stats"]
            } else {
                p
            };
            truthy(&stats["gameEndedInEarlySurrender"])
        });
    secs < REMAKE_SECS || (secs < 2 * REMAKE_SECS && early)
}

/// The record of the player with this `puuid`, from one game of the match
/// history. `None` when the player is not in the game or the game has no
/// champion. A list entry holds only the own participant, so a game with one
/// participant counts as the game of the player.
pub fn parse_game(game: &Value, puuid: &str) -> Option<Game> {
    let id = game["gameId"].as_i64().filter(|&i| i > 0)?;
    let queue = game["queueId"].as_i64()?;
    let people = game["participants"].as_array()?;
    let identities = game["participantIdentities"]
        .as_array()
        .map_or(&[][..], Vec::as_slice);
    let me = people
        .iter()
        .enumerate()
        .find(|(i, p)| !puuid.is_empty() && puuid_of(p, *i, identities) == Some(puuid))
        .map(|(_, p)| p)
        .or_else(|| (people.len() == 1).then(|| &people[0]))?;
    // The LCU keeps the numbers in `stats`. A flat participant has them inline.
    let stats = if me["stats"].is_object() {
        &me["stats"]
    } else {
        me
    };
    let champion = me["championId"]
        .as_i64()
        .or_else(|| stats["championId"].as_i64())
        .filter(|&c| c > 0)?;
    let augments = (1..=AUGMENT_SLOTS)
        .filter_map(|n| stats[format!("playerAugment{n}")].as_i64())
        .filter(|&a| a > 0)
        .collect();
    Some(Game {
        id,
        date: iso_date(game["gameCreation"].as_i64().unwrap_or(0)),
        queue,
        champion,
        augments,
        win: truthy(&stats["win"]),
        kills: count(&stats["kills"]),
        deaths: count(&stats["deaths"]),
        assists: count(&stats["assists"]),
        secs: count(&game["gameDuration"]),
    })
}

/// One entry of the match list.
pub struct ListEntry<'a> {
    pub id: i64,
    pub queue: i64,
    /// The entry itself. It holds the own participant, so it can stand in for
    /// the whole game when that call fails.
    pub raw: &'a Value,
}

/// The games of `/lol-match-history/v1/products/lol/<puuid>/matches`, newest
/// first. The list sits in `games.games`. Some clients put it in `games`.
pub fn parse_list(v: &Value) -> Vec<ListEntry<'_>> {
    let list = v["games"]["games"]
        .as_array()
        .or_else(|| v["games"].as_array());
    let mut out: Vec<(i64, ListEntry)> = list
        .into_iter()
        .flatten()
        .filter_map(|g| {
            Some((
                g["gameCreation"].as_i64().unwrap_or(0),
                ListEntry {
                    id: g["gameId"].as_i64()?,
                    queue: g["queueId"].as_i64()?,
                    raw: g,
                },
            ))
        })
        .collect();
    out.sort_by(|a, b| b.0.cmp(&a.0).then(b.1.id.cmp(&a.1.id)));
    out.into_iter().map(|(_, e)| e).collect()
}

/// The entries that Hexsight has to read at a client start: the Mayhem games among
/// the first `limit` games of the list (it is newest first), without the games
/// that `history.json` has already. A game that the file has costs no call.
pub fn missing<'a>(
    list: Vec<ListEntry<'a>>,
    known: &HashSet<i64>,
    limit: usize,
) -> Vec<ListEntry<'a>> {
    list.into_iter()
        .filter(|e| is_mayhem(e.queue))
        .take(limit)
        .filter(|e| !known.contains(&e.id))
        .collect()
}

// ---------------------------------------------------------------------------
// The file. One game is one line, so a person can read it.

/// `history.json` in the data folder.
pub fn path() -> PathBuf {
    crate::cache::app_dir().join("history.json")
}

#[derive(Serialize, Deserialize)]
struct FileData {
    version: u32,
    games: Vec<Game>,
}

/// Add games to the list. A game with an id that the list has already is
/// skipped. The list stays newest first and keeps `MAX_GAMES` games at most.
/// Returns how many games were new.
pub fn merge(list: &mut Vec<Game>, new: impl IntoIterator<Item = Game>) -> usize {
    let mut added = 0;
    for g in new {
        if !list.iter().any(|x| x.id == g.id) {
            list.push(g);
            added += 1;
        }
    }
    list.sort_by(|a, b| b.date.cmp(&a.date).then(b.id.cmp(&a.id)));
    list.truncate(MAX_GAMES);
    added
}

/// The games of the file. `None` when the file does not exist, so the first
/// import can run. A file that does not parse moves to `history.bad.json`
/// first, so the import never overwrites it.
pub fn load(path: &Path) -> Option<Vec<Game>> {
    let bytes = std::fs::read(path).ok()?;
    match serde_json::from_slice::<FileData>(&bytes) {
        Ok(f) => {
            let mut games = Vec::new();
            merge(&mut games, f.games);
            Some(games)
        }
        Err(_) => {
            let _ = std::fs::rename(path, path.with_extension("bad.json"));
            None
        }
    }
}

/// Write the games through a temp file, then rename it over the file. A cut
/// write never leaves a broken file.
pub fn save(path: &Path, games: &[Game]) -> Result<(), String> {
    let mut text = String::from("{\"version\":1,\"games\":[\n");
    for (i, g) in games.iter().enumerate() {
        if i > 0 {
            text.push_str(",\n");
        }
        text.push_str(&serde_json::to_string(g).map_err(|e| e.to_string())?);
    }
    text.push_str("\n]}\n");
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, text)
        .and_then(|()| std::fs::rename(&tmp, path))
        .map_err(|e| {
            let _ = std::fs::remove_file(&tmp);
            e.to_string()
        })
}

// ---------------------------------------------------------------------------
// Numbers for the "My games" page.

/// Games and wins of one champion, or of all games.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Tally {
    pub games: u32,
    pub wins: u32,
}

impl Tally {
    /// Share of won games, 0 to 1. Zero without games.
    pub fn rate(&self) -> f32 {
        if self.games == 0 {
            0.0
        } else {
            self.wins as f32 / self.games as f32
        }
    }

    fn add(&mut self, win: bool) {
        self.games += 1;
        self.wins += u32::from(win);
    }
}

/// The whole list in numbers.
#[derive(Debug, Default)]
pub struct Summary {
    pub total: Tally,
    /// Champions you played, most games first.
    pub champions: Vec<(i64, Tally)>,
}

/// Most games first, then most wins, then the lowest id (a stable order).
fn by_games(map: HashMap<i64, Tally>) -> Vec<(i64, Tally)> {
    let mut v: Vec<(i64, Tally)> = map.into_iter().collect();
    v.sort_by(|a, b| {
        b.1.games
            .cmp(&a.1.games)
            .then(b.1.wins.cmp(&a.1.wins))
            .then(a.0.cmp(&b.0))
    });
    v
}

pub fn summarize(games: &[Game]) -> Summary {
    let mut total = Tally::default();
    let mut champions: HashMap<i64, Tally> = HashMap::new();
    for g in games {
        total.add(g.win);
        champions.entry(g.champion).or_default().add(g.win);
    }
    Summary {
        total,
        champions: by_games(champions),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::i18n::Lang;

    /// A game in the shape of `/lol-match-history/v1/games/<id>`, cut to four
    /// of the ten players. The numbers are made up. The field names are the
    /// ones that Yhprum/mayhem-tracker reads.
    const GAME: &str = r#"{
        "gameCreation": 1792747231762,
        "gameCreationDate": "2026-10-23T09:20:31.762Z",
        "gameDuration": 1186,
        "gameId": 7000100,
        "gameMode": "KIWI",
        "gameType": "MATCHED_GAME",
        "gameVersion": "16.21.829.8620",
        "mapId": 12,
        "participantIdentities": [
            {"participantId": 1, "player": {"puuid": "puuid-anne", "gameName": "Anne", "tagLine": "TR1", "summonerName": "Anne", "profileIcon": 29}},
            {"participantId": 2, "player": {"puuid": "puuid-me", "gameName": "Me", "tagLine": "TR1", "summonerName": "Me", "profileIcon": 4568}},
            {"participantId": 3, "player": {"puuid": "puuid-bob", "gameName": "Bob", "tagLine": "TR2", "summonerName": "Bob", "profileIcon": 12}},
            {"participantId": 4, "player": {"puuid": "00000000-0000-0000-0000-000000000000", "gameName": "", "tagLine": "", "summonerName": "Bot", "profileIcon": 0}}
        ],
        "participants": [
            {"participantId": 1, "teamId": 100, "championId": 1, "spell1Id": 4, "spell2Id": 32,
             "stats": {"win": false, "kills": 3, "deaths": 9, "assists": 4, "playerAugment1": 1205, "playerAugment2": 0}},
            {"participantId": 2, "teamId": 100, "championId": 157, "spell1Id": 4, "spell2Id": 32,
             "stats": {"win": true, "kills": 12, "deaths": 5, "assists": 9, "item0": 3031, "champLevel": 18,
                       "playerAugment1": 1058, "playerAugment2": 3078, "playerAugment3": 1206, "playerAugment4": 1337,
                       "playerAugment5": 0, "playerAugment6": 0}},
            {"participantId": 3, "teamId": 200, "championId": 51, "spell1Id": 4, "spell2Id": 32,
             "stats": {"win": false, "kills": 7, "deaths": 8, "assists": 2, "playerAugment1": 3116}},
            {"participantId": 4, "teamId": 200, "championId": 89, "spell1Id": 4, "spell2Id": 32,
             "stats": {"win": false, "kills": 0, "deaths": 4, "assists": 6}}
        ],
        "platformId": "TR1",
        "queueId": 2400,
        "seasonId": 16,
        "teams": []
    }"#;

    fn game() -> Value {
        serde_json::from_str(GAME).unwrap()
    }

    #[test]
    fn reads_the_own_participant_of_a_whole_game() {
        let g = parse_game(&game(), "puuid-me").unwrap();
        assert_eq!(g.id, 7000100);
        assert_eq!(g.queue, 2400);
        assert_eq!(g.champion, 157);
        // The empty slots (0) are left out. The order of the slots stays.
        assert_eq!(g.augments, vec![1058, 3078, 1206, 1337]);
        assert!(g.win);
        assert_eq!((g.kills, g.deaths, g.assists), (12, 5, 9));
        // The length in seconds: 19 minutes 46 seconds.
        assert_eq!(g.secs, 1186);
        assert_eq!(g.date, "2026-10-23T09:20:31Z");
    }

    #[test]
    fn other_players_give_their_own_numbers() {
        let g = parse_game(&game(), "puuid-bob").unwrap();
        assert_eq!((g.champion, g.augments, g.win), (51, vec![3116], false));
        // A player who is not in the game, and an empty id, find nobody.
        assert!(parse_game(&game(), "puuid-nobody").is_none());
        assert!(parse_game(&game(), "").is_none());
    }

    #[test]
    fn a_list_entry_with_one_participant_is_the_own_game() {
        let mut entry = game();
        let me = entry["participants"][1].clone();
        entry["participants"] = Value::Array(vec![me]);
        entry["participantIdentities"] = Value::Array(Vec::new());
        // The entry has no identity, but it holds only the own participant.
        let g = parse_game(&entry, "puuid-me").unwrap();
        assert_eq!((g.champion, g.augments.len()), (157, 4));
    }

    #[test]
    fn a_participant_can_hold_its_own_puuid() {
        let mut g = game();
        g["participantIdentities"] = Value::Array(Vec::new());
        g["participants"][2]["puuid"] = Value::from("puuid-me");
        assert_eq!(parse_game(&g, "puuid-me").unwrap().champion, 51);
    }

    #[test]
    fn a_flat_participant_has_the_numbers_inline() {
        let flat: Value = serde_json::from_str(
            r#"{"gameId": 5, "queueId": 2450, "gameCreation": 0, "participants": [
                {"puuid": "p", "championId": 9, "win": true, "kills": 1, "deaths": 2, "assists": 3, "playerAugment1": 1001}]}"#,
        )
        .unwrap();
        let g = parse_game(&flat, "p").unwrap();
        assert_eq!(
            (g.champion, g.augments, g.win, g.kills, g.deaths, g.assists),
            (9, vec![1001], true, 1, 2, 3)
        );
        // No creation time: the date stays empty.
        assert_eq!(g.date, "");
    }

    #[test]
    fn a_result_can_be_a_word() {
        let mut g = game();
        g["participants"][1]["stats"]["win"] = Value::from("Fail");
        assert!(!parse_game(&g, "puuid-me").unwrap().win);
        g["participants"][1]["stats"]["win"] = Value::from("Win");
        assert!(parse_game(&g, "puuid-me").unwrap().win);
    }

    #[test]
    fn a_bad_game_gives_nothing() {
        for text in [
            r#"{}"#,
            r#"{"gameId": 1, "queueId": 2400}"#,
            r#"{"gameId": 0, "queueId": 2400, "participants": []}"#,
        ] {
            let v: Value = serde_json::from_str(text).unwrap();
            assert!(parse_game(&v, "puuid-me").is_none(), "{text}");
        }
        // A participant without a champion is no game.
        let mut g = game();
        g["participants"][1]["championId"] = Value::from(0);
        assert!(parse_game(&g, "puuid-me").is_none());
    }

    #[test]
    fn a_remake_is_found() {
        assert!(!is_remake(&game()));
        let mut g = game();
        g["gameDuration"] = Value::from(240);
        assert!(is_remake(&g));
        // An early surrender in the first ten minutes.
        g["gameDuration"] = Value::from(420);
        assert!(!is_remake(&g));
        g["participants"][0]["stats"]["gameEndedInEarlySurrender"] = Value::from(true);
        assert!(is_remake(&g));
        g["gameDuration"] = Value::from(700);
        assert!(!is_remake(&g));
    }

    #[test]
    fn dates_come_from_milliseconds() {
        assert_eq!(iso_date(0), "");
        assert_eq!(iso_date(-5), "");
        assert_eq!(iso_date(1_000), "1970-01-01T00:00:01Z");
        assert_eq!(iso_date(1_700_000_000_000), "2023-11-14T22:13:20Z");
        // A leap day, and the day after it.
        assert_eq!(iso_date(1_709_208_000_000), "2024-02-29T12:00:00Z");
        assert_eq!(iso_date(1_709_251_200_000), "2024-03-01T00:00:00Z");
        assert_eq!(iso_date(1_792_747_231_762), "2026-10-23T09:20:31Z");
        // The text sorts like the time.
        assert!(iso_date(1_709_208_000_000) < iso_date(1_709_251_200_000));
    }

    #[test]
    fn a_date_text_reads_back_to_the_same_second() {
        assert_eq!(parse_iso("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(parse_iso("2026-10-23T09:20:31Z"), Some(1_792_747_231));
        assert_eq!(parse_iso("2024-02-29T12:00:00Z"), Some(1_709_208_000));
        // Every second that `iso_date` writes reads back.
        for secs in (0..4_102_444_800_i64).step_by(7_654_321) {
            assert_eq!(
                parse_iso(&iso_date(secs * 1000 + 1000)),
                Some(secs + 1),
                "{secs}"
            );
        }
        // The year 2000 has a 29th of February and the year 2100 has none.
        let day = |text: &str| parse_iso(text).unwrap();
        assert_eq!(
            day("2000-03-01T00:00:00Z") - day("2000-02-28T00:00:00Z"),
            2 * 86_400
        );
        assert_eq!(
            day("2100-03-01T00:00:00Z") - day("2100-02-28T00:00:00Z"),
            86_400
        );
        // The calendar functions are the inverse of each other, before and after 1970.
        for days in [-800_000, -1, 0, 1, 59, 60, 365, 19_000, 20_453, 2_932_896] {
            let (y, m, d) = civil_from_days(days);
            assert_eq!(days_from_civil(y, m, d), days, "{days}: {y}-{m}-{d}");
        }
        assert_eq!(civil_from_days(20_453), (2025, 12, 31));
        assert_eq!(civil_from_days(20_454), (2026, 1, 1));
        assert_eq!(days_from_civil(2026, 10, 23), 20_749);
    }

    #[test]
    fn a_date_text_of_another_form_reads_as_no_date() {
        // The file keeps "" for a game with no creation time.
        for text in [
            "",
            "  ",
            "2026-10-23",
            "2026-10-23T09:20:31",
            "2026-10-23 09:20:31Z",
            "2026-10-23T09:20:31.762Z",
            "2026-13-01T00:00:00Z",
            "2026-00-10T00:00:00Z",
            "2026-10-32T00:00:00Z",
            "2026-10-23T24:00:00Z",
            "2026-10-23T09:60:00Z",
            "2026-10-23T09:20:60Z",
            "20x6-10-23T09:20:31Z",
            "2026-10-23T09:20:3-Z",
        ] {
            assert_eq!(parse_iso(text), None, "{text:?}");
        }
    }

    /// The list of `/lol-match-history/v1/products/lol/<puuid>/matches`: three
    /// entries in an odd order, one of them in another queue.
    const LIST: &str = r#"{
        "accountId": 2000001, "platformId": "TR1",
        "games": {"gameBeginDate": "2026-10-01", "gameCount": 3, "gameIndexBegin": 0, "gameIndexEnd": 3,
            "games": [
                {"gameId": 7000090, "queueId": 2400, "gameCreation": 1792600000000, "participants": [], "participantIdentities": []},
                {"gameId": 7000099, "queueId": 450, "gameCreation": 1792700000000, "participants": [], "participantIdentities": []},
                {"gameId": 7000095, "queueId": 2450, "gameCreation": 1792650000000, "participants": [], "participantIdentities": []}
            ]}
    }"#;

    #[test]
    fn the_list_comes_newest_first() {
        let v: Value = serde_json::from_str(LIST).unwrap();
        let list = parse_list(&v);
        assert_eq!(
            list.iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![7000099, 7000095, 7000090]
        );
        assert_eq!(list.iter().filter(|e| is_mayhem(e.queue)).count(), 2);
        // Some clients put the games right in `games`.
        let flat: Value = serde_json::from_str(
            r#"{"games": [{"gameId": 3, "queueId": 2400}, {"gameId": 4, "queueId": 2400}]}"#,
        )
        .unwrap();
        assert_eq!(
            parse_list(&flat).iter().map(|e| e.id).collect::<Vec<_>>(),
            vec![4, 3]
        );
        assert!(parse_list(&Value::Null).is_empty());
    }

    fn rec(id: i64, date: &str, champion: i64, augments: &[i64], win: bool) -> Game {
        Game {
            id,
            date: date.into(),
            queue: 2400,
            champion,
            augments: augments.to_vec(),
            win,
            kills: 1,
            deaths: 2,
            assists: 3,
            secs: 1200,
        }
    }

    #[test]
    fn a_new_game_goes_in_once_and_the_list_stays_newest_first() {
        let mut list = vec![rec(1, "2026-10-01T10:00:00Z", 157, &[1], true)];
        let added = merge(
            &mut list,
            [
                rec(3, "2026-10-03T10:00:00Z", 51, &[2], false),
                rec(1, "2026-10-01T10:00:00Z", 157, &[1], true),
                rec(2, "2026-10-02T10:00:00Z", 89, &[3], true),
            ],
        );
        assert_eq!(added, 2);
        assert_eq!(list.iter().map(|g| g.id).collect::<Vec<_>>(), vec![3, 2, 1]);
    }

    #[test]
    fn the_list_keeps_500_games() {
        let mut list = Vec::new();
        let games = (0..520).map(|i| {
            rec(
                i,
                &format!("2026-01-01T00:{:02}:{:02}Z", i / 60 % 60, i % 60),
                1,
                &[],
                true,
            )
        });
        merge(&mut list, games);
        assert_eq!(list.len(), MAX_GAMES);
        // The oldest 20 games are gone.
        assert_eq!(list[0].id, 519);
        assert_eq!(list.last().map(|g| g.id), Some(20));
    }

    #[test]
    fn the_file_writes_and_reads_back() {
        let dir =
            std::env::temp_dir().join(format!("hexsight-test-history-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");
        // No file: the first import can run.
        assert!(load(&path).is_none());
        let games = vec![
            rec(2, "2026-10-02T10:00:00Z", 89, &[3, 4], true),
            rec(1, "2026-10-01T10:00:00Z", 157, &[], false),
        ];
        save(&path, &games).unwrap();
        assert_eq!(load(&path), Some(games));
        // One game is one line, and no temp file stays.
        let text = std::fs::read_to_string(&path).unwrap();
        assert_eq!(text.lines().count(), 4);
        assert!(!path.with_extension("tmp").exists());
        // An empty list is a file too, so the import runs only once.
        save(&path, &[]).unwrap();
        assert_eq!(load(&path), Some(Vec::new()));
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn a_game_from_an_older_version_has_no_length() {
        let old = r#"{"id": 5, "date": "2026-10-05T10:00:00Z", "queue": 2400, "champion": 157, "augments": [1, 2], "win": true, "kills": 1, "deaths": 2, "assists": 3}"#;
        let g: Game = serde_json::from_str(old).unwrap();
        assert_eq!(g.secs, 0);
        assert_eq!(g.id, 5);
    }

    #[test]
    fn a_broken_file_moves_aside() {
        let dir =
            std::env::temp_dir().join(format!("hexsight-test-history-bad-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("history.json");
        std::fs::write(&path, "{ not json").unwrap();
        assert!(load(&path).is_none());
        assert!(!path.exists());
        assert_eq!(
            std::fs::read_to_string(dir.join("history.bad.json")).unwrap(),
            "{ not json"
        );
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn sample() -> Vec<Game> {
        vec![
            rec(4, "2026-10-04T10:00:00Z", 157, &[10, 20], true),
            rec(3, "2026-10-03T10:00:00Z", 157, &[10, 30], true),
            rec(2, "2026-10-02T10:00:00Z", 51, &[10, 20], false),
            rec(1, "2026-10-01T10:00:00Z", 157, &[10, 10], false),
        ]
    }

    #[test]
    fn the_numbers_follow_the_games() {
        let s = summarize(&sample());
        assert_eq!(s.total, Tally { games: 4, wins: 2 });
        assert_eq!(
            s.champions,
            vec![
                (157, Tally { games: 3, wins: 2 }),
                (51, Tally { games: 1, wins: 0 })
            ]
        );
        assert!((s.total.rate() - 0.5).abs() < 1e-6);
        assert_eq!(Tally::default().rate(), 0.0);
        assert!(summarize(&[]).total.games == 0);
    }

    #[test]
    fn one_game_reads_in_the_singular() {
        // The caption needs 3 games, so a count of 1 shows in the totals and the lines of My games.
        assert_eq!(Lang::En.totals(1, 1, 0), "1 game · 1W 0L");
        assert_eq!(Lang::Tr.totals(1, 0, 1), "1 maç · 0G 1M");
    }

    #[test]
    fn a_sync_reads_only_the_mayhem_games_that_the_file_lacks() {
        let v: Value = serde_json::from_str(LIST).unwrap();
        let none = HashSet::new();
        // The newest first: 7000099 is another queue, so the Mayhem games are 7000095 and 7000090.
        assert_eq!(
            missing(parse_list(&v), &none, 20)
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![7000095, 7000090]
        );
        // A game that the file has costs no call.
        let known: HashSet<i64> = [7000095].into_iter().collect();
        assert_eq!(
            missing(parse_list(&v), &known, 20)
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![7000090]
        );
        // All games known: nothing to read, so no call and no write.
        let all: HashSet<i64> = [7000095, 7000090, 7000099].into_iter().collect();
        assert!(missing(parse_list(&v), &all, 20).is_empty());
        // The limit counts Mayhem games, newest first, before the known games leave.
        assert_eq!(
            missing(parse_list(&v), &none, 1)
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>(),
            vec![7000095]
        );
        assert!(missing(parse_list(&v), &known, 1).is_empty());
        assert!(missing(parse_list(&Value::Null), &none, 20).is_empty());
    }

    /// Runs the parse, the merge and the save on answers saved from a real client.
    /// HEXSIGHT_LCU_DIR=<folder with summoner.json, list.json, game_*.json and a copy of history.json>
    /// Run: cargo test --release real_client_answers -- --ignored --nocapture
    #[test]
    #[ignore]
    fn real_client_answers() {
        let dir =
            PathBuf::from(std::env::var_os("HEXSIGHT_LCU_DIR").expect("set HEXSIGHT_LCU_DIR"));
        let read = |name: &str| -> Value {
            serde_json::from_slice(&std::fs::read(dir.join(name)).unwrap()).unwrap()
        };
        let puuid = read("summoner.json")["puuid"].as_str().unwrap().to_string();
        let mut games = load(&dir.join("history.json")).expect("history.json copy");
        let known: HashSet<i64> = games.iter().map(|g| g.id).collect();
        let mut parsed = Vec::new();
        for entry in std::fs::read_dir(&dir).unwrap().flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with("game_") {
                continue;
            }
            let g = read(&name);
            let p = parse_game(&g, &puuid);
            println!(
                "{name}: remake {}, parsed {:?}",
                is_remake(&g),
                p.as_ref()
                    .map(|x| (x.id, x.queue, x.champion, &x.date, x.secs))
            );
            parsed.extend(p);
        }
        let list = read("list.json");
        println!(
            "list: {} entries, missing Mayhem games {:?}",
            parse_list(&list).len(),
            missing(parse_list(&list), &known, 20)
                .iter()
                .map(|e| e.id)
                .collect::<Vec<_>>()
        );
        let added = merge(&mut games, parsed);
        println!(
            "merge added {added}, save {:?}",
            save(&dir.join("history.out.json"), &games)
        );
    }
}
