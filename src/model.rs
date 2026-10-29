//! Internal data model. Data sources map their JSON into these types.

use std::collections::{HashMap, HashSet};

#[derive(Clone, Debug)]
pub struct Augment {
    pub name: String,
    /// English name. OCR also matches it.
    pub name_en: String,
    pub icon: String,
}

#[derive(Clone, Debug)]
pub struct Champion {
    pub id: i64,
    pub name: String,
    pub alias: String,
    pub icon: String,
}

#[derive(Clone, Debug)]
pub struct Item {
    pub name: String,
    pub icon: String,
}

#[derive(Clone, Debug, Default)]
pub struct AugStat {
    pub augment_id: i64,
    pub win_rate: f32,
    pub games: u32,
    /// Index into TIER_NAMES. 0 = S+, higher is worse.
    pub tier: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Build {
    pub name: String,
    /// The core items in buying order.
    pub items: Vec<i64>,
    /// The top starting set. A repeated id is a repeated buy (two potions).
    pub starters: Vec<i64>,
    /// Not shown anywhere now: the item sets carry no rate. Kept with the data.
    #[allow(dead_code)]
    pub win_rate: f32,
    /// The next starting set that differs from the top one. Empty when none.
    pub starters_alt: Vec<i64>,
    /// Real boots, best first. The "no boots" choice (id 0) is not in the list.
    pub boots: Vec<i64>,
    /// True when most players of this build buy no boots (the data ranks "none" first).
    pub boots_skipped: bool,
    /// Items bought after the core, best first.
    pub later: Vec<i64>,
    /// Core items of the other builds of the same style, for a different game.
    pub situational: Vec<i64>,
    /// Potions and elixirs that the top starting sets buy. A repeated id is a repeated buy.
    pub consumables: Vec<i64>,
    /// The skill order of the champion, for example "Q>W>E". Empty when unknown.
    pub skills: String,
}

#[derive(Clone, Debug, Default)]
pub struct Combo {
    pub augments: Vec<i64>,
    pub win_rate: f32,
    pub games: u32,
}

/// One summoner spell pair of the data. It keeps the rank and the games, and no
/// win rate: the card of spells shows none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SpellPair {
    /// The spell ids. The data lists two different spells.
    pub spells: [i64; 2],
    /// Place in the ranking of the data source. 1 is the top pair. The source ranks
    /// by its own score, not by how often players take the pair.
    pub rank: u32,
    /// The games with this pair (`sampleCount`). The pick rate of the pair is this count
    /// divided by the games of the champion, so the pair with the most games is the pair
    /// that players take most.
    pub games: u32,
}

/// An item of the shop with what the item suggestions need: the price, the recipe,
/// and the tags of CommunityDragon as a bit set (`cat`).
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct ItemInfo {
    /// The full price in gold (`priceTotal`).
    pub total: u32,
    /// The components, one id for each part (`from`).
    pub from: Vec<i64>,
    /// The tags (`categories`) as bits of `cat`.
    pub cats: u16,
    /// How many tags the item has in the data. An item with many tags (the Golden
    /// Spatula has 13) is no clear item of any kind.
    pub tags: u8,
    /// The shop sells the item.
    pub in_store: bool,
}

/// The bits of `ItemInfo::cats`, from the `categories` words of `items.json`.
pub mod cat {
    pub const BOOTS: u16 = 1 << 0;
    /// `SpellDamage`: ability power.
    pub const AP: u16 = 1 << 1;
    /// `Damage`: attack damage.
    pub const AD: u16 = 1 << 2;
    pub const CRIT: u16 = 1 << 3;
    pub const AS: u16 = 1 << 4;
    pub const ARMOR_PEN: u16 = 1 << 5;
    pub const MAGIC_PEN: u16 = 1 << 6;
    pub const ARMOR: u16 = 1 << 7;
    /// `SpellBlock`: magic resist.
    pub const MR: u16 = 1 << 8;
    pub const HEALTH: u16 = 1 << 9;
    /// `LifeSteal` or `SpellVamp`.
    pub const LIFESTEAL: u16 = 1 << 10;
    pub const CONSUMABLE: u16 = 1 << 11;
    /// `Trinket` or `Vision`.
    pub const TRINKET: u16 = 1 << 12;

    /// The bit of one `categories` word. 0 for a word that no rule needs.
    pub fn bit(word: &str) -> u16 {
        match word {
            "Boots" => BOOTS,
            "SpellDamage" => AP,
            "Damage" => AD,
            "CriticalStrike" => CRIT,
            "AttackSpeed" => AS,
            "ArmorPenetration" => ARMOR_PEN,
            "MagicPenetration" => MAGIC_PEN,
            "Armor" => ARMOR,
            "SpellBlock" => MR,
            "Health" => HEALTH,
            "LifeSteal" | "SpellVamp" => LIFESTEAL,
            "Consumable" => CONSUMABLE,
            "Trinket" | "Vision" => TRINKET,
            _ => 0,
        }
    }
}

