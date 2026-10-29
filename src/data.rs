//! Data sources.
//! - Static data (names, icons, Mayhem augment set): CommunityDragon,
//!   in the client locale.
//! - Mayhem stats: aramkit public JSON (main), aramhex-data on GitHub (fallback,
//!   CC BY-NC 4.0, non-commercial use only).

use crate::cache;
use crate::i18n::Key;
use crate::model::*;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

/// Why a data load failed. The window turns it into text in the language of
/// the interface (`Lang::load_error`), so the footer follows the setting.
#[derive(Clone, Debug, PartialEq)]
pub enum LoadError {
    /// A file did not download. The key names the file in words.
    Download(Key),
    /// A file came, but it lacks what Hexsight needs.
    Invalid(Key),
    /// The names in the client language did not download. The text is the
    /// folder of that language.
    Names(String),
    /// The main stats source failed, and the fallback failed too.
    Both(Box<LoadError>, Box<LoadError>),
}

const CDRAGON: &str = "https://raw.communitydragon.org/latest/plugins/rcp-be-lol-game-data/global";
const ARAMKIT: &str = "https://data.aramkit.com";
const ARAMHEX: &str = "https://raw.githubusercontent.com/czgreat/aramhex-data/main";
const DAY: Duration = Duration::from_secs(24 * 3600);
const STATS_AGE: Duration = Duration::from_secs(6 * 3600);

/// CDragon folder name for a client locale: "en_US" -> "default", "tr_TR" -> "tr_tr".
pub fn cdragon_locale(locale: &str) -> String {
    if locale.is_empty() || locale.eq_ignore_ascii_case("en_US") {
        "default".into()
    } else {
        locale.to_lowercase()
    }
}

/// "/lol-game-data/assets/ASSETS/UX/x.png" -> CDragon URL (paths are lower case there).
/// An empty path gives an empty URL. Callers must skip an empty URL.
pub fn asset_url(path: &str) -> String {
    let rest = path
        .trim_start_matches("/lol-game-data/assets/")
        .to_lowercase();
    if rest.is_empty() {
        return String::new();
    }
    format!("{CDRAGON}/default/{rest}")
}

fn arr(v: &Value) -> impl Iterator<Item = &Value> {
    v.as_array().into_iter().flatten()
}

fn f(v: &Value) -> f32 {
    v.as_f64().unwrap_or(0.0) as f32
}

fn u(v: &Value) -> u32 {
    v.as_u64().unwrap_or(0) as u32
}

/// Name and icon of each entry, by id. `None` when the file did not download.
fn icon_list(agent: &ureq::Agent, url: &str) -> Option<HashMap<i64, Item>> {
    let v = cache::get_json(agent, url, DAY)?;
    let mut out = HashMap::new();
    for x in arr(&v) {
        if let Some(id) = x["id"].as_i64() {
            let name = x["name"].as_str().unwrap_or("").to_string();
            out.insert(
                id,
                Item {
                    name,
                    icon: asset_url(x["iconPath"].as_str().unwrap_or("")),
                },
            );
        }
    }
    Some(out)
}

/// The champions of `champion-summary.json`, by id. The list also has copies
/// for another mode (ids 60001 and up, alias "Jade_Annie") with the shown name
/// of the real champion. They stay out: a lookup by name could find the copy,
/// and no stats source has data for it. No real alias has a "_".
pub fn parse_champions(summary: &Value) -> HashMap<i64, Champion> {
    let mut out = HashMap::new();
    for c in arr(summary) {
        let id = c["id"].as_i64().unwrap_or(-1);
        let alias = c["alias"].as_str().unwrap_or("");
        if id <= 0 || alias.contains('_') {
            continue;
        }
        out.insert(
            id,
            Champion {
                id,
                name: c["name"].as_str().unwrap_or("").into(),
                alias: alias.into(),
                icon: asset_url(c["squarePortraitPath"].as_str().unwrap_or("")),
            },
        );
    }
    out
}

/// The roles of each champion in `champion-summary.json` (`"roles":["mage"]`), in lower
/// case. The copies for another mode stay out, as in `parse_champions`. A champion
/// without roles has no entry.
pub fn parse_roles(summary: &Value) -> HashMap<i64, Vec<String>> {
    let mut out = HashMap::new();
    for c in arr(summary) {
        let id = c["id"].as_i64().unwrap_or(-1);
        if id <= 0 || c["alias"].as_str().unwrap_or("").contains('_') {
            continue;
        }
        let roles: Vec<String> = arr(&c["roles"])
            .filter_map(|r| r.as_str())
            .map(str::to_lowercase)
            .collect();
        if !roles.is_empty() {
            out.insert(id, roles);
        }
    }
    out
}

/// The items of `items.json`: name and icon of each entry, and the price, the recipe
/// and the tags of each entry (`ItemInfo`). Both maps come from one file.
pub fn parse_items(v: &Value) -> (HashMap<i64, Item>, HashMap<i64, ItemInfo>) {
    let mut items = HashMap::new();
    let mut info = HashMap::new();
    for x in arr(v) {
        let Some(id) = x["id"].as_i64() else { continue };
        let name = x["name"].as_str().unwrap_or("").to_string();
        items.insert(
            id,
            Item {
                name,
                icon: asset_url(x["iconPath"].as_str().unwrap_or("")),
            },
        );
        let words: Vec<&str> = arr(&x["categories"]).filter_map(|c| c.as_str()).collect();
        info.insert(
            id,
            ItemInfo {
                total: u(&x["priceTotal"]),
                from: arr(&x["from"]).filter_map(|c| c.as_i64()).collect(),
                cats: words.iter().fold(0, |acc, w| acc | cat::bit(w)),
                tags: words.len().min(255) as u8,
                in_store: x["inStore"].as_bool().unwrap_or(false),
            },
        );
    }
    (items, info)
}

/// The id of the champion that the Live Client names. `raw` is its
/// `rawChampionName` ("game_character_displayname_MonkeyKing") and `name` its
/// `championName` (the shown name, in the language of the game). The alias
/// decides. The shown name is only a fallback, and then the lowest id wins, so
/// the answer never depends on the order of the map.
pub fn live_champion(champions: &HashMap<i64, Champion>, raw: &str, name: &str) -> Option<i64> {
    let alias = raw.rsplit('_').next().unwrap_or(raw);
    if let Some(c) = champions
        .values()
        .find(|c| !alias.is_empty() && c.alias.eq_ignore_ascii_case(alias))
    {
        return Some(c.id);
    }
    champions
        .values()
        .filter(|c| !name.is_empty() && c.name == name)
        .map(|c| c.id)
        .min()
}

