//! The build-path tracker: "On your Malignance path" (F1), and the plan-ahead line (F7).
//!
//! Owner: package C. A path is a profile of the data: the first three finished items of a style,
//! with its buying orders and the items bought after it (`model::Path`). Your finished items give
//! each path of your style a weight. The top path then gives the core, later and other-build
//! candidates of `rank`, in place of the most played build.
//!
//! Policy: no path weight, no `w`, no later share and no rate leaves this module. `Advice` gets only
//! the index of the path, an item id, and the plan ids. The weights come from game counts only.

use super::{
    Build, CORE_BASE, CORE_STEP, ChampStats, FINISHED_GOLD, Input, LATER_BASE, LATER_STEP,
    OTHER_BASE, OTHER_STEP, Pass, Reason, StaticData, cat, with_earlier_forms,
};
use crate::model::Path;

/// The smoothing of the later shares: a path with few route games leans on the share of all paths.
const K: f64 = 400.0;
/// The factor of an owned item that is not in the core of a path, with 3 finished items or fewer.
/// It is also the factor of each missing core item from 3 finished items on.
const EPS: f64 = 0.003;
/// The smallest factor of an owned item that is not in the core, from 4 finished items on.
const LS_FLOOR: f64 = 0.01;
/// The factor of the pseudo-path "other" for each counted item.
const OTHER_PER_ITEM: f64 = 0.05;
/// The weight from which the top path names itself on the card, and the hysteresis gap.
const SURE: f64 = 0.55;
const SWITCH_GAP: f64 = 0.15;
/// The later items: the smallest mixed share, and how many.
const LATER_MIN: f64 = 0.03;
const LATER_MAX: usize = 8;
/// The other paths whose core comes in as "other build": the smallest weight, and how many items.
const SITUATIONAL_MIN: f64 = 0.05;
const SITUATIONAL_MAX: usize = 6;
/// The plan: the smallest mixed share of a later step, and how many steps.
const PLAN_MIN: f64 = 0.15;
const PLAN_MAX: usize = 2;

/// The path that drives the card in this pass. `rank` reads only the `pub(super)` fields, and
/// nothing of it reaches `Advice` except `path`, `item` and the ids. The private fields are for
/// `plan`.
pub(super) struct PathPick {
    /// The candidates in place of today's core, later and other-build loops: (item, base, reason).
    /// `rank` puts them first, then the boots, the slot items and the augment links as today.
    pub(super) cands: Vec<(i64, i32, Reason)>,
    /// `Advice::path`: the index of the top path, only when its weight is 0.55 or more.
    pub(super) path: Option<u16>,
    /// `Advice::path_item`: the core item of the top path that names it on the card.
    pub(super) item: Option<i64>,
    /// The items for `Advice::wanted` in place of the core and later items of the build: the core of
    /// the top path and the later list.
    pub(super) wanted: Vec<i64>,
    /// The core items of the top path that you do not own, in candidate order.
    core_left: Vec<i64>,
    /// The later items of all paths, by their mixed share, highest first. Never shown.
    mix: Vec<(i64, f64)>,
}

/// The weights of the paths of one style for your items.
struct Weighed<'a> {
    /// The paths of the style: the index in `ChampStats::paths`, and the path.
    paths: Vec<(usize, &'a Path)>,
    /// The weight of each path, and of the pseudo-path "other". All of them sum to 1.
    w: Vec<f64>,
    other: f64,
    /// Your finished non-boots items that some path knows (in a core or in the later items).
    counted: Vec<i64>,
}

/// Your finished non-boots items (with their earlier forms), each once.
fn owned_finished(sd: &StaticData, held: &[i64]) -> Vec<i64> {
    let mut out: Vec<i64> = Vec::new();
    for &id in held {
        let fin = sd
            .item_info
            .get(&id)
            .is_some_and(|i| i.total >= FINISHED_GOLD && i.cats & cat::BOOTS == 0);
        if fin && !out.contains(&id) {
            out.push(id);
        }
    }
    out
}

/// The games of an item after the core of a path, over all its routes. 0 when none.
fn later_n(p: &Path, id: i64) -> u32 {
    p.later.iter().find(|r| r.0 == id).map_or(0, |r| r.1)
}

