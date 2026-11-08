//! The shared helpers of the suggestion tests: the static data of the fixtures, the Heimerdinger
//! stats, small builds, players, rosters, inputs and contexts. The tests of `suggest` and of its
//! child modules (`path`, `threat`, `recap`, `stats`) use them.

use super::*;

pub(crate) const HEIMER: &str = include_str!("../../tests/fixtures/heimerdinger-suggest.json");
pub(crate) const ITEMS: &str = include_str!("../../tests/fixtures/items-trimmed.json");
pub(crate) const PLAYERLIST: &str = include_str!("../../tests/fixtures/playerlist.json");
pub(crate) const PLAYERLIST_REAL: &str = include_str!("../../tests/fixtures/playerlist-real.json");

/// The champions of the fake games: id, name, alias and the first role. The ids 1 to 10 are fake
/// (with a real alias). From 11 on, each champion has its real id, except Kayle (real id 10), who
/// gets 1010 with the role marksman for the tests of the damage split. The contexts of `ctx` use the
/// ids 100 to 109 for healers and 200 to 209 for tanks, so no champion here has such an id.
pub(crate) const CHAMPIONS: [(i64, &str, &str, &str); 23] = [
    (1, "Ahri", "Ahri", "mage"),
    (2, "Jinx", "Jinx", "marksman"),
    (3, "Garen", "Garen", "fighter"),
    (4, "Zed", "Zed", "assassin"),
    (5, "Malphite", "Malphite", "tank"),
    (6, "Soraka", "Soraka", "support"),
    (7, "Lux", "Lux", "mage"),
    (8, "Yuumi", "Yuumi", "support"),
    (9, "Nasus", "Nasus", "fighter"),
    (10, "Blitzcrank", "Blitzcrank", "tank"),
    (28, "Evelynn", "Evelynn", "assassin"),
    (32, "Amumu", "Amumu", "tank"),
    (51, "Caitlyn", "Caitlyn", "marksman"),
    (55, "Katarina", "Katarina", "assassin"),
    (74, "Heimerdinger", "Heimerdinger", "mage"),
    (82, "Mordekaiser", "Mordekaiser", "fighter"),
    (84, "Akali", "Akali", "assassin"),
    (111, "Nautilus", "Nautilus", "tank"),
    (134, "Syndra", "Syndra", "mage"),
    (161, "Vel'Koz", "Velkoz", "mage"),
    (223, "Tahm Kench", "TahmKench", "tank"),
    (266, "Aatrox", "Aatrox", "fighter"),
    (1010, "Kayle", "Kayle", "marksman"),
];

/// The static data of the fixtures: the trimmed item list, and the champions of `CHAMPIONS` with
/// their role.
pub(crate) fn sd() -> StaticData {
    let (items, item_info) = data::parse_items(&serde_json::from_str(ITEMS).unwrap());
    let mut out = StaticData {
        items,
        item_info,
        ..Default::default()
    };
    for (id, name, alias, role) in CHAMPIONS {
        out.champions.insert(
            id,
            Champion {
                id,
                name: name.into(),
                alias: alias.into(),
                icon: String::new(),
            },
        );
        out.roles.insert(id, vec![role.into()]);
    }
    out
}

/// The real Heimerdinger stats of the fixture.
pub(crate) fn heimer() -> ChampStats {
    data::kit_champion(&serde_json::from_str(HEIMER).unwrap())
}

/// A champion with one build: the style, the core and the later items.
pub(crate) fn with_build(name: &str, core: &[i64], later: &[i64]) -> ChampStats {
    ChampStats {
        builds: vec![Build {
            name: name.into(),
            items: core.to_vec(),
            later: later.to_vec(),
            ..Default::default()
        }],
        ..Default::default()
    }
}

/// You: Heimerdinger on the side ORDER, level 9, with these items.
pub(crate) fn me(items: &[i64]) -> Player {
    Player {
        champion: 74,
        side: Side::Order,
        items: items.to_vec(),
        level: 9,
        ..Default::default()
    }
}

/// An enemy on the side CHAOS, level 9, with these items and a score of 0/0/0.
pub(crate) fn enemy(champion: i64, items: &[i64]) -> Player {
    Player {
        champion,
        side: Side::Chaos,
        items: items.to_vec(),
        level: 9,
        ..Default::default()
    }
}

/// An enemy with a score line.
pub(crate) fn scored_enemy(champion: i64, items: &[i64], k: u16, d: u16, a: u16) -> Player {
    Player {
        kills: k,
        deaths: d,
        assists: a,
        ..enemy(champion, items)
    }
}

/// A roster of you (with no items, index 0) and these enemies.
pub(crate) fn roster(enemies: Vec<Player>) -> Roster {
    let mut players = vec![me(&[])];
    players.extend(enemies);
    Roster { players, me: 0 }
}

/// The context of a roster with no shares of the champions' own data (the roles only).
pub(crate) fn context_of(r: &Roster, sd: &StaticData) -> GameContext {
    context(r, sd, &HashMap::new())
}

/// An input with no gold, no context, no augments and none of the optional parts.
pub(crate) fn input<'a>(me: &'a Player, details: &'a ChampStats, sd: &'a StaticData) -> Input<'a> {
    Input {
        me,
        gold: None,
        ctx: None,
        details,
        sd,
        offered: &[],
        known: &[],
        stats: None,
        killer: None,
        recap: None,
        prev_path: None,
        prev_fed: None,
        prev_rules: None,
    }
}