pub fn load_static(agent: &ureq::Agent, locale: &str) -> Result<StaticData, LoadError> {
    let loc = cdragon_locale(locale);
    let mut out = StaticData::default();

    let champs = cache::get_json(
        agent,
        &format!("{CDRAGON}/{loc}/v1/champion-summary.json"),
        DAY,
    )
    .ok_or(LoadError::Download(Key::FileChampions))?;
    out.champions = parse_champions(&champs);
    out.roles = parse_roles(&champs);

    // The KIWI list is the exact Mayhem augment set. Its entries end with the augmentNameId.
    let lists = cache::get_json(
        agent,
        &format!("{CDRAGON}/default/v1/augment-lists.json"),
        DAY,
    )
    .ok_or(LoadError::Download(Key::FileAugmentSet))?;
    let kiwi_names: std::collections::HashSet<String> = arr(&lists)
        .filter(|l| l["modeName"] == "KIWI")
        .flat_map(|l| arr(&l["augmentList"]))
        .filter_map(|p| p.as_str()?.rsplit('/').next().map(str::to_string))
        .collect();

    let english = cache::get_json(
        agent,
        &format!("{CDRAGON}/default/v1/cherry-augments.json"),
        DAY,
    )
    .ok_or(LoadError::Download(Key::FileAugmentList))?;
    // Names in the client language are required. English names cannot match the
    // text that OCR reads from a localized card, so a failed download is an error.
    let local = if loc == "default" {
        english.clone()
    } else {
        cache::get_json(
            agent,
            &format!("{CDRAGON}/{loc}/v1/cherry-augments.json"),
            DAY,
        )
        .ok_or_else(|| LoadError::Names(loc.clone()))?
    };
    let local_names: HashMap<i64, String> = arr(&local)
        .filter_map(|a| Some((a["id"].as_i64()?, a["nameTRA"].as_str()?.to_string())))
        .collect();
    for a in arr(&english) {
        let id = a["id"].as_i64().unwrap_or(-1);
        let api = a["augmentNameId"].as_str().unwrap_or("");
        if id < 0 || !kiwi_names.contains(api) {
            continue;
        }
        let name_en = a["nameTRA"].as_str().unwrap_or("").to_string();
        out.kiwi.insert(id);
        out.augments.insert(
            id,
            Augment {
                name: local_names
                    .get(&id)
                    .filter(|n| !n.is_empty())
                    .cloned()
                    .unwrap_or(name_en.clone()),
                name_en,
                icon: asset_url(a["augmentSmallIconPath"].as_str().unwrap_or("")),
            },
        );
    }

    // One file gives the names and icons, and the prices, recipes and tags of the item suggestions.
    let item_file = cache::get_json(agent, &format!("{CDRAGON}/{loc}/v1/items.json"), DAY)
        .ok_or(LoadError::Download(Key::FileItems))?;
    (out.items, out.item_info) = parse_items(&item_file);
    // The spell names are an extra. Without them the spell card stays away, and the rest works.
    out.spells =
        icon_list(agent, &format!("{CDRAGON}/{loc}/v1/summoner-spells.json")).unwrap_or_default();
    Ok(out)
}

// ---------------------------------------------------------------------------
// Stats index: patch, champion tier list, and where to find per-champion files.

pub struct Stats {
    pub patch: String,
    pub matches: u64,
    pub source: &'static str,
    pub champions: HashMap<i64, ChampRow>,
    data_path: String,
    /// Fallback source keeps all champions in one table.
    hex: Option<HashMap<i64, Value>>,
}

impl Stats {
    /// Stats with these champion rows and nothing to load, for the tests of the window.
    #[cfg(test)]
    pub fn of_champions(champions: HashMap<i64, ChampRow>) -> Stats {
        Stats {
            patch: "16.20".into(),
            matches: 120_000,
            source: "aramkit",
            champions,
            data_path: String::new(),
            hex: None,
        }
    }

    pub fn load(agent: &ureq::Agent) -> Result<Stats, LoadError> {
        match Self::load_aramkit(agent) {
            Ok(s) => Ok(s),
            Err(e1) => {
                Self::load_aramhex(agent).map_err(|e2| LoadError::Both(Box::new(e1), Box::new(e2)))
            }
        }
    }

    fn load_aramkit(agent: &ureq::Agent) -> Result<Stats, LoadError> {
        let idx = cache::get_json(
            agent,
            &format!("{ARAMKIT}/data/versions.json"),
            Duration::from_secs(1800),
        )
        .ok_or(LoadError::Download(Key::FileAramkitIndex))?;
        // The index must name the newest version and the path of its data.
        let bad_index = || LoadError::Invalid(Key::FileAramkitIndex);
        let latest = idx["latest"].as_str().ok_or_else(bad_index)?;
        let ver = arr(&idx["versions"])
            .find(|v| v["version"] == latest)
            .ok_or_else(bad_index)?;
        let data_path = ver["dataPath"].as_str().ok_or_else(bad_index)?.to_string();

        let ranks = cache::get_json(
            agent,
            &format!("{ARAMKIT}/{data_path}/stats/all/champion-rankings.json"),
            STATS_AGE,
        )
        .ok_or(LoadError::Download(Key::FileAramkitChampions))?;
        let mut champions = HashMap::new();
        for r in arr(&ranks["rows"]) {
            if let Some(id) = r["id"].as_i64() {
                champions.insert(
                    id,
                    ChampRow {
                        win_rate: f(&r["winRate"]),
                        games: u(&r["sampleCount"]),
                        tier: tier_index(r["tier"].as_str().unwrap_or("")),
                    },
                );
            }
        }

        Ok(Stats {
            patch: latest.to_string(),
            matches: ver["allMatches"].as_u64().unwrap_or(0),
            source: "aramkit",
            champions,
            data_path,
            hex: None,
        })
    }