/// The share of an item after the core over all paths of the style: `m(i)` of the smoothing.
fn mean_share(paths: &[(usize, &Path)], id: i64) -> f64 {
    let n: f64 = paths.iter().map(|(_, p)| f64::from(later_n(p, id))).sum();
    let rg: f64 = paths.iter().map(|(_, p)| f64::from(p.route_games)).sum();
    if rg > 0.0 { n / rg } else { 0.0 }
}

/// The smoothed later share of an item on a path: `(n + K·m) / (route_games + K)`.
fn smoothed(p: &Path, id: i64, m: f64) -> f64 {
    (f64::from(later_n(p, id)) + K * m) / (f64::from(p.route_games) + K)
}

/// The weights of the paths of the style `style` for the items `held`. `None` when the style has no
/// path, or when no weight is above 0.
fn weigh<'a>(
    details: &'a ChampStats,
    sd: &StaticData,
    held: &[i64],
    style: usize,
) -> Option<Weighed<'a>> {
    let paths: Vec<(usize, &Path)> = details
        .paths
        .iter()
        .enumerate()
        .filter(|(_, p)| usize::from(p.style) == style)
        .collect();
    if paths.is_empty() {
        return None;
    }
    // An item in no core and in no later list carries no evidence (an augment gave it, or few buy it).
    let counted: Vec<i64> = owned_finished(sd, held)
        .into_iter()
        .filter(|id| {
            paths
                .iter()
                .any(|(_, p)| p.core.contains(id) || later_n(p, *id) > 0)
        })
        .collect();
    let n = counted.len();
    let mut w: Vec<f64> = paths
        .iter()
        .map(|(_, p)| {
            let mut x = f64::from(p.games);
            for &id in &counted {
                x *= if p.core.contains(&id) {
                    1.0
                } else if n <= 3 {
                    EPS
                } else {
                    smoothed(p, id, mean_share(&paths, id)).max(LS_FLOOR)
                };
            }
            // A profile is the first three finished items: with three or more, a missing core item is unlikely.
            if n >= 3 {
                let missing = p.core.iter().filter(|c| !counted.contains(c)).count();
                x *= EPS.powi(missing as i32);
            }
            x
        })
        .collect();
    let style_games = details.style_games.get(style).copied().unwrap_or(0);
    let known: u64 = paths.iter().map(|(_, p)| u64::from(p.games)).sum();
    let mut other =
        u64::from(style_games).saturating_sub(known) as f64 * OTHER_PER_ITEM.powi(n as i32);
    let total: f64 = w.iter().sum::<f64>() + other;
    if total <= 0.0 {
        return None;
    }
    for x in &mut w {
        *x /= total;
    }
    other /= total;
    Some(Weighed {
        paths,
        w,
        other,
        counted,
    })
}

/// The hysteresis: keep the last path unless the new top is sure (w >= 0.55) and clearly ahead
/// (w >= w[prev] + 0.15). True when the new top takes over.
fn switches(w_top: f64, w_prev: f64) -> bool {
    w_top >= SURE && w_top >= w_prev + SWITCH_GAP
}

/// The path to follow: the index in `Weighed::paths`. `None` when "other" has the largest weight.
/// `prev` is `Advice::path` of the last pass (an index in `ChampStats::paths`).
fn choose(wd: &Weighed, prev: Option<u16>) -> Option<usize> {
    let mut top = 0;
    for (j, &x) in wd.w.iter().enumerate() {
        if x > wd.w[top] {
            top = j;
        }
    }
    if wd.other >= wd.w[top] {
        return None;
    }
    let prev = prev.and_then(|g| wd.paths.iter().position(|(i, _)| *i == usize::from(g)));
    match prev {
        Some(j) if j != top && !switches(wd.w[top], wd.w[j]) => Some(j),
        _ => Some(top),
    }
}

