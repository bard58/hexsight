//! Suggestions during a game: the next items, a summary of the game, and the mark
//! "fits your build" for an offered augment.
//!
//! This module is pure. It takes plain data in and gives plain data out. It draws
//! nothing and reads no network. The poll thread calls it once per pass, and the
//! window only reads the result.
//!
//! Riot's rules for third-party apps: Hexsight suggests and the player decides.
//! Hexsight buys nothing and picks nothing. No value here comes from the win rate
//! of an augment or an item. A reason that comes from a rule of Hexsight says so
//! (`Reason::is_rule`): the window labels it as Hexsight's own rule, not as statistics.
//!
//! Player names never enter this module. A `Roster` has the champion, the side, the
//! items and the score line of each player, and the index of the player.

use crate::data;
use crate::model::{AugLink, Build, ChampStats, Champion, StaticData, cat};
use serde_json::Value;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};

// The child modules. Each one has an owner package and holds its hooks of `rank` and `advise`.
// `rank` stays here and calls them; the children read the private items of this module.
mod path;
pub mod recap;
pub mod stats;
#[cfg(test)]
pub(crate) mod testkit;
mod threat;

pub use recap::{Killer, Recap};
pub use stats::MyStats;
use threat::Rules;

// ---------------------------------------------------------------------------
// The constants. All of them are first values: a placeholder, to tune on real
// games (as `CONF_HIGH` in app.rs). The tests pin the behavior, and the exact
// numbers only where a test says so.

/// Score of the first core item. Each next core item has `CORE_STEP` less.
const CORE_BASE: i32 = 100;
const CORE_STEP: i32 = 8;
/// Boots: after the first finished item, and before it.
const BOOTS_LATE: i32 = 90;
const BOOTS_EARLY: i32 = 55;
/// The second pair of boots of the build scores this much less than the first.
const BOOTS_STEP: i32 = 2;
/// An item of the list "later" of the build, and the step for each next one.
const LATER_BASE: i32 = 62;
const LATER_STEP: i32 = 4;
/// An item that many players take as their next finished item (the slot lists).
const SLOT_BASE: i32 = 58;
const SLOT_STEP: i32 = 3;
/// An item from the other builds of the same champion.
const OTHER_BASE: i32 = 30;
const OTHER_STEP: i32 = 2;
/// The base of an item that the data ties to a known or an offered augment, and
/// the base of the anti-heal item and of the resist item that a rule adds.
const LINK_KNOWN_BASE: i32 = 60;
const LINK_OFFERED_BASE: i32 = 30;
const ANTI_HEAL_BASE: i32 = 50;
const RESIST_BASE: i32 = 50;
/// Added when you own a part of the item, so the plan does not jump after a purchase.
const STABLE: i32 = 8;
/// The bonus of a resist rule: for a tank, a utility or a bruiser build, and for the others.
const RESIST_TANK: i32 = 28;
const RESIST_CARRY: i32 = 16;
/// The bonus of the anti-heal rule and of the penetration rule.
const ANTI_HEAL_BONUS: i32 = 30;
const PEN_BONUS: i32 = 20;
/// An offered augment adds a little. A known augment adds more. Neither reads a win rate.
const OFFERED_BONUS: i32 = 14;
const KNOWN_BONUS: i32 = 35;
/// An item under this score is not suggested.
const MIN_SCORE: i32 = 40;
/// The third pick goes to an item that a rule raised, if it has at least this score.
const RULE_PICK_MIN: i32 = 60;
/// Before your first finished item, the rule item takes the third pick only this close to its score.
const RULE_PICK_GAP: i32 = 15;
/// An item with at least this price, and no boots, counts as a finished item.
const FINISHED_GOLD: u32 = 2000;
/// The price of the basic Boots. Boots above it, or with parts, are finished boots.
const BASIC_BOOTS_GOLD: u32 = 300;
/// An item with more tags than this is no clear item of any kind: no rule touches it.
const MAX_RULE_TAGS: u8 = 6;
/// At most this many items that a rule adds (injects) in one pass. Order: killer, fed, healer, mix.
const MAX_INJECTS: usize = 2;
/// The priority of a rule reason on a tie of points: the lower one wins.
const PRIO_KILLER: u8 = 0;
const PRIO_FED: u8 = 1;
const PRIO_ANTI_HEAL: u8 = 2;
const PRIO_PEN: u8 = 3;
const PRIO_RESIST: u8 = 4;
/// How many levels of the recipe tree Hexsight follows.
const RECIPE_DEPTH: u8 = 4;
/// The most items in a suggestion.
const MAX_PICKS: usize = 3;

/// An enemy champion starts with a guess of its damage type, worth this much gold of items.
/// The guess fades as the enemy buys items.
const PRIOR_GOLD: f32 = 1500.0;
/// An enemy is fed when its kills minus its deaths reach `FED_NET` and it has `FED_MIN_KILLS` kills.
const FED_NET: i32 = 5;
const FED_MIN_KILLS: u16 = 6;
/// A fed enemy deals more of the damage of its team: its weight in the damage mix.
const FED_WEIGHT: f32 = 1.5;
/// The mix needs at least this many enemy champions that Hexsight knows.
const MIX_MIN_ENEMIES: usize = 3;
/// A damage type from this share of the enemy damage on triggers a resist rule.
const MIX_RULE_PCT: u8 = 60;
/// Healers or tanky enemies from this count on trigger a rule.
const RULE_MIN_COUNT: usize = 2;
/// Your damage is mostly magic from this share on, and mostly physical up to the other.
const CLASS_AP: f32 = 0.6;
const CLASS_AD: f32 = 0.4;

/// Champions that heal a lot, by alias: Hexsight's own list, not statistics. Any enemy with a
/// finished item of lifesteal or omnivamp counts as a healer too.
const HEALERS: [&str; 17] = [
    "Soraka",
    "Yuumi",
    "Sona",
    "Nami",
    "Lulu",
    "Janna",
    "Seraphine",
    "Milio",
    "Taric",
    "Aatrox",
    "Warwick",
    "Vladimir",
    "DrMundo",
    "Swain",
    "Sylas",
    "Fiddlesticks",
    "Briar",
];

/// Items that cut healing, by the class of the player. A patch can take one out of the
/// shop: the drop rule removes it, so a stale id does no harm.
const ANTI_HEAL_AP: [i64; 2] = [3165, 3916];
const ANTI_HEAL_AD: [i64; 3] = [3033, 6609, 3123];
const ANTI_HEAL_TANK: [i64; 2] = [3075, 3076];

/// Items with magic resist, and items with armor, by the class of the player. The resist rule adds
/// the first one that the shop sells when the list has no such item. All were in the shop of
/// CommunityDragon: Banshee's Veil; Zhonya's Hourglass; Maw of Malmortius, Mercurial Scimitar and
/// Wit's End; Death's Dance and Guardian Angel; Force of Nature and Kaenic Rookern; Randuin's Omen
/// and Frozen Heart.
const MR_AP: [i64; 1] = [3102];
const ARMOR_AP: [i64; 1] = [3157];
const MR_AD: [i64; 3] = [3156, 3139, 3091];
const ARMOR_AD: [i64; 2] = [6333, 3026];
const MR_TANK: [i64; 2] = [4401, 2504];
const ARMOR_TANK: [i64; 2] = [3143, 3110];

fn is_anti_heal(id: i64) -> bool {
    ANTI_HEAL_AP.contains(&id) || ANTI_HEAL_AD.contains(&id) || ANTI_HEAL_TANK.contains(&id)
}

// ---------------------------------------------------------------------------
// The roster: the ten players of the Live Client `playerlist`.

/// The side of a player. The Live Client writes `ORDER` and `CHAOS`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Side {
    Order,
    Chaos,
    /// The answer has no side, or a word that Hexsight does not know.
    #[default]
    Unknown,
}

/// One player. It holds no name.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Player {
    /// The champion id. 0 when Hexsight cannot tell.
    pub champion: i64,
    pub side: Side,
    /// The items in the six item slots. A potion, a ward and the trinket are left out.
    pub items: Vec<i64>,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub level: u8,
    pub dead: bool,
}

/// All players of the game, and the place of the player who runs Hexsight.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Roster {
    pub players: Vec<Player>,
    /// The index in `players` of the player who runs Hexsight.
    pub me: usize,
}

impl Roster {
    pub fn me(&self) -> &Player {
        &self.players[self.me]
    }

    /// The players of the other side. Empty when the side of the player is not known.
    pub fn enemies(&self) -> impl Iterator<Item = &Player> {
        let side = self.me().side;
        self.players
            .iter()
            .enumerate()
            .filter(move |(i, p)| {
                *i != self.me && side != Side::Unknown && p.side != Side::Unknown && p.side != side
            })
            .map(|(_, p)| p)
    }
}

fn count(v: &Value) -> u16 {
    v.as_u64().unwrap_or(0).min(u16::MAX as u64) as u16
}

/// The roster of the answer of `playerlist`. `my_id` is the Riot ID (or the summoner name) of the
/// player, as `activeplayer` gives it. Each field is optional: a player without `items` has none,
/// without `scores` has 0/0/0, and a `team` that is no known word is `Side::Unknown`. `None` when
/// the answer is no list or no row is the player. The names are used to find the player and are
/// not kept.
pub fn parse_players(
    list: &Value,
    my_id: &str,
    champions: &HashMap<i64, Champion>,
) -> Option<Roster> {
    let mut players = Vec::new();
    let mut me = None;
    for p in list.as_array()? {
        let name = p["riotId"]
            .as_str()
            .or(p["summonerName"].as_str())
            .unwrap_or("");
        if me.is_none() && !my_id.is_empty() && name == my_id {
            me = Some(players.len());
        }
        let raw = p["rawChampionName"].as_str().unwrap_or("");
        let shown = p["championName"].as_str().unwrap_or("");
        let items = p["items"]
            .as_array()
            .into_iter()
            .flatten()
            // The slot 6 is the trinket. A consumable is a potion or a ward. Neither is an item to build.
            .filter(|i| {
                i["slot"].as_i64().is_none_or(|s| (0..6).contains(&s))
                    && !i["consumable"].as_bool().unwrap_or(false)
            })
            .filter_map(|i| i["itemID"].as_i64().filter(|&id| id > 0))
            .collect();
        players.push(Player {
            champion: data::live_champion(champions, raw, shown).unwrap_or(0),
            side: match p["team"].as_str().map(str::to_ascii_uppercase).as_deref() {
                Some("ORDER") => Side::Order,
                Some("CHAOS") => Side::Chaos,
                _ => Side::Unknown,
            },
            items,
            kills: count(&p["scores"]["kills"]),
            deaths: count(&p["scores"]["deaths"]),
            assists: count(&p["scores"]["assists"]),
            level: count(&p["level"]).min(u8::MAX as u16) as u8,
            dead: p["isDead"].as_bool().unwrap_or(false),
        });
    }
    Some(Roster { me: me?, players })
}

// ---------------------------------------------------------------------------
// The game context.

/// The fed enemy: its champion, its score line and its level.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Fed {
    pub champion: i64,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub level: u8,
}

/// The build kind of an enemy, with the count of its items of that kind (2 or more).
/// `threat::foe_kind` decides it.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum FoeKind {
    #[default]
    None,
    Crit(u8),
    Ap(u8),
    Ad(u8),
    /// Items with armor or magic resist and no damage.
    Tank(u8),
}

/// One known enemy: its score line, its items in short, and its build kind. It holds no name.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Foe {
    pub champion: i64,
    pub kills: u16,
    pub deaths: u16,
    pub assists: u16,
    pub level: u8,
    pub dead: bool,
    /// The finished non-boots items (`FINISHED_GOLD` or more).
    pub finished: u8,
    /// The weighted gold of ability power and attack damage of the enemy (`share_of`): the prior of
    /// its champion as `PRIOR_GOLD`, plus the gold of its damage items. The AP share of the foe is
    /// `ap_gold / (ap_gold + ad_gold)`.
    pub ap_gold: u32,
    pub ad_gold: u32,
    pub kind: FoeKind,
    /// The enemy heals a lot (`is_healer`).
    pub heals: bool,
}

/// What the enemy team looks like. Every part is Hexsight's own estimate, not statistics.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct GameContext {
    /// The share of ability power in the enemy damage, in steps of 5 (0 to 100). `None` with
    /// fewer than `MIX_MIN_ENEMIES` known enemy champions.
    pub ap_pct: Option<u8>,
    pub fed: Option<Fed>,
    /// The champions of the enemies that heal a lot: the fed enemy first, then by kills (most
    /// first), then by champion id.
    pub healers: Vec<i64>,
    /// The champions of the enemies that have a lot of armor or magic resist.
    pub tanks: Vec<i64>,
    /// How many enemies the roster has.
    pub enemies: u8,
    /// One entry for each known enemy (champion id above 0), by champion id.
    pub foes: Vec<Foe>,
}

fn is_fed(p: &Player) -> bool {
    p.kills >= FED_MIN_KILLS && i32::from(p.kills) - i32::from(p.deaths) >= FED_NET
}

/// The first guess of the damage of a champion: (ability power, attack damage), each 0 to 1.
/// It comes from the first role in the champion summary. A champion without a role is half and half.
fn prior(sd: &StaticData, champion: i64) -> (f32, f32) {
    match sd
        .roles
        .get(&champion)
        .and_then(|r| r.first())
        .map(String::as_str)
    {
        Some("mage") => (1.0, 0.0),
        Some("marksman") => (0.0, 1.0),
        Some("fighter") => (0.25, 0.75),
        Some("assassin") => (0.4, 0.6),
        Some("support") => (0.6, 0.4),
        _ => (0.5, 0.5),
    }
}

/// The gold of the items of a player that add ability power and attack damage. An item with both
/// tags adds half to each. Boots, potions and trinkets add nothing, nor do resist and health items.
fn item_gold(sd: &StaticData, items: &[i64]) -> (f32, f32) {
    let (mut ap, mut ad) = (0.0, 0.0);
    for info in items.iter().filter_map(|id| sd.item_info.get(id)) {
        if info.cats & (cat::BOOTS | cat::CONSUMABLE | cat::TRINKET) != 0 {
            continue;
        }
        let is_ap = info.cats & (cat::AP | cat::MAGIC_PEN) != 0;
        let is_ad = info.cats & (cat::AD | cat::CRIT | cat::AS | cat::ARMOR_PEN) != 0;
        let gold = info.total as f32;
        match (is_ap, is_ad) {
            (true, true) => {
                ap += gold / 2.0;
                ad += gold / 2.0;
            }
            (true, false) => ap += gold,
            (false, true) => ad += gold,
            (false, false) => {}
        }
    }
    (ap, ad)
}

fn is_healer(sd: &StaticData, p: &Player) -> bool {
    let alias = sd
        .champions
        .get(&p.champion)
        .map_or("", |c| c.alias.as_str());
    HEALERS.iter().any(|h| h.eq_ignore_ascii_case(alias))
        || p.items.iter().any(|id| {
            sd.item_info.get(id).is_some_and(|i| {
                i.cats & cat::LIFESTEAL != 0 && i.cats & cat::BOOTS == 0 && i.total >= FINISHED_GOLD
            })
        })
}

fn is_tanky(sd: &StaticData, p: &Player) -> bool {
    let resist = p
        .items
        .iter()
        .filter_map(|id| sd.item_info.get(id))
        // A damage item with armor or magic resist (Zhonya's Hourglass) counts only with health too.
        .filter(|i| {
            i.cats & cat::BOOTS == 0
                && i.cats & (cat::ARMOR | cat::MR) != 0
                && (i.cats & (cat::AP | cat::AD) == 0 || i.cats & cat::HEALTH != 0)
        })
        .count();
    let tank_role = sd
        .roles
        .get(&p.champion)
        .is_some_and(|r| r.iter().any(|r| r == "tank"));
    resist >= 2 || (tank_role && resist >= 1)
}

/// The weighted (ability power, attack damage) gold of a player: its prior as `PRIOR_GOLD`, plus the
/// gold of its damage items. The prior is the magic share of the champion's own data in `shares`
/// (0 to 1) when it is there, and the guess from the role (`prior`) when not.
fn share_of(sd: &StaticData, p: &Player, shares: &HashMap<i64, f32>) -> (f32, f32) {
    let (prior_ap, prior_ad) = shares
        .get(&p.champion)
        .map(|&m| {
            let m = m.clamp(0.0, 1.0);
            (m, 1.0 - m)
        })
        .unwrap_or_else(|| prior(sd, p.champion));
    let (gold_ap, gold_ad) = item_gold(sd, &p.items);
    (
        prior_ap * PRIOR_GOLD + gold_ap,
        prior_ad * PRIOR_GOLD + gold_ad,
    )
}

/// The finished non-boots items of a player: `FINISHED_GOLD` or more.
fn finished_items(sd: &StaticData, items: &[i64]) -> usize {
    items
        .iter()
        .filter(|&id| {
            sd.item_info
                .get(id)
                .is_some_and(|i| i.total >= FINISHED_GOLD && i.cats & cat::BOOTS == 0)
        })
        .count()
}