    fn load_aramhex(agent: &ureq::Agent) -> Result<Stats, LoadError> {
        let latest = cache::get_json(
            agent,
            &format!("{ARAMHEX}/latest.json"),
            Duration::from_secs(1800),
        )
        .ok_or(LoadError::Download(Key::FileAramhexIndex))?;
        let path = latest["path"]
            .as_str()
            .ok_or(LoadError::Invalid(Key::FileAramhexIndex))?;
        let heroes = cache::get_json(agent, &format!("{ARAMHEX}/{path}/heroes.json"), STATS_AGE)
            .ok_or(LoadError::Download(Key::FileAramhexChampions))?;
        let table = cache::get_json(
            agent,
            &format!("{ARAMHEX}/{path}/hero_augments.json"),
            STATS_AGE,
        )
        .ok_or(LoadError::Download(Key::FileAramhexAugments))?;
        let mut champions = HashMap::new();
        for (k, v) in heroes.as_object().into_iter().flatten() {
            if let Ok(id) = k.parse::<i64>() {
                let rank = u(&v["rank"]);
                champions.insert(
                    id,
                    ChampRow {
                        win_rate: f(&v["win_rate"]),
                        games: u(&v["games"]),
                        tier: (rank / 30).min(5) as u8,
                    },
                );
            }
        }
        let hex = table.as_object().map(|m| {
            m.iter()
                .filter_map(|(k, v)| Some((k.parse::<i64>().ok()?, v.clone())))
                .collect()
        });
        Ok(Stats {
            patch: latest["patch"].as_str().unwrap_or("?").into(),
            matches: latest["matches"].as_u64().unwrap_or(0),
            source: "aramhex",
            champions,
            data_path: String::new(),
            hex,
        })
    }

    /// Full stats for one champion. Downloads one file (aramkit) on first use.
    pub fn champion(&self, agent: &ureq::Agent, id: i64) -> Result<ChampStats, NoStats> {
        if let Some(hex) = &self.hex {
            let row = self.champions.get(&id).copied().unwrap_or_default();
            return hex
                .get(&id)
                .and_then(|t| hex_champion(t, row))
                .ok_or(NoStats::Missing);
        }
        let url = format!(
            "{ARAMKIT}/{}/stats/all/champion-details/{id}.json",
            self.data_path
        );
        match cache::get_json_why(agent, &url, STATS_AGE) {
            Ok(d) => Ok(kit_champion(&d)),
            Err(f) => Err(no_stats(f.why, self.champions.contains_key(&id))),
        }
    }
}

/// Why `Stats::champion` gave no stats.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NoStats {
    /// The source has no stats for this champion. Only the next stats index can
    /// change that, so Hexsight does not ask again until then.
    Missing,
    /// The network, a timeout, a server error, or a bad file. A later try can work.
    Transient,
}

/// The kind of a failed champion download. `listed` says if the stats index
/// has a row for the champion. A 404, 410 or 403 says the file does not exist
/// (file hosts often answer 403 for a missing file). That is final only for a
/// champion that the index does not list. A listed champion has a file, which
/// can be not uploaded yet, so it is tried again. Anything else can work on a
/// later try.
fn no_stats(why: cache::Fail, listed: bool) -> NoStats {
    match why {
        cache::Fail::Status(403 | 404 | 410) if !listed => NoStats::Missing,
        _ => NoStats::Transient,
    }
}

fn kit_aug(a: &Value) -> Option<AugStat> {
    Some(AugStat {
        augment_id: a["id"].as_i64()?,
        win_rate: f(&a["winRate"]),
        games: u(&a["sampleCount"]),
        tier: tier_index(a["tier"].as_str().unwrap_or("")),
    })
}

fn ids(v: &Value) -> Vec<i64> {
    arr(v).filter_map(|x| x["id"].as_i64()).collect()
}

/// Ids of a ranked list whose rows hold `item.id` (the boots and the later items).
fn ranked_ids(v: &Value) -> Vec<i64> {
    arr(v).filter_map(|x| x["item"]["id"].as_i64()).collect()
}

/// True when two item lists hold the same items, whatever the order.
fn same_items(a: &[i64], b: &[i64]) -> bool {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    a.sort_unstable();
    b.sort_unstable();
    a == b
}

/// Potions and elixirs: the items of a starting set that the shop shows as consumables.
const CONSUMABLES: [i64; 7] = [2003, 2031, 2033, 2055, 2138, 2139, 2140];

/// The consumables of the top three starting sets. Each id comes as often as the
/// set that buys it most times, so two potions in a set stay two.
fn consumables(sets: &[Vec<i64>]) -> Vec<i64> {
    let mut out: Vec<(i64, usize)> = Vec::new();
    for set in sets.iter().take(3) {
        for &id in set.iter().filter(|id| CONSUMABLES.contains(id)) {
            let count = set.iter().filter(|&&x| x == id).count();
            match out.iter_mut().find(|(i, _)| *i == id) {
                Some(row) => row.1 = row.1.max(count),
                None => out.push((id, count)),
            }
        }
    }
    out.into_iter()
        .flat_map(|(id, n)| std::iter::repeat_n(id, n))
        .collect()
}

/// The top skill order as the data writes it ("Q>W>E"). Only the keys Q, W, E, R
/// and the arrow pass, so no foreign text reaches the title of a block.
fn kit_skills(v: &Value) -> String {
    let order = arr(v)
        .next()
        .and_then(|s| s["order"].as_str())
        .unwrap_or("");
    let keys: Vec<&str> = order.split('>').collect();
    let ok =
        (2..=4).contains(&keys.len()) && keys.iter().all(|k| matches!(*k, "Q" | "W" | "E" | "R"));
    if ok { order.to_string() } else { String::new() }
}

/// The spell pairs of the key `summoners`: `[{"rank":1,"spells":[{"id":4},{"id":13}],"sampleCount":8194,...}]`.
/// A row needs two different spell ids and a rank. The win rate and the pick rate
/// of the row are not read: the spell card shows no rate.
fn kit_spells(v: &Value) -> Vec<SpellPair> {
    arr(v)
        .filter_map(|row| {
            let ids: Vec<i64> = arr(&row["spells"])
                .filter_map(|s| s["id"].as_i64())
                .collect();
            let [a, b] = ids[..] else { return None };
            if a <= 0 || b <= 0 || a == b {
                return None;
            }
            Some(SpellPair {
                spells: [a, b],
                rank: row["rank"].as_u64()? as u32,
                games: u(&row["sampleCount"]),
            })
        })
        .collect()
}

/// The spell pair to suggest: the pair that players take most, which is the pair
/// with the most games (the highest pick rate), among the pairs with at least
/// `min_games` games. The rank of the source does not decide: it can put a rare pair
/// first (Heimerdinger: Flash and Clarity at rank 1 with 2.4 % of the picks, Flash and
/// Ghost at rank 2 with 54.7 %). A tie in the games goes to the better rank. `None` when
/// no pair has enough games. It never picks for the player.
pub fn top_spell_pair(pairs: &[SpellPair], min_games: u32) -> Option<[i64; 2]> {
    pairs
        .iter()
        .filter(|p| p.games >= min_games)
        .max_by_key(|p| (p.games, std::cmp::Reverse(p.rank)))
        .map(|p| p.spells)
}

/// The fewest games behind a row of the item lists. The same limit as `min_games`.
const ITEM_MIN_GAMES: u32 = 300;