/// The mixed later share of each item over the real paths, `Σ w·Ls / Σ w`, highest first (a tie goes
/// to the lower id). Without the items in `skip` and without boots.
fn later_mix(wd: &Weighed, sd: &StaticData, skip: &[i64]) -> Vec<(i64, f64)> {
    let sum: f64 = wd.w.iter().sum();
    let mut ids: Vec<i64> = wd
        .paths
        .iter()
        .flat_map(|(_, p)| p.later.iter().map(|r| r.0))
        .filter(|id| !skip.contains(id))
        .filter(|id| {
            sd.item_info
                .get(id)
                .is_none_or(|i| i.cats & cat::BOOTS == 0)
        })
        .collect();
    ids.sort_unstable();
    ids.dedup();
    if sum <= 0.0 {
        return Vec::new();
    }
    let mut out: Vec<(i64, f64)> = ids
        .into_iter()
        .map(|id| {
            let m = mean_share(&wd.paths, id);
            let x: f64 = wd
                .paths
                .iter()
                .zip(&wd.w)
                .map(|((_, p), w)| w * smoothed(p, id, m))
                .sum();
            (id, x / sum)
        })
        .collect();
    out.sort_by(|a, b| b.1.total_cmp(&a.1).then(a.0.cmp(&b.0)));
    out
}

/// The place of an item in the most played route of a path (in its core when no route has it).
fn route_place(p: &Path, id: i64) -> usize {
    p.routes
        .first()
        .and_then(|r| r.0.iter().position(|&x| x == id))
        .or_else(|| p.core.iter().position(|&x| x == id))
        .unwrap_or(p.core.len())
}

/// The core items of a path that you do not own: first the item that most route games buy as their
/// next core item, then by the most played route.
fn core_left(p: &Path, held: &[i64]) -> Vec<i64> {
    let mut left: Vec<(i64, u64)> = p
        .core
        .iter()
        .filter(|id| !held.contains(id))
        .map(|&id| (id, 0))
        .collect();
    for (order, games) in &p.routes {
        let next = order
            .iter()
            .find(|id| p.core.contains(id) && !held.contains(id));
        if let Some(row) = next.and_then(|n| left.iter_mut().find(|r| r.0 == *n)) {
            row.1 += u64::from(*games);
        }
    }
    left.sort_by_key(|&(id, games)| (std::cmp::Reverse(games), route_place(p, id)));
    left.into_iter().map(|r| r.0).collect()
}

/// The core item that names a path: the one in the fewest other cores of the style (by their games).
/// A tie goes to the first in the most played route.
fn path_item(wd: &Weighed, star: usize) -> Option<i64> {
    let p = wd.paths[star].1;
    let shared = |id: i64| -> u64 {
        wd.paths
            .iter()
            .enumerate()
            .filter(|(j, (_, o))| *j != star && o.core.contains(&id))
            .map(|(_, (_, o))| u64::from(o.games))
            .sum()
    };
    p.core
        .iter()
        .copied()
        .min_by_key(|&id| (shared(id), route_place(p, id)))
}

/// The candidates of the top path, for the items that you own. `None` makes `rank` run today's
/// Step-1 loops: with no finished item that a path knows, with no path for the style `p.build_at`,
/// or when the pseudo-path "other" has the largest weight. `Input::prev_path` is the hysteresis:
/// keep it unless the new top has w >= 0.55 and w >= w[prev] + 0.15.
pub(super) fn data_cands(p: &Pass) -> Option<PathPick> {
    let (sd, details) = (p.input.sd, p.input.details);
    let wd = weigh(details, sd, &p.held, p.build_at)?;
    if wd.counted.is_empty() {
        return None;
    }
    let star = choose(&wd, p.input.prev_path)?;
    let (global, top) = wd.paths[star];
    let mut cands = Vec::new();
    // The rest of the core of the top path. The ladder counts the core items that you own, as
    // today's core loop does: after one item, the next core item scores 92 and a pair of boots (90)
    // still comes before the last one.
    let left = core_left(top, &p.held);
    let owned_core = top.core.len() - left.len();
    for (k, &id) in left.iter().enumerate() {
        let place = route_place(top, id).min(usize::from(u8::MAX - 1));
        cands.push((
            id,
            CORE_BASE - CORE_STEP * (owned_core + k) as i32,
            Reason::Core(place as u8 + 1),
        ));
    }
    // The later items of all paths, mixed by the weights.
    let skip: Vec<i64> = p.held.iter().chain(&top.core).copied().collect();
    let mix = later_mix(&wd, sd, &skip);
    let later: Vec<i64> = mix
        .iter()
        .filter(|r| r.1 >= LATER_MIN)
        .take(LATER_MAX)
        .map(|r| r.0)
        .collect();
    for (j, &id) in later.iter().enumerate() {
        cands.push((id, LATER_BASE - LATER_STEP * j as i32, Reason::Later));
    }
    // The cores of the other paths that still have some weight, heaviest first.
    let mut others: Vec<usize> = (0..wd.paths.len())
        .filter(|&j| j != star && wd.w[j] >= SITUATIONAL_MIN)
        .collect();
    others.sort_by(|&a, &b| wd.w[b].total_cmp(&wd.w[a]).then(a.cmp(&b)));
    let mut situational: Vec<i64> = Vec::new();
    for j in others {
        for &id in &wd.paths[j].1.core {
            if !top.core.contains(&id) && !situational.contains(&id) {
                situational.push(id);
            }
        }
    }
    for (k, &id) in situational.iter().take(SITUATIONAL_MAX).enumerate() {
        cands.push((id, OTHER_BASE - OTHER_STEP * k as i32, Reason::OtherBuild));
    }
    let sure = wd.w[star] >= SURE;
    Some(PathPick {
        cands,
        path: sure.then(|| u16::try_from(global).ok()).flatten(),
        item: if sure { path_item(&wd, star) } else { None },
        wanted: top.core.iter().chain(&later).copied().collect(),
        core_left: left,
        mix,
    })
}