/// An item that the data ties to an augment: players who hold the augment buy it.
/// It filters the item suggestions and the fit mark. It never shows a number. It holds
/// no win rate and no change of one: the source has such a field (`synergyDelta`), and
/// Hexsight does not read it, so no suggestion depends on the win rate of an augment.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct AugLink {
    pub item: i64,
    pub augment: i64,
    /// The share of the players with the augment who buy the item (0 to 1).
    pub with_pick: f32,
}

impl AugLink {
    /// A link that counts as "goes with the augment": half of its players or more buy the item.
    pub fn strong(&self) -> bool {
        self.with_pick >= 0.5
    }
}

#[derive(Clone, Debug, Default)]
pub struct ChampStats {
    pub augments: Vec<AugStat>,
    /// Ranking per pick screen (1st to 4th). Empty when the source has none.
    pub stages: Vec<Vec<AugStat>>,
    pub combos: Vec<Combo>,
    pub builds: Vec<Build>,
    /// Summoner spell pairs. Empty when the source has none.
    pub spells: Vec<SpellPair>,
    /// The items that players take as their 1st to 6th finished item, the four most
    /// picked of each slot. Empty when the source has none.
    pub slots: [Vec<i64>; 6],
    /// Items tied to augments. Empty when the source has none.
    pub aug_links: Vec<AugLink>,
    /// The share of magic damage in the damage that the champion deals to champions
    /// (0 to 1). `None` when the source gives no damage.
    pub magic_share: Option<f32>,
}

impl ChampStats {
    pub fn get(&self, augment_id: i64) -> Option<&AugStat> {
        self.augments.iter().find(|a| a.augment_id == augment_id)
    }
}

/// Champion row of the stats index: win rate, games and tier.
#[derive(Clone, Copy, Debug, Default)]
pub struct ChampRow {
    pub win_rate: f32,
    pub games: u32,
    pub tier: u8,
}

#[derive(Default)]
pub struct StaticData {
    pub champions: HashMap<i64, Champion>,
    /// Mayhem augments only (the KIWI list).
    pub augments: HashMap<i64, Augment>,
    pub items: HashMap<i64, Item>,
    /// Price, recipe and tags of the items, by id. Empty when the list did not download.
    pub item_info: HashMap<i64, ItemInfo>,
    /// The roles of each champion (`mage`, `tank` ...) from the champion summary.
    pub roles: HashMap<i64, Vec<String>>,
    /// Summoner spells, by id. Empty when the list did not download: the spell card then stays away.
    pub spells: HashMap<i64, Item>,
    pub kiwi: HashSet<i64>,
}

/// ARAM Mayhem queues (2400 and variants, plus Classic Mayhem 2450/3280).
pub fn is_mayhem(q: i64) -> bool {
    matches!(
        q,
        2400 | 2401 | 2403 | 2405 | 2410 | 2450 | 3240 | 3270 | 3280
    )
}

/// Queues that have the ARAM bench: ARAM (65, 100, 450, 720) and every Mayhem queue.
/// The main window shows its champ select page (the bench, the build) only for these.
pub fn is_aram(q: i64) -> bool {
    matches!(q, 65 | 100 | 450 | 720) || is_mayhem(q)
}

pub const TIER_NAMES: [&str; 6] = ["S+", "S", "A", "B", "C", "D"];

pub fn tier_index(s: &str) -> u8 {
    match s {
        "S+" => 0,
        "S" => 1,
        "A" => 2,
        "B" => 3,
        "C" => 4,
        _ => 5,
    }
}

/// Tier from win rate delta to the champion average (percentage points).
pub fn tier_from_delta(delta: f32) -> u8 {
    match delta {
        d if d >= 4.0 => 0,
        d if d >= 2.0 => 1,
        d if d >= 0.5 => 2,
        d if d >= -1.0 => 3,
        d if d >= -3.0 => 4,
        _ => 5,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn aram_queues_have_the_bench_and_other_queues_do_not() {
        // ARAM and its old and clash variants.
        for q in [65, 100, 450, 720] {
            assert!(is_aram(q), "{q}");
            assert!(!is_mayhem(q), "{q} is ARAM, not Mayhem");
        }
        // Every Mayhem queue is an ARAM queue.
        for q in [2400, 2401, 2403, 2405, 2410, 2450, 3240, 3270, 3280] {
            assert!(is_aram(q) && is_mayhem(q), "{q}");
        }
        // Ranked, normal draft, a custom game (0) and an unknown queue (-1) are not.
        for q in [420, 400, 440, 430, 0, -1] {
            assert!(!is_aram(q), "{q}");
        }
    }
}