/// The items that players take as their 1st to 6th finished item (`items.filtered.slots`),
/// the four most picked of each slot. The source ranks a slot by win rate, so a rare item
/// can come first. The pick rate decides here, and a row under `ITEM_MIN_GAMES` is left out.
fn kit_slots(items: &Value) -> [Vec<i64>; 6] {
    std::array::from_fn(|i| {
        let mut rows: Vec<(f64, i64)> = arr(&items["filtered"]["slots"][(i + 1).to_string()])
            .filter(|r| u(&r["sampleCount"]) >= ITEM_MIN_GAMES)
            .filter_map(|r| Some((r["pickRate"].as_f64().unwrap_or(0.0), r["id"].as_i64()?)))
            .collect();
        rows.sort_by(|a, b| b.0.total_cmp(&a.0).then(a.1.cmp(&b.1)));
        rows.into_iter().take(4).map(|r| r.1).collect()
    })
}

/// The items with an `augmentSpecific` row (`items.unfiltered.all`): the augment, and the
/// share of its players who buy the item. A row under `ITEM_MIN_GAMES` is left out.
fn kit_aug_links(items: &Value) -> Vec<AugLink> {
    let mut out = Vec::new();
    for row in arr(&items["unfiltered"]["all"]) {
        let Some(item) = row["id"].as_i64() else {
            continue;
        };
        for l in arr(&row["augmentSpecific"]).filter(|l| u(&l["sampleCount"]) >= ITEM_MIN_GAMES) {
            if let Some(augment) = l["id"].as_i64() {
                out.push(AugLink {
                    item,
                    augment,
                    with_pick: f(&l["pickRate"]),
                });
            }
        }
    }
    out
}

/// Magic damage as a share of physical plus magic damage to champions. True damage does not
/// count. `None` when the champion deals no damage in the data.
fn kit_magic_share(stats: &Value) -> Option<f32> {
    let (phys, magic) = (
        f(&stats["physicalDamageToChampions"]),
        f(&stats["magicDamageToChampions"]),
    );
    (phys + magic > 0.0).then(|| magic / (phys + magic))
}

pub fn kit_champion(d: &Value) -> ChampStats {
    let augs = &d["augments"];
    let mut out = ChampStats {
        augments: arr(&augs["all"]).filter_map(kit_aug).collect(),
        stages: (1..=4)
            .map(|s| {
                arr(&augs["stages"][s.to_string()])
                    .filter_map(kit_aug)
                    .collect()
            })
            .collect(),
        combos: arr(&d["augmentCombinations"])
            .take(20)
            .map(|c| Combo {
                augments: arr(&c["augmentIds"]).filter_map(|x| x.as_i64()).collect(),
                win_rate: f(&c["winRate"]),
                games: u(&c["sampleCount"]),
            })
            .collect(),
        builds: Vec::new(),
        spells: kit_spells(&d["summoners"]),
        slots: kit_slots(&d["items"]),
        aug_links: kit_aug_links(&d["items"]),
        magic_share: kit_magic_share(&d["champion"]["stats"]),
    };
    // Sort by tier, then win rate. Ignore rows with tiny samples at the end.
    let key = |a: &AugStat| (a.games < 300, a.tier);
    out.augments
        .sort_by(|a, b| key(a).cmp(&key(b)).then(b.win_rate.total_cmp(&a.win_rate)));
    for s in &mut out.stages {
        s.sort_by(|a, b| key(a).cmp(&key(b)).then(b.win_rate.total_cmp(&a.win_rate)));
    }
    let skills = kit_skills(&d["skills"]);
    for a in arr(&d["builds"]["filtered"]["archetypes"]).take(3) {
        let Some(p) = arr(&a["profiles"]).next() else {
            continue;
        };
        let route = arr(&p["routes"]).next();
        let core = route
            .map(|r| ids(&r["purchaseOrder"]))
            .unwrap_or_else(|| ids(&p["itemSet"]));
        let sets: Vec<Vec<i64>> = route
            .map(|r| arr(&r["starters"]).map(|s| ids(&s["items"])).collect())
            .unwrap_or_default();
        let starters = sets.first().cloned().unwrap_or_default();
        // The next set that buys other items than the top one (the order of the buys does not matter).
        let starters_alt = sets
            .iter()
            .skip(1)
            .find(|s| !s.is_empty() && !same_items(s, &starters))
            .cloned()
            .unwrap_or_default();
        let boots_ranked = route.map(|r| ranked_ids(&r["boots"])).unwrap_or_default();
        let later = route
            .map(|r| ranked_ids(&r["laterItems"]))
            .unwrap_or_default();
        // The core items of the other profiles, in profile order, for the block "Situational".
        let situational = arr(&a["profiles"])
            .skip(1)
            .take(4)
            .flat_map(|o| ids(&o["itemSet"]))
            .filter(|id| !core.contains(id))
            .collect();
        out.builds.push(Build {
            name: a["key"].as_str().unwrap_or("").replace('_', " "),
            items: core,
            starters,
            win_rate: f(&p["winRate"]),
            starters_alt,
            boots_skipped: boots_ranked.first() == Some(&0),
            boots: boots_ranked
                .into_iter()
                .filter(|&id| id > 0)
                .take(5)
                .collect(),
            later: later.into_iter().filter(|&id| id > 0).take(12).collect(),
            situational,
            consumables: consumables(&sets),
            skills: skills.clone(),
        });
    }
    out
}

pub(crate) fn hex_champion(table: &Value, row: ChampRow) -> Option<ChampStats> {
    let mut augments = Vec::new();
    for (k, v) in table.as_object()? {
        let Ok(id) = k.parse::<i64>() else { continue };
        let games = u(&v["games"]);
        if games == 0 {
            continue;
        }
        let wins = f(&v["wins"]);
        augments.push(AugStat {
            augment_id: id,
            win_rate: f(&v["win_rate"]),
            games,
            tier: tier_from_delta((shrunk(wins, games, row.win_rate) - row.win_rate) * 100.0),
        });
    }
    augments.sort_by(|a, b| a.tier.cmp(&b.tier).then(b.win_rate.total_cmp(&a.win_rate)));
    Some(ChampStats {
        augments,
        ..Default::default()
    })
}