/// Like `input`, for a player who owns these items. The player lives for the rest of the test run.
pub(crate) fn input_of<'a>(
    items: &[i64],
    details: &'a ChampStats,
    sd: &'a StaticData,
) -> Input<'a> {
    let player: &'a Player = Box::leak(Box::new(me(items)));
    input(player, details, sd)
}

/// The ids of the picks.
pub(crate) fn ids(a: &Advice) -> Vec<i64> {
    a.items.iter().map(|p| p.id).collect()
}

/// The score of a candidate, `None` when it is not in the ranking.
pub(crate) fn score_of(r: &Ranking, id: i64) -> Option<i32> {
    r.scored.iter().find(|s| s.id == id).map(|s| s.score)
}

/// The reason of a candidate, `None` when it is not in the ranking.
pub(crate) fn reason_of(r: &Ranking, id: i64) -> Option<Reason> {
    r.scored.iter().find(|s| s.id == id).map(|s| s.reason)
}

/// A context with this mix, `healers` healers (ids 100 and up) and `tanks` tanks (ids 200 and up).
pub(crate) fn ctx(ap_pct: Option<u8>, healers: usize, tanks: usize) -> GameContext {
    GameContext {
        ap_pct,
        healers: (0..healers as i64).map(|i| 100 + i).collect(),
        tanks: (0..tanks as i64).map(|i| 200 + i).collect(),
        enemies: 5,
        ..Default::default()
    }
}

// ---------------------------------------------------------------------------
// End-to-end games: a roster goes through `context`, the recap `Tracker` and `advise`, as in the
// worker. The integration tests of `suggest` and `app` read the advice of these games.

/// Syndra, the fed AP mage of `fed_mage_game`.
pub(crate) const E2E_SYNDRA: i64 = 134;
/// Caitlyn, the killer of `death_game`.
pub(crate) const E2E_CAITLYN: i64 = 51;

/// You: Heimerdinger with Blackfire Torch, against five known enemies. Syndra is fed at 9/1/0 with
/// Rabadon's Deathcap and Shadowflame; the others have small scores.
pub(crate) fn fed_mage_roster() -> Roster {
    let mut players = vec![me(&[2503])];
    players.extend([
        scored_enemy(E2E_SYNDRA, &[3089, 4645], 9, 1, 0),
        scored_enemy(E2E_CAITLYN, &[3031], 1, 3, 2),
        scored_enemy(32, &[], 0, 2, 4),
        scored_enemy(266, &[], 2, 2, 1),
        scored_enemy(111, &[], 0, 1, 6),
    ]);
    Roster { players, me: 0 }
}

/// The advice of `fed_mage_roster` with these Heimerdinger stats, as the worker makes it (no share
/// of the enemies' own data, no gold, no live stats).
pub(crate) fn fed_mage_advice(details: &ChampStats) -> Advice {
    let (sd, r) = (sd(), fed_mage_roster());
    let summary = context(&r, &sd, &HashMap::new());
    let mut i = input(r.me(), details, &sd);
    i.ctx = Some(&summary);
    advise(&i)
}

/// A real Heimerdinger game with a fed AP mage (`fed_mage_roster`).
pub(crate) fn fed_mage_game() -> Advice {
    fed_mage_advice(&heimer())
}

/// You die three times to Caitlyn (three crit items, so a clear AD foe): each time an alive roster,
/// then a dead one with your deaths one up and only Caitlyn's kills one up, 2 seconds apart. The
/// rosters go through the recap `Tracker` as in the worker, and the advice of the last dead roster
/// is made with its `killer` and `recap`. Returns the last dead roster, the tracker and the advice.
pub(crate) fn death_game() -> (Roster, recap::Tracker, Advice) {
    let (sd, d) = (sd(), heimer());
    let mut alive = roster(vec![
        scored_enemy(E2E_CAITLYN, &[3031, 3036, 3046], 2, 1, 0),
        scored_enemy(E2E_SYNDRA, &[3089], 1, 1, 1),
        scored_enemy(32, &[], 0, 1, 3),
        scored_enemy(266, &[], 1, 0, 1),
        scored_enemy(111, &[], 0, 0, 2),
    ]);
    alive.players[0].items = vec![2503];
    let mut t = recap::Tracker::default();
    let t0 = std::time::Instant::now();
    let mut dead = alive.clone();
    for n in 0..3u64 {
        let at = t0 + std::time::Duration::from_secs(20 * n);
        t.observe(&alive, at);
        dead = alive.clone();
        dead.players[0].dead = true;
        dead.players[0].deaths += 1;
        dead.players[1].kills += 1;
        t.observe(&dead, at + std::time::Duration::from_secs(2));
        // Respawn: alive again with the new score.
        alive = dead.clone();
        alive.players[0].dead = false;
    }
    let summary = context(&dead, &sd, &HashMap::new());
    let mut i = input(dead.me(), &d, &sd);
    i.ctx = Some(&summary);
    i.killer = t.killer();
    i.recap = t.recap();
    let a = advise(&i);
    (dead, t, a)
}