/// The plan after the picks (F7), at most 2 ids. With owned' = your items and the non-boots
/// `picks`: the rest of the core of the top path (of `build.items` with no path), then the later
/// mix with a share of 0.15 or more, without owned' and the picks. Only items in the store. The
/// plan ignores rules and boots, and it is not in `Advice::signature`.
pub(super) fn plan(
    input: &Input,
    build: &Build,
    pick: Option<&PathPick>,
    picks: &[i64],
) -> Vec<i64> {
    let sd = input.sd;
    let held = with_earlier_forms(sd, &input.me.items);
    let boots = |id: &i64| {
        sd.item_info
            .get(id)
            .is_some_and(|i| i.cats & cat::BOOTS != 0)
    };
    let mut owned: Vec<i64> = held.clone();
    owned.extend(picks.iter().filter(|id| !boots(id)));
    // Without a path, the core of the build and the later mix of the paths of its style.
    let fallback;
    let (core, mix): (&[i64], &[(i64, f64)]) = match pick {
        Some(p) => (&p.core_left, &p.mix),
        None => {
            let style = input
                .details
                .builds
                .iter()
                .position(|b| std::ptr::eq(b, build));
            fallback = style
                .and_then(|s| weigh(input.details, sd, &held, s))
                .map(|wd| {
                    let skip: Vec<i64> = held.iter().chain(&build.items).copied().collect();
                    later_mix(&wd, sd, &skip)
                })
                .unwrap_or_default();
            (&build.items, &fallback)
        }
    };
    let later = mix.iter().filter(|r| r.1 >= PLAN_MIN).map(|r| r.0);
    let mut out: Vec<i64> = Vec::new();
    for id in core.iter().copied().chain(later) {
        let in_store = sd.item_info.get(&id).is_some_and(|i| i.in_store);
        if in_store
            && !boots(&id)
            && !owned.contains(&id)
            && !picks.contains(&id)
            && !out.contains(&id)
        {
            out.push(id);
        }
        if out.len() == PLAN_MAX {
            break;
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::super::{advise, rank};
    use super::*;

    // The paths of the real Heimerdinger fixture (`tests/fixtures/heimerdinger-suggest.json`), style
    // "ap", in the order of the source. The cores (`itemSet`) and the games (`sampleCount`):
    //   P1 [2503, 6653, 3089]  11480      P4 [2503, 3157, 6653]  11412
    //   P2 [2503, 6653, 4645]  15184      P5 [3118, 6653, 3116]  13232
    //   P3 [6653, 2503, 3116] 126203
    // The style has 254861 games, so "other" has 254861 - 177511 = 77350.
    // The indexes below are 0-based: P1 is 0 and P5 is 4.
    const P1: usize = 0;
    const P2: usize = 1;
    const P3: usize = 2;
    const P4: usize = 3;
    const P5: usize = 4;

    fn weights(owned: &[i64]) -> (Vec<f64>, f64) {
        let (sd, d) = (sd(), heimer());
        let held = with_earlier_forms(&sd, owned);
        let wd = weigh(&d, &sd, &held, 0).unwrap();
        (wd.w, wd.other)
    }

    fn top(owned: &[i64]) -> Option<usize> {
        let (sd, d) = (sd(), heimer());
        let held = with_earlier_forms(&sd, owned);
        let wd = weigh(&d, &sd, &held, 0).unwrap();
        choose(&wd, None)
    }

    /// The ranked candidates without boots: (id, score, reason).
    fn non_boots(owned: &[i64]) -> Vec<(i64, i32, Reason)> {
        let (sd, d) = (sd(), heimer());
        rank(&input_of(owned, &d, &sd))
            .scored
            .iter()
            .filter(|s| !s.boots)
            .map(|s| (s.id, s.score, s.reason))
            .collect()
    }

    #[test]
    fn malignance_alone_follows_p5() {
        // By hand: P5 holds 3118 in its core (factor 1), the others get EPS = 0.003 each, and
        // "other" gets 0.05 for the one item. 13232 / (13232 + 164279 * 0.003 + 77350 * 0.05)
        // = 13232 / 17592.3 = 0.752, and other = 3867.5 / 17592.3 = 0.220.
        let (w, other) = weights(&[3118]);
        assert!((0.70..=0.80).contains(&w[P5]), "{}", w[P5]);
        assert!((other - 0.22).abs() < 0.01, "{other}");
        assert_eq!(top(&[3118]), Some(P5));
        // The routes of P5 after 3118: [3118, 6653, 3116] 6099 and [6653, 3116, 3118] 1005 buy 6653
        // next (7104 games), [3118, 3116, 6653] 4981 buys 3116 next. The top route is
        // [3118, 6653, 3116], so 6653 is Core(2) and 3116 is Core(3). The ladder counts the owned
        // core item, as today's core loop does.
        let picks = non_boots(&[3118]);
        assert_eq!(picks[0], (6653, CORE_BASE - CORE_STEP, Reason::Core(2)));
        assert_eq!(picks[1], (3116, CORE_BASE - 2 * CORE_STEP, Reason::Core(3)));
        let (sd, d) = (sd(), heimer());
        let a = advise(&input_of(&[3118], &d, &sd));
        let first = a
            .items
            .iter()
            .find(|p| p.id != 3020 && p.reason != Reason::Boots)
            .unwrap();
        assert_eq!(first.id, 6653);
        assert_ne!(first.id, 2503);
        assert_eq!((a.path, a.path_item), (Some(P5 as u16), Some(3118)));
    }

    #[test]
    fn malignance_liandry_and_rylai_drop_the_blackfire_torch() {
        // P5 is complete. P3 has 3118 off its core (0.003) and misses 2503 (0.003): 126203 * 9e-6
        // = 1.1, and "other" is 77350 * 0.05^3 = 9.7, so w(P5) = 13232 / 13243 > 0.99.
        let (w, _) = weights(&[3118, 6653, 3116]);
        assert!(w[P5] >= 0.95, "{}", w[P5]);
        let (sd, d) = (sd(), heimer());
        let a = advise(&input_of(&[3118, 6653, 3116], &d, &sd));
        let picks: Vec<i64> = a
            .items
            .iter()
            .filter(|p| p.reason != Reason::Boots)
            .map(|p| p.id)
            .collect();
        // Today Blackfire Torch (2503) is pick 1, as "core item 1" of the most played build.
        assert!(!ids(&a).contains(&2503), "{:?}", ids(&a));
        assert!(!picks.is_empty());
        assert!(
            picks.iter().all(|id| [4645, 3089, 3157].contains(id)),
            "{picks:?}"
        );
    }

    #[test]
    fn torch_liandry_and_rabadon_follow_p1() {
        // P1 is complete. Each other path has one item off its core and one core item missing
        // (0.003^2), and "other" is 77350 * 0.05^3 = 9.7, so w(P1) = 11480 / 11491 = 0.999.
        let owned = [2503, 6653, 3089];
        let (w, _) = weights(&owned);
        assert!(w[P1] >= 0.95, "{}", w[P1]);
        assert_eq!(top(&owned), Some(P1));
        // The later mix is P1's own: 4645 (3967 of 10649 route games), 3116 (2746), 3135 (2551).
        let later: Vec<i64> = non_boots(&owned)
            .into_iter()
            .filter(|c| c.2 == Reason::Later)
            .map(|c| c.0)
            .take(2)
            .collect();
        assert_eq!(later, vec![4645, 3116]);
    }

    #[test]
    fn four_items_need_the_missing_core_term() {
        // From 4 items on, an item off the core gets its smoothed later share. P1 has 4645 at 0.37,
        // P2 has 3089 at 0.52 (7159 of 13870), so P2 wins. P3 has 4645 (0.22) and 3089 (0.39) off
        // its core: 126203 * 0.22 * 0.39 = 10.9k would beat P2 (7.8k) without the missing-core term.
        // With it (3116 is missing: 0.003), P3 drops to 33.
        let owned = [2503, 6653, 4645, 3089];
        assert_eq!(top(&owned), Some(P2));
        let (w, _) = weights(&owned);
        assert!(w[P2] > w[P1] && w[P1] > w[P3], "{w:?}");
    }

    #[test]
    fn nothing_owned_ranks_as_today() {
        let (sd, d) = (sd(), heimer());
        let i = input_of(&[], &d, &sd);
        let r = rank(&i);
        assert!(r.path.is_none());
        assert_eq!(r.scored[0].id, 2503);
        assert_eq!(r.scored[1].id, 6653);
        assert_eq!(score_of(&r, 2503), Some(100));
        assert_eq!(score_of(&r, 6653), Some(92));
        // The same without the paths at all.
        let bare = ChampStats {
            paths: Vec::new(),
            ..d.clone()
        };
        let r2 = rank(&input_of(&[], &bare, &sd));
        let order = |r: &super::super::Ranking| -> Vec<(i64, i32)> {
            r.scored.iter().map(|s| (s.id, s.score)).collect()
        };
        assert_eq!(order(&r), order(&r2));
    }

    #[test]
    fn the_last_path_holds_until_the_new_top_is_sure_and_clearly_ahead() {
        assert!(!switches(0.60, 0.50));
        assert!(switches(0.72, 0.50));
        // Sure but not ahead enough, and ahead but not sure.
        assert!(!switches(0.60, 0.46));
        assert!(!switches(0.54, 0.10));
        // With the real data: Malignance alone gives P5 at 0.75 and P3 at 0.02, so P5 takes over.
        let (sd, d) = (sd(), heimer());
        let held = with_earlier_forms(&sd, &[3118]);
        let wd = weigh(&d, &sd, &held, 0).unwrap();
        assert_eq!(choose(&wd, Some(P3 as u16)), Some(P5));
        // A hand weight set (the choice does not need a sum of 1): P3 at 0.50 stays against a top
        // at 0.60, and gives way at 0.72.
        let hand = |top: f64, prev: f64| Weighed {
            paths: wd.paths.clone(),
            w: vec![0.0, 0.0, prev, 0.0, top],
            other: 0.0,
            counted: vec![3118],
        };
        assert_eq!(choose(&hand(0.60, 0.50), Some(P3 as u16)), Some(P3));
        assert_eq!(choose(&hand(0.72, 0.50), Some(P3 as u16)), Some(P5));
        // With no last path, the top wins at once.
        assert_eq!(choose(&hand(0.60, 0.50), None), Some(P5));
        // A kept path below 0.55 drives the card, but it gives no `Advice::path`.
        let mut i = input_of(&[3118], &d, &sd);
        i.prev_path = Some(P3 as u16);
        assert_eq!(advise(&i).path, Some(P5 as u16));
        // An unknown last path (another style) is ignored.
        i.prev_path = Some(40);
        assert_eq!(advise(&i).path, Some(P5 as u16));
    }

    #[test]
    fn the_weights_sum_to_one_and_repeat() {
        for owned in [
            &[3118][..],
            &[2503, 6653],
            &[3118, 6653, 3116],
            &[2503, 6653, 4645, 3089],
            &[2503, 6653, 4645, 3089, 3135, 3157],
        ] {
            let (w, other) = weights(owned);
            let sum: f64 = w.iter().sum::<f64>() + other;
            assert!((sum - 1.0).abs() < 1e-9, "{owned:?}: {sum}");
            assert_eq!(weights(owned), (w, other), "{owned:?}");
            let (sd, d) = (sd(), heimer());
            assert_eq!(
                advise(&input_of(owned, &d, &sd)),
                advise(&input_of(owned, &d, &sd))
            );
        }
    }

    #[test]
    fn an_item_that_no_path_knows_changes_no_weight() {
        // 3033 (Mortal Reminder) is a finished item in the item list and in no core or later list of
        // Heimerdinger: an augment can give such an item. It counts as no evidence.
        let (sd, d) = (sd(), heimer());
        assert!(
            sd.item_info
                .get(&3033)
                .is_some_and(|i| i.total >= FINISHED_GOLD)
        );
        assert!(
            d.paths
                .iter()
                .all(|p| !p.core.contains(&3033) && later_n(p, 3033) == 0)
        );
        assert_eq!(weights(&[3118, 3033]), weights(&[3118]));
        assert_eq!(weights(&[3118, 6653, 3033]), weights(&[3118, 6653]));
        // Alone, it gives no path: today's build drives the card.
        assert!(rank(&input_of(&[3033], &d, &sd)).path.is_none());
    }

    #[test]
    fn the_smoothed_share_leans_on_the_mean_only_for_small_counts() {
        let d = heimer();
        let paths: Vec<(usize, &Path)> = d.paths.iter().enumerate().collect();
        // With n = 0 the share is K·m / (rg + K).
        let p5 = paths[P5].1;
        assert_eq!(later_n(p5, 3116), 0);
        let m = mean_share(&paths, 3116);
        assert!(m > 0.0);
        let want = K * m / (f64::from(p5.route_games) + K);
        assert!((smoothed(p5, 3116, m) - want).abs() < 1e-12);
        // A path with many route games keeps its raw rate: P3 buys 3089 in 46785 of 119665.
        let p3 = paths[P3].1;
        let raw = 46785.0 / 119665.0;
        assert!((smoothed(p3, 3089, mean_share(&paths, 3089)) - raw).abs() < 0.01);
    }

    #[test]
    fn the_path_item_is_the_rarest_core_item() {
        let (sd, d) = (sd(), heimer());
        let held: Vec<i64> = Vec::new();
        let wd = weigh(&d, &sd, &held, 0).unwrap();
        assert_eq!(path_item(&wd, P5), Some(3118));
        assert_eq!(path_item(&wd, P3), Some(3116));
        assert_eq!(path_item(&wd, P4), Some(3157));
    }

    #[test]
    fn no_finished_item_no_path_or_other_on_top_gives_none() {
        let (sd, d) = (sd(), heimer());
        // No finished item: only a part and boots.
        assert!(rank(&input_of(&[1052, 3020], &d, &sd)).path.is_none());
        // No path for the style.
        let bare = ChampStats {
            paths: Vec::new(),
            ..d.clone()
        };
        assert!(rank(&input_of(&[3118], &bare, &sd)).path.is_none());
        // "Other" on top: a style with many more games than its paths.
        let wide = ChampStats {
            style_games: vec![5_000_000],
            ..d.clone()
        };
        let held = with_earlier_forms(&sd, &[3118]);
        let wd = weigh(&wide, &sd, &held, 0).unwrap();
        assert!(wd.other > wd.w[P5]);
        assert!(rank(&input_of(&[3118], &wide, &sd)).path.is_none());
    }

    #[test]
    fn the_path_drives_wanted_and_names_no_number() {
        let (sd, d) = (sd(), heimer());
        let a = advise(&input_of(&[3118], &d, &sd));
        // The core of P5 that you do not own, and the later list, are wanted. 2503 is in P5's later.
        assert!(a.wanted.contains(&6653) && a.wanted.contains(&3116));
        assert!(!a.wanted.contains(&3118));
        // Advice holds the index and an item id only: a u16 and an i64.
        assert_eq!(a.path, Some(P5 as u16));
        assert_eq!(a.path_item, Some(3118));
    }

    #[test]
    fn advice_still_has_no_number_from_a_win_rate() {
        // The win rates of the source change nothing: set them all to 0 and 1, the advice is equal.
        fn with_rates(v: &mut serde_json::Value, rate: f64) {
            match v {
                serde_json::Value::Object(m) => {
                    for (k, x) in m.iter_mut() {
                        if k == "winRate" {
                            *x = serde_json::json!(rate);
                        } else {
                            with_rates(x, rate);
                        }
                    }
                }
                serde_json::Value::Array(a) => a.iter_mut().for_each(|x| with_rates(x, rate)),
                _ => {}
            }
        }
        let sd = sd();
        let raw: serde_json::Value = serde_json::from_str(HEIMER).unwrap();
        let (mut lo, mut hi) = (raw.clone(), raw);
        with_rates(&mut lo, 0.0);
        with_rates(&mut hi, 1.0);
        let (lo, hi) = (
            crate::data::kit_champion(&lo),
            crate::data::kit_champion(&hi),
        );
        assert_eq!(lo.paths, hi.paths);
        for owned in [&[3118][..], &[2503, 6653, 3089], &[3118, 6653, 3116]] {
            assert_eq!(
                advise(&input_of(owned, &lo, &sd)),
                advise(&input_of(owned, &hi, &sd)),
                "{owned:?}"
            );
        }
    }

    // ---- The plan (F7).

    fn plan_of(owned: &[i64], picks: &[i64]) -> Vec<i64> {
        let (sd, d) = (sd(), heimer());
        let i = input_of(owned, &d, &sd);
        let r = rank(&i);
        plan(&i, &d.builds[r.build], r.path.as_ref(), picks)
    }

    #[test]
    fn the_plan_follows_the_later_mix_after_a_full_core() {
        // P3 is complete. Its later shares: 3089 0.39, 3157 0.25, 4645 0.22, 3165 0.20.
        let plan = plan_of(&[6653, 2503, 3116], &[3089, 3020]);
        assert_eq!(plan.len(), 2, "{plan:?}");
        assert_ne!(plan[0], plan[1]);
        assert!(
            plan.iter()
                .all(|id| ![6653, 2503, 3116, 3089, 3020].contains(id))
        );
        assert!([3157, 4645, 3165].contains(&plan[0]), "{plan:?}");
    }

    #[test]
    fn the_plan_starts_with_the_rest_of_the_core() {
        // With 2503 alone, P3 is on top (126203 of 177386 = 0.71). After the pick 6653, its core
        // still lacks 3116.
        assert_eq!(top(&[2503]), Some(P3));
        assert_eq!(plan_of(&[2503], &[6653]).first(), Some(&3116));
    }

    #[test]
    fn no_later_step_with_enough_share_gives_an_empty_plan() {
        let sd = sd();
        // One path with its core owned, and a later item that few buy (100 of 10000).
        let d = ChampStats {
            builds: vec![Build {
                name: "ap".into(),
                items: vec![2503, 6653, 3089],
                ..Default::default()
            }],
            paths: vec![Path {
                style: 0,
                core: vec![2503, 6653, 3089],
                games: 10_000,
                routes: vec![(vec![2503, 6653, 3089], 10_000)],
                route_games: 10_000,
                later: vec![(4645, 100)],
            }],
            style_games: vec![10_000],
            ..Default::default()
        };
        let i = input_of(&[2503, 6653, 3089], &d, &sd);
        let r = rank(&i);
        assert!(r.path.is_some());
        assert!(plan(&i, &d.builds[0], r.path.as_ref(), &[]).is_empty());
        // The same without a path: the core is owned, and no path gives a later step.
        let bare = ChampStats {
            paths: Vec::new(),
            ..d.clone()
        };
        let i = input_of(&[2503, 6653, 3089], &bare, &sd);
        assert!(plan(&i, &bare.builds[0], None, &[]).is_empty());
    }

    #[test]
    fn the_plan_keeps_only_store_items_and_no_boots() {
        let (sd, d) = (sd(), heimer());
        for owned in [&[][..], &[2503], &[3118], &[6653, 2503, 3116]] {
            let i = input_of(owned, &d, &sd);
            let a = advise(&i);
            assert!(a.plan.len() <= 2);
            for id in &a.plan {
                let info = sd.item_info.get(id).unwrap();
                assert!(info.in_store && info.cats & cat::BOOTS == 0, "{id}");
                assert!(
                    !owned.contains(id) && !ids(&a).contains(id),
                    "{owned:?} {id}"
                );
            }
        }
    }

    #[test]
    fn the_signature_ignores_the_plan() {
        let (sd, d) = (sd(), heimer());
        let a = advise(&input_of(&[2503], &d, &sd));
        assert!(!a.plan.is_empty());
        let b = super::super::Advice {
            plan: Vec::new(),
            ..a.clone()
        };
        assert_eq!(a.signature(), b.signature());
    }
}