/// Bayesian shrink toward the champion average. Small samples move less.
pub fn shrunk(wins: f32, games: u32, prior: f32) -> f32 {
    const K: f32 = 150.0;
    (wins + K * prior) / (games as f32 + K)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn locale_and_asset_paths() {
        assert_eq!(cdragon_locale("tr_TR"), "tr_tr");
        assert_eq!(cdragon_locale("en_US"), "default");
        assert_eq!(
            asset_url("/lol-game-data/assets/ASSETS/UX/Kiwi/Augments/Icons/X_small.png"),
            format!("{CDRAGON}/default/assets/ux/kiwi/augments/icons/x_small.png")
        );
    }

    #[test]
    fn empty_asset_path_gives_an_empty_url() {
        assert_eq!(asset_url(""), "");
        assert_eq!(asset_url("/lol-game-data/assets/"), "");
    }

    #[test]
    fn parses_aramkit_champion() {
        let d: Value = serde_json::from_str(
            r#"{"champion":{"stats":{"sampleCount":1000,"winRate":0.55}},
            "augments":{"all":[{"id":2,"tier":"A","sampleCount":500,"winRate":0.56,"pickRate":0.1},
                               {"id":1,"tier":"S","sampleCount":400,"winRate":0.6,"pickRate":0.1},
                               {"id":3,"tier":"S","sampleCount":10,"winRate":0.9,"pickRate":0.01}],
                        "stages":{"1":[{"id":1,"tier":"S","sampleCount":400,"winRate":0.6}]}},
            "builds":{"filtered":{"archetypes":[{"key":"on_hit","pickRate":0.5,
                "profiles":[{"winRate":0.6,"itemSet":[{"id":9}],"routes":[{"purchaseOrder":[{"id":7},{"id":8}],
                "starters":[{"items":[{"id":1038}]}]}]}]}]}}}"#,
        )
        .unwrap();
        let c = kit_champion(&d);
        let order: Vec<i64> = c.augments.iter().map(|a| a.augment_id).collect();
        // Tiny sample (id 3) goes last even with tier S.
        assert_eq!(order, vec![1, 2, 3]);
        assert_eq!(c.stages.len(), 4);
        assert_eq!(c.stages[0].len(), 1);
        assert_eq!(c.builds[0].items, vec![7, 8]);
        assert_eq!(c.builds[0].starters, vec![1038]);
        assert_eq!(c.builds[0].name, "on hit");
    }

    #[test]
    fn parses_the_detail_of_a_build() {
        let d: Value = serde_json::from_str(
            r#"{"skills":[{"order":"Q>E>W"},{"order":"W>Q>E"}],
            "builds":{"filtered":{"archetypes":[{"key":"ap","profiles":[
              {"itemSet":[{"id":1},{"id":2}],"routes":[{"purchaseOrder":[{"id":1},{"id":2}],
                "starters":[{"items":[{"id":3802}]},{"items":[{"id":3802}]},{"items":[{"id":2003},{"id":2003},{"id":1056}]},{"items":[{"id":2031}]}],
                "boots":[{"item":{"id":3020}},{"item":{"id":0}},{"item":{"id":3158}}],
                "laterItems":[{"item":{"id":5}},{"item":{"id":0}},{"item":{"id":6}}]}]},
              {"itemSet":[{"id":2},{"id":7},{"id":8}]},
              {"itemSet":[{"id":7},{"id":9}]}]}]}}}"#,
        )
        .unwrap();
        let b = &kit_champion(&d).builds[0];
        assert_eq!(b.skills, "Q>E>W");
        // The second start differs from the first, even when a set repeats it.
        assert_eq!(b.starters, vec![3802]);
        assert_eq!(b.starters_alt, vec![2003, 2003, 1056]);
        // Boots that come first are real boots here, and "none" (0) never lands in the list.
        assert!(!b.boots_skipped);
        assert_eq!(b.boots, vec![3020, 3158]);
        assert_eq!(b.later, vec![5, 6]);
        // The core items of the other profiles that the core lacks (the repeat is cut later).
        assert_eq!(b.situational, vec![7, 8, 7, 9]);
        // Only the top three starting sets count for consumables, and a repeat stays a repeat.
        assert_eq!(b.consumables, vec![2003, 2003]);
        // A skill text that is not Q, W, E, R and arrows is dropped.
        for bad in ["", "Q>W>X", "<b>hi</b>", "Q", "Q>W>E>R>Q"] {
            assert_eq!(
                kit_skills(&serde_json::json!([{ "order": bad }])),
                "",
                "{bad}"
            );
        }
        assert_eq!(
            kit_skills(&serde_json::json!([{ "order": "R>Q>W>E" }])),
            "R>Q>W>E"
        );
    }

    /// The key `summoners` of a champion file, in the shape of the source (Heimerdinger, patch 16.21).
    const SUMMONERS: &str = r#"[{"rank":1,"spells":[{"id":4},{"id":13}],"sampleCount":8194,"pickRate":0.0243,"winRate":0.5138},
        {"rank":2,"spells":[{"id":4},{"id":6}],"sampleCount":184278,"pickRate":0.5471,"winRate":0.5025},
        {"rank":3,"spells":[{"id":4},{"id":32}],"sampleCount":114944,"pickRate":0.3412,"winRate":0.5004},
        {"rank":4,"spells":[{"id":4},{"id":1}],"sampleCount":787,"pickRate":0.0023,"winRate":0.5388}]"#;

    fn pairs_of(text: &str) -> Vec<SpellPair> {
        kit_spells(&serde_json::from_str(text).unwrap())
    }

    #[test]
    fn the_spell_pair_is_the_one_players_take_most_with_enough_games() {
        let pairs = pairs_of(SUMMONERS);
        assert_eq!(pairs.len(), 4);
        assert_eq!(
            pairs[0],
            SpellPair {
                spells: [4, 13],
                rank: 1,
                games: 8194
            }
        );
        // Heimerdinger: rank 1 is Flash and Clarity (2.4 % of the picks), but Flash and Ghost has 54.7 %.
        assert_eq!(top_spell_pair(&pairs, 300), Some([4, 6]));
        // The games of a pair follow its pick rate in the source, so the most games is the highest pick rate.
        let rate = |text: &str| -> Vec<f64> {
            serde_json::from_str::<Value>(text)
                .unwrap()
                .as_array()
                .unwrap()
                .iter()
                .map(|r| r["pickRate"].as_f64().unwrap())
                .collect()
        };
        let best_rate = rate(SUMMONERS)
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map(|(i, _)| i);
        assert_eq!(best_rate, Some(1));
        assert_eq!(
            pairs
                .iter()
                .enumerate()
                .max_by_key(|(_, p)| p.games)
                .map(|(i, _)| i),
            best_rate
        );
        // The pair needs the minimum games. If the most played pair is too small, no pair counts.
        assert_eq!(top_spell_pair(&pairs, 184_278), Some([4, 6]));
        assert_eq!(top_spell_pair(&pairs, 184_279), None);
        assert_eq!(top_spell_pair(&[], 300), None);
        // The order of the list does not matter, and a tie in the games goes to the better rank.
        let mut shuffled = pairs.clone();
        shuffled.reverse();
        assert_eq!(top_spell_pair(&shuffled, 300), Some([4, 6]));
        let tie = [
            SpellPair {
                spells: [4, 1],
                rank: 3,
                games: 900,
            },
            SpellPair {
                spells: [4, 6],
                rank: 2,
                games: 900,
            },
        ];
        assert_eq!(top_spell_pair(&tie, 300), Some([4, 6]));
        // A small pair at rank 1 does not beat a played pair at a worse rank, in any order.
        let rare = [
            SpellPair {
                spells: [4, 13],
                rank: 1,
                games: 400,
            },
            SpellPair {
                spells: [4, 32],
                rank: 9,
                games: 50_000,
            },
        ];
        assert_eq!(top_spell_pair(&rare, 300), Some([4, 32]));
        assert_eq!(top_spell_pair(&[rare[1], rare[0]], 300), Some([4, 32]));
    }

    #[test]
    fn a_bad_spell_row_is_dropped_and_the_pair_keeps_no_rate() {
        let pairs = pairs_of(
            r#"[{"rank":1,"spells":[{"id":4}],"sampleCount":900},
                {"rank":2,"spells":[{"id":4},{"id":4}],"sampleCount":900},
                {"rank":3,"spells":[{"id":0},{"id":6}],"sampleCount":900},
                {"rank":4,"spells":[{"id":4},{"id":6},{"id":7}],"sampleCount":900},
                {"spells":[{"id":4},{"id":6}],"sampleCount":900},
                {"rank":6,"spells":[{"id":4},{"id":6}],"sampleCount":900,"winRate":0.9}, "x"]"#,
        );
        assert_eq!(
            pairs,
            vec![SpellPair {
                spells: [4, 6],
                rank: 6,
                games: 900
            }]
        );
        // No `summoners` key (the fallback source, or an old file) gives no pair and no card.
        assert!(kit_champion(&serde_json::json!({})).spells.is_empty());
        assert!(kit_spells(&Value::Null).is_empty());
        // The file of a champion carries the pairs through `kit_champion`.
        let d: Value = serde_json::from_str(&format!(r#"{{"summoners":{SUMMONERS}}}"#)).unwrap();
        assert_eq!(top_spell_pair(&kit_champion(&d).spells, 300), Some([4, 6]));
    }

    #[test]
    fn shrink_moves_small_samples_less() {
        let small = shrunk(9.0, 10, 0.5);
        let big = shrunk(900.0, 1000, 0.5);
        assert!(small < big);
    }

    // ---- Why a champion can end in `failed` (the "no data" text and the "?" badges).

    fn champ(id: i64, name: &str, alias: &str) -> Champion {
        Champion {
            id,
            name: name.into(),
            alias: alias.into(),
            icon: String::new(),
        }
    }

    /// The old lookup of `read_live` in worker.rs, word for word: the alias or the shown name.
    fn old_live_lookup(champions: &HashMap<i64, Champion>, alias: &str, name: &str) -> Option<i64> {
        champions
            .values()
            .find(|c| c.alias.eq_ignore_ascii_case(alias) || c.name == name)
            .map(|c| c.id)
    }

    /// Real champions and their copies for another mode, in a new map (a new order).
    fn with_copies() -> HashMap<i64, Champion> {
        let all = [
            champ(74, "Heimerdinger", "Heimerdinger"),
            champ(60074, "Heimerdinger", "Jade_Heimerdinger"),
            champ(6, "Urgot", "Urgot"),
            champ(60045, "Veigar", "Jade_Veigar"),
            champ(45, "Veigar", "Veigar"),
        ];
        all.into_iter().map(|c| (c.id, c)).collect()
    }

    #[test]
    fn the_shown_name_can_pick_the_jade_copy_of_a_champion() {
        // CDragon's champion-summary now lists copies for another mode (ids 60001 and up,
        // alias "Jade_<alias>") with the same shown name. The Live Client gives
        // rawChampionName "game_character_displayname_Heimerdinger" and championName
        // "Heimerdinger". Each HashMap has its own order, so the old name match found the
        // copy in some runs of the app. aramkit has no file for 60074: "no data".
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let m = with_copies();
            seen.insert(old_live_lookup(&m, "Heimerdinger", "Heimerdinger"));
            // The new lookup goes by the alias, so it is never wrong.
            assert_eq!(
                live_champion(
                    &m,
                    "game_character_displayname_Heimerdinger",
                    "Heimerdinger"
                ),
                Some(74)
            );
        }
        assert!(seen.contains(&Some(60074)), "{seen:?}");
        assert!(seen.contains(&Some(74)), "{seen:?}");
    }

    #[test]
    fn the_shown_name_is_only_a_fallback_and_takes_the_lowest_id() {
        for _ in 0..50 {
            let m = with_copies();
            // No alias that matches: the shown name decides, and the real champion has the lower id.
            assert_eq!(live_champion(&m, "", "Veigar"), Some(45));
            assert_eq!(
                live_champion(&m, "game_character_displayname_Unknown", "Heimerdinger"),
                Some(74)
            );
            // The alias wins over a shown name of another champion (a localized name can differ).
            assert_eq!(
                live_champion(&m, "game_character_displayname_Urgot", "Veigar"),
                Some(6)
            );
            // The alias is not case sensitive. Nothing that matches gives nothing.
            assert_eq!(live_champion(&m, "URGOT", ""), Some(6));
            assert_eq!(live_champion(&m, "", ""), None);
            assert_eq!(
                live_champion(&m, "game_character_displayname_Nobody", "Nobody"),
                None
            );
        }
    }

    #[test]
    fn the_summary_keeps_the_real_champions_and_drops_the_copies() {
        let summary: Value = serde_json::from_str(
            r#"[{"id":-1,"name":"None","alias":"None","squarePortraitPath":""},
                {"id":74,"name":"Heimerdinger","alias":"Heimerdinger","squarePortraitPath":"/lol-game-data/assets/v1/champion-icons/74.png"},
                {"id":62,"name":"Wukong","alias":"MonkeyKing","squarePortraitPath":""},
                {"id":60074,"name":"Heimerdinger","alias":"Jade_Heimerdinger","squarePortraitPath":"/lol-game-data/assets/v1/champion-icons/60074.png"}]"#,
        )
        .unwrap();
        let m = parse_champions(&summary);
        let mut ids: Vec<i64> = m.keys().copied().collect();
        ids.sort();
        assert_eq!(ids, vec![62, 74]);
        assert_eq!(
            m[&74].icon,
            format!("{CDRAGON}/default/v1/champion-icons/74.png")
        );
        assert_eq!(m[&62].alias, "MonkeyKing");
        // Even the old lookup finds the real champion now.
        assert_eq!(
            old_live_lookup(&m, "Heimerdinger", "Heimerdinger"),
            Some(74)
        );
    }

    #[test]
    fn a_champion_that_the_fallback_table_lacks_gets_no_stats_without_the_network() {
        // The aramhex fallback keeps all champions in one table. A champion that is
        // not in it (a new champion, or a wrong id such as 60074) is missing at once,
        // and `fetch_loop` does not ask again until the next stats refresh (3 hours).
        let table: Value =
            serde_json::from_str(r#"{"74":{"3036":{"games":500,"wins":300,"win_rate":0.6}}}"#)
                .unwrap();
        let hex: HashMap<i64, Value> = table
            .as_object()
            .unwrap()
            .iter()
            .map(|(k, v)| (k.parse().unwrap(), v.clone()))
            .collect();
        let st = Stats {
            patch: "16.21".into(),
            matches: 1,
            source: "aramhex",
            champions: HashMap::new(),
            data_path: String::new(),
            hex: Some(hex),
        };
        let agent = cache::web_agent();
        assert_eq!(st.champion(&agent, 60074).err(), Some(NoStats::Missing));
        let heimer = st.champion(&agent, 74).unwrap();
        assert_eq!(
            heimer
                .augments
                .iter()
                .map(|a| a.augment_id)
                .collect::<Vec<_>>(),
            vec![3036]
        );
    }

    #[test]
    fn only_a_file_that_does_not_exist_is_final() {
        use cache::Fail;
        // The server says the file does not exist, for a champion the index does not list.
        for code in [403, 404, 410] {
            assert_eq!(
                no_stats(Fail::Status(code), false),
                NoStats::Missing,
                "{code}"
            );
        }
        // The network, a server error, or a bad file: try again, listed or not.
        for why in [
            Fail::Network,
            Fail::Timeout,
            Fail::Body,
            Fail::Invalid,
            Fail::Other,
            Fail::Status(500),
            Fail::Status(429),
        ] {
            assert_eq!(no_stats(why, true), NoStats::Transient, "{why:?}");
            assert_eq!(no_stats(why, false), NoStats::Transient, "{why:?}");
        }
    }

    #[test]
    fn a_missing_file_of_a_listed_champion_is_tried_again() {
        use cache::Fail;
        // A new index can list a champion before its file is on the server. A
        // final mark would keep "no data" until the next index (3 hours), so
        // Hexsight tries again after the pause.
        for code in [403, 404, 410] {
            assert_eq!(
                no_stats(Fail::Status(code), true),
                NoStats::Transient,
                "{code}"
            );
        }
    }

    /// Checks copies of the owner's cache files from the Heimerdinger games. Run with
    /// HEXSIGHT_SUMMARY_COPY (champion-summary.json, tr_tr) and HEXSIGHT_DETAILS_COPY
    /// (aramkit champion-details/74.json) and `--ignored`. Never point them at the real cache.
    #[test]
    #[ignore]
    fn the_files_of_the_heimerdinger_games() {
        let (Some(summary), Some(details)) = (
            std::env::var_os("HEXSIGHT_SUMMARY_COPY"),
            std::env::var_os("HEXSIGHT_DETAILS_COPY"),
        ) else {
            panic!("set HEXSIGHT_SUMMARY_COPY and HEXSIGHT_DETAILS_COPY");
        };
        // The details file parses as the app parses it, with the three cards of the game.
        let d: Value = serde_json::from_slice(&std::fs::read(details).unwrap()).unwrap();
        let cs = kit_champion(&d);
        for id in [3036, 1322, 3078] {
            assert!(cs.get(id).is_some(), "{id} in augments.all");
            assert!(
                cs.stages[0]
                    .iter()
                    .chain(&cs.stages[1])
                    .any(|a| a.augment_id == id),
                "{id} in stage 1 or 2"
            );
        }
        // The summary has two champions with the shown name "Heimerdinger".
        let s: Value = serde_json::from_slice(&std::fs::read(summary).unwrap()).unwrap();
        let mut all = HashMap::new();
        for c in arr(&s) {
            let id = c["id"].as_i64().unwrap_or(-1);
            if id > 0 {
                all.insert(
                    id,
                    champ(
                        id,
                        c["name"].as_str().unwrap_or(""),
                        c["alias"].as_str().unwrap_or(""),
                    ),
                );
            }
        }
        let mut heimer: Vec<i64> = all
            .values()
            .filter(|c| c.name == "Heimerdinger")
            .map(|c| c.id)
            .collect();
        heimer.sort();
        assert_eq!(heimer, vec![74, 60074]);
        let copies = all
            .values()
            .filter(|c| c.alias.starts_with("Jade_"))
            .count();
        assert!(copies > 50, "{copies} Jade copies");
        // The map that the app builds now has no copy, and both lookups give 74.
        let champions = parse_champions(&s);
        assert!(champions.keys().all(|&id| id < 60000), "a copy stayed");
        assert_eq!(champions.len(), all.len() - copies);
        assert_eq!(
            live_champion(
                &champions,
                "game_character_displayname_Heimerdinger",
                "Heimerdinger"
            ),
            Some(74)
        );
        assert_eq!(
            old_live_lookup(&champions, "Heimerdinger", "Heimerdinger"),
            Some(74)
        );
    }

    // ---- The data of the item suggestions.

    /// The real Heimerdinger file of aramkit (patch 16.21), cut to the parts the suggestions read.
    const HEIMER_SUGGEST: &str = include_str!("../tests/fixtures/heimerdinger-suggest.json");

    /// The real `items.json` of CommunityDragon (patch 16.21), cut to the items the tests need.
    const ITEMS_16_21: &str = include_str!("../tests/fixtures/items-trimmed.json");

    #[test]
    fn the_slots_hold_the_four_most_picked_items_of_each_slot_and_ignore_small_samples() {
        let d: Value = serde_json::from_str(
            r#"{"items":{"filtered":{"slots":{
                "1":[{"id":1,"sampleCount":900,"pickRate":0.10},{"id":2,"sampleCount":9000,"pickRate":0.60},
                     {"id":3,"sampleCount":50,"pickRate":0.90},{"id":4,"sampleCount":900,"pickRate":0.10},
                     {"id":5,"sampleCount":900,"pickRate":0.05},{"id":6,"sampleCount":900,"pickRate":0.04},{"id":7,"sampleCount":900,"pickRate":0.03}],
                "3":[{"id":9,"sampleCount":400,"pickRate":0.5}]}}}}"#,
        )
        .unwrap();
        let slots = kit_champion(&d).slots;
        // The pick rate decides, not the order of the file. A tie goes to the lower id. A small sample is out.
        assert_eq!(slots[0], vec![2, 1, 4, 5]);
        assert_eq!(slots[2], vec![9]);
        assert!(slots[1].is_empty() && slots[5].is_empty());
        // The real file: the item that most players take first is Blackfire Torch (2503), not the rank 1 row (3089).
        let real = kit_champion(&serde_json::from_str(HEIMER_SUGGEST).unwrap());
        assert_eq!(real.slots[0], vec![2503, 6653, 3118, 3116]);
        assert!(real.slots.iter().all(|s| s.len() == 4));
    }

    #[test]
    fn an_augment_link_keeps_the_item_the_augment_and_the_share_of_players() {
        let real = kit_champion(&serde_json::from_str(HEIMER_SUGGEST).unwrap());
        // Seven items of the file have an `augmentSpecific` row.
        assert_eq!(real.aug_links.len(), 7);
        let storm = real.aug_links.iter().find(|l| l.item == 4646).unwrap();
        assert_eq!((storm.augment, storm.with_pick), (3150, 0.8033));
        assert!(storm.strong());
        // Few players buy these items with their augment, so the links are weak. The win rate
        // change of the pair (`synergyDelta`) is not read, whatever its sign.
        let botrk = real.aug_links.iter().find(|l| l.item == 3153).unwrap();
        assert!(!botrk.strong());
        let lich = real.aug_links.iter().find(|l| l.item == 3100).unwrap();
        assert!(!lich.strong());
        // A row with a small sample is left out. A file without the key has no link.
        let d: Value = serde_json::from_str(
            r#"{"items":{"unfiltered":{"all":[{"id":5,"augmentSpecific":[{"id":8,"sampleCount":299,"pickRate":0.9,"synergyDelta":0.1},{"id":9,"sampleCount":300,"pickRate":0.6,"synergyDelta":-0.9}]},{"id":6}]}}}"#,
        )
        .unwrap();
        assert_eq!(
            kit_champion(&d).aug_links,
            vec![AugLink {
                item: 5,
                augment: 9,
                with_pick: 0.6
            }]
        );
        assert!(kit_champion(&serde_json::json!({})).aug_links.is_empty());
    }

    #[test]
    fn the_magic_share_comes_from_the_damage_split_and_is_none_without_damage() {
        let real = kit_champion(&serde_json::from_str(HEIMER_SUGGEST).unwrap());
        // 39,025 magic against 678 physical damage. True damage does not count.
        let share = real.magic_share.unwrap();
        assert!(
            (share - 39_025.0 / (39_025.0 + 678.0)).abs() < 1e-6,
            "{share}"
        );
        assert_eq!(
            kit_magic_share(
                &serde_json::json!({ "physicalDamageToChampions": 100, "magicDamageToChampions": 300, "trueDamageToChampions": 900 })
            ),
            Some(0.75)
        );
        assert_eq!(
            kit_magic_share(
                &serde_json::json!({ "physicalDamageToChampions": 0, "magicDamageToChampions": 0, "trueDamageToChampions": 50 })
            ),
            None
        );
        assert_eq!(kit_magic_share(&Value::Null), None);
        assert_eq!(kit_champion(&serde_json::json!({})).magic_share, None);
        // The fallback source has none of the three new parts.
        let hex = hex_champion(
            &serde_json::json!({ "3036": { "games": 500, "wins": 300, "win_rate": 0.6 } }),
            ChampRow::default(),
        )
        .unwrap();
        assert!(
            hex.slots.iter().all(Vec::is_empty)
                && hex.aug_links.is_empty()
                && hex.magic_share.is_none()
        );
    }

    #[test]
    fn the_item_list_keeps_price_recipe_and_category_bits() {
        let (items, info) = parse_items(&serde_json::from_str(ITEMS_16_21).unwrap());
        let rabadon = &info[&3089];
        assert_eq!(
            (
                rabadon.total,
                rabadon.from.clone(),
                rabadon.cats,
                rabadon.in_store
            ),
            (3500, vec![1058, 1058], cat::AP, true)
        );
        assert_eq!(items[&3089].name, "Rabadon's Deathcap");
        // Boots, magic resist and the tag count.
        assert_eq!(info[&3111].cats, cat::BOOTS | cat::MR);
        assert_eq!(info[&3111].tags, 3);
        // Lifesteal, spell vamp and armor penetration land on their bits. Omnivamp has its own word in the data.
        assert!(info[&3072].cats & cat::LIFESTEAL != 0 && info[&3072].cats & cat::AD != 0);
        assert!(info[&3071].cats & cat::ARMOR_PEN != 0);
        assert!(info[&3135].cats & cat::MAGIC_PEN != 0 && info[&3135].cats & cat::AP != 0);
        // A trinket and a potion.
        assert!(info[&3340].cats & cat::TRINKET != 0);
        assert!(info[&2003].cats & cat::CONSUMABLE != 0);
        // Words that no rule needs set no bit, and a missing field gives a plain entry.
        assert_eq!(cat::bit("Mana"), 0);
        let (_, odd) = parse_items(&serde_json::json!([{ "id": 7 }, { "name": "no id" }]));
        assert_eq!(odd[&7], ItemInfo::default());
        assert_eq!(odd.len(), 1);
        // The spell list is read as before, with name and icon only.
        let spells = serde_json::json!([{ "id": 4, "name": "Flash", "iconPath": "/lol-game-data/assets/DATA/Spells/Icons2D/Summoner_flash.png" }]);
        assert_eq!(parse_items(&spells).0[&4].name, "Flash");
    }

    #[test]
    fn the_champion_summary_keeps_the_roles() {
        let summary: Value = serde_json::from_str(
            r#"[{"id":-1,"alias":"None","roles":[]},
                {"id":74,"alias":"Heimerdinger","roles":["Mage","support"]},
                {"id":62,"alias":"MonkeyKing","roles":[]},
                {"id":60074,"alias":"Jade_Heimerdinger","roles":["mage"]},
                {"id":6,"alias":"Urgot"}]"#,
        )
        .unwrap();
        let roles = parse_roles(&summary);
        // Lower case, in the order of the file. No roles, no entry. The copies for another mode stay out.
        assert_eq!(roles[&74], vec!["mage".to_string(), "support".to_string()]);
        assert_eq!(roles.len(), 1);
    }
}