/// The summary of the enemy team: the damage mix, the fed enemy, the healers, the tanks and the
/// foes. `shares` holds the magic share of each enemy champion from its own data (by champion id),
/// and an empty map uses the role of each champion.
pub fn context(roster: &Roster, sd: &StaticData, shares: &HashMap<i64, f32>) -> GameContext {
    let enemies: Vec<&Player> = roster.enemies().collect();
    let mut out = GameContext {
        enemies: enemies.len().min(255) as u8,
        ..Default::default()
    };

    // The damage mix: each known enemy champion gives a share of ability power, weighted by the gold
    // of its damage items (the prior gold at least). A fed enemy counts more.
    let (mut sum, mut weights, mut known) = (0.0, 0.0, 0);
    for p in enemies.iter().filter(|p| p.champion > 0) {
        known += 1;
        let (ap, ad) = share_of(sd, p, shares);
        let (gold_ap, gold_ad) = item_gold(sd, &p.items);
        let gold = (gold_ap + gold_ad).max(PRIOR_GOLD);
        let weight = gold * if is_fed(p) { FED_WEIGHT } else { 1.0 };
        sum += weight * ap / (ap + ad);
        weights += weight;
    }
    if known >= MIX_MIN_ENEMIES {
        out.ap_pct = Some(((100.0 * sum / weights / 5.0).round() * 5.0) as u8);
    }

    // The fed enemy: the highest kills minus deaths, and a tie goes to more kills.
    out.fed = enemies
        .iter()
        .filter(|p| is_fed(p))
        .max_by_key(|p| (i32::from(p.kills) - i32::from(p.deaths), p.kills))
        .map(|p| Fed {
            champion: p.champion,
            kills: p.kills,
            deaths: p.deaths,
            assists: p.assists,
            level: p.level,
        });
    // The healers: the fed enemy first, then the most kills, then the lower id.
    let fed = out.fed.map(|f| f.champion);
    let mut healers: Vec<&Player> = enemies
        .iter()
        .copied()
        .filter(|p| p.champion > 0 && is_healer(sd, p))
        .collect();
    healers.sort_by_key(|p| {
        (
            Some(p.champion) != fed,
            std::cmp::Reverse(p.kills),
            p.champion,
        )
    });
    out.healers = healers.iter().map(|p| p.champion).collect();
    out.tanks = enemies
        .iter()
        .filter(|p| p.champion > 0 && is_tanky(sd, p))
        .map(|p| p.champion)
        .collect();
    // The foes, by champion id. The sort is stable, so two of the same champion keep the roster order.
    out.foes = enemies
        .iter()
        .filter(|p| p.champion > 0)
        .map(|p| {
            let (ap, ad) = share_of(sd, p, shares);
            Foe {
                champion: p.champion,
                kills: p.kills,
                deaths: p.deaths,
                assists: p.assists,
                level: p.level,
                dead: p.dead,
                finished: finished_items(sd, &p.items).min(255) as u8,
                ap_gold: ap.round() as u32,
                ad_gold: ad.round() as u32,
                kind: threat::foe_kind(sd, p),
                heals: is_healer(sd, p),
            }
        })
        .collect();
    out.foes.sort_by_key(|f| f.champion);
    out
}

// ---------------------------------------------------------------------------
// The next item.

/// Why an item is on the list. It holds no text: the window writes the words in its
/// language. The first five come from build data. The next four come from a rule of Hexsight.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Reason {
    /// The number of the item in the core build (1 is the first).
    Core(u8),
    Boots,
    /// An item that the build buys after the core.
    Later,
    /// An item that many players take as their Nth finished item.
    Slot(u8),
    /// An item that other builds of the champion use.
    OtherBuild,
    /// Against an enemy team with this share (percent) of ability power.
    VsAp(u8),
    /// Against an enemy team with this share (percent) of attack damage.
    VsAd(u8),
    /// Against this many healers.
    AntiHeal(u8),
    /// Against this many tanky enemies.
    VsTanks(u8),
    /// The data ties the item to this augment. `known` is true when Hexsight knows that you took it,
    /// and false when the augment was only offered.
    Pairs {
        augment: i64,
        known: bool,
    },
    /// Against the fed enemy: its champion and its kills and deaths.
    VsFed {
        champion: i64,
        kills: u8,
        deaths: u8,
    },
    /// Against the enemy that killed you, and how many times it did.
    VsKiller {
        champion: i64,
        times: u8,
    },
}

impl Reason {
    /// The reason comes from a rule of Hexsight, not from build data.
    pub fn is_rule(self) -> bool {
        matches!(
            self,
            Reason::VsAp(_)
                | Reason::VsAd(_)
                | Reason::AntiHeal(_)
                | Reason::VsTanks(_)
                | Reason::VsFed { .. }
                | Reason::VsKiller { .. }
        )
    }
}

/// Which team rules fired in a pass: the input of the hysteresis of the next pass
/// (`Input::prev_rules`).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct RuleState {
    pub vs_ap: bool,
    pub vs_ad: bool,
    pub tanks: bool,
    pub healers: bool,
}

/// One suggested item.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ItemPick {
    pub id: i64,
    pub reason: Reason,
    /// The gold you still need for the item, after the parts that you own.
    pub need: u32,
    /// You have the gold. False too when the gold is not known.
    pub ready: bool,
    /// The part of the item that your gold pays for now, when the item is not ready (`buy_now`).
    pub part: Option<i64>,
}

/// The result of `advise`. It holds ids, counts and gold only: no rate of any kind.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Advice {
    /// Up to three items, best first.
    pub items: Vec<ItemPick>,
    /// The build style that the suggestion follows, as the data writes it ("ap", "on hit").
    pub style: String,
    pub ctx: Option<GameContext>,
    /// Your item slots are full and no part of a suggested item is among them.
    pub slots_full: bool,
    pub me_dead: bool,
    /// The champion has no build in the data.
    pub no_build: bool,
    /// The gold was known, so `ready` means something.
    pub has_gold: bool,
    /// The items you could still want: the suggested ones, and the core and later items of the
    /// build, minus what you own. The mark "fits your build" looks here.
    pub wanted: Vec<i64>,
    /// The index of the build path that you follow, when the path tracker is sure of it. The worker
    /// passes it back as `Input::prev_path`. No weight of the path is here.
    pub path: Option<u16>,
    /// The item that names the path on the card ("On your Malignance path").
    pub path_item: Option<i64>,
    /// Up to two items after the picks ("Then: X → Y"). Not in `signature`.
    pub plan: Vec<i64>,
    /// The fed enemy when the fed rule fired. The worker passes it back as `Input::prev_fed`.
    pub fed_rule: Option<i64>,
    /// The recap of your current death (`Input::recap`).
    pub recap: Option<Recap>,
    /// The enemy that killed you, now or as the nemesis, with its count (`Input::killer`). The recap
    /// line reads the count here, also when no pick carries `Reason::VsKiller`.
    pub killer: Option<Killer>,
    /// Your own armor and magic resist, rounded, when the Live Client gave both.
    pub my_stats: Option<(u16, u16)>,
    /// The team rules that fired. The worker passes it back as `Input::prev_rules`.
    pub rules: RuleState,
}

impl Advice {
    /// A number that changes when the list of items or the killers of the recap change. The overlay
    /// pops up for a while after such a change. It ignores `ready`: that flag flips when the gold
    /// reaches a price, which often happens after a kill in a fight, and the card must not grow then.
    /// It ignores the path and the plan too, because the overlay shows neither.
    pub fn signature(&self) -> u64 {
        let mut h = std::collections::hash_map::DefaultHasher::new();
        for p in &self.items {
            p.id.hash(&mut h);
        }
        if let Some(r) = &self.recap {
            r.killers.hash(&mut h);
        }
        h.finish()
    }
}

/// Everything that `advise` reads.
pub struct Input<'a> {
    pub me: &'a Player,
    /// Your gold, when the Live Client gave it.
    pub gold: Option<u32>,
    pub ctx: Option<&'a GameContext>,
    pub details: &'a ChampStats,
    pub sd: &'a StaticData,
    /// The augments that the card reader saw offered in this game. Hexsight cannot know which one you took.
    pub offered: &'a [i64],
    /// The augments that Hexsight knows you took. The Live Client does not give them, so this is empty.
    pub known: &'a [i64],
    /// Your own armor, magic resist and health, from `activeplayer`.
    pub stats: Option<MyStats>,
    /// The enemy that killed you (now, or as the nemesis), for the killer rule.
    pub killer: Option<Killer>,
    /// The recap of your current death. `advise` copies it into `Advice::recap`.
    pub recap: Option<&'a Recap>,
    /// `Advice::path` of the last pass, for the hysteresis of the path tracker.
    pub prev_path: Option<u16>,
    /// `Advice::fed_rule` of the last pass, so a fed enemy that dies once keeps the rule.
    pub prev_fed: Option<i64>,
    /// `Advice::rules` of the last pass, for the hysteresis of the team rules.
    pub prev_rules: Option<RuleState>,
}

/// Your items and the items that they were before an upgrade in the game: Muramana counts as Manamune too.
fn with_earlier_forms(sd: &StaticData, owned: &[i64]) -> Vec<i64> {
    let mut out = owned.to_vec();
    for &id in owned {
        let mut at = id;
        for _ in 0..RECIPE_DEPTH {
            let Some(earlier) = sd.item_info.get(&at).and_then(|i| i.upgrade_of) else {
                break;
            };
            out.push(earlier);
            at = earlier;
        }
    }
    out
}

/// All parts of an item, the parts of the parts and so on (`RECIPE_DEPTH` levels). The item that
/// turns into this one counts as a part, so Muramana also holds Manamune and its Tear of the Goddess.
fn recipe_tree(sd: &StaticData, id: i64) -> Vec<i64> {
    fn walk(sd: &StaticData, id: i64, depth: u8, out: &mut Vec<i64>) {
        if depth == 0 {
            return;
        }
        let Some(info) = sd.item_info.get(&id) else {
            return;
        };
        for &part in info.from.iter().chain(&info.upgrade_of) {
            if !out.contains(&part) {
                out.push(part);
            }
            walk(sd, part, depth - 1, out);
        }
    }
    let mut out = Vec::new();
    walk(sd, id, RECIPE_DEPTH, &mut out);
    out
}

/// The gold still needed for an item: its price, minus the price of each part that you own.
/// A part counts once, and a part that you own is not split into its own parts.
fn gold_need(sd: &StaticData, id: i64, owned: &[i64]) -> u32 {
    fn walk(sd: &StaticData, id: i64, depth: u8, pool: &mut Vec<i64>, have: &mut u32) {
        if depth == 0 {
            return;
        }
        for &part in sd.item_info.get(&id).map_or(&[][..], |i| &i.from[..]) {
            if let Some(at) = pool.iter().position(|&o| o == part) {
                pool.swap_remove(at);
                *have += sd.item_info.get(&part).map_or(0, |i| i.total);
            } else {
                walk(sd, part, depth - 1, pool, have);
            }
        }
    }
    let (mut pool, mut have) = (owned.to_vec(), 0);
    walk(sd, id, RECIPE_DEPTH, &mut pool, &mut have);
    sd.item_info
        .get(&id)
        .map_or(0, |i| i.total)
        .saturating_sub(have)
}

/// The part to buy now when your gold does not pay for the whole item: the most expensive part of its
/// recipe tree that you do not own and that your gold pays for. A part that you own is skipped with its
/// own parts, as in `gold_need`, and the cost of a part is its own need. `None` when no part fits.
fn buy_now(sd: &StaticData, id: i64, owned: &[i64], gold: u32) -> Option<i64> {
    fn walk(
        sd: &StaticData,
        id: i64,
        depth: u8,
        pool: &mut Vec<i64>,
        gold: u32,
        best: &mut Option<(u32, i64)>,
    ) {
        if depth == 0 {
            return;
        }
        for &part in sd.item_info.get(&id).map_or(&[][..], |i| &i.from[..]) {
            if let Some(at) = pool.iter().position(|&o| o == part) {
                pool.swap_remove(at);
                continue;
            }
            let cost = gold_need(sd, part, pool);
            if cost > 0 && cost <= gold && best.is_none_or(|(c, _)| cost > c) {
                *best = Some((cost, part));
            }
            walk(sd, part, depth - 1, pool, gold, best);
        }
    }
    let (mut pool, mut best) = (owned.to_vec(), None);
    walk(sd, id, RECIPE_DEPTH, &mut pool, gold, &mut best);
    best.map(|(_, part)| part)
}

/// The build that fits what you own: the one with the most items in common with your items (its
/// core, later and other items). A tie goes to the first build, the one that most players take.
fn pick_build(builds: &[Build], owned: &[i64]) -> usize {
    let overlap = |b: &Build| -> usize {
        let all: HashSet<i64> = b
            .items
            .iter()
            .chain(&b.later)
            .chain(&b.situational)
            .copied()
            .collect();
        all.iter().filter(|id| owned.contains(id)).count()
    };
    let mut best = 0;
    for (i, b) in builds.iter().enumerate().skip(1) {
        if overlap(b) > overlap(&builds[best]) {
            best = i;
        }
    }
    best
}

/// Your damage class: mostly ability power, mostly attack damage, or mixed (or unknown). It comes
/// from the gold of the damage items in the core of your build, and from the damage split of your
/// champion when the core has none.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Ap,
    Ad,
    Mixed,
}

fn class_of(sd: &StaticData, build: &Build, details: &ChampStats) -> Class {
    let (ap, ad) = item_gold(sd, &build.items);
    let share = if ap + ad > 0.0 {
        Some(ap / (ap + ad))
    } else {
        details.magic_share
    };
    match share {
        Some(m) if m >= CLASS_AP => Class::Ap,
        Some(m) if m <= CLASS_AD => Class::Ad,
        _ => Class::Mixed,
    }
}

/// A tank build: two core items with health and armor or magic resist, or the style "tank".
fn is_tank_build(sd: &StaticData, build: &Build) -> bool {
    let tanky = build
        .items
        .iter()
        .filter_map(|id| sd.item_info.get(id))
        .filter(|i| i.cats & cat::HEALTH != 0 && i.cats & (cat::ARMOR | cat::MR) != 0)
        .count();
    tanky >= 2 || build.name.eq_ignore_ascii_case("tank")
}

/// The bonus of a rule, less for each finished item of the kind that you own: all of it with
/// none, half with one, and nothing with two or more.
fn shrink(points: i32, owned: usize) -> i32 {
    match owned {
        0 => points,
        1 => points / 2,
        _ => 0,
    }
}

/// The bonus of a resist rule for one bit: `min(shrink(resist, owned), round(resist * factor))`,
/// with the factor of your own resist (`stats::factor`). It is computed once for each bit.
fn resist_bonus(resist: i32, owned: usize, have: Option<f32>) -> i32 {
    shrink(resist, owned).min((resist as f32 * stats::factor(have)).round() as i32)
}

/// A rule that can add a resist item: the resist bit (`cat::MR` or `cat::ARMOR`), the reason, the
/// inject list for your class (the first item that the shop sells comes in), and the priority of its
/// reason on a tie. The points are the bonus of the bit in `Pass` (`Pass::bonus`). The first trigger
/// that adds an item for a bit is the only one for that bit.
struct Trigger {
    bit: u16,
    reason: Reason,
    list: &'static [i64],
    priority: u8,
}

/// The points that a rule gives to one candidate. `resist` hits of one item combine with `max()`,
/// the other hits add up. On a tie of points, the lower `priority` gives the reason.
struct Hit {
    points: i32,
    priority: u8,
    reason: Reason,
    resist: bool,
}

/// What one pass of `rank` knows before its candidates. The hooks of the child modules read it.
struct Pass<'a> {
    input: &'a Input<'a>,
    /// The build that fits your items, and its index in `details.builds` (the style).
    build: &'a Build,
    build_at: usize,
    class: Class,
    tank: bool,
    /// Your items with their earlier forms (`with_earlier_forms`).
    held: Vec<i64>,
    /// How many finished non-boots items you own, and whether you own a finished pair of boots.
    #[allow(dead_code)] // For the hooks: no hook reads it yet, `rank` uses its own copy.
    finished: usize,
    #[allow(dead_code)] // For the hooks: no hook reads it yet, `rank` uses its own copy.
    owns_boots: bool,
    /// The bonus of each resist bit, after `shrink` and `stats::factor`.
    mr_bonus: i32,
    armor_bonus: i32,
    #[allow(dead_code)] // For the hooks: no hook reads it yet, `rank` uses its own copy.
    rules: Rules,
}

impl Pass<'_> {
    /// The bonus of a resist bit: `mr_bonus` for `cat::MR`, `armor_bonus` for `cat::ARMOR`, else 0.
    fn bonus(&self, bit: u16) -> i32 {
        match bit {
            cat::MR => self.mr_bonus,
            cat::ARMOR => self.armor_bonus,
            _ => 0,
        }
    }
}

