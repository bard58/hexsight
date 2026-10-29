//! Write the builds of a champion as item sets the in-game shop shows.
//! One file for each build: <League>\Config\Global\Recommended\Hexsight-0.json,
//! Hexsight-1.json ... The game reads every .json file of that folder, so the sets
//! sit beside the ones of other apps. We touch our own files only.
//! The shop shows the names of the blocks, so they follow the interface language.
//! A set holds the start (with the skill order), the core in buying order, boots,
//! later items, situational items and consumables. It shows no win rate or pick rate.

use crate::i18n::{Key, Lang};
use crate::model::{Build, ChampStats, StaticData};
use serde_json::json;
use std::path::{Path, PathBuf};

/// The start of the name of every file we write.
const SET_PREFIX: &str = "Hexsight";

/// The name of the file for the build at `index`.
fn file_name(index: usize) -> String {
    format!("{SET_PREFIX}-{index}.json")
}

/// The name this app had before it became Hexsight. Its item set files
/// (Augur.json, Augur-<n>.json) are still ours to delete. Legacy name, keep it.
const LEGACY_NAME: &str = "Augur";

/// True for the files that we wrote: "Hexsight.json", "Hexsight-<n>.json", and the
/// same two names under the legacy name. No other file matches, so a set of
/// another app stays.
fn is_ours(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    let Some(stem) = name.strip_suffix(".json") else {
        return false;
    };
    let new = SET_PREFIX.to_ascii_lowercase();
    let legacy = LEGACY_NAME.to_ascii_lowercase();
    [new, legacy].iter().any(|prefix| {
        stem == prefix.as_str()
            || stem
                .strip_prefix(prefix.as_str())
                .and_then(|rest| rest.strip_prefix('-'))
                .is_some_and(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
    })
}

/// How many items the blocks hold at most.
const MAX_BOOTS: usize = 3;
const MAX_LATER: usize = 6;
const MAX_SITUATIONAL: usize = 6;

/// The ids once each, in the order of first sight.
fn unique(ids: impl IntoIterator<Item = i64>) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    for id in ids {
        if !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// The item rows of a block. A repeated id is one row with a higher count, at
/// the place of its first sight ("two potions").
fn rows(ids: &[i64]) -> Vec<(i64, u32)> {
    let mut out: Vec<(i64, u32)> = Vec::new();
    for &id in ids {
        match out.iter_mut().find(|(i, _)| *i == id) {
            Some(row) => row.1 += 1,
            None => out.push((id, 1)),
        }
    }
    out
}

/// One block of the set, or nothing when it holds no item.
fn block(title: String, items: Vec<(i64, u32)>) -> Option<serde_json::Value> {
    let items: Vec<serde_json::Value> = items
        .into_iter()
        .map(|(id, count)| json!({"id": id.to_string(), "count": count}))
        .collect();
    (!items.is_empty()).then(|| json!({"type": title, "items": items}))
}

/// The item set of one build. `index` is the place of the build in the data
/// (most picked first), and the place of the set in the shop. The blocks come in
/// the order of play: start, a second start, the core in buying order, boots,
/// later items, situational items, consumables. A block with no item is left
/// out, and so is an item that the item list does not know. The titles carry no
/// win rate and no pick rate, so `_hide_rates` (the setting "Hide win rates")
/// changes nothing here.
pub fn set_json(
    lang: Lang,
    champion_id: i64,
    index: usize,
    build: &Build,
    sd: &StaticData,
    _hide_rates: bool,
) -> serde_json::Value {
    let name = sd
        .champions
        .get(&champion_id)
        .map(|c| c.name.as_str())
        .unwrap_or(lang.t(Key::ItemChampion));
    let key = sd
        .champions
        .get(&champion_id)
        .map(|c| c.alias.as_str())
        .unwrap_or("");
    let style = lang.style(&build.name);
    // An empty item list means the list did not load: then no id is dropped.
    let known = |ids: &[i64]| -> Vec<i64> {
        ids.iter()
            .copied()
            .filter(|id| sd.items.is_empty() || sd.items.contains_key(id))
            .collect()
    };
    let core = known(&build.items);
    let later: Vec<i64> = unique(known(&build.later))
        .into_iter()
        .take(MAX_LATER)
        .collect();
    // Situational: the core items of the other profiles, then the later items that did not
    // fit the later block. Nothing that the core or the later block already shows.
    let pool: Vec<i64> = build
        .situational
        .iter()
        .chain(&build.later)
        .copied()
        .collect();
    let situational: Vec<i64> = unique(known(&pool))
        .into_iter()
        .filter(|id| !core.contains(id) && !later.contains(id))
        .take(MAX_SITUATIONAL)
        .collect();
    let skills = build.skills.replace('>', " > ");
    let start = lang.t(Key::ItemStart);
    let start_title = if skills.is_empty() {
        start.to_string()
    } else {
        lang.fmt(Key::ItemStartSkills, &[&start, &skills])
    };
    let start_ids = known(&build.starters);
    let mut alt_ids = known(&build.starters_alt);
    let (mut a, mut b) = (alt_ids.clone(), start_ids.clone());
    a.sort_unstable();
    b.sort_unstable();
    if a == b {
        alt_ids.clear();
    }
    let boots_title = lang.t(if build.boots_skipped {
        Key::ItemBootsSkip
    } else {
        Key::ItemBoots
    });
    let blocks: Vec<serde_json::Value> = [
        block(start_title, rows(&start_ids)),
        block(lang.t(Key::ItemStartOther).into(), rows(&alt_ids)),
        block(lang.t(Key::ItemCore).into(), rows(&core)),
        block(
            boots_title.into(),
            rows(
                &unique(known(&build.boots))
                    .into_iter()
                    .take(MAX_BOOTS)
                    .collect::<Vec<_>>(),
            ),
        ),
        block(lang.t(Key::ItemLater).into(), rows(&later)),
        block(lang.t(Key::ItemSituational).into(), rows(&situational)),
        block(
            lang.t(Key::ItemConsumables).into(),
            rows(&known(&build.consumables)),
        ),
    ]
    .into_iter()
    .flatten()
    .collect();
    json!({
        "title": format!("Hexsight - {name} {style}").trim_end().to_string(),
        "associatedMaps": [12],
        "associatedChampions": [champion_id],
        "champion": key,
        "map": "any",
        "mode": "any",
        "type": "custom",
        "priority": true,
        "sortrank": index,
        "blocks": blocks,
    })
}

/// All sets of a champion: the file name and the content, in data order.
pub fn sets(
    lang: Lang,
    champion_id: i64,
    cs: &ChampStats,
    sd: &StaticData,
    hide_rates: bool,
) -> Vec<(String, serde_json::Value)> {
    cs.builds
        .iter()
        .enumerate()
        .map(|(i, b)| {
            (
                file_name(i),
                set_json(lang, champion_id, i, b, sd, hide_rates),
            )
        })
        .collect()
}

/// The folder of the item sets in the League folder.
fn recommended_dir(league_dir: &Path) -> PathBuf {
    league_dir.join("Config").join("Global").join("Recommended")
}

/// Delete our files in `dir` except the names in `keep`. Other files stay. A
/// file that cannot go (locked or read-only) does not stop the others. The
/// result is the first error.
fn remove_ours(dir: &Path, keep: &[String]) -> Result<(), String> {
    let entries = match std::fs::read_dir(dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(e) => return Err(e.to_string()),
    };
    let mut first_error = None;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().into_owned();
        let kept = keep.iter().any(|k| k.eq_ignore_ascii_case(&name));
        if is_ours(&name)
            && !kept
            && entry.path().is_file()
            && let Err(e) = std::fs::remove_file(entry.path())
        {
            first_error.get_or_insert_with(|| e.to_string());
        }
    }
    first_error.map_or(Ok(()), Err)
}

/// Write one set for each build of the champion and delete the older files of
/// Hexsight (and of the legacy name) that this write does not replace. Returns the style of each set in the
/// language of the interface, in data order ("Tank", "AP"), for the message
/// "Written to the shop: Tank, AP".
pub fn write(
    lang: Lang,
    league_dir: &Path,
    champion_id: i64,
    cs: &ChampStats,
    sd: &StaticData,
    hide_rates: bool,
) -> Result<Vec<String>, String> {
    if cs.builds.is_empty() {
        return Err(lang.t(Key::ErrNoBuild).into());
    }
    let dir = recommended_dir(league_dir);
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    let sets = sets(lang, champion_id, cs, sd, hide_rates);
    for (name, set) in &sets {
        let body = serde_json::to_vec_pretty(set).map_err(|e| e.to_string())?;
        std::fs::write(dir.join(name), body).map_err(|e| e.to_string())?;
    }
    let names: Vec<String> = sets.into_iter().map(|(name, _)| name).collect();
    // The new sets are in the shop. An old file that stays (locked) is no reason to report a failure.
    let _ = remove_ours(&dir, &names);
    Ok(cs.builds.iter().map(|b| lang.style(&b.name)).collect())
}

/// Delete every item set of Hexsight (and of the legacy name). It runs when the setting "Send the build to
/// the in-game shop" is off.
pub fn clear(league_dir: &Path) -> Result<(), String> {
    remove_ours(&recommended_dir(league_dir), &[])
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{Build, Champion};

    fn build(name: &str, items: &[i64], starters: &[i64], win_rate: f32) -> Build {
        Build {
            name: name.into(),
            items: items.to_vec(),
            starters: starters.to_vec(),
            win_rate,
            ..Default::default()
        }
    }

    /// Galio: a tank build (the most picked), an AP build, and a build with a style we do not know.
    fn sample() -> (StaticData, ChampStats) {
        let mut sd = StaticData::default();
        sd.champions.insert(
            3,
            Champion {
                id: 3,
                name: "Galio".into(),
                alias: "Galio".into(),
                icon: String::new(),
            },
        );
        let cs = ChampStats {
            builds: vec![
                build("tank", &[3068, 3075, 3110], &[1054, 2003], 0.6),
                build("ap", &[3020, 3157], &[1056], 0.55),
                build("mage", &[3089], &[], 0.5),
            ],
            ..Default::default()
        };
        (sd, cs)
    }

    /// A new empty folder as the League folder. The test removes it at the end.
    struct League(PathBuf);

    impl League {
        fn new(tag: &str) -> League {
            let dir = std::env::temp_dir().join(format!(
                "hexsight-test-itemset-{tag}-{}",
                std::process::id()
            ));
            let _ = std::fs::remove_dir_all(&dir);
            std::fs::create_dir_all(dir.join("Config").join("Global").join("Recommended")).unwrap();
            League(dir)
        }
        fn shop(&self) -> PathBuf {
            recommended_dir(&self.0)
        }
        fn put(&self, name: &str) {
            std::fs::write(self.shop().join(name), "{}").unwrap();
        }
        fn files(&self) -> Vec<String> {
            let mut names: Vec<String> = std::fs::read_dir(self.shop())
                .unwrap()
                .flatten()
                .map(|e| e.file_name().to_string_lossy().into_owned())
                .collect();
            names.sort();
            names
        }
        fn read(&self, name: &str) -> serde_json::Value {
            serde_json::from_slice(&std::fs::read(self.shop().join(name)).unwrap()).unwrap()
        }
    }

    impl Drop for League {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn one_file_for_each_build_in_data_order() {
        let (sd, cs) = sample();
        let league = League::new("files");
        let styles = write(Lang::En, &league.0, 3, &cs, &sd, false).unwrap();
        assert_eq!(styles, ["Tank", "AP", "Mage"]);
        assert_eq!(
            league.files(),
            ["Hexsight-0.json", "Hexsight-1.json", "Hexsight-2.json"]
        );
        for (i, name) in ["Hexsight-0.json", "Hexsight-1.json", "Hexsight-2.json"]
            .iter()
            .enumerate()
        {
            let v = league.read(name);
            assert_eq!(v["sortrank"], i);
            assert_eq!(v["associatedChampions"][0], 3);
            assert_eq!(v["associatedMaps"][0], 12);
            assert_eq!(v["champion"], "Galio");
        }
    }

    #[test]
    fn the_title_has_the_champion_and_the_style() {
        let (sd, cs) = sample();
        let titles = |lang| {
            sets(lang, 3, &cs, &sd, false)
                .iter()
                .map(|(_, v)| v["title"].as_str().unwrap().to_string())
                .collect::<Vec<_>>()
        };
        assert_eq!(
            titles(Lang::En),
            [
                "Hexsight - Galio Tank",
                "Hexsight - Galio AP",
                "Hexsight - Galio Mage"
            ]
        );
        // A champion that is not in the data gets the generic name of the language.
        let empty = StaticData::default();
        assert_eq!(
            set_json(Lang::Tr, 1, 0, &cs.builds[0], &empty, false)["title"],
            "Hexsight - Şampiyon Tank"
        );
        assert_eq!(
            set_json(Lang::En, 1, 0, &cs.builds[0], &empty, false)["title"],
            "Hexsight - Champion Tank"
        );
        // A build with no name has no trailing space in the title.
        assert_eq!(
            set_json(Lang::En, 3, 0, &build("", &[1], &[], 0.5), &sd, false)["title"],
            "Hexsight - Galio"
        );
    }

    #[test]
    fn a_set_has_a_start_block_and_a_core_block() {
        let (sd, cs) = sample();
        let all = sets(Lang::En, 3, &cs, &sd, false);
        let v = &all[0].1;
        assert_eq!(v["blocks"].as_array().unwrap().len(), 2);
        assert_eq!(v["blocks"][0]["type"], "Start");
        assert_eq!(v["blocks"][0]["items"][0]["id"], "1054");
        assert_eq!(v["blocks"][0]["items"][1]["id"], "2003");
        // The core block has no win rate in its title.
        assert_eq!(v["blocks"][1]["type"], "Core build (in order)");
        // The items keep the order of purchase.
        let ids: Vec<&str> = v["blocks"][1]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap())
            .collect();
        assert_eq!(ids, ["3068", "3075", "3110"]);
        assert_eq!(v["blocks"][1]["items"][0]["count"], 1);
        // The starters of a set are the starters of its own build.
        assert_eq!(all[1].1["blocks"][0]["items"][0]["id"], "1056");
        // A build with no starters has the core block only.
        assert_eq!(all[2].1["blocks"].as_array().unwrap().len(), 1);
        assert_eq!(all[2].1["blocks"][0]["items"][0]["id"], "3089");
    }

    #[test]
    fn the_shop_shows_the_blocks_in_the_interface_language() {
        let (sd, cs) = sample();
        let tr = sets(Lang::Tr, 3, &cs, &sd, false);
        let v = &tr[0].1;
        assert_eq!(v["blocks"][0]["type"], "Başlangıç");
        assert_eq!(v["blocks"][1]["type"], "Çekirdek dizilim (sırayla)");
        assert_eq!(v["title"], "Hexsight - Galio Tank");
        // An unknown style shows as written in both languages.
        assert_eq!(tr[2].1["title"], "Hexsight - Galio Mage");
        // The items and the champion do not depend on the language.
        assert_eq!(v["blocks"][1]["items"][1]["id"], "3075");
        assert_eq!(v["associatedChampions"][0], 3);
        let styles = write(Lang::Tr, &League::new("tr").0, 3, &cs, &sd, false).unwrap();
        assert_eq!(styles, ["Tank", "AP", "Mage"]);
        // The labels of the styles.
        let mut other = cs.clone();
        other.builds = vec![
            build("bruiser", &[1], &[], 0.5),
            build("on hit", &[2], &[], 0.5),
        ];
        assert_eq!(
            sets(Lang::Tr, 3, &other, &sd, false)[0].1["title"],
            "Hexsight - Galio Dövüşçü"
        );
        assert_eq!(
            sets(Lang::Tr, 3, &other, &sd, false)[1].1["title"],
            "Hexsight - Galio İsabet etkisi"
        );
        assert_eq!(
            sets(Lang::En, 3, &other, &sd, false)[0].1["title"],
            "Hexsight - Galio Bruiser"
        );
        assert_eq!(
            sets(Lang::En, 3, &other, &sd, false)[1].1["title"],
            "Hexsight - Galio On-hit"
        );
    }

    #[test]
    fn the_titles_carry_no_rate_with_any_setting() {
        let (sd, cs) = sample();
        for lang in [Lang::En, Lang::Tr] {
            for hide in [false, true] {
                let league = League::new("hide");
                write(lang, &league.0, 3, &cs, &sd, hide).unwrap();
                for name in league.files() {
                    let v = league.read(&name);
                    // Titles and block names only: the ids of the items are numbers by nature.
                    let mut text = v["title"].as_str().unwrap().to_string();
                    for b in v["blocks"].as_array().unwrap() {
                        text += b["type"].as_str().unwrap();
                    }
                    assert!(
                        !text.contains('%') && !text.chars().any(|c| c.is_ascii_digit()),
                        "{lang:?} {name}: {text}"
                    );
                    assert_eq!(
                        v["blocks"].as_array().unwrap().last().unwrap()["type"],
                        lang.t(Key::ItemCore)
                    );
                }
                // The rest of the set does not change.
                assert_eq!(
                    league.read("Hexsight-0.json")["blocks"][0]["items"][0]["id"],
                    "1054"
                );
                assert_eq!(
                    league.read("Hexsight-0.json")["blocks"][1]["items"][1]["id"],
                    "3075"
                );
            }
        }
    }

    /// The builds, the skill order and the items of the real Heimerdinger file (aramkit
    /// champion-details 74, 16.21), cut to the part that the item sets read.
    const HEIMER: &str = include_str!("../tests/fixtures/heimerdinger-builds.json");

    /// An item list that knows every id of the file except `unknown`.
    fn heimer_data(unknown: &[i64]) -> (StaticData, ChampStats) {
        let d: serde_json::Value = serde_json::from_str(HEIMER).unwrap();
        let cs = crate::data::kit_champion(&d);
        let mut sd = StaticData::default();
        sd.champions.insert(
            74,
            Champion {
                id: 74,
                name: "Heimerdinger".into(),
                alias: "Heimerdinger".into(),
                icon: String::new(),
            },
        );
        let mut ids = Vec::new();
        for b in &cs.builds {
            for list in [
                &b.items,
                &b.starters,
                &b.starters_alt,
                &b.boots,
                &b.later,
                &b.situational,
                &b.consumables,
            ] {
                ids.extend(list.iter().copied());
            }
        }
        for id in ids.into_iter().filter(|id| !unknown.contains(id)) {
            sd.items.insert(
                id,
                crate::model::Item {
                    name: format!("Item {id}"),
                    icon: String::new(),
                },
            );
        }
        (sd, cs)
    }

    fn block_ids(v: &serde_json::Value, block: usize) -> Vec<String> {
        v["blocks"][block]["items"]
            .as_array()
            .unwrap()
            .iter()
            .map(|i| i["id"].as_str().unwrap().to_string())
            .collect()
    }

    fn block_titles(v: &serde_json::Value) -> Vec<String> {
        v["blocks"]
            .as_array()
            .unwrap()
            .iter()
            .map(|b| b["type"].as_str().unwrap().to_string())
            .collect()
    }

    #[test]
    fn the_real_heimerdinger_file_makes_a_full_set() {
        let (sd, cs) = heimer_data(&[]);
        assert_eq!(cs.builds.len(), 1);
        let all = sets(Lang::En, 74, &cs, &sd, false);
        assert_eq!(all.len(), 1);
        let v = &all[0].1;
        assert_eq!(v["title"], "Hexsight - Heimerdinger AP");
        assert_eq!(v["associatedChampions"][0], 74);
        assert_eq!(v["associatedMaps"][0], 12);
        // The order of the blocks: start, second start, core, boots, later, situational, consumables.
        assert_eq!(
            block_titles(v),
            [
                "Start · Skills: Q > W > E",
                "Start (other option)",
                "Core build (in order)",
                "Boots: most players skip them",
                "Later items",
                "Situational",
                "Consumables"
            ]
        );
        // The top start is the Lost Chapter. The next set differs from it.
        assert_eq!(block_ids(v, 0), ["3802"]);
        assert_eq!(block_ids(v, 1), ["2031", "3802"]);
        // The core in buying order. Item 0 ("no boots") is no item of the set.
        assert_eq!(block_ids(v, 2), ["2503", "6653", "3089"]);
        assert_eq!(block_ids(v, 3), ["3020", "3008", "3158"]);
        assert_eq!(block_ids(v, 4).len(), 6);
        assert_eq!(block_ids(v, 4)[..3], ["4645", "3135", "3157"]);
        // The situational items: the other profiles' core items (4645, 3116, 3157, 3118) all sit in
        // the later block already, so the block takes the later items that did not fit there.
        let shown: Vec<String> = [block_ids(v, 2), block_ids(v, 4)].concat();
        assert!(
            block_ids(v, 5).iter().all(|id| !shown.contains(id)),
            "{:?}",
            block_ids(v, 5)
        );
        assert_eq!(block_ids(v, 5)[0], "3165");
        // The consumables of the top three starting sets: the refillable potion. The two health
        // potions sit in the fourth set, which is too far down.
        assert_eq!(block_ids(v, 6), ["2031"]);
        assert_eq!(v["blocks"][6]["items"][0]["count"], 1);
        // No block holds an id twice, and no id is 0.
        for i in 0..7 {
            let ids = block_ids(v, i);
            assert!(ids.iter().all(|id| id != "0"));
            let mut sorted = ids.clone();
            sorted.sort();
            sorted.dedup();
            assert_eq!(sorted.len(), ids.len(), "block {i}");
        }
        // The shop reads these fields of the file.
        for field in [
            "title",
            "type",
            "map",
            "mode",
            "priority",
            "sortrank",
            "blocks",
            "associatedMaps",
            "associatedChampions",
        ] {
            assert!(v.get(field).is_some(), "{field}");
        }
        for b in v["blocks"].as_array().unwrap() {
            assert!(
                b["type"].is_string()
                    && b["items"][0]["id"].is_string()
                    && b["items"][0]["count"].is_u64()
            );
        }
    }

    #[test]
    fn a_full_set_has_no_rate_in_any_title_in_both_languages() {
        let (sd, cs) = heimer_data(&[]);
        for lang in [Lang::En, Lang::Tr] {
            for hide in [false, true] {
                let v = &sets(lang, 74, &cs, &sd, hide)[0].1;
                let titles = block_titles(v);
                assert_eq!(titles.len(), 7);
                let text = format!("{} {}", v["title"].as_str().unwrap(), titles.join(" "));
                assert!(
                    !text.contains('%') && !text.chars().any(|c| c.is_ascii_digit()),
                    "{lang:?}: {text}"
                );
                let lower = text.to_lowercase();
                for word in ["win", "rate", "pick", "kazanma", "oran", "seçilme"] {
                    assert!(!lower.contains(word), "{lang:?} {word}: {text}");
                }
            }
        }
        // Turkish titles, in the order of play.
        assert_eq!(
            block_titles(&sets(Lang::Tr, 74, &cs, &sd, false)[0].1),
            [
                "Başlangıç · Yetenek sırası: Q > W > E",
                "Başlangıç (diğer seçenek)",
                "Çekirdek dizilim (sırayla)",
                "Bot: çoğu oyuncu almıyor",
                "Sonradan alınanlar",
                "Duruma göre",
                "Tüketilebilir eşyalar"
            ]
        );
        // The items do not depend on the language.
        let en = sets(Lang::En, 74, &cs, &sd, false);
        let tr = sets(Lang::Tr, 74, &cs, &sd, false);
        assert_eq!(en[0].1["blocks"][2]["items"], tr[0].1["blocks"][2]["items"]);
    }

    #[test]
    fn the_boots_title_says_so_when_most_players_buy_none() {
        let (sd, cs) = heimer_data(&[]);
        let mut b = cs.builds[0].clone();
        assert!(b.boots_skipped);
        assert_eq!(
            block_titles(&set_json(Lang::En, 74, 0, &b, &sd, false))[3],
            "Boots: most players skip them"
        );
        assert_eq!(
            block_titles(&set_json(Lang::Tr, 74, 0, &b, &sd, false))[3],
            "Bot: çoğu oyuncu almıyor"
        );
        // The real boots stay in the block, whatever the title says.
        assert_eq!(
            block_ids(&set_json(Lang::En, 74, 0, &b, &sd, false), 3),
            ["3020", "3008", "3158"]
        );
        // With real boots first, the title is plain.
        b.boots_skipped = false;
        assert_eq!(
            block_titles(&set_json(Lang::En, 74, 0, &b, &sd, false))[3],
            "Boots"
        );
        assert_eq!(
            block_titles(&set_json(Lang::Tr, 74, 0, &b, &sd, false))[3],
            "Botlar"
        );
        // The parser reads the same from the file: put real boots first and "none" second.
        let mut d: serde_json::Value = serde_json::from_str(HEIMER).unwrap();
        d["builds"]["filtered"]["archetypes"][0]["profiles"][0]["routes"][0]["boots"]
            .as_array_mut()
            .unwrap()
            .swap(0, 1);
        let again = crate::data::kit_champion(&d).builds.remove(0);
        assert!(!again.boots_skipped);
        assert_eq!(&again.boots[..3], [3020, 3008, 3158]);
        // No boots data at all: no block, no claim.
        b.boots.clear();
        assert!(
            !block_titles(&set_json(Lang::En, 74, 0, &b, &sd, false))
                .iter()
                .any(|t| t.starts_with("Boots"))
        );
    }

    #[test]
    fn an_id_that_the_item_list_does_not_know_is_dropped() {
        // 3089 (core), 3020 (boots) and 3802 (start) are not in the list. Nor is 2003 (a potion).
        let (sd, cs) = heimer_data(&[3089, 3020, 3802, 2003]);
        let v = &sets(Lang::En, 74, &cs, &sd, false)[0].1;
        let titles = block_titles(v);
        // The start block lost its only item, so it is gone. The second start keeps the potion.
        assert_eq!(titles[0], "Start (other option)");
        assert_eq!(block_ids(v, 0), ["2031"]);
        assert_eq!(block_ids(v, 1), ["2503", "6653"]);
        for i in 0..titles.len() {
            let ids = block_ids(v, i);
            assert!(
                !ids.iter()
                    .any(|id| ["3089", "3020", "3802", "2003"].contains(&id.as_str())),
                "block {i}: {ids:?}"
            );
        }
        // The consumable block keeps the potion that the list knows.
        let at = titles.iter().position(|t| t == "Consumables").unwrap();
        assert_eq!(block_ids(v, at), ["2031"]);
        // A block that loses all its items goes away.
        let mut one = cs.builds[0].clone();
        one.consumables = vec![2003];
        assert!(
            !block_titles(&set_json(Lang::En, 74, 0, &one, &sd, false))
                .contains(&"Consumables".to_string())
        );
        // The later block takes the six best ids that the list knows.
        let at = titles.iter().position(|t| t == "Later items").unwrap();
        assert_eq!(block_ids(v, at).len(), 6);
    }

    #[test]
    fn blocks_without_items_are_left_out_and_the_rest_keeps_its_order() {
        let (sd, _) = sample();
        let mut b = build("tank", &[3068, 3075], &[1054, 2003], 0.6);
        b.skills = "W>Q>E".into();
        b.starters_alt = vec![2003, 1054]; // the same items in another order: not a second option
        b.later = vec![3075, 3157, 3157];
        b.situational = vec![3068, 3157, 3089, 3089];
        b.consumables = vec![2003, 2003];
        let v = set_json(Lang::En, 3, 0, &b, &sd, false);
        assert_eq!(
            block_titles(&v),
            [
                "Start · Skills: W > Q > E",
                "Core build (in order)",
                "Later items",
                "Situational",
                "Consumables"
            ]
        );
        // A repeated id is one row with a count.
        assert_eq!(block_ids(&v, 4), ["2003"]);
        assert_eq!(v["blocks"][4]["items"][0]["count"], 2);
        assert_eq!(block_ids(&v, 2), ["3075", "3157"]);
        // Situational: not in the core and not in the later block, once each.
        assert_eq!(block_ids(&v, 3), ["3089"]);
        // A build with nothing at all has no block.
        assert!(
            set_json(Lang::En, 3, 0, &build("ap", &[], &[], 0.5), &sd, false)["blocks"]
                .as_array()
                .unwrap()
                .is_empty()
        );
        // Boots never appear as an empty block.
        assert!(!block_titles(&v).iter().any(|t| t.starts_with("Boots")));
    }

    #[test]
    fn a_thin_file_writes_what_it_has() {
        // A build with only a core set (no routes): what a thin source gives.
        let d: serde_json::Value = serde_json::from_str(r#"{"builds":{"filtered":{"archetypes":[{"key":"tank","profiles":[{"winRate":0.5,"itemSet":[{"id":3068},{"id":3075}]}]}]}}}"#).unwrap();
        let cs = crate::data::kit_champion(&d);
        let (sd, _) = sample();
        let v = &sets(Lang::En, 3, &cs, &sd, false)[0].1;
        assert_eq!(block_titles(v), ["Core build (in order)"]);
        assert_eq!(block_ids(v, 0), ["3068", "3075"]);
        // The aramhex fallback has the augment table of the champion and nothing about items: no build, no set.
        let table: serde_json::Value =
            serde_json::from_str(r#"{"3036":{"games":500,"wins":300,"win_rate":0.6}}"#).unwrap();
        let from_table =
            crate::data::hex_champion(&table, crate::model::ChampRow::default()).unwrap();
        assert!(from_table.builds.is_empty());
        let league = League::new("hex");
        league.put("Hexsight-0.json");
        assert_eq!(
            write(Lang::En, &league.0, 3, &from_table, &sd, false),
            Err("no build data".to_string())
        );
        assert_eq!(
            write(Lang::Tr, &league.0, 3, &from_table, &sd, false),
            Err("dizilim verisi yok".to_string())
        );
        assert_eq!(league.files(), ["Hexsight-0.json"]);
    }

    #[test]
    fn a_new_write_deletes_our_old_files_and_nothing_else() {
        let (sd, mut cs) = sample();
        let league = League::new("clean");
        for name in [
            "Hexsight.json",
            "Hexsight-3.json",
            "hexsight-7.JSON",
            "Augur.json",
            "Augur-4.json",
            "augur-8.JSON",
            "!!!blitz-0.json",
            "!!!blitz-1.json",
            "opgg-Galio.json",
            "Hexsight-notes.json",
            "Hexsights.json",
            "Hexsight.txt",
            "Augur-notes.json",
            "Augurs.json",
            "Augur.txt",
            "readme.txt",
        ] {
            league.put(name);
        }
        // A folder with a name like ours stays too.
        std::fs::create_dir(league.shop().join("Hexsight-9.json")).unwrap();
        write(Lang::En, &league.0, 3, &cs, &sd, false).unwrap();
        assert_eq!(
            league.files(),
            [
                "!!!blitz-0.json",
                "!!!blitz-1.json",
                "Augur-notes.json",
                "Augur.txt",
                "Augurs.json",
                "Hexsight-0.json",
                "Hexsight-1.json",
                "Hexsight-2.json",
                "Hexsight-9.json",
                "Hexsight-notes.json",
                "Hexsight.txt",
                "Hexsights.json",
                "opgg-Galio.json",
                "readme.txt"
            ]
        );
        // A champion with fewer builds: the files of the other builds go away.
        cs.builds.truncate(1);
        write(Lang::En, &league.0, 3, &cs, &sd, false).unwrap();
        let files = league.files();
        assert!(files.contains(&"Hexsight-0.json".to_string()));
        assert!(
            !files.contains(&"Hexsight-1.json".to_string())
                && !files.contains(&"Hexsight-2.json".to_string())
        );
        assert!(
            files.contains(&"!!!blitz-0.json".to_string())
                && files.contains(&"opgg-Galio.json".to_string())
        );
        assert_eq!(
            league.read("Hexsight-0.json")["title"],
            "Hexsight - Galio Tank"
        );
    }

    #[test]
    fn the_old_single_file_goes_away() {
        let (sd, cs) = sample();
        let league = League::new("old");
        league.put("Hexsight.json");
        league.put("Augur.json");
        league.put("Augur-1.json");
        write(Lang::En, &league.0, 3, &cs, &sd, false).unwrap();
        assert!(!league.shop().join("Hexsight.json").exists());
        assert!(
            !league.shop().join("Augur.json").exists()
                && !league.shop().join("Augur-1.json").exists()
        );
        assert!(league.shop().join("Hexsight-0.json").exists());
    }

    #[test]
    fn with_the_setting_off_our_files_are_deleted() {
        let (sd, cs) = sample();
        let league = League::new("off");
        write(Lang::En, &league.0, 3, &cs, &sd, false).unwrap();
        league.put("Hexsight.json");
        league.put("Augur.json");
        league.put("Augur-2.json");
        league.put("!!!blitz-2.json");
        clear(&league.0).unwrap();
        assert_eq!(league.files(), ["!!!blitz-2.json"]);
        // Clearing twice, or with no folder at all, is no error.
        clear(&league.0).unwrap();
        assert!(
            clear(
                &std::env::temp_dir()
                    .join(format!("hexsight-test-itemset-none-{}", std::process::id()))
            )
            .is_ok()
        );
    }

    /// A locked old file does not stop the cleanup of the others, and it does not fail a write.
    #[cfg(windows)]
    #[test]
    fn a_file_that_will_not_go_does_not_stop_the_cleanup() {
        use std::os::windows::fs::OpenOptionsExt;
        let (sd, cs) = sample();
        let league = League::new("locked");
        for name in ["Hexsight-1.json", "Hexsight-5.json", "Hexsight-6.json"] {
            league.put(name);
        }
        // An open handle with no sharing blocks the delete, as a game or a virus scanner would.
        let lock = std::fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(league.shop().join("Hexsight-5.json"))
            .unwrap();
        // The write succeeds, and the files that can go are gone.
        write(Lang::En, &league.0, 3, &cs, &sd, false).unwrap();
        assert_eq!(
            league.files(),
            [
                "Hexsight-0.json",
                "Hexsight-1.json",
                "Hexsight-2.json",
                "Hexsight-5.json"
            ]
        );
        // The clear reports the locked file, and still deletes the others.
        assert!(clear(&league.0).is_err());
        assert_eq!(league.files(), ["Hexsight-5.json"]);
        // Free the file, so the test folder can go.
        drop(lock);
        clear(&league.0).unwrap();
        assert!(league.files().is_empty());
    }

    #[test]
    fn no_build_writes_nothing() {
        let (sd, mut cs) = sample();
        cs.builds.clear();
        let league = League::new("empty");
        league.put("Hexsight-0.json");
        assert_eq!(
            write(Lang::En, &league.0, 3, &cs, &sd, false),
            Err("no build data".to_string())
        );
        assert_eq!(
            write(Lang::Tr, &league.0, 3, &cs, &sd, false),
            Err("dizilim verisi yok".to_string())
        );
        assert_eq!(league.files(), ["Hexsight-0.json"]);
    }

    #[test]
    fn the_file_name_check_matches_our_files_only() {
        for name in [
            "Hexsight.json",
            "hexsight.json",
            "Hexsight-0.json",
            "Hexsight-12.json",
            "HEXSIGHT-3.JSON",
            "Augur.json",
            "augur.json",
            "Augur-0.json",
            "Augur-12.json",
            "AUGUR-3.JSON",
        ] {
            assert!(is_ours(name), "{name}");
        }
        for name in [
            "",
            "Hexsight",
            "Hexsight-.json",
            "Hexsight-x.json",
            "Hexsight-1a.json",
            "Hexsight-1.json.bak",
            "!!!blitz-0.json",
            "my-Hexsight.json",
            "Hexsight 1.json",
            "Augur",
            "Augur-.json",
            "Augur-x.json",
            "Augur-1.json.bak",
            "Augurs.json",
            "my-Augur.json",
        ] {
            assert!(!is_ours(name), "{name}");
        }
    }
}