/// A candidate before the score: the item, its base score and the data reason.
struct Cand {
    id: i64,
    base: i32,
    reason: Reason,
}

/// Add a candidate. When the item comes twice, the higher base wins, and the first one wins a tie.
fn put(cands: &mut HashMap<i64, Cand>, id: i64, base: i32, reason: Reason) {
    if cands.get(&id).is_none_or(|c| c.base < base) {
        cands.insert(id, Cand { id, base, reason });
    }
}

/// Why a candidate does not make the list.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Fate {
    Stays,
    /// Your slots are full and you own no part of it.
    NoSlot,
    /// You own it, it is part of an item that you own, the shop does not sell it, or it is a second pair of boots.
    Out,
}

/// A candidate with its score.
#[derive(Debug)]
pub(crate) struct Scored {
    id: i64,
    score: i32,
    price: u32,
    /// The points that the rules added, 0 when no rule touched the item.
    rule: i32,
    /// The reason to show.
    reason: Reason,
    boots: bool,
}

/// The scored candidates of `advise`, best first, with the numbers that the tests check.
pub(crate) struct Ranking {
    scored: Vec<Scored>,
    build: usize,
    /// At least one candidate fell out because the slots are full.
    no_slot: bool,
    /// How many finished items you own.
    finished: usize,
    /// The team rules that fired.
    rules: RuleState,
    /// The fed enemy when the fed rule fired.
    fed_rule: Option<i64>,
    /// The path that drove the candidates, when the path tracker found one.
    path: Option<path::PathPick>,
}

fn rank(input: &Input) -> Ranking {
    let (sd, me, details) = (input.sd, input.me, input.details);
    let builds = &details.builds;
    let owned: &[i64] = &me.items;
    let info = |id: i64| sd.item_info.get(&id);
    let built_in: HashSet<i64> = owned.iter().flat_map(|&id| recipe_tree(sd, id)).collect();
    let finished = finished_items(sd, owned);
    let free_slots = 6usize.saturating_sub(owned.len());
    // The basic Boots are no pair yet: their upgrades stay on the list.
    let owns_boots = owned.iter().any(|&id| {
        info(id).is_some_and(|i| {
            i.cats & cat::BOOTS != 0 && (i.total > BASIC_BOOTS_GOLD || !i.from.is_empty())
        })
    });
    let held = with_earlier_forms(sd, owned);
    let build_at = pick_build(builds, &held);
    let build = &builds[build_at];
    let class = class_of(sd, build, details);
    let tank = is_tank_build(sd, build);
    let rules = Rules::of(input.ctx, input.prev_rules);
    // The finished items that you own with one of these tags. Boots do not count.
    let owned_with = |bits: u16| {
        owned
            .iter()
            .filter(|&&id| {
                info(id).is_some_and(|i| {
                    i.cats & bits != 0 && i.cats & cat::BOOTS == 0 && i.total >= FINISHED_GOLD
                })
            })
            .count()
    };
    // The bonus of each rule, less for each such item that you own, and less with your own resist.
    let resist = if tank
        || matches!(
            build.name.to_ascii_lowercase().as_str(),
            "utility" | "bruiser"
        ) {
        RESIST_TANK
    } else {
        RESIST_CARRY
    };
    let stats = input.stats.unwrap_or_default();
    let pass = Pass {
        input,
        build,
        build_at,
        class,
        tank,
        held,
        finished,
        owns_boots,
        mr_bonus: resist_bonus(resist, owned_with(cat::MR), stats.mr),
        armor_bonus: resist_bonus(resist, owned_with(cat::ARMOR), stats.armor),
        rules,
    };
    let pen = match class {
        Class::Ap => cat::MAGIC_PEN,
        Class::Ad => cat::ARMOR_PEN,
        Class::Mixed => 0,
    };
    let pen_bonus = shrink(PEN_BONUS, owned_with(pen));

    // The resist triggers, in inject order: the killer and the fed enemy, then the damage mix.
    let threat = threat::triggers(&pass);
    let class_list = |[tank_list, ap_list, ad_list]: [&'static [i64]; 3]| -> &'static [i64] {
        if tank {
            tank_list
        } else {
            match class {
                Class::Ap => ap_list,
                Class::Ad => ad_list,
                Class::Mixed => &[],
            }
        }
    };
    let mut threat_triggers = threat.triggers;
    let mut mix_triggers: Vec<Trigger> = Vec::new();
    mix_triggers.extend(rules.vs_ap.map(|ap| Trigger {
        bit: cat::MR,
        reason: Reason::VsAp(ap),
        list: class_list([&MR_TANK, &MR_AP, &MR_AD]),
        priority: PRIO_RESIST,
    }));
    mix_triggers.extend(rules.vs_ad.map(|ad| Trigger {
        bit: cat::ARMOR,
        reason: Reason::VsAd(ad),
        list: class_list([&ARMOR_TANK, &ARMOR_AP, &ARMOR_AD]),
        priority: PRIO_RESIST,
    }));
    // A bonus of 0 gives no hit and no inject.
    threat_triggers.retain(|t| pass.bonus(t.bit) > 0);
    mix_triggers.retain(|t| pass.bonus(t.bit) > 0);

    let fate = |id: i64| -> Fate {
        let Some(i) = info(id) else { return Fate::Out };
        if owned.contains(&id)
            || built_in.contains(&id)
            || !i.in_store
            || i.cats & (cat::CONSUMABLE | cat::TRINKET) != 0
            || (i.cats & cat::BOOTS != 0 && owns_boots)
        {
            Fate::Out
        } else if free_slots == 0 && !recipe_tree(sd, id).iter().any(|p| owned.contains(p)) {
            Fate::NoSlot
        } else {
            Fate::Stays
        }
    };

    // Step 1: the candidates, each with its base score and its data reason. A path that drives the
    // card gives the core, later and other-build candidates. Without one, the build gives them.
    let mut cands: HashMap<i64, Cand> = HashMap::new();
    let path = path::data_cands(&pass);
    if let Some(p) = &path {
        for &(id, base, reason) in &p.cands {
            put(&mut cands, id, base, reason);
        }
    } else {
        for (i, &id) in build.items.iter().enumerate() {
            put(
                &mut cands,
                id,
                CORE_BASE - CORE_STEP * i as i32,
                Reason::Core(i as u8 + 1),
            );
        }
    }
    if !build.boots_skipped {
        // The boots come in pick rate order: the most played pair gets the higher score, so a tie
        // on price never puts a rare pair first.
        for (i, &id) in build.boots.iter().take(2).enumerate() {
            let base = if finished >= 1 {
                BOOTS_LATE
            } else {
                BOOTS_EARLY
            };
            put(&mut cands, id, base - BOOTS_STEP * i as i32, Reason::Boots);
        }
    }
    if path.is_none() {
        for (j, &id) in build.later.iter().take(8).enumerate() {
            put(
                &mut cands,
                id,
                LATER_BASE - LATER_STEP * j as i32,
                Reason::Later,
            );
        }
    }
    // The slot lists count the boots: with a finished pair, your next item is one slot further.
    let slot = finished + usize::from(owns_boots);
    if let Some(row) = details.slots.get(slot) {
        for (k, &id) in row.iter().take(4).enumerate() {
            put(
                &mut cands,
                id,
                SLOT_BASE - SLOT_STEP * k as i32,
                Reason::Slot(slot as u8 + 1),
            );
        }
    }
    if path.is_none() {
        for (k, &id) in build.situational.iter().take(6).enumerate() {
            put(
                &mut cands,
                id,
                OTHER_BASE - OTHER_STEP * k as i32,
                Reason::OtherBuild,
            );
        }
    }
    // Items that the data ties to an augment come in even when the build lacks them.
    for l in details.aug_links.iter().filter(|l| l.strong()) {
        if input.known.contains(&l.augment) {
            put(
                &mut cands,
                l.item,
                LINK_KNOWN_BASE,
                Reason::Pairs {
                    augment: l.augment,
                    known: true,
                },
            );
        } else if input.offered.contains(&l.augment) {
            put(
                &mut cands,
                l.item,
                LINK_OFFERED_BASE,
                Reason::Pairs {
                    augment: l.augment,
                    known: false,
                },
            );
        }
    }

    // The injects: a rule adds an item when the candidates lack one of its kind. At most
    // `MAX_INJECTS` in one pass, in the order killer, fed, healer, mix, and one for each resist bit.
    let mut injects = 0;
    let has_resist = |cands: &HashMap<i64, Cand>, bit: u16| {
        cands.keys().any(|&id| {
            info(id).is_some_and(|i| i.cats & bit != 0 && i.tags <= MAX_RULE_TAGS)
                && fate(id) == Fate::Stays
        })
    };
    let inject_resist = |cands: &mut HashMap<i64, Cand>, t: &Trigger, injects: &mut usize| {
        if *injects < MAX_INJECTS
            && !has_resist(cands, t.bit)
            && let Some(&id) = t.list.iter().find(|&&id| fate(id) == Fate::Stays)
        {
            put(cands, id, RESIST_BASE, t.reason);
            *injects += 1;
        }
    };
    // The killer and the fed enemy.
    for t in &threat_triggers {
        inject_resist(&mut cands, t, &mut injects);
    }
    // Healers: add the anti-heal item of your class when the candidates have none and you own none.
    if let Some(n) = rules.healers {
        let have = cands
            .keys()
            .any(|&id| is_anti_heal(id) && fate(id) == Fate::Stays);
        let own = owned
            .iter()
            .any(|&id| is_anti_heal(id) && info(id).is_some_and(|i| i.total >= FINISHED_GOLD));
        let list: &[i64] = if tank {
            &ANTI_HEAL_TANK
        } else {
            match class {
                Class::Ap => &ANTI_HEAL_AP,
                Class::Ad => &ANTI_HEAL_AD,
                Class::Mixed => &[],
            }
        };
        if !have
            && !own
            && injects < MAX_INJECTS
            && let Some(&id) = list.iter().find(|&&id| fate(id) == Fate::Stays)
        {
            put(&mut cands, id, ANTI_HEAL_BASE, Reason::AntiHeal(n));
            injects += 1;
        }
    }
    // The damage mix: add the resist item of your class when no candidate has that resist.
    for t in &mix_triggers {
        inject_resist(&mut cands, t, &mut injects);
    }

    // Step 2: drop what cannot be bought, then add the bonuses.
    let mut no_slot = false;
    let mut scored = Vec::new();
    for c in cands.into_values() {
        match fate(c.id) {
            Fate::Out => continue,
            Fate::NoSlot => {
                no_slot = true;
                continue;
            }
            Fate::Stays => {}
        }
        let Some(item) = info(c.id) else { continue };
        let mut score = c.base;
        if recipe_tree(sd, c.id).iter().any(|p| owned.contains(p)) {
            score += STABLE;
        }
        // The rules. An item with many tags is left alone. The resist hits of all triggers combine
        // with `max()`; the other hits add up.
        let mut hits: Vec<Hit> = Vec::new();
        if item.tags <= MAX_RULE_TAGS {
            for t in threat_triggers
                .iter()
                .chain(&mix_triggers)
                .filter(|t| item.cats & t.bit != 0)
            {
                hits.push(Hit {
                    points: pass.bonus(t.bit),
                    priority: t.priority,
                    reason: t.reason,
                    resist: true,
                });
            }
            if let Some(n) = rules.healers.filter(|_| is_anti_heal(c.id)) {
                hits.push(Hit {
                    points: ANTI_HEAL_BONUS,
                    priority: PRIO_ANTI_HEAL,
                    reason: Reason::AntiHeal(n),
                    resist: false,
                });
            }
            if let Some(n) = rules
                .tanks
                .filter(|_| item.cats & pen != 0 && pen_bonus > 0)
            {
                hits.push(Hit {
                    points: pen_bonus,
                    priority: PRIO_PEN,
                    reason: Reason::VsTanks(n),
                    resist: false,
                });
            }
            hits.extend(threat::hits(&pass, c.id, item));
        }
        let resist_points = hits
            .iter()
            .filter(|h| h.resist)
            .map(|h| h.points)
            .max()
            .unwrap_or(0);
        let rule: i32 = resist_points
            + hits
                .iter()
                .filter(|h| !h.resist)
                .map(|h| h.points)
                .sum::<i32>();
        score += rule;
        // Augment links: a known augment adds more than an offered one. A link is the share of the players
        // with the augment who buy the item. The win rate of the pair is not read.
        let (mut known_pair, mut offered_pair) = (None, None);
        for l in details.aug_links.iter().filter(|l| l.item == c.id) {
            if input.known.contains(&l.augment) {
                if l.strong() && known_pair.is_none() {
                    known_pair = Some(l.augment);
                    score += KNOWN_BONUS;
                }
            } else if l.strong() && input.offered.contains(&l.augment) && offered_pair.is_none() {
                offered_pair = Some(l.augment);
                score += OFFERED_BONUS;
            }
        }
        // The reason to show: a known pair, then a rule (the largest, and a tie goes to the lower
        // priority: killer, fed, anti-heal, penetration, resist), then an offered pair, then the data reason.
        let rule_reason = hits
            .iter()
            .max_by_key(|h| (h.points, std::cmp::Reverse(h.priority)))
            .map(|h| h.reason);
        let reason = known_pair
            .map(|augment| Reason::Pairs {
                augment,
                known: true,
            })
            .or(rule_reason)
            .or(offered_pair.map(|augment| Reason::Pairs {
                augment,
                known: false,
            }))
            .unwrap_or(c.reason);
        scored.push(Scored {
            id: c.id,
            score,
            price: item.total,
            rule,
            reason,
            boots: item.cats & cat::BOOTS != 0,
        });
    }
    scored.retain(|s| s.score >= MIN_SCORE);
    // Best first. A tie goes to the cheaper item, then to the lower id, so the order never depends on a map.
    scored.sort_by(|a, b| {
        b.score
            .cmp(&a.score)
            .then(a.price.cmp(&b.price))
            .then(a.id.cmp(&b.id))
    });
    // One pair of boots at most.
    let mut seen_boots = false;
    scored.retain(|s| !s.boots || !std::mem::replace(&mut seen_boots, true));
    Ranking {
        scored,
        build: build_at,
        no_slot,
        finished,
        rules: rules.state(),
        fed_rule: threat.fed_rule,
        path,
    }
}

/// The next items for the player. See the module description for the steps.
///
/// Picks 1 and 2 follow the score. Pick 3 goes to the best item that a rule raised (with a score of at
/// least `RULE_PICK_MIN`) when there is one, else to the next by score. Before your first finished
/// item, the rule item must also be within `RULE_PICK_GAP` points of the next by score. Hexsight
/// suggests; it buys nothing.
pub fn advise(input: &Input) -> Advice {
    let mut out = Advice {
        ctx: input.ctx.cloned(),
        me_dead: input.me.dead,
        has_gold: input.gold.is_some(),
        recap: input.recap.cloned(),
        killer: input.killer,
        my_stats: input.stats.and_then(|s| {
            Some((
                s.armor?.round().max(0.0) as u16,
                s.mr?.round().max(0.0) as u16,
            ))
        }),
        ..Default::default()
    };
    if input.details.builds.is_empty() {
        out.no_build = true;
        return out;
    }
    let ranking = rank(input);
    let build = &input.details.builds[ranking.build];
    out.style = build.name.clone();
    let owned = &input.me.items;
    let mut chosen: Vec<&Scored> = ranking.scored.iter().take(2).collect();
    let next = ranking.scored.get(2);
    let third = ranking
        .scored
        .iter()
        .skip(2)
        .find(|s| {
            s.rule > 0
                && s.score >= RULE_PICK_MIN
                && (ranking.finished >= 1
                    || next.is_some_and(|n| n.score - s.score <= RULE_PICK_GAP))
        })
        .or(next);
    chosen.extend(third);
    out.items = chosen
        .into_iter()
        .take(MAX_PICKS)
        .map(|s| {
            let need = gold_need(input.sd, s.id, owned);
            ItemPick {
                id: s.id,
                reason: s.reason,
                need,
                ready: input.gold.is_some_and(|g| g >= need),
                part: input
                    .gold
                    .filter(|&g| g < need)
                    .and_then(|g| buy_now(input.sd, s.id, owned, g)),
            }
        })
        .collect();
    out.slots_full = ranking.no_slot && out.items.is_empty() && owned.len() >= 6;
    out.rules = ranking.rules;
    out.fed_rule = ranking.fed_rule;
    let held = with_earlier_forms(input.sd, owned);
    // The items you could still want: from the path when one drives the card, else from the build.
    let build_items: Vec<i64> = match &ranking.path {
        Some(p) => p.wanted.clone(),
        None => build.items.iter().chain(&build.later).copied().collect(),
    };
    let mut wanted: Vec<i64> = out
        .items
        .iter()
        .map(|p| p.id)
        .chain(build_items)
        .filter(|id| !held.contains(id))
        .collect();
    wanted.sort_unstable();
    wanted.dedup();
    out.wanted = wanted;
    out.path = ranking.path.as_ref().and_then(|p| p.path);
    out.path_item = ranking.path.as_ref().and_then(|p| p.item);
    let picks: Vec<i64> = out.items.iter().map(|p| p.id).collect();
    out.plan = path::plan(input, build, ranking.path.as_ref(), &picks);
    out
}

// ---------------------------------------------------------------------------
// The mark "fits your build".

/// The item that makes an offered augment fit your build: the data ties the augment to an item
/// that you could still want (`Advice::wanted`). The first strong link counts. The mark has no
/// number, and it does not rank the cards.
pub fn fit_item(details: &ChampStats, wanted: &[i64], augment: i64) -> Option<i64> {
    details
        .aug_links
        .iter()
        .find(|l: &&AugLink| l.augment == augment && l.strong() && wanted.contains(&l.item))
        .map(|l| l.item)
}

#[cfg(test)]
mod tests {
    use super::*;

    use super::testkit::*;

    // ---- The player list.

    #[test]
    fn the_player_list_gives_items_scores_team_and_the_row_of_the_player() {
        let list: Value = serde_json::from_str(PLAYERLIST).unwrap();
        let sd = sd();
        let r = parse_players(&list, "Player3", &sd.champions).unwrap();
        assert_eq!(r.players.len(), 10);
        assert_eq!(r.me, 2);
        assert_eq!(r.me().champion, 74);
        assert_eq!(r.me().items, vec![3802, 1058]);
        // Ahri: Luden's and Sorcerer's Shoes. The potion (a consumable) and the trinket (slot 6) are no items.
        let ahri = &r.players[0];
        assert_eq!(
            (ahri.champion, ahri.side, ahri.items.clone()),
            (1, Side::Order, vec![6655, 3020])
        );
        assert_eq!(
            (ahri.kills, ahri.deaths, ahri.assists, ahri.level, ahri.dead),
            (3, 1, 5, 9, false)
        );
        // Zed is on the other side, with a score line.
        let zed = &r.players[5];
        assert_eq!((zed.champion, zed.side), (4, Side::Chaos));
        assert_eq!((zed.kills, zed.deaths, zed.assists), (9, 2, 4));
        assert!(r.players[4].dead);
        // The enemies are the five of the other side.
        let enemy_champs: Vec<i64> = r.enemies().map(|p| p.champion).collect();
        assert_eq!(enemy_champs.len(), 5);
        assert!(r.enemies().all(|p| p.side == Side::Chaos));
    }

    #[test]
    fn consumables_and_the_trinket_are_not_items() {
        let list = serde_json::json!([{
            "riotId": "A", "team": "ORDER", "rawChampionName": "game_character_displayname_Ahri",
            "items": [
                {"itemID": 6655, "slot": 0, "consumable": false, "count": 1},
                {"itemID": 2003, "slot": 1, "consumable": true, "count": 3},
                {"itemID": 3340, "slot": 6, "consumable": false, "count": 1},
                {"itemID": 0, "slot": 2},
                {"itemID": 3020, "consumable": false}
            ]
        }]);
        let r = parse_players(&list, "A", &sd().champions).unwrap();
        // The slot is optional: an item without a slot counts. Slot 6 and a consumable do not.
        assert_eq!(r.me().items, vec![6655, 3020]);
    }

    #[test]
    fn a_player_with_missing_fields_still_parses() {
        let list: Value = serde_json::from_str(PLAYERLIST).unwrap();
        // The seventh entry of the fixture has no `items` and no `scores`.
        assert!(list[6].get("items").is_none() && list[6].get("scores").is_none());
        let r = parse_players(&list, "Player3", &sd().champions).unwrap();
        let hole = &r.players[6];
        assert_eq!(
            (
                hole.champion,
                hole.side,
                hole.items.len(),
                hole.kills,
                hole.deaths,
                hole.assists
            ),
            (5, Side::Chaos, 0, 0, 0, 0)
        );
        // A row with nothing but a name parses too: no champion, no side.
        let bare = parse_players(
            &serde_json::json!([{ "riotId": "X" }, "not a player", {}]),
            "X",
            &sd().champions,
        )
        .unwrap();
        assert_eq!(bare.players.len(), 3);
        assert_eq!(bare.me(), &Player::default());
        // The side is read in any case, and a strange word is no side.
        let sides = parse_players(&serde_json::json!([{ "riotId": "X", "team": "order" }, { "team": "NEUTRAL" }, { "team": 5 }]), "X", &sd().champions).unwrap();
        assert_eq!(
            sides.players.iter().map(|p| p.side).collect::<Vec<_>>(),
            vec![Side::Order, Side::Unknown, Side::Unknown]
        );
        // Without a side for you, there are no enemies.
        assert_eq!(sides.enemies().count(), 0);
    }

    #[test]
    fn a_list_without_the_player_gives_none() {
        let list: Value = serde_json::from_str(PLAYERLIST).unwrap();
        let champions = sd().champions;
        assert!(parse_players(&list, "Nobody", &champions).is_none());
        // No name to look for finds no one, also when a row has no name.
        assert!(parse_players(&list, "", &champions).is_none());
        assert!(
            parse_players(
                &serde_json::json!([{ "championName": "Ahri" }]),
                "",
                &champions
            )
            .is_none()
        );
        // Not a list.
        for bad in [
            Value::Null,
            serde_json::json!({}),
            serde_json::json!("x"),
            serde_json::json!([]),
        ] {
            assert!(
                parse_players(&bad, "Player3", &champions).is_none(),
                "{bad}"
            );
        }
        // The summoner name stands in for a missing Riot ID.
        let old = serde_json::json!([{ "summonerName": "Old" }]);
        assert!(parse_players(&old, "Old", &champions).is_some());
    }

    #[test]
    fn the_player_list_of_a_real_mayhem_game_parses() {
        // The list of a real Mayhem game, with the names and the champions replaced and the fields cut
        // down. It shows what the first fixture only guessed: the
        // gold of the game is a float, a potion has no `consumable` flag, an item slot can be skipped
        // (the slots 0, 1 and 4), the trinket is in the slot 6, and a champion can be one that
        // Hexsight does not know (Player2 here: its name is lost to the replacement, so the alias fits no champion).
        let list: Value = serde_json::from_str(PLAYERLIST_REAL).unwrap();
        let aliases = [
            "Ashe", "Akali", "Rammus", "Jinx", "Velkoz", "Kaisa", "Amumu", "Nautilus", "Caitlyn",
        ];
        let champions: HashMap<i64, Champion> = aliases
            .iter()
            .enumerate()
            .map(|(i, a)| {
                (
                    i as i64 + 1,
                    Champion {
                        id: i as i64 + 1,
                        name: (*a).into(),
                        alias: (*a).into(),
                        icon: String::new(),
                    },
                )
            })
            .collect();
        let r = parse_players(&list, "Player10", &champions).unwrap();
        assert_eq!((r.players.len(), r.me), (10, 9));
        // Caitlyn (id 9) is on the side CHAOS, so the five ORDER players are the enemies.
        assert_eq!(r.me().champion, 9);
        assert_eq!(r.me().side, Side::Chaos);
        assert_eq!(r.me().items, vec![3508, 3070, 3044, 1042]);
        assert_eq!(r.enemies().count(), 5);
        // The trinket (slot 6) is out, and so is an unknown champion's id (0). The dead player is dead.
        assert_eq!(r.players[0].items, vec![3032, 3144, 1042]);
        assert_eq!(r.players[0].champion, 1);
        assert_eq!((r.players[1].champion, r.players[1].dead), (0, true));
        assert_eq!(r.players[2].items, vec![6676, 3070, 1001]);
        assert_eq!(
            (
                r.players[5].kills,
                r.players[5].deaths,
                r.players[5].assists,
                r.players[5].level
            ),
            (5, 4, 11, 8)
        );
        // A potion has no flag in this list, so it stays among the items (it holds a slot in the game).
        assert_eq!(r.players[3].items, vec![3118, 2031, 3020]);
        // The summary and the roster work on it with no data of items: no panic, three enemies known at least.
        let c = context_of(&r, &StaticData::default());
        assert_eq!(c.enemies, 5);
    }

    #[test]
    fn the_roster_keeps_no_name() {
        // Listing every field of the types: a new field (a name) would not compile here.
        let r = parse_players(
            &serde_json::from_str(PLAYERLIST).unwrap(),
            "Player3",
            &sd().champions,
        )
        .unwrap();
        let Roster { players, me: _ } = r;
        for p in players {
            let Player {
                champion: _,
                side: _,
                items: _,
                kills: _,
                deaths: _,
                assists: _,
                level: _,
                dead: _,
            } = p;
        }
        let Fed {
            champion: _,
            kills: _,
            deaths: _,
            assists: _,
            level: _,
        } = Fed {
            champion: 1,
            kills: 1,
            deaths: 1,
            assists: 1,
            level: 1,
        };
        // No text of the fixture's names reaches the debug text of the roster.
        let r = parse_players(
            &serde_json::from_str(PLAYERLIST).unwrap(),
            "Player3",
            &sd().champions,
        )
        .unwrap();
        for n in 1..=10 {
            assert!(
                !format!("{r:?}").contains(&format!("Player{n}")),
                "Player{n}"
            );
        }
    }

    // ---- The game context.

    #[test]
    fn the_champion_role_is_the_prior_and_items_take_over() {
        let sd = sd();
        // Three mages, no items: all ability power.
        let none = roster(vec![enemy(1, &[]), enemy(7, &[]), enemy(1, &[])]);
        assert_eq!(context_of(&none, &sd).ap_pct, Some(100));
        // The prior is worth 1500 gold. 3000 gold of attack damage items (Black Cleaver) take it to a third:
        // 1500 / (1500 + 3000) is 33 %, and the mix goes in steps of 5.
        let bought = roster(vec![
            enemy(1, &[3071]),
            enemy(7, &[3071]),
            enemy(1, &[3071]),
        ]);
        assert_eq!(context_of(&bought, &sd).ap_pct, Some(35));
        // A mixed team: mage 100, marksman 0, fighter 25. The mean is 41.7 %, which rounds to 40.
        let mixed = roster(vec![enemy(1, &[]), enemy(2, &[]), enemy(3, &[])]);
        assert_eq!(context_of(&mixed, &sd).ap_pct, Some(40));
        // Boots, health, armor and a potion add no damage.
        let tanky = roster(vec![
            enemy(1, &[3111, 3075, 2003]),
            enemy(1, &[]),
            enemy(1, &[]),
        ]);
        assert_eq!(context_of(&tanky, &sd).ap_pct, Some(100));
        // An item with both tags (Nashor's Tooth: attack speed and ability power) adds half to each.
        let (ap, ad) = item_gold(&sd, &[3115]);
        assert_eq!(ap, ad);
        assert_eq!(ap + ad, 2900.0);
    }

    #[test]
    fn a_fed_enemy_weighs_one_and_a_half() {
        let sd = sd();
        // A mage (share 1), a marksman (0) and a fighter (0.25).
        let calm = roster(vec![
            scored_enemy(1, &[], 2, 2, 2),
            enemy(2, &[]),
            enemy(3, &[]),
        ]);
        assert_eq!(context_of(&calm, &sd).ap_pct, Some(40));
        // The mage goes 9/2: its weight is 1.5, so the mean is (1.5 + 0 + 0.25) / 3.5 = 50 %.
        let fed = roster(vec![
            scored_enemy(1, &[], 9, 2, 4),
            enemy(2, &[]),
            enemy(3, &[]),
        ]);
        assert_eq!(context_of(&fed, &sd).ap_pct, Some(50));
        // The weight does not hide the other champions: a fed marksman moves the mix to attack damage.
        let adc = roster(vec![
            enemy(1, &[]),
            scored_enemy(2, &[], 9, 2, 4),
            enemy(3, &[]),
        ]);
        assert_eq!(context_of(&adc, &sd).ap_pct, Some(35));
    }

    #[test]
    fn fed_needs_a_net_of_five_and_six_kills() {
        let p = |k, d| Player {
            kills: k,
            deaths: d,
            ..Default::default()
        };
        assert!(is_fed(&p(6, 1)));
        assert!(!is_fed(&p(5, 0)));
        assert!(is_fed(&p(9, 4)));
        assert!(!is_fed(&p(8, 4)));
        assert!(is_fed(&p(20, 0)));
        assert!(!is_fed(&p(0, 0)));
        // The summary shows a fed enemy with its score line and level.
        let r = roster(vec![
            Player {
                level: 12,
                ..scored_enemy(4, &[], 9, 2, 4)
            },
            enemy(1, &[]),
            enemy(2, &[]),
        ]);
        let fed = context_of(&r, &sd()).fed.unwrap();
        assert_eq!(
            fed,
            Fed {
                champion: 4,
                kills: 9,
                deaths: 2,
                assists: 4,
                level: 12
            }
        );
        assert!(
            context_of(
                &roster(vec![scored_enemy(4, &[], 5, 0, 0), enemy(1, &[])]),
                &sd()
            )
            .fed
            .is_none()
        );
    }

    #[test]
    fn the_fed_enemy_is_the_highest_net_and_a_tie_goes_to_more_kills() {
        let r = roster(vec![
            scored_enemy(1, &[], 6, 0, 0),
            scored_enemy(4, &[], 8, 1, 0),
            scored_enemy(2, &[], 9, 2, 0),
            scored_enemy(3, &[], 7, 5, 0),
        ]);
        // 6/0 is net 6, 8/1 is net 7, 9/2 is net 7, and 7/5 is not fed. The tie of 8/1 and 9/2 goes to the 9 kills.
        assert_eq!(context_of(&r, &sd()).fed.map(|f| f.champion), Some(2));
        // A higher net beats more kills.
        let r = roster(vec![
            scored_enemy(1, &[], 12, 5, 0),
            scored_enemy(2, &[], 8, 0, 0),
        ]);
        assert_eq!(context_of(&r, &sd()).fed.map(|f| f.champion), Some(2));
    }

    #[test]
    fn healers_count_once_by_alias_or_lifesteal_item() {
        let sd = sd();
        let r = roster(vec![
            // Soraka is in the list and also buys Bloodthirster: one healer.
            enemy(6, &[3072]),
            // Garen is no healer by name, but Ravenous Hydra (a finished item of lifesteal) heals.
            enemy(3, &[3074]),
            // Jinx has none of it.
            enemy(2, &[3031]),
            enemy(8, &[]),
            enemy(5, &[]),
        ]);
        // No enemy is fed and all have 0 kills: the healers come by id.
        assert_eq!(context_of(&r, &sd).healers, vec![3, 6, 8]);
        // The alias match ignores case, and a champion without an alias entry is no healer.
        let mut low = sd;
        low.champions.get_mut(&2).unwrap().alias = "soraka".into();
        assert_eq!(context_of(&r, &low).healers, vec![2, 3, 6, 8]);
        // An enemy that Hexsight cannot name is not counted, whatever it owns.
        assert!(
            context_of(&roster(vec![enemy(0, &[3072])]), &low)
                .healers
                .is_empty()
        );
    }

    #[test]
    fn an_enemy_is_tanky_with_two_resist_items() {
        let sd = sd();
        // Thornmail (armor) and Spirit Visage (magic resist): two resist items.
        let r = roster(vec![
            enemy(3, &[3075, 3065]),
            enemy(3, &[3075]),
            enemy(5, &[3075]),
            enemy(5, &[]),
            enemy(3, &[3111, 3075]),
        ]);
        // The first has two. The second has one and no tank role. The third is a tank with one. The fourth has none.
        // The fifth has Mercury's Treads, which are boots and do not count.
        assert_eq!(context_of(&r, &sd).tanks, vec![3, 5]);
        // Health items without resist do not count, and Frozen Heart and Thornmail both do.
        assert_eq!(
            context_of(&roster(vec![enemy(3, &[2504, 3110])]), &sd)
                .tanks
                .len(),
            1
        );
        assert!(
            context_of(&roster(vec![enemy(3, &[3071, 3074])]), &sd)
                .tanks
                .is_empty()
        );
    }

    #[test]
    fn the_mix_needs_three_known_enemies() {
        let sd = sd();
        assert_eq!(
            context_of(&roster(vec![enemy(1, &[]), enemy(2, &[])]), &sd).ap_pct,
            None
        );
        // An enemy that Hexsight cannot name does not count as known.
        assert_eq!(
            context_of(
                &roster(vec![enemy(1, &[]), enemy(2, &[]), enemy(0, &[])]),
                &sd
            )
            .ap_pct,
            None
        );
        assert!(
            context_of(
                &roster(vec![enemy(1, &[]), enemy(2, &[]), enemy(3, &[])]),
                &sd
            )
            .ap_pct
            .is_some()
        );
        // The count of enemies includes the unknown one.
        assert_eq!(
            context_of(&roster(vec![enemy(1, &[]), enemy(0, &[])]), &sd).enemies,
            2
        );
        // Your team is no enemy.
        let mut r = roster(vec![enemy(1, &[]), enemy(2, &[]), enemy(3, &[])]);
        r.players.push(Player {
            side: Side::Order,
            ..enemy(7, &[])
        });
        assert_eq!(context_of(&r, &sd).enemies, 3);
    }

    #[test]
    fn roles_are_the_prior_when_the_type_is_missing() {
        let sd = sd();
        assert_eq!(prior(&sd, 1), (1.0, 0.0));
        assert_eq!(prior(&sd, 2), (0.0, 1.0));
        assert_eq!(prior(&sd, 3), (0.25, 0.75));
        assert_eq!(prior(&sd, 4), (0.4, 0.6));
        assert_eq!(prior(&sd, 5), (0.5, 0.5));
        assert_eq!(prior(&sd, 6), (0.6, 0.4));
        // No role, or a champion that the summary lacks: half and half.
        assert_eq!(prior(&sd, 999), (0.5, 0.5));
        let mut odd = sd;
        odd.roles.insert(1, Vec::new());
        assert_eq!(prior(&odd, 1), (0.5, 0.5));
    }

    // ---- The next item.

    #[test]
    fn the_first_core_item_you_do_not_own_comes_first_with_its_number() {
        let (sd, d) = (sd(), heimer());
        let a = advise(&input_of(&[], &d, &sd));
        // Heimerdinger, style "ap": the core is Blackfire Torch, Liandry's Torment, Rylai's Crystal Scepter.
        assert_eq!(a.style, "ap");
        assert_eq!(ids(&a), vec![2503, 6653, 3116]);
        let reasons: Vec<Reason> = a.items.iter().map(|p| p.reason).collect();
        assert_eq!(
            reasons,
            vec![Reason::Core(1), Reason::Core(2), Reason::Core(3)]
        );
        // With the first item bought, the second comes first. The build buys boots, so a pair comes
        // in after the first finished item (the most played pair, Sorcerer's Shoes), before the last core item.
        let a = advise(&input_of(&[2503], &d, &sd));
        assert_eq!(ids(&a), vec![6653, 3020, 3116]);
        assert_eq!(a.items[0].reason, Reason::Core(2));
        assert_eq!(a.items[1].reason, Reason::Boots);
        assert_eq!(a.items[2].reason, Reason::Core(3));
        assert!(!a.slots_full && !a.no_build);
    }

    #[test]
    fn owned_items_and_finished_components_drop_out() {
        let sd = sd();
        // Lost Chapter (3802) is a part of Blackfire Torch (2503). Owning the torch drops the part.
        let d = with_build("ap", &[3089], &[3802, 3157, 3102]);
        let a = advise(&input_of(&[2503], &d, &sd));
        assert!(
            !ids(&a).contains(&3802) && !ids(&a).contains(&2503),
            "{:?}",
            ids(&a)
        );
        assert_eq!(ids(&a), vec![3089, 3157, 3102]);
        // An item that you own is never suggested again.
        let a = advise(&input_of(&[3089, 3157], &d, &sd));
        assert_eq!(ids(&a), vec![3802, 3102]);
        // The Hexsight data lists items that CommunityDragon does not sell or does not know: they drop out too.
        let mut unsold = sd;
        unsold.item_info.get_mut(&3157).unwrap().in_store = false;
        unsold.item_info.remove(&3102);
        let a = advise(&input_of(&[], &d, &unsold));
        assert_eq!(ids(&a), vec![3089, 3802]);
    }

    #[test]
    fn an_upgraded_item_drops_its_first_form_and_the_parts() {
        let mut sd = sd();
        // The real data: Muramana (3042) has no `from`, only `specialRecipe` = Manamune (3004).
        let item = |total, from: &[i64], upgrade_of, in_store| crate::model::ItemInfo {
            total,
            from: from.to_vec(),
            upgrade_of,
            cats: 0,
            tags: 1,
            in_store,
        };
        sd.item_info.insert(3070, item(400, &[], None, true));
        sd.item_info.insert(3004, item(2900, &[3070], None, true));
        sd.item_info
            .insert(3042, item(2900, &[], Some(3004), false));
        let d = with_build("ap", &[3004], &[3070, 3089]);
        let a = advise(&input_of(&[3042], &d, &sd));
        assert_eq!(ids(&a), vec![3089]);
        assert!(!a.wanted.contains(&3004) && !a.wanted.contains(&3042));
        // Before the upgrade the list is the same, and with no item Manamune is still on it.
        assert_eq!(ids(&advise(&input_of(&[3004], &d, &sd))), vec![3089]);
        assert!(ids(&advise(&input_of(&[], &d, &sd))).contains(&3004));
        // The upgraded item counts for the build that lists its first form.
        let builds = [
            with_build("ap", &[3089], &[]).builds.remove(0),
            with_build("ad", &[3004], &[]).builds.remove(0),
        ];
        assert_eq!(pick_build(&builds, &with_earlier_forms(&sd, &[3042])), 1);
    }

    #[test]
    fn an_owned_component_keeps_its_item_in_the_list() {
        let sd = sd();
        // Void Staff (3135) is built from Blasting Wand (1026). The list: core, then Shadowflame (62), then Void Staff (58).
        let d = with_build("ap", &[2503], &[4645, 3135]);
        let before = rank(&input_of(&[], &d, &sd));
        assert_eq!(
            before
                .scored
                .iter()
                .map(|s| s.id)
                .take(3)
                .collect::<Vec<_>>(),
            vec![2503, 4645, 3135]
        );
        assert_eq!(score_of(&before, 3135), Some(LATER_BASE - LATER_STEP));
        // You buy a Blasting Wand. Void Staff gets 8 points and now comes before Shadowflame, so the plan holds on the way to it.
        let after = rank(&input_of(&[1026], &d, &sd));
        assert_eq!(
            score_of(&after, 3135),
            Some(LATER_BASE - LATER_STEP + STABLE)
        );
        assert_eq!(
            after
                .scored
                .iter()
                .map(|s| s.id)
                .take(3)
                .collect::<Vec<_>>(),
            vec![2503, 3135, 4645]
        );
        // The part itself is not on the list: you own it.
        assert!(score_of(&after, 1026).is_none());
    }

    #[test]
    fn boots_come_once_after_the_first_finished_item() {
        let sd = sd();
        let mut d = with_build("ap", &[2503, 6653, 3089], &[]);
        d.builds[0].boots = vec![3020, 3158];
        // Before the first finished item the first pair scores 55: it comes after the core items. The most
        // played pair stays (Sorcerer's Shoes), and the other one is not on the list.
        let first = rank(&input_of(&[], &d, &sd));
        assert_eq!(score_of(&first, 3020), Some(BOOTS_EARLY));
        assert_eq!(score_of(&first, 3158), None);
        assert_eq!(
            first
                .scored
                .iter()
                .map(|s| s.id)
                .take(3)
                .collect::<Vec<_>>(),
            vec![2503, 6653, 3089]
        );
        // After it, the first pair scores 90. The most played pair comes first, and only one pair shows.
        let a = advise(&input_of(&[2503], &d, &sd));
        assert_eq!(ids(&a), vec![6653, 3020, 3089]);
        assert_eq!(
            score_of(&rank(&input_of(&[2503], &d, &sd)), 3020),
            Some(BOOTS_LATE)
        );
        assert_eq!(a.items[1].reason, Reason::Boots);
        let all = rank(&input_of(&[2503], &d, &sd));
        assert_eq!(all.scored.iter().filter(|s| s.boots).count(), 1);
    }

    #[test]
    fn a_build_without_boots_gives_none_and_owned_boots_drop_all() {
        let sd = sd();
        // A build that skips boots adds none (the slot list can still name a pair). Most players
        // of the real Heimerdinger build buy boots, so the test says that they skip them.
        let mut d = heimer();
        assert!(!d.builds[0].boots_skipped);
        d.builds[0].boots_skipped = true;
        let a = advise(&input_of(&[2503], &d, &sd));
        assert!(a.items.iter().all(|p| p.reason != Reason::Boots));
        // With boots in the build and a pair in your items, no pair is suggested.
        let mut d = with_build("ap", &[2503, 6653, 3089], &[]);
        d.builds[0].boots = vec![3020, 3158];
        let owns = rank(&input_of(&[2503, 3111], &d, &sd));
        assert!(owns.scored.iter().all(|s| !s.boots));
        // A pair of boots counts as no finished item.
        let a = advise(&input_of(&[3111], &d, &sd));
        assert_eq!(
            score_of(&rank(&input_of(&[3111], &d, &sd)), 2503),
            Some(CORE_BASE)
        );
        assert_eq!(ids(&a), vec![2503, 6653, 3089]);
    }

    #[test]
    fn the_build_style_follows_the_items_you_own_and_a_tie_goes_to_the_first() {
        let sd = sd();
        let mut d = with_build("ap", &[2503, 6653], &[]);
        d.builds.push(Build {
            name: "tank".into(),
            items: vec![3075, 3065],
            ..Default::default()
        });
        assert_eq!(advise(&input_of(&[], &d, &sd)).style, "ap");
        // One item of the second build is enough.
        assert_eq!(advise(&input_of(&[3065], &d, &sd)).style, "tank");
        // One item of each build: a tie, and the first build (the most played) wins.
        assert_eq!(advise(&input_of(&[3065, 2503], &d, &sd)).style, "ap");
        // The later and other items of a build count too.
        d.builds[1].later = vec![3157];
        assert_eq!(advise(&input_of(&[3157], &d, &sd)).style, "tank");
    }

    #[test]
    fn magic_resist_rises_against_an_ap_team_more_for_a_tank_than_a_carry() {
        let sd = sd();
        let against_ap = ctx(Some(70), 0, 0);
        let by = |name: &str| {
            let d = with_build(name, &[2503, 6653, 3089], &[3065]);
            let mut i = input_of(&[], &d, &sd);
            i.ctx = Some(&against_ap);
            let r = rank(&i);
            (
                score_of(&r, 3065),
                r.scored.iter().find(|s| s.id == 3065).map(|s| s.reason),
            )
        };
        // Spirit Visage (magic resist) is the first later item: 62 points. A carry gets 16, a tank 28.
        assert_eq!(
            by("ap"),
            (Some(LATER_BASE + RESIST_CARRY), Some(Reason::VsAp(70)))
        );
        assert_eq!(
            by("tank"),
            (Some(LATER_BASE + RESIST_TANK), Some(Reason::VsAp(70)))
        );
        assert_eq!(by("bruiser").0, Some(LATER_BASE + RESIST_TANK));
        assert_eq!(by("utility").0, Some(LATER_BASE + RESIST_TANK));
        // 59 % ability power is under the limit of 60 %: no rule.
        let d = with_build("ap", &[2503], &[3065]);
        let weak = ctx(Some(55), 0, 0);
        let mut i = input_of(&[], &d, &sd);
        i.ctx = Some(&weak);
        assert_eq!(score_of(&rank(&i), 3065), Some(LATER_BASE));
        // A reason of a rule is a rule of Hexsight.
        assert!(Reason::VsAp(70).is_rule() && !Reason::Later.is_rule());
    }

    #[test]
    fn armor_rises_against_an_ad_team() {
        let sd = sd();
        let d = with_build("ap", &[2503, 6653, 3089], &[3075]);
        let against_ad = ctx(Some(20), 0, 0);
        let mut i = input_of(&[], &d, &sd);
        i.ctx = Some(&against_ad);
        let r = rank(&i);
        // Thornmail (armor) is a later item with 62 points, and gets 16. The reason names the 80 % attack damage.
        assert_eq!(score_of(&r, 3075), Some(LATER_BASE + RESIST_CARRY));
        assert_eq!(
            r.scored.iter().find(|s| s.id == 3075).unwrap().reason,
            Reason::VsAd(80)
        );
        // Against a team of ability power it gets nothing.
        let against_ap = ctx(Some(80), 0, 0);
        i.ctx = Some(&against_ap);
        assert_eq!(score_of(&rank(&i), 3075), Some(LATER_BASE));
        // No context, no rule.
        i.ctx = None;
        assert_eq!(score_of(&rank(&i), 3075), Some(LATER_BASE));
    }

    #[test]
    fn two_healers_inject_the_anti_heal_item_of_your_class_and_one_does_not() {
        let sd = sd();
        // A build without any anti-heal item. Heimerdinger deals magic damage: the list for ability power.
        let mut d = heimer();
        d.builds[0].later.retain(|&id| id != 3165);
        let two = ctx(Some(50), 2, 0);
        let mut i = input_of(&[], &d, &sd);
        i.ctx = Some(&two);
        let r = rank(&i);
        // Morellonomicon comes in with 50 + 30 points and takes the third place, for the situation.
        assert_eq!(score_of(&r, 3165), Some(ANTI_HEAL_BASE + ANTI_HEAL_BONUS));
        let a = advise(&i);
        assert_eq!(ids(&a), vec![2503, 6653, 3165]);
        assert_eq!(a.items[2].reason, Reason::AntiHeal(2));
        // One healer is no reason.
        let one = ctx(Some(50), 1, 0);
        i.ctx = Some(&one);
        assert!(score_of(&rank(&i), 3165).is_none());
        assert_eq!(ids(&advise(&i)), vec![2503, 6653, 3116]);
        // The real build already lists Morellonomicon, fourth. It gets the 30 points and no second entry.
        let real = heimer();
        let mut i = input_of(&[], &real, &sd);
        i.ctx = Some(&two);
        let r = rank(&i);
        assert_eq!(r.scored.iter().filter(|s| s.id == 3165).count(), 1);
        assert_eq!(
            score_of(&r, 3165),
            Some(LATER_BASE - 3 * LATER_STEP + ANTI_HEAL_BONUS)
        );
        // An attack damage champion gets the attack damage list, and a tank build the armor list.
        let mut ad = with_build("ad", &[3031, 3071, 3036], &[]);
        ad.magic_share = Some(0.1);
        let mut i = input_of(&[], &ad, &sd);
        i.ctx = Some(&two);
        assert_eq!(
            score_of(&rank(&i), 3033),
            Some(ANTI_HEAL_BASE + ANTI_HEAL_BONUS)
        );
        let tank = with_build("tank", &[3110, 3065, 3143], &[]);
        let mut i = input_of(&[], &tank, &sd);
        i.ctx = Some(&two);
        assert_eq!(
            score_of(&rank(&i), 3075),
            Some(ANTI_HEAL_BASE + ANTI_HEAL_BONUS)
        );
        // No known class and no tank build: nothing to inject. Infinity Edge and Rabadon's Deathcap cost the same.
        let mixed = with_build("hybrid", &[3031, 3089], &[]);
        let mut i = input_of(&[], &mixed, &sd);
        i.ctx = Some(&two);
        assert!(rank(&i).scored.iter().all(|s| !is_anti_heal(s.id)));
    }

    #[test]
    fn an_owned_anti_heal_item_is_not_suggested_again() {
        let sd = sd();
        let mut d = heimer();
        d.builds[0].later.retain(|&id| id != 3165);
        let two = ctx(Some(50), 2, 0);
        for owns in [3165, 3033] {
            let mut i = input_of(&[owns], &d, &sd);
            i.ctx = Some(&two);
            let r = rank(&i);
            // The owned item is not on the list, and no other anti-heal item comes in.
            assert!(r.scored.iter().all(|s| !is_anti_heal(s.id)), "{owns}");
        }
        // Owning only the part (Oblivion Orb) keeps the finished item on offer.
        let mut i = input_of(&[3916], &d, &sd);
        i.ctx = Some(&two);
        assert_eq!(
            score_of(&rank(&i), 3165),
            Some(ANTI_HEAL_BASE + ANTI_HEAL_BONUS + STABLE)
        );
    }

    #[test]
    fn penetration_follows_your_damage_type_against_two_tanks() {
        let sd = sd();
        let tanks = ctx(Some(50), 0, 2);
        // An ability power champion gets points for magic penetration: Void Staff and Shadowflame.
        let mut ap = with_build("ap", &[2503], &[4645, 3135, 3036]);
        ap.magic_share = Some(0.9);
        let mut i = input_of(&[], &ap, &sd);
        i.ctx = Some(&tanks);
        let r = rank(&i);
        assert_eq!(
            score_of(&r, 3135),
            Some(LATER_BASE - LATER_STEP + PEN_BONUS)
        );
        assert_eq!(score_of(&r, 3036), Some(LATER_BASE - 2 * LATER_STEP));
        assert_eq!(
            r.scored.iter().find(|s| s.id == 3135).unwrap().reason,
            Reason::VsTanks(2)
        );
        // An attack damage champion gets points for armor penetration, not magic penetration.
        // Lord Dominik's Regards (armor penetration). Black Cleaver has seven tags, so no rule touches it.
        let mut ad = with_build("ad", &[3031], &[3036, 3135]);
        ad.magic_share = Some(0.05);
        let mut i = input_of(&[], &ad, &sd);
        i.ctx = Some(&tanks);
        let r = rank(&i);
        assert_eq!(score_of(&r, 3036), Some(LATER_BASE + PEN_BONUS));
        assert_eq!(score_of(&r, 3135), Some(LATER_BASE - LATER_STEP));
        // A mixed build gets none. One tank is no reason.
        let mut mixed = with_build("hybrid", &[3031, 3089], &[3036, 3135]);
        mixed.magic_share = Some(0.5);
        let mut i = input_of(&[], &mixed, &sd);
        i.ctx = Some(&tanks);
        assert_eq!(score_of(&rank(&i), 3036), Some(LATER_BASE));
        let one = ctx(Some(50), 0, 1);
        let mut i = input_of(&[], &ad, &sd);
        i.ctx = Some(&one);
        assert_eq!(score_of(&rank(&i), 3036), Some(LATER_BASE));
    }

    #[test]
    fn the_third_pick_is_kept_for_a_rule_that_fires() {
        let sd = sd();
        // Core items score 100, 92 and 84. Spirit Visage is a later item: 62 + 16 = 78 against an AP team.
        let d = with_build("ap", &[2503, 6653, 3089], &[3065]);
        let calm = ctx(Some(50), 0, 0);
        let mut i = input_of(&[], &d, &sd);
        i.ctx = Some(&calm);
        assert_eq!(ids(&advise(&i)), vec![2503, 6653, 3089]);
        // The rule fires: the third place goes to the item the rule raised, over the third core item.
        let ap = ctx(Some(80), 0, 0);
        i.ctx = Some(&ap);
        let a = advise(&i);
        assert_eq!(ids(&a), vec![2503, 6653, 3065]);
        assert_eq!(a.items[2].reason, Reason::VsAp(80));
        // Spirit Visage as the 4th later item has 50 + 16 = 66 points. That is 18 under the third core item
        // (84), more than 15: with no finished item the core item stays.
        let low = with_build("ap", &[2503, 6653, 3089], &[3157, 3118, 3116, 3065]);
        let mut i = input_of(&[], &low, &sd);
        i.ctx = Some(&ap);
        let r = rank(&i);
        assert_eq!(
            score_of(&r, 3065),
            Some(LATER_BASE - 3 * LATER_STEP + RESIST_CARRY)
        );
        assert_eq!(ids(&advise(&i)), vec![2503, 6653, 3089]);
        // With a finished item that you own (Void Staff), the rule item takes the place.
        let mut i = input_of(&[3135], &low, &sd);
        i.ctx = Some(&ap);
        assert_eq!(ids(&advise(&i)), vec![2503, 6653, 3065]);
        let very_low = with_build(
            "ap",
            &[2503, 6653, 3089],
            &[3157, 3118, 3116, 3003, 3135, 4645, 3115, 3065],
        );
        let mut i = input_of(&[3135], &very_low, &sd);
        i.ctx = Some(&ap);
        // 62 - 28 + 16 = 50, under RULE_PICK_MIN: the third core item stays, also with a finished item.
        assert!(score_of(&rank(&i), 3065).is_some_and(|s| s < RULE_PICK_MIN));
        assert_eq!(ids(&advise(&i)), vec![2503, 6653, 3089]);
    }

    #[test]
    fn an_item_with_many_tags_is_never_moved_by_a_rule() {
        let sd = sd();
        // The Golden Spatula has 13 tags, with armor and magic resist among them.
        assert!(sd.item_info[&4403].tags > MAX_RULE_TAGS);
        let d = with_build("ap", &[2503], &[4403]);
        for team in [ctx(Some(80), 0, 0), ctx(Some(20), 0, 0)] {
            let mut i = input_of(&[], &d, &sd);
            i.ctx = Some(&team);
            assert_eq!(score_of(&rank(&i), 4403), Some(LATER_BASE));
        }
    }

    #[test]
    fn an_offered_augment_adds_a_little_and_a_known_one_adds_more() {
        let sd = sd();
        let d = heimer();
        // The data ties Stormsurge (4646) to the augment 3150. The build does not list the item.
        let plain = rank(&input_of(&[], &d, &sd));
        assert!(score_of(&plain, 4646).is_none());
        let mut i = input_of(&[], &d, &sd);
        i.offered = &[3150];
        let offered = rank(&i);
        assert_eq!(
            score_of(&offered, 4646),
            Some(LINK_OFFERED_BASE + OFFERED_BONUS)
        );
        assert_eq!(
            offered.scored.iter().find(|s| s.id == 4646).unwrap().reason,
            Reason::Pairs {
                augment: 3150,
                known: false
            }
        );
        // An offered augment alone does not lift it into the three: the core items score higher.
        assert_eq!(ids(&advise(&i)), vec![2503, 6653, 3116]);
        // A known augment: 60 + 35 points. The item comes second, and the reason says that you took it.
        let mut k = input_of(&[], &d, &sd);
        k.known = &[3150];
        let a = advise(&k);
        assert_eq!(ids(&a), vec![2503, 4646, 6653]);
        assert_eq!(
            a.items[1].reason,
            Reason::Pairs {
                augment: 3150,
                known: true
            }
        );
        assert_eq!(
            score_of(&rank(&k), 4646),
            Some(LINK_KNOWN_BASE + KNOWN_BONUS)
        );
        // A weak link (Lich Bane and its augment: few players buy it) brings nothing in.
        let mut weak = input_of(&[], &d, &sd);
        weak.known = &[2018, 1205];
        let r = rank(&weak);
        assert!(score_of(&r, 3100).is_none() && score_of(&r, 3004).is_none());
        // An item of the build that the augment ties gets the points on top: a known link on a core item.
        let core = ChampStats {
            aug_links: vec![AugLink {
                item: 3089,
                augment: 77,
                with_pick: 0.9,
            }],
            ..with_build("ap", &[2503, 6653, 3089], &[])
        };
        let mut k = input_of(&[], &core, &sd);
        k.known = &[77];
        assert_eq!(
            score_of(&rank(&k), 3089),
            Some(CORE_BASE - 2 * CORE_STEP + KNOWN_BONUS)
        );
    }

    #[test]
    fn a_weak_link_changes_nothing_whatever_the_win_rate_of_the_pair_is() {
        let sd = sd();
        let d = ChampStats {
            aug_links: heimer().aug_links,
            ..with_build("ap", &[2503], &[3153])
        };
        // Blade of The Ruined King has a weak link to the augment 1029: few players buy it. The source
        // also has a win rate change for the pair, and Hexsight does not read it, so the item scores
        // the same with the augment known, offered, or neither.
        assert!(
            d.aug_links
                .iter()
                .any(|l| l.item == 3153 && l.augment == 1029 && !l.strong())
        );
        let base = score_of(&rank(&input_of(&[], &d, &sd)), 3153);
        assert_eq!(base, Some(LATER_BASE));
        let mut known = input_of(&[], &d, &sd);
        known.known = &[1029];
        assert_eq!(score_of(&rank(&known), 3153), base);
        let mut offered = input_of(&[], &d, &sd);
        offered.offered = &[1029];
        assert_eq!(score_of(&rank(&offered), 3153), base);
    }

    #[test]
    fn gold_need_counts_the_owned_components() {
        let sd = sd();
        let price = |id: i64| sd.item_info[&id].total;
        assert_eq!(gold_need(&sd, 3089, &[]), price(3089));
        // One Needlessly Large Rod (1058) of the two in the recipe.
        assert_eq!(gold_need(&sd, 3089, &[1058]), price(3089) - price(1058));
        assert_eq!(
            gold_need(&sd, 3089, &[1058, 1058]),
            price(3089) - 2 * price(1058)
        );
        // A third copy counts for nothing. An unrelated item counts for nothing.
        assert_eq!(
            gold_need(&sd, 3089, &[1058, 1058, 1058, 3157]),
            price(3089) - 2 * price(1058)
        );
        // A part that you own is not split up: Lost Chapter (3802) counts once, with its own price.
        assert_eq!(gold_need(&sd, 2503, &[3802]), price(2503) - price(3802));
        // A deeper part counts when its upper part is not owned: Amplifying Tome (1052) is in the Lost Chapter.
        assert_eq!(gold_need(&sd, 2503, &[1052]), price(2503) - price(1052));
        // An item that Hexsight does not know has no need.
        assert_eq!(gold_need(&sd, 999_999, &[1058]), 0);
    }

    #[test]
    fn buy_now_is_the_most_expensive_missing_part_that_the_gold_pays_for() {
        let sd = sd();
        // Blackfire Torch (2503): Lost Chapter (3802, 1200) and Fated Ashes (2508, 900, from a Tome).
        assert_eq!(buy_now(&sd, 2503, &[], 1300), Some(3802));
        assert_eq!(buy_now(&sd, 2503, &[], 1200), Some(3802));
        assert_eq!(buy_now(&sd, 2503, &[], 1199), Some(2508));
        // A gold limit under both: the deeper parts. The Amplifying Tome (1052, 400) is the dearest.
        assert_eq!(buy_now(&sd, 2503, &[], 450), Some(1052));
        // An owned part is skipped with its parts: with the Lost Chapter, the ashes or their Tome.
        assert_eq!(buy_now(&sd, 2503, &[3802], 1000), Some(2508));
        assert_eq!(buy_now(&sd, 2503, &[3802], 800), Some(1052));
        // The second Needlessly Large Rod of a Rabadon's Deathcap, when you own the first.
        assert_eq!(buy_now(&sd, 3089, &[1058], 1300), Some(1058));
        // No part fits the gold, or the item is not known: nothing.
        assert_eq!(buy_now(&sd, 2503, &[3802], 300), None);
        assert_eq!(buy_now(&sd, 3089, &[1058], 1100), None);
        assert_eq!(buy_now(&sd, 999_999, &[], 5000), None);
        // In the advice: a part for an item that is not ready, and none for a ready one or without gold.
        let d = heimer();
        let mut i = input_of(&[], &d, &sd);
        i.gold = Some(1300);
        let a = advise(&i);
        assert_eq!((a.items[0].id, a.items[0].part), (2503, Some(3802)));
        i.gold = Some(sd.item_info[&2503].total);
        assert_eq!(advise(&i).items[0].part, None);
        i.gold = Some(100);
        assert_eq!(advise(&i).items[0].part, None);
        assert!(
            advise(&input_of(&[], &d, &sd))
                .items
                .iter()
                .all(|p| p.part.is_none())
        );
    }

    #[test]
    fn ready_and_short_follow_the_gold() {
        let sd = sd();
        let d = heimer();
        let price = sd.item_info[&2503].total;
        let mut i = input_of(&[], &d, &sd);
        i.gold = Some(price);
        let a = advise(&i);
        assert!(a.has_gold);
        // Blackfire Torch is ready with exactly its price. Liandry's Torment costs more and is not.
        // Rylai's Crystal Scepter costs less, so it is ready too.
        assert_eq!(
            a.items.iter().map(|p| p.ready).collect::<Vec<_>>(),
            vec![true, false, true]
        );
        assert_eq!(a.items[0].need, price);
        // One gold less: not ready.
        i.gold = Some(price - 1);
        assert!(!advise(&i).items[0].ready);
        // The need falls with the parts you own: the torch with a Lost Chapter needs the rest only.
        let owner = me(&[3802]);
        let mut j = input(&owner, &d, &sd);
        j.gold = Some(1600);
        let a = advise(&j);
        assert_eq!(a.items[0].id, 2503);
        assert_eq!(
            (a.items[0].need, a.items[0].ready),
            (price - sd.item_info[&3802].total, true)
        );
        // Without gold nothing is ready, and the advice says it does not know.
        let a = advise(&input_of(&[], &d, &sd));
        assert!(!a.has_gold && a.items.iter().all(|p| !p.ready));
    }

    #[test]
    fn full_slots_give_slots_full_unless_you_own_a_component() {
        let sd = sd();
        let d = heimer();
        // Six items, none a part of a suggested item: no room.
        let full = me(&[3157, 3102, 3118, 3116, 3135, 3003]);
        let a = advise(&input(&full, &d, &sd));
        assert!(a.items.is_empty());
        assert!(a.slots_full);
        // One of the six is a part of Blackfire Torch (Lost Chapter): the torch replaces it, so it is on the list.
        let part = me(&[3157, 3102, 3118, 3116, 3135, 3802]);
        let a = advise(&input(&part, &d, &sd));
        assert!(!a.slots_full);
        assert_eq!(a.items[0].id, 2503);
        // Five items and a free slot: no problem.
        assert!(!advise(&input_of(&[3157, 3102, 3118, 3116, 3135], &d, &sd)).slots_full);
    }

    #[test]
    fn at_most_three_items_at_most_one_boots_and_nothing_below_the_minimum() {
        let sd = sd();
        let mut d = heimer();
        d.builds[0].boots_skipped = false;
        d.builds[0].boots = vec![3020, 3158];
        for owned in [
            vec![],
            vec![2503],
            vec![2503, 6653],
            vec![2503, 6653, 3089, 3020],
        ] {
            let p = me(&owned);
            let r = rank(&input(&p, &d, &sd));
            assert!(r.scored.iter().all(|s| s.score >= MIN_SCORE), "{owned:?}");
            assert!(
                r.scored.iter().filter(|s| s.boots).count() <= 1,
                "{owned:?}"
            );
            assert!(
                advise(&input(&p, &d, &sd)).items.len() <= MAX_PICKS,
                "{owned:?}"
            );
        }
        // Items from other builds start at 30 and fall: they never reach 40 alone.
        let other = ChampStats {
            builds: vec![Build {
                name: "ap".into(),
                situational: vec![3089, 3157],
                ..Default::default()
            }],
            ..Default::default()
        };
        let a = advise(&input_of(&[], &other, &sd));
        assert!(a.items.is_empty() && !a.slots_full && !a.no_build);
        // A champion without a build has no suggestion, and says so.
        let none = advise(&input_of(&[], &ChampStats::default(), &sd));
        assert!(none.no_build && none.items.is_empty() && none.style.is_empty());
    }

    #[test]
    fn a_part_or_boots_with_lifesteal_make_no_healer() {
        let sd = sd();
        // Doran's Blade, Vampiric Scepter (parts under 2000 gold) and Gluttonous Greaves (boots) have the
        // tag of lifesteal, but Garen is no healer with them. Bloodthirster is a finished item: he is one.
        for (items, healer) in [
            (&[1055][..], false),
            (&[1053], false),
            (&[3008], false),
            (&[3072], true),
        ] {
            let c = context_of(&roster(vec![enemy(3, items)]), &sd);
            assert_eq!(c.healers.contains(&3), healer, "{items:?}");
        }
    }

    #[test]
    fn one_healer_fires_the_anti_heal_rule_only_when_it_is_the_fed_enemy() {
        let sd = sd();
        let mut d = heimer();
        d.builds[0].later.retain(|&id| id != 3165);
        let anti_heal = |enemies: Vec<Player>| {
            let c = context_of(&roster(enemies), &sd);
            let mut i = input_of(&[], &d, &sd);
            i.ctx = Some(&c);
            rank(&i)
                .scored
                .iter()
                .find(|s| s.id == 3165)
                .map(|s| s.reason)
        };
        // Soraka goes 9/2: the one healer is the fed enemy. Morellonomicon comes in for one healer.
        assert_eq!(
            anti_heal(vec![
                scored_enemy(6, &[], 9, 2, 4),
                enemy(2, &[]),
                enemy(4, &[])
            ]),
            Some(Reason::AntiHeal(1))
        );
        // Soraka is calm and Zed is fed: one healer, and no rule.
        assert_eq!(
            anti_heal(vec![
                enemy(6, &[]),
                enemy(2, &[]),
                scored_enemy(4, &[], 9, 2, 4)
            ]),
            None
        );
    }

    #[test]
    fn basic_boots_are_no_pair_and_their_upgrades_stay() {
        let sd = sd();
        let mut d = with_build("ap", &[2503, 6653, 3089], &[]);
        d.builds[0].boots = vec![3020, 3158];
        // Boots (1001, 300 gold, no parts) are no finished pair. Both upgrades stay and get the 8 points
        // for the part that you own. The most played pair (Sorcerer's Shoes) is the one that shows.
        assert!(
            sd.item_info[&1001].from.is_empty() && sd.item_info[&1001].total == BASIC_BOOTS_GOLD
        );
        let r = rank(&input_of(&[2503, 1001], &d, &sd));
        assert_eq!(score_of(&r, 3020), Some(BOOTS_LATE + STABLE));
        assert_eq!(score_of(&r, 3158), None);
        assert_eq!(r.scored.iter().filter(|s| s.boots).count(), 1);
        // Sorcerer's Shoes are a finished pair: no boots come.
        let r = rank(&input_of(&[2503, 3020], &d, &sd));
        assert!(r.scored.iter().all(|s| !s.boots));
    }

    #[test]
    fn the_first_boots_of_the_build_beat_a_cheaper_second_pair() {
        let mut sd = sd();
        let mut d = with_build("ap", &[2503, 6653, 3089], &[]);
        d.builds[0].boots = vec![3020, 3158];
        // Ionian Boots of Lucidity (the second pair) cost less than Sorcerer's Shoes (the first pair).
        assert!(sd.item_info[&3158].total < sd.item_info[&3020].total);
        // The first pair wins on points, not on price.
        let r = rank(&input_of(&[2503], &d, &sd));
        assert_eq!(score_of(&r, 3020), Some(BOOTS_LATE));
        assert_eq!(score_of(&r, 3158), None);
        // With the first pair out of the shop, the second pair shows with 2 points less.
        sd.item_info.get_mut(&3020).unwrap().in_store = false;
        let r = rank(&input_of(&[2503], &d, &sd));
        assert_eq!(score_of(&r, 3158), Some(BOOTS_LATE - BOOTS_STEP));
        // In the other order, Ionian Boots are the first pair and get the full points.
        sd.item_info.get_mut(&3020).unwrap().in_store = true;
        d.builds[0].boots = vec![3158, 3020];
        let r = rank(&input_of(&[2503], &d, &sd));
        assert_eq!(score_of(&r, 3158), Some(BOOTS_LATE));
        assert_eq!(score_of(&r, 3020), None);
    }

    #[test]
    fn the_mix_weighs_each_enemy_by_the_gold_of_its_damage_items() {
        let sd = sd();
        // A mage with six items of ability power (18950 gold), and a marksman and a support with none.
        // The two without items count as the prior gold (1500 each). The mix is
        // (18950 + 0 + 0.6 * 1500) / (18950 + 1500 + 1500) = 90 %. Each enemy counted the same before: 55 %.
        let r = roster(vec![
            enemy(1, &[3089, 6653, 3135, 4645, 3157, 3102]),
            enemy(2, &[]),
            enemy(6, &[]),
        ]);
        assert_eq!(context_of(&r, &sd).ap_pct, Some(90));
        // A tank with resist items only weighs as the prior: the same mix as with no items.
        let bare = roster(vec![enemy(5, &[]), enemy(1, &[]), enemy(2, &[])]);
        let tanky = roster(vec![
            enemy(5, &[3075, 3065, 3143]),
            enemy(1, &[]),
            enemy(2, &[]),
        ]);
        assert_eq!(
            context_of(&tanky, &sd).ap_pct,
            context_of(&bare, &sd).ap_pct
        );
    }

    #[test]
    fn a_resist_or_penetration_bonus_shrinks_with_each_such_item_you_own() {
        let sd = sd();
        let score = |owned: &[i64], later: &[i64], team: &GameContext, id: i64| {
            let d = with_build("ap", &[2503, 6653, 3089], later);
            let mut i = input_of(owned, &d, &sd);
            i.ctx = Some(team);
            let r = rank(&i);
            r.scored
                .iter()
                .find(|s| s.id == id)
                .map(|s| (s.score, s.reason))
        };
        // Spirit Visage against an AP team: 16 points with no magic resist item, 8 with Banshee's Veil, and
        // none with Banshee's Veil and Force of Nature. Mercury's Treads are boots and do not count.
        let ap = ctx(Some(80), 0, 0);
        assert_eq!(
            score(&[], &[3065], &ap, 3065),
            Some((LATER_BASE + RESIST_CARRY, Reason::VsAp(80)))
        );
        assert_eq!(
            score(&[3102, 3111], &[3065], &ap, 3065),
            Some((LATER_BASE + RESIST_CARRY / 2, Reason::VsAp(80)))
        );
        assert_eq!(
            score(&[3102, 4401], &[3065], &ap, 3065),
            Some((LATER_BASE, Reason::Later))
        );
        // Thornmail against an AD team: half with Zhonya's Hourglass (armor).
        let ad = ctx(Some(20), 0, 0);
        assert_eq!(
            score(&[3157], &[3075], &ad, 3075),
            Some((LATER_BASE + RESIST_CARRY / 2, Reason::VsAd(80)))
        );
        // Void Staff against two tanks: half with Shadowflame (magic penetration). Sorcerer's Shoes are boots.
        let tanks = ctx(Some(50), 0, 2);
        assert_eq!(
            score(&[4645, 3020], &[3135], &tanks, 3135),
            Some((LATER_BASE + PEN_BONUS / 2, Reason::VsTanks(2)))
        );
    }

    #[test]
    fn a_resist_rule_adds_a_resist_item_of_your_class_when_the_list_has_none() {
        let sd = sd();
        let added = |d: &ChampStats, owned: &[i64], team: &GameContext| {
            let mut i = input_of(owned, d, &sd);
            i.ctx = Some(team);
            let r = rank(&i);
            r.scored
                .iter()
                .filter(|s| s.reason == Reason::VsAp(80) || s.reason == Reason::VsAd(80))
                .map(|s| (s.id, s.score))
                .collect::<Vec<_>>()
        };
        let (ap, ad) = (ctx(Some(80), 0, 0), ctx(Some(20), 0, 0));
        // An AP build without magic resist gets Banshee's Veil: 50 + 16 points, and the reason names the rule.
        let mage = with_build("ap", &[2503, 6653, 3089], &[]);
        assert_eq!(
            added(&mage, &[], &ap),
            vec![(3102, RESIST_BASE + RESIST_CARRY)]
        );
        // Against an AD team it gets Zhonya's Hourglass.
        assert_eq!(
            added(&mage, &[], &ad),
            vec![(3157, RESIST_BASE + RESIST_CARRY)]
        );
        // With a finished item, the added item takes the third place.
        let mut i = input_of(&[2503], &mage, &sd);
        i.ctx = Some(&ap);
        let a = advise(&i);
        assert_eq!(ids(&a), vec![6653, 3089, 3102]);
        assert_eq!(a.items[2].reason, Reason::VsAp(80));
        // With one magic resist item the bonus is half, and with two it is gone, so nothing is added.
        assert_eq!(
            added(&mage, &[4401], &ap),
            vec![(3102, RESIST_BASE + RESIST_CARRY / 2)]
        );
        assert!(added(&mage, &[4401, 2504], &ap).is_empty());
        // An AD build gets Maw of Malmortius, and Death's Dance against an AD team.
        let marksman = with_build("ad", &[3031, 3071, 3036], &[]);
        assert_eq!(
            added(&marksman, &[], &ap),
            vec![(3156, RESIST_BASE + RESIST_CARRY)]
        );
        assert_eq!(
            added(&marksman, &[], &ad),
            vec![(6333, RESIST_BASE + RESIST_CARRY)]
        );
        // A tank build (two core items with health and magic resist) gets Randuin's Omen against AD.
        let tank = with_build("ap", &[3065, 4401], &[]);
        assert_eq!(
            added(&tank, &[], &ad),
            vec![(3143, RESIST_BASE + RESIST_TANK)]
        );
        // A list with a magic resist item gets nothing more: Spirit Visage gets the points.
        let visage = with_build("ap", &[2503, 6653, 3089], &[3065]);
        assert_eq!(
            added(&visage, &[], &ap),
            vec![(3065, LATER_BASE + RESIST_CARRY)]
        );
        // A mixed build has no list.
        let mixed = with_build("hybrid", &[3031, 3089], &[]);
        assert!(added(&mixed, &[], &ap).is_empty());
    }

    #[test]
    fn the_class_and_the_tank_build_come_from_the_core_items() {
        let sd = sd();
        let two = ctx(Some(50), 2, 0);
        let injected = |d: &ChampStats| {
            let mut i = input_of(&[], d, &sd);
            i.ctx = Some(&two);
            rank(&i)
                .scored
                .iter()
                .filter(|s| s.reason == Reason::AntiHeal(2))
                .map(|s| s.id)
                .collect::<Vec<_>>()
        };
        // The damage split says AD, but the core is all ability power: the AP list (Morellonomicon).
        let mut ap = with_build("ap", &[2503, 6653, 3089], &[]);
        ap.magic_share = Some(0.1);
        assert_eq!(injected(&ap), vec![3165]);
        // A core without damage items (Frozen Heart) falls back to the damage split: the AD list.
        let mut none = with_build("ap", &[3110], &[]);
        none.magic_share = Some(0.1);
        assert_eq!(injected(&none), vec![3033]);
        // Two core items with health and resist make a tank build, whatever its name: Thornmail.
        let tank = with_build("ap", &[3065, 3143], &[]);
        assert!(is_tank_build(&sd, &tank.builds[0]));
        assert_eq!(injected(&tank), vec![3075]);
        // One such item is no tank build. The name "tank" still is one.
        assert!(!is_tank_build(
            &sd,
            &with_build("ap", &[3065, 3089], &[]).builds[0]
        ));
        assert!(is_tank_build(&sd, &with_build("tank", &[], &[]).builds[0]));
    }

    #[test]
    fn a_damage_item_with_resist_makes_no_tank_without_health() {
        let sd = sd();
        // Zhonya's Hourglass, Banshee's Veil, Death's Dance and Maw of Malmortius give resist with damage.
        for items in [&[3157, 6333][..], &[3102, 3156], &[3075, 6333]] {
            assert!(
                context_of(&roster(vec![enemy(3, items)]), &sd)
                    .tanks
                    .is_empty(),
                "{items:?}"
            );
        }
        // A tank with one of them is no tanky enemy either.
        assert!(
            context_of(&roster(vec![enemy(5, &[3157])]), &sd)
                .tanks
                .is_empty()
        );
        // Thornmail and Spirit Visage still count.
        assert_eq!(
            context_of(&roster(vec![enemy(3, &[3075, 3065])]), &sd).tanks,
            vec![3]
        );
    }

    #[test]
    fn the_heimerdinger_game_gives_sensible_tips() {
        let sd = sd();
        let d = heimer();
        // A real Heimerdinger build against an AP team with a fed Soraka and a Garen with Ravenous Hydra.
        let r = roster(vec![
            enemy(1, &[3089, 6653]),
            enemy(7, &[3102]),
            scored_enemy(6, &[], 9, 2, 4),
            enemy(5, &[3075, 3065]),
            enemy(3, &[3074, 3143]),
        ]);
        let c = context_of(&r, &sd);
        assert_eq!(c.healers, vec![6, 3]);
        assert_eq!(c.tanks, vec![5]);
        let p = me(&[2503, 3020]);
        let mut i = input(&p, &d, &sd);
        i.ctx = Some(&c);
        i.gold = Some(1200);
        let a = advise(&i);
        assert_eq!(a.style, "ap");
        // Three items, none owned, none a pair of boots (you own Sorcerer's Shoes), all in the shop.
        assert_eq!(a.items.len(), MAX_PICKS);
        for pick in &a.items {
            let info = &sd.item_info[&pick.id];
            assert!(info.in_store && info.cats & cat::BOOTS == 0, "{}", pick.id);
            assert!(!p.items.contains(&pick.id), "{}", pick.id);
        }
        // The next two core items of the most played build (Liandry's, Rylai's), and Morellonomicon
        // against the two healers (the fed Soraka and Garen).
        assert_eq!(ids(&a), vec![6653, 3116, 3165]);
        assert_eq!(a.items[2].reason, Reason::AntiHeal(2));
    }

    #[test]
    fn the_same_input_gives_the_same_order() {
        let sd = sd();
        let d = heimer();
        let two = GameContext {
            healers: vec![6, 8],
            tanks: vec![5, 3],
            ap_pct: Some(65),
            ..Default::default()
        };
        let p = me(&[1026, 3020]);
        let mut i = input(&p, &d, &sd);
        i.ctx = Some(&two);
        i.offered = &[3150, 1205];
        let first = advise(&i);
        for _ in 0..50 {
            assert_eq!(advise(&i), first);
        }
    }

    #[test]
    fn the_signature_follows_the_items_only_and_not_the_ready_flags() {
        let sd = sd();
        let d = heimer();
        let p = me(&[]);
        let mut i = input(&p, &d, &sd);
        i.gold = Some(100);
        let a = advise(&i);
        // More gold that changes no flag: the same signature.
        i.gold = Some(200);
        assert_eq!(advise(&i).signature(), a.signature());
        // A flag changes: the item is ready. The gold reaches a price in a fight, and the
        // card must not pop up then, so the signature stays the same.
        i.gold = Some(3000);
        let rich = advise(&i);
        assert!(
            rich.items.iter().any(|p| p.ready),
            "the test needs a ready item"
        );
        assert_eq!(
            rich.items.iter().map(|p| p.id).collect::<Vec<_>>(),
            a.items.iter().map(|p| p.id).collect::<Vec<_>>()
        );
        assert_eq!(rich.signature(), a.signature());
        // The items change.
        let bought = me(&[2503]);
        let mut j = input(&bought, &d, &sd);
        j.gold = Some(100);
        assert_ne!(advise(&j).signature(), a.signature());
        // The context, the dead flag and the wanted list do not count.
        let calm = GameContext::default();
        i.gold = Some(100);
        i.ctx = Some(&calm);
        assert_eq!(advise(&i).signature(), a.signature());
    }

    // ---- The mark "fits your build".

    #[test]
    fn an_offered_augment_fits_when_it_links_to_an_item_of_your_build() {
        let sd = sd();
        // Stormsurge is in the later items of this build, and the data ties it to the augment 3150.
        let d = ChampStats {
            aug_links: heimer().aug_links,
            ..with_build("ap", &[2503, 6653], &[4646])
        };
        let a = advise(&input_of(&[], &d, &sd));
        assert!(a.wanted.contains(&4646));
        assert_eq!(fit_item(&d, &a.wanted, 3150), Some(4646));
        // The items of the core and the later list count even when they are not among the three picks.
        assert!(a.wanted.contains(&6653));
        let core = ChampStats {
            aug_links: vec![AugLink {
                item: 6653,
                augment: 9,
                with_pick: 0.7,
            }],
            ..d.clone()
        };
        assert_eq!(fit_item(&core, &a.wanted, 9), Some(6653));
        // Only the share of the players counts: a link that few players buy gives no mark.
        let few = ChampStats {
            aug_links: vec![AugLink {
                item: 6653,
                augment: 9,
                with_pick: 0.1,
            }],
            ..d.clone()
        };
        assert_eq!(fit_item(&few, &a.wanted, 9), None);
    }

    #[test]
    fn no_link_or_a_weak_link_or_an_owned_item_gives_no_fit() {
        let sd = sd();
        let d = ChampStats {
            aug_links: heimer().aug_links,
            ..with_build("ap", &[2503, 6653], &[4646, 3004, 3153])
        };
        let a = advise(&input_of(&[], &d, &sd));
        // No link for this augment.
        assert_eq!(fit_item(&d, &a.wanted, 424_242), None);
        // Weak links: Manamune (few players buy it), Blade of The Ruined King (few players buy it too).
        assert_eq!(fit_item(&d, &a.wanted, 1205), None);
        assert_eq!(fit_item(&d, &a.wanted, 1029), None);
        // A strong link to an item that is not in the build and not among the picks.
        assert_eq!(fit_item(&d, &a.wanted, 1156), None);
        // You own the item: it is no longer wanted.
        let owner = me(&[4646]);
        let b = advise(&input(&owner, &d, &sd));
        assert!(!b.wanted.contains(&4646));
        assert_eq!(fit_item(&d, &b.wanted, 3150), None);
        // The test is a plain lookup: the mark has no number.
        assert_eq!(fit_item(&d, &[], 3150), None);
    }

    // ---- The policy.

    #[test]
    fn advice_has_no_number_that_comes_from_a_win_rate() {
        // Listing every field of the types: a new field must be looked at, so it cannot be a rate by accident.
        let sd = sd();
        let a = advise(&input_of(&[], &heimer(), &sd));
        let Advice {
            items,
            style: _,
            ctx: _,
            slots_full: _,
            me_dead: _,
            no_build: _,
            has_gold: _,
            wanted: _,
            path: _,
            path_item: _,
            plan: _,
            fed_rule: _,
            recap: _,
            // A champion id and a count of your deaths.
            killer: _,
            my_stats: _,
            rules: _,
        } = a;
        for p in items {
            let ItemPick {
                id: _,
                reason: _,
                need: _,
                ready: _,
                part: _,
            } = p;
        }
        let GameContext {
            ap_pct: _,
            fed: _,
            healers: _,
            tanks: _,
            enemies: _,
            foes: _,
        } = GameContext::default();
        let Foe {
            champion: _,
            kills: _,
            deaths: _,
            assists: _,
            level: _,
            dead: _,
            finished: _,
            ap_gold: _,
            ad_gold: _,
            kind: _,
            heals: _,
        } = Foe::default();
        let Recap { killers: _ } = Recap::default();
        let RuleState {
            vs_ap: _,
            vs_ad: _,
            tanks: _,
            healers: _,
        } = RuleState::default();
        // The only reasons that carry a number: the core place, the slot, the counts and shares of the
        // enemy team, and the score line of an enemy.
        for r in [
            Reason::VsFed {
                champion: 4,
                kills: 9,
                deaths: 1,
            },
            Reason::VsKiller {
                champion: 4,
                times: 3,
            },
            Reason::Core(1),
            Reason::Slot(2),
            Reason::VsAp(70),
            Reason::VsAd(70),
            Reason::AntiHeal(2),
            Reason::VsTanks(2),
        ] {
            assert!(matches!(
                r,
                Reason::Core(_)
                    | Reason::Slot(_)
                    | Reason::VsAp(_)
                    | Reason::VsAd(_)
                    | Reason::AntiHeal(_)
                    | Reason::VsTanks(_)
                    | Reason::VsFed { .. }
                    | Reason::VsKiller { .. }
            ));
        }
    }

    // ---- The foundation of v0.1.3: the slot fix, the shares, the foes and the new fields.

    /// The slot reasons in a ranking.
    fn slot_reasons(r: &Ranking) -> Vec<Reason> {
        r.scored
            .iter()
            .map(|s| s.reason)
            .filter(|r| matches!(r, Reason::Slot(_)))
            .collect()
    }

    /// A build with one core item and one item in each of the first three slot rows: Rabadon's
    /// Deathcap (1st), Void Staff (2nd) and Zhonya's Hourglass (3rd).
    fn slotted() -> ChampStats {
        let mut d = with_build("ap", &[2503], &[]);
        d.slots[0] = vec![3089];
        d.slots[1] = vec![3135];
        d.slots[2] = vec![3157];
        d
    }

    #[test]
    fn owned_one_finished_and_sorcerers_shoes_gives_slot_3() {
        let sd = sd();
        // The slot lists of the data count the boots: the real rows hold Sorcerer's Shoes (3020).
        assert!(heimer().slots.iter().any(|row| row.contains(&3020)));
        // One finished item and a finished pair: the next item is the third.
        let r = rank(&input_of(&[6653, 3020], &slotted(), &sd));
        assert_eq!(slot_reasons(&r), vec![Reason::Slot(3)]);
        assert_eq!(reason_of(&r, 3157), Some(Reason::Slot(3)));
        assert_eq!(score_of(&r, 3157), Some(SLOT_BASE));
        assert_eq!(score_of(&r, 3135), None);
        // Without the boots it is the second.
        let r = rank(&input_of(&[6653], &slotted(), &sd));
        assert_eq!(reason_of(&r, 3135), Some(Reason::Slot(2)));
    }

    #[test]
    fn basic_boots_keep_slot_2() {
        let sd = sd();
        // The basic Boots are no finished pair: the next item is the second of the slot lists.
        let r = rank(&input_of(&[6653, 1001], &slotted(), &sd));
        assert_eq!(slot_reasons(&r), vec![Reason::Slot(2)]);
        assert_eq!(reason_of(&r, 3135), Some(Reason::Slot(2)));
        // With no item at all, the first.
        let r = rank(&input_of(&[], &slotted(), &sd));
        assert_eq!(reason_of(&r, 3089), Some(Reason::Slot(1)));
    }

    #[test]
    fn context_with_an_empty_share_map_is_today() {
        let sd = sd();
        // All context tests above run with the empty map (`context_of`). The numbers of the role prior
        // stay: three mages are 100 % AP, and a mage, a marksman and a fighter are 40 %.
        let mages = roster(vec![enemy(1, &[]), enemy(7, &[]), enemy(1, &[])]);
        let mixed = roster(vec![enemy(1, &[]), enemy(2, &[]), enemy(3, &[])]);
        assert_eq!(context(&mages, &sd, &HashMap::new()).ap_pct, Some(100));
        assert_eq!(context(&mixed, &sd, &HashMap::new()).ap_pct, Some(40));
        // A share of a champion that is not in the game changes nothing.
        let other = HashMap::from([(999, 0.0)]);
        for r in [&mages, &mixed] {
            assert_eq!(context(r, &sd, &other), context_of(r, &sd));
        }
        // A share of the champion's own data takes the place of its role: Jinx at 1.0 deals magic damage.
        let jinx = HashMap::from([(2, 1.0)]);
        assert_eq!(context(&mixed, &sd, &jinx).ap_pct, Some(75));
        assert_eq!(share_of(&sd, &enemy(2, &[]), &jinx), (PRIOR_GOLD, 0.0));
        assert_eq!(
            share_of(&sd, &enemy(2, &[]), &HashMap::new()),
            (0.0, PRIOR_GOLD)
        );
    }

    #[test]
    fn foes_follow_the_roster_with_their_kind() {
        let sd = sd();
        let r = roster(vec![
            Player {
                level: 12,
                dead: true,
                ..scored_enemy(51, &[3031, 3036, 3020], 9, 1, 4)
            },
            scored_enemy(6, &[3072], 2, 3, 7),
            enemy(0, &[3089]),
            enemy(1, &[3089, 1058]),
        ]);
        let c = context_of(&r, &sd);
        // One foe for each known enemy, by champion id. The unknown enemy is no foe.
        assert_eq!(
            c.foes.iter().map(|f| f.champion).collect::<Vec<_>>(),
            vec![1, 6, 51]
        );
        let cait = c.foes[2];
        assert_eq!(
            (cait.kills, cait.deaths, cait.assists, cait.level, cait.dead),
            (9, 1, 4, 12, true)
        );
        // Two finished items: the boots do not count. The gold is the prior plus the damage items.
        assert_eq!(cait.finished, 2);
        let (ap, ad) = share_of(&sd, &r.players[1], &HashMap::new());
        assert_eq!(
            (cait.ap_gold, cait.ad_gold),
            (ap.round() as u32, ad.round() as u32)
        );
        assert!(cait.ad_gold > cait.ap_gold);
        // Rabadon's Deathcap is finished, a Needlessly Large Rod is not.
        assert_eq!(c.foes[0].finished, 1);
        // Soraka heals, Caitlyn does not.
        assert!(c.foes[1].heals && !cait.heals);
        // Caitlyn has two crit items; the others have fewer than two of a kind.
        assert_eq!(cait.kind, FoeKind::Crit(2));
        assert!(c.foes[..2].iter().all(|f| f.kind == FoeKind::None));
    }

    #[test]
    fn the_healers_come_with_the_fed_enemy_first_then_by_kills() {
        let sd = sd();
        // Soraka (6) with 2 kills, Yuumi (8) with 4 kills, and Aatrox (266) fed at 9/2.
        let r = roster(vec![
            scored_enemy(6, &[], 2, 0, 0),
            scored_enemy(8, &[], 4, 0, 0),
            scored_enemy(266, &[], 9, 2, 0),
        ]);
        assert_eq!(context_of(&r, &sd).healers, vec![266, 8, 6]);
        // Equal kills go by id.
        let r = roster(vec![enemy(8, &[]), enemy(6, &[])]);
        assert_eq!(context_of(&r, &sd).healers, vec![6, 8]);
    }

    #[test]
    fn the_new_fields_start_empty_and_change_nothing() {
        let (sd, d) = (sd(), heimer());
        let team = ctx(Some(70), 2, 2);
        for owned in [&[][..], &[2503], &[2503, 3020], &[6653, 3020, 3116]] {
            let mut i = input_of(owned, &d, &sd);
            i.ctx = Some(&team);
            let a = advise(&i);
            assert_eq!(
                (a.fed_rule, a.recap.clone(), a.my_stats),
                (None, None, None),
                "{owned:?}"
            );
            // The rules that fired are in the advice.
            assert_eq!(
                a.rules,
                RuleState {
                    vs_ap: true,
                    vs_ad: false,
                    tanks: true,
                    healers: true
                }
            );
            // The previous state, stats with no field, an unknown fed enemy and the path of this
            // pass passed back (as the worker does) change nothing.
            i.prev_rules = Some(RuleState::default());
            i.prev_path = a.path;
            i.prev_fed = Some(4);
            i.stats = Some(MyStats::default());
            assert_eq!(advise(&i), a, "{owned:?}");
        }
        // Your own stats reach the advice rounded, when both resists are there.
        let mut i = input_of(&[], &d, &sd);
        i.stats = Some(MyStats {
            armor: Some(171.6),
            mr: Some(48.2),
            max_health: Some(2000.0),
        });
        assert_eq!(advise(&i).my_stats, Some((172, 48)));
        i.stats = Some(MyStats {
            armor: Some(171.6),
            ..Default::default()
        });
        assert_eq!(advise(&i).my_stats, None);
    }

    #[test]
    fn the_signature_changes_with_the_killers_of_the_recap_only() {
        let (sd, d) = (sd(), heimer());
        let base = advise(&input_of(&[], &d, &sd));
        let zed = Recap { killers: vec![4] };
        let lux = Recap { killers: vec![7] };
        let mut i = input_of(&[], &d, &sd);
        i.recap = Some(&zed);
        let a = advise(&i);
        assert_eq!(a.recap.as_ref(), Some(&zed));
        assert_ne!(a.signature(), base.signature());
        i.recap = Some(&lux);
        assert_ne!(advise(&i).signature(), a.signature());
        // The plan and the path are not in the signature.
        let other = Advice {
            plan: vec![3089, 3135],
            path: Some(2),
            path_item: Some(3118),
            ..base.clone()
        };
        assert_eq!(other.signature(), base.signature());
    }

    #[test]
    fn the_resist_bonus_and_the_inject_cap_keep_today_s_numbers() {
        let (sd, mage) = (sd(), with_build("ap", &[2503, 6653, 3089], &[]));
        let i = input_of(&[], &mage, &sd);
        let pass = Pass {
            input: &i,
            build: &mage.builds[0],
            build_at: 0,
            class: Class::Ap,
            tank: false,
            held: Vec::new(),
            finished: 0,
            owns_boots: false,
            mr_bonus: 16,
            armor_bonus: 0,
            rules: Rules::default(),
        };
        assert_eq!(
            (pass.bonus(cat::MR), pass.bonus(cat::ARMOR), pass.bonus(0)),
            (16, 0, 0)
        );
        // The bonus of a resist: the shrink, and never more than the stat factor allows (1.0 in F0).
        assert_eq!(resist_bonus(RESIST_CARRY, 0, None), RESIST_CARRY);
        assert_eq!(resist_bonus(RESIST_CARRY, 1, Some(40.0)), RESIST_CARRY / 2);
        assert_eq!(resist_bonus(RESIST_TANK, 2, None), 0);
        // The reason priorities keep the order of v0.1.2: killer, fed, anti-heal, penetration, resist.
        const {
            assert!(PRIO_KILLER < PRIO_FED && PRIO_FED < PRIO_ANTI_HEAL);
            assert!(PRIO_ANTI_HEAL < PRIO_PEN && PRIO_PEN < PRIO_RESIST);
        }
        // Healers and an AP team: two injects (Morellonomicon and Banshee's Veil), which is the cap.
        let both = ctx(Some(80), 2, 0);
        let mut i = input_of(&[], &mage, &sd);
        i.ctx = Some(&both);
        let r = rank(&i);
        assert_eq!(score_of(&r, 3165), Some(ANTI_HEAL_BASE + ANTI_HEAL_BONUS));
        assert_eq!(score_of(&r, 3102), Some(RESIST_BASE + RESIST_CARRY));
        assert_eq!(reason_of(&r, 3102), Some(Reason::VsAp(80)));
        assert_eq!(MAX_INJECTS, 2);
    }
}

/// The packages together: a roster goes through `context`, the recap tracker, the threat rules,
/// the path tracker and the plan in one `advise`, as in the worker.
#[cfg(test)]
mod integration {
    use super::testkit::*;
    use super::*;

    fn has_bit(sd: &StaticData, id: i64, bit: u16) -> bool {
        sd.item_info.get(&id).is_some_and(|i| i.cats & bit != 0)
    }

    #[test]
    fn a_heimerdinger_game_with_a_fed_ap_mage_gives_vs_fed_magic_resist_a_path_and_a_plan() {
        let sd = sd();
        let a = fed_mage_game();
        // The fed rule fired on Syndra, and a pick carries it on a magic resist item.
        assert_eq!(a.fed_rule, Some(E2E_SYNDRA), "{a:?}");
        let fed = a
            .items
            .iter()
            .find(|p| matches!(p.reason, Reason::VsFed { champion, .. } if champion == E2E_SYNDRA))
            .unwrap_or_else(|| panic!("no VsFed pick in {:?}", a.items));
        assert!(has_bit(&sd, fed.id, cat::MR), "{fed:?}");
        assert_eq!(
            fed.reason,
            Reason::VsFed {
                champion: E2E_SYNDRA,
                kills: 9,
                deaths: 1
            }
        );
        // The context names Syndra as fed, with her build kind.
        let c = a.ctx.as_ref().unwrap();
        assert_eq!(c.fed.map(|f| f.champion), Some(E2E_SYNDRA));
        let syndra = c.foes.iter().find(|f| f.champion == E2E_SYNDRA).unwrap();
        assert_eq!(syndra.kind, FoeKind::Ap(2));
        // Blackfire Torch puts you on P3 ([6653, 2503, 3116]), named by its rarest core item, and the
        // plan names up to two more items that you neither own nor get now.
        assert_eq!(a.path, Some(2), "{a:?}");
        assert!(a.path_item.is_some_and(|id| sd.items.contains_key(&id)));
        assert!(!a.plan.is_empty() && a.plan.len() <= 2, "{:?}", a.plan);
        for id in &a.plan {
            assert!(!a.items.iter().any(|p| p.id == *id) && *id != 2503, "{id}");
        }
        // The worker passes the path and the fed enemy back: the next pass gives the same advice.
        let (sd2, d, r) = (super::testkit::sd(), heimer(), fed_mage_roster());
        let summary = context(&r, &sd2, &HashMap::new());
        let mut i = input(r.me(), &d, &sd2);
        i.ctx = Some(&summary);
        i.prev_path = a.path;
        i.prev_fed = a.fed_rule;
        i.prev_rules = Some(a.rules);
        assert_eq!(advise(&i), a);
    }

    #[test]
    fn the_fed_mage_game_has_no_number_from_a_win_rate() {
        fn with_rates(v: &mut Value, rate: f64) {
            match v {
                Value::Object(m) => {
                    for (k, x) in m.iter_mut() {
                        if k == "winRate" {
                            *x = serde_json::json!(rate);
                        } else {
                            with_rates(x, rate);
                        }
                    }
                }
                Value::Array(a) => a.iter_mut().for_each(|x| with_rates(x, rate)),
                _ => {}
            }
        }
        let raw: Value = serde_json::from_str(HEIMER).unwrap();
        let (mut lo, mut hi) = (raw.clone(), raw);
        with_rates(&mut lo, 0.0);
        with_rates(&mut hi, 1.0);
        let lo = fed_mage_advice(&data::kit_champion(&lo));
        let hi = fed_mage_advice(&data::kit_champion(&hi));
        assert_eq!(lo, hi);
        assert_eq!(lo, fed_mage_game());
    }

    #[test]
    fn three_deaths_to_caitlyn_feed_the_killer_rule_through_the_recap_tracker() {
        let sd = sd();
        let (dead, t, a) = death_game();
        assert!(a.me_dead && dead.me().deaths == 3);
        // The tracker names Caitlyn for this death and counts three.
        let caitlyn = Killer {
            champion: E2E_CAITLYN,
            times: 3,
        };
        assert_eq!(t.killer(), Some(caitlyn));
        assert_eq!(
            a.recap,
            Some(Recap {
                killers: vec![E2E_CAITLYN]
            })
        );
        assert_eq!(a.killer, Some(caitlyn));
        // Caitlyn deals physical damage: an armor pick with the killer reason.
        let pick = a
            .items
            .iter()
            .find(|p| {
                p.reason
                    == Reason::VsKiller {
                        champion: E2E_CAITLYN,
                        times: 3,
                    }
            })
            .unwrap_or_else(|| panic!("no VsKiller pick in {:?}", a.items));
        assert!(has_bit(&sd, pick.id, cat::ARMOR), "{pick:?}");
        // A new killer in the recap changes the signature, so the overlay pops up.
        let other = Advice {
            recap: Some(Recap {
                killers: vec![E2E_SYNDRA],
            }),
            ..a.clone()
        };
        assert_ne!(a.signature(), other.signature());
    }

    #[test]
    fn a_killer_without_a_recap_still_reaches_the_advice() {
        // After a respawn the recap is gone, and the nemesis is still the killer of the rule.
        let (sd, d) = (sd(), heimer());
        let (dead, mut t, _) = death_game();
        let mut alive = dead.clone();
        alive.players[0].dead = false;
        t.observe(&alive, std::time::Instant::now());
        assert_eq!(t.recap(), None);
        let summary = context(&alive, &sd, &HashMap::new());
        let mut i = input(alive.me(), &d, &sd);
        i.ctx = Some(&summary);
        i.killer = t.killer();
        i.recap = t.recap();
        let a = advise(&i);
        assert_eq!(
            a.killer,
            Some(Killer {
                champion: E2E_CAITLYN,
                times: 3
            })
        );
        assert_eq!(a.recap, None);
    }
}
