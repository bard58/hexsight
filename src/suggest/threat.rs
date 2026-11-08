//! The threat rules: which rules fire for the enemy team, the fed carry (F2, `Reason::VsFed`), the
//! killer (F5, `Reason::VsKiller`), and the build kind of each enemy (`FoeKind`).
//!
//! Owner: package A. The team rules of the game context (`Rules::of`, with a hysteresis), the build
//! kind of each enemy, and the killer and fed triggers of `rank`.
//!
//! How `rank` uses this module:
//! - `Rules::of` decides the team rules (VsAp, VsAd, healers, tanks). `rank` turns VsAp and VsAd into
//!   resist triggers itself, after the triggers of `triggers`.
//! - `triggers` gives the resist triggers of the killer and the fed enemy, in that order, and the
//!   fed enemy for `Advice::fed_rule`. `rank` gives every trigger the resist bonus of its bit
//!   (`Pass::mr_bonus` or `Pass::armor_bonus`): a bonus of 0 gives no hit and no inject.
//! - Each trigger gives a hit to every candidate with its bit (and `MAX_RULE_TAGS` tags or fewer).
//!   The resist hits of one item combine with `max()`, not the sum.
//! - `hits` gives extra hits for one candidate that are no resist trigger. They add up as before.
//!
//! Every input here is visible in the game: the items, the score line and the level of each enemy,
//! and who killed you. No rate of any kind is read.

use super::{
    ARMOR_AD, ARMOR_AP, ARMOR_TANK, Class, FINISHED_GOLD, Foe, FoeKind, GameContext, Hit,
    MAX_RULE_TAGS, MIX_RULE_PCT, MR_AD, MR_AP, MR_TANK, PRIO_FED, PRIO_KILLER, Pass, Player,
    RULE_MIN_COUNT, Reason, RuleState, Trigger,
};
use crate::model::{ItemInfo, StaticData, cat};

/// A damage rule that fired stays on down to this share (percent) of its damage type.
const MIX_OFF_PCT: u8 = 50;
/// A fed enemy keeps the fed rule while its kills minus deaths stay at this or more.
const FED_KEEP_NET: i32 = 3;
/// The fed rule needs this many finished items on the enemy, or this many levels above you.
const FED_MIN_FINISHED: u8 = 2;
const FED_LEVEL_LEAD: u8 = 2;
/// An enemy with this AP share or more gets magic resist; with `AD_SHARE` or less, armor.
const AP_SHARE: f32 = 0.60;
const AD_SHARE: f32 = 0.40;
/// A build kind needs this many items of the kind.
const KIND_MIN: u8 = 2;
/// The magic resist inject of a tank build that heals (a LIFESTEAL item in its core or later):
/// Spirit Visage first, then Force of Nature and Kaenic Rookern.
const MR_TANK_HEAL: [i64; 3] = [3065, 4401, 2504];

/// The rules that fire, from the game context. The value of a rule is the number for its reason.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) struct Rules {
    pub(super) vs_ap: Option<u8>,
    pub(super) vs_ad: Option<u8>,
    pub(super) healers: Option<u8>,
    pub(super) tanks: Option<u8>,
}

impl Rules {
    /// The rules of this pass. `prev` is the state of the last pass (`Input::prev_rules`), for a
    /// hysteresis, so a rule does not flicker at its limit:
    /// - VsAp and VsAd switch on at `MIX_RULE_PCT` (60). A rule that was on stays on down to
    ///   `MIX_OFF_PCT` (50), and goes off below it.
    /// - Healers and tanks switch on at `RULE_MIN_COUNT` (2; healers also with one healer that is
    ///   the fed enemy). The tank rule that was on stays on with one less.
    /// - The healer rule with one healer stays on only while that healer is the fed enemy, or was on
    ///   and the healer still has kills - deaths >= `FED_KEEP_NET`. So it never holds for the whole
    ///   game for a healer that is no longer fed.
    pub(super) fn of(ctx: Option<&GameContext>, prev: Option<RuleState>) -> Self {
        let Some(c) = ctx else { return Self::default() };
        let prev = prev.unwrap_or_default();
        let mix = |pct: u8, was: bool| pct >= MIX_RULE_PCT || (was && pct >= MIX_OFF_PCT);
        let count = |n: usize, was: bool| {
            n >= 1 && (n >= RULE_MIN_COUNT || (was && n + 1 >= RULE_MIN_COUNT))
        };
        let fed_healer = c.fed.is_some_and(|f| c.healers.contains(&f.champion));
        // A healer that was the reason of the rule and is still ahead (the fed hysteresis).
        let kept_healer = prev.healers
            && c.healers.iter().any(|&h| {
                c.foes.iter().any(|f| {
                    f.champion == h && i32::from(f.kills) - i32::from(f.deaths) >= FED_KEEP_NET
                })
            });
        Rules {
            vs_ap: c.ap_pct.filter(|&ap| mix(ap, prev.vs_ap)),
            vs_ad: c
                .ap_pct
                .map(|ap| 100 - ap)
                .filter(|&ad| mix(ad, prev.vs_ad)),
            // Two healers, or one healer that is (or stays) the fed enemy.
            healers: Some(c.healers.len())
                .filter(|&n| n >= RULE_MIN_COUNT || (n >= 1 && (fed_healer || kept_healer)))
                .map(|n| n.min(255) as u8),
            tanks: Some(c.tanks.len())
                .filter(|&n| count(n, prev.tanks))
                .map(|n| n.min(255) as u8),
        }
    }

    /// Which rules fired, for `Advice::rules` and the next `Input::prev_rules`.
    pub(super) fn state(&self) -> RuleState {
        RuleState {
            vs_ap: self.vs_ap.is_some(),
            vs_ad: self.vs_ad.is_some(),
            tanks: self.tanks.is_some(),
            healers: self.healers.is_some(),
        }
    }
}

/// The build kind of an enemy. Count its finished non-boots items with `MAX_RULE_TAGS` tags or
/// fewer, each once, under its first match:
/// - `cat::CRIT` gives Crit;
/// - `cat::AP` or `cat::MAGIC_PEN` gives Ap;
/// - `cat::AD` or `cat::ARMOR_PEN` gives Ad;
/// - `cat::ARMOR` or `cat::MR` without AP or AD gives Tank.
///
/// The largest count wins, and a tie goes in the order crit > ap > ad > tank. A count under 2 is
/// `FoeKind::None`.
pub(super) fn foe_kind(sd: &StaticData, p: &Player) -> FoeKind {
    // The counts of crit, AP, AD and tank items.
    let mut n = [0u8; 4];
    for i in p.items.iter().filter_map(|id| sd.item_info.get(id)) {
        if i.total < FINISHED_GOLD || i.cats & cat::BOOTS != 0 || i.tags > MAX_RULE_TAGS {
            continue;
        }
        let at = if i.cats & cat::CRIT != 0 {
            0
        } else if i.cats & (cat::AP | cat::MAGIC_PEN) != 0 {
            1
        } else if i.cats & (cat::AD | cat::ARMOR_PEN) != 0 {
            2
        } else if i.cats & (cat::ARMOR | cat::MR) != 0 {
            3
        } else {
            continue;
        };
        n[at] = n[at].saturating_add(1);
    }
    // The first of the largest counts.
    let mut at = 0;
    for k in 1..n.len() {
        if n[k] > n[at] {
            at = k;
        }
    }
    let most = n[at];
    if most < KIND_MIN {
        return FoeKind::None;
    }
    match at {
        0 => FoeKind::Crit(most),
        1 => FoeKind::Ap(most),
        2 => FoeKind::Ad(most),
        _ => FoeKind::Tank(most),
    }
}

/// What the threat rules add to one pass of `rank`.
#[derive(Default)]
pub(super) struct Threat {
    /// The resist triggers, in inject order: the killer first, then the fed enemy. At most one per
    /// rule and bit. The list of a trigger is the inject list for your class (`MR_*` / `ARMOR_*`,
    /// or `[3065, 4401, 2504]` for MR with a tank build that has a LIFESTEAL item).
    pub(super) triggers: Vec<Trigger>,
    /// The champion of the fed enemy when the VsFed rule fired (`Advice::fed_rule`). The worker
    /// passes it back as `Input::prev_fed`.
    pub(super) fed_rule: Option<i64>,
}

/// The AP share of a foe, 0 to 1. `None` when it has no gold of either kind.
fn ap_share(f: &Foe) -> Option<f32> {
    let all = f.ap_gold + f.ad_gold;
    (all > 0).then(|| f.ap_gold as f32 / all as f32)
}

/// The resist bit against a foe: magic resist for an AP share of `AP_SHARE` or more, armor for
/// `AD_SHARE` or less, or (with `crit`) for a crit build. A mixed foe gives none.
fn resist_bit(f: &Foe, crit: bool) -> Option<u16> {
    let share = ap_share(f);
    if share.is_some_and(|s| s >= AP_SHARE) {
        Some(cat::MR)
    } else if share.is_some_and(|s| s <= AD_SHARE) || (crit && matches!(f.kind, FoeKind::Crit(_))) {
        Some(cat::ARMOR)
    } else {
        None
    }
}

/// The inject list of a resist bit for your class. A tank build that heals (a LIFESTEAL item in its
/// core or later) gets Spirit Visage first for magic resist.
fn inject_list(p: &Pass, bit: u16) -> &'static [i64] {
    let mr = bit == cat::MR;
    if p.tank {
        let heals = p.build.items.iter().chain(&p.build.later).any(|id| {
            p.input
                .sd
                .item_info
                .get(id)
                .is_some_and(|i| i.cats & cat::LIFESTEAL != 0)
        });
        return match (mr, heals) {
            (true, true) => &MR_TANK_HEAL,
            (true, false) => &MR_TANK,
            (false, _) => &ARMOR_TANK,
        };
    }
    match (p.class, mr) {
        (Class::Ap, true) => &MR_AP,
        (Class::Ap, false) => &ARMOR_AP,
        (Class::Ad, true) => &MR_AD,
        (Class::Ad, false) => &ARMOR_AD,
        (Class::Mixed, _) => &[],
    }
}

/// The foe that the fed rule looks at: `ctx.fed`, or else `prev_fed` while that foe still has
/// kills - deaths >= `FED_KEEP_NET`.
fn fed_foe(c: &GameContext, prev_fed: Option<i64>) -> Option<&Foe> {
    let foe = |id: i64| c.foes.iter().find(|f| f.champion == id);
    match c.fed {
        Some(f) => foe(f.champion),
        None => prev_fed
            .and_then(foe)
            .filter(|f| i32::from(f.kills) - i32::from(f.deaths) >= FED_KEEP_NET),
    }
}

/// The killer and fed triggers of this pass.
/// - VsKiller (priority `PRIO_KILLER`): `Input::killer` is a foe with an AP share of 0.60 or more
///   (MR), or 0.40 or less (armor). A mixed killer gives no trigger.
/// - VsFed (priority `PRIO_FED`): `ctx.fed`, or `Input::prev_fed` while that foe still has
///   kills - deaths >= 3. It fires only when the foe has 2 finished items or more, or is 2 levels
///   or more above you. AP share >= 0.60 gives MR; <= 0.40 or kind Crit gives armor. Kind Tank
///   never fires it: the penetration rule covers tanks.
///
/// The AP share of a foe is `ap_gold / (ap_gold + ad_gold)` (`Foe`). `fed_rule` is set when the
/// VsFed trigger is made, so the rule keeps its foe on the next pass even when your own resist
/// gives it a bonus of 0 now.
pub(super) fn triggers(p: &Pass) -> Threat {
    let mut out = Threat::default();
    let Some(c) = p.input.ctx else { return out };
    if let Some(k) = p.input.killer
        && let Some(f) = c.foes.iter().find(|f| f.champion == k.champion)
        && let Some(bit) = resist_bit(f, false)
    {
        out.triggers.push(Trigger {
            bit,
            reason: Reason::VsKiller {
                champion: k.champion,
                times: k.times,
            },
            list: inject_list(p, bit),
            priority: PRIO_KILLER,
        });
    }
    if let Some(f) = fed_foe(c, p.input.prev_fed)
        && !matches!(f.kind, FoeKind::Tank(_))
        && (f.finished >= FED_MIN_FINISHED
            || f.level >= p.input.me.level.saturating_add(FED_LEVEL_LEAD))
        && let Some(bit) = resist_bit(f, true)
    {
        out.triggers.push(Trigger {
            bit,
            reason: Reason::VsFed {
                champion: f.champion,
                kills: f.kills.min(255) as u8,
                deaths: f.deaths.min(255) as u8,
            },
            list: inject_list(p, bit),
            priority: PRIO_FED,
        });
        out.fed_rule = Some(f.champion);
    }
    out
}

/// Extra hits for one candidate that are no resist trigger (they add to the score and can give the
/// reason). The resist hits of the triggers do not come from here: the killer and fed rules are
/// resist triggers only, so this module adds no extra hit today.
pub(super) fn hits(_p: &Pass, _id: i64, _item: &ItemInfo) -> Vec<Hit> {
    Vec::new()
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::super::testkit::*;
    use super::super::{
        Advice, ChampStats, GameContext, Killer, Player, Ranking, Reason, RuleState, advise,
        context, rank,
    };
    use super::*;
    use crate::i18n::{Key, Lang};

    const SYNDRA: i64 = 134;
    const CAITLYN: i64 = 51;
    /// Rabadon's Deathcap and Shadowflame: two finished AP items.
    const AP_ITEMS: [i64; 2] = [3089, 4645];
    /// Infinity Edge, Lord Dominik's Regards and Phantom Dancer: three crit items.
    const CRIT_ITEMS: [i64; 3] = [3031, 3036, 3046];

    /// The context of these enemies, with the damage mix set by hand.
    fn team(enemies: Vec<Player>, ap_pct: Option<u8>) -> GameContext {
        let mut c = context_of(&roster(enemies), &sd());
        c.ap_pct = ap_pct;
        c
    }

    fn syndra(k: u16, d: u16) -> Player {
        scored_enemy(SYNDRA, &AP_ITEMS, k, d, 0)
    }

    fn vs_fed(champion: i64, kills: u8, deaths: u8) -> Reason {
        Reason::VsFed {
            champion,
            kills,
            deaths,
        }
    }

    /// The ranking of you (with these items, these stats and this context) and the advice.
    fn run(
        owned: &[i64],
        d: &ChampStats,
        c: &GameContext,
        prev_fed: Option<i64>,
        killer: Option<Killer>,
    ) -> (Ranking, Advice) {
        let sd = sd();
        let mut i = input_of(owned, d, &sd);
        i.ctx = Some(c);
        i.prev_fed = prev_fed;
        i.killer = killer;
        (rank(&i), advise(&i))
    }

    fn has_reason(r: &Ranking, f: impl Fn(Reason) -> bool) -> bool {
        r.scored.iter().any(|s| f(s.reason))
    }

    fn any_fed(r: &Ranking) -> bool {
        has_reason(r, |x| matches!(x, Reason::VsFed { .. }))
    }

    // ---- F2: the fed carry.

    #[test]
    fn a_fed_ap_mage_brings_banshees_veil_with_vs_fed() {
        let d = heimer();
        let c = team(vec![syndra(9, 1)], Some(45));
        let (r, a) = run(&[], &d, &c, None, None);
        assert_eq!(reason_of(&r, 3102), Some(vs_fed(SYNDRA, 9, 1)));
        assert_eq!((r.fed_rule, a.fed_rule), (Some(SYNDRA), Some(SYNDRA)));
        // The mix alone (45% AP) fires no resist rule.
        assert!(!a.rules.vs_ap && !a.rules.vs_ad);
    }

    #[test]
    fn the_same_enemy_at_3_4_is_no_fed_rule() {
        let d = heimer();
        let c = team(vec![syndra(3, 4)], Some(45));
        let (r, a) = run(&[], &d, &c, None, None);
        assert!(!any_fed(&r));
        assert_eq!(a.fed_rule, None);
    }

    #[test]
    fn a_fed_crit_caitlyn_brings_randuins_omen_to_a_tank_build() {
        let d = with_build("tank", &[3089], &[]);
        let c = team(vec![scored_enemy(CAITLYN, &CRIT_ITEMS, 9, 1, 4)], None);
        assert_eq!(c.foes[0].kind, FoeKind::Crit(3));
        let (r, a) = run(&[], &d, &c, None, None);
        assert_eq!(reason_of(&r, 3143), Some(vs_fed(CAITLYN, 9, 1)));
        assert!(ids(&a).contains(&3143));
        assert_eq!(a.fed_rule, Some(CAITLYN));
    }

    #[test]
    fn a_hybrid_fed_enemy_gives_no_hit() {
        let sd = sd();
        let d = heimer();
        let fed = Player {
            level: 12,
            ..scored_enemy(SYNDRA, &[], 9, 1, 0)
        };
        let c = context(&roster(vec![fed]), &sd, &HashMap::from([(SYNDRA, 0.5)]));
        let f = c.foes[0];
        assert_eq!(f.ap_gold, f.ad_gold);
        let (r, a) = run(&[], &d, &c, None, None);
        assert!(!any_fed(&r));
        assert_eq!(a.fed_rule, None);
    }

    #[test]
    fn vs_ap_and_vs_fed_on_the_same_item_add_the_resist_points_once() {
        let d = heimer();
        let fed = team(vec![syndra(9, 1)], Some(70));
        let calm = team(vec![syndra(3, 4)], Some(70));
        let (fed_r, _) = run(&[], &d, &fed, None, None);
        let (calm_r, _) = run(&[], &d, &calm, None, None);
        assert_eq!(reason_of(&calm_r, 3102), Some(Reason::VsAp(70)));
        // The fed rule wins the reason on the tie (priority 1 before 4), and adds no points.
        assert_eq!(reason_of(&fed_r, 3102), Some(vs_fed(SYNDRA, 9, 1)));
        assert!(score_of(&fed_r, 3102).is_some());
        assert_eq!(score_of(&fed_r, 3102), score_of(&calm_r, 3102));
    }

    #[test]
    fn the_fed_rule_holds_down_to_a_net_of_3_with_prev_fed() {
        let d = heimer();
        // Net 5 with 6 kills: fed.
        let (r, a) = run(&[], &d, &team(vec![syndra(6, 1)], Some(45)), None, None);
        assert_eq!(
            (reason_of(&r, 3102), a.fed_rule),
            (Some(vs_fed(SYNDRA, 6, 1)), Some(SYNDRA))
        );
        // Net 4: no longer fed, but the rule of the last pass holds.
        let net4 = team(vec![syndra(6, 2)], Some(45));
        assert!(net4.fed.is_none());
        let (r, a) = run(&[], &d, &net4, Some(SYNDRA), None);
        assert_eq!(
            (reason_of(&r, 3102), a.fed_rule),
            (Some(vs_fed(SYNDRA, 6, 2)), Some(SYNDRA))
        );
        // Without the rule of the last pass, net 4 does not fire it.
        let (r, a) = run(&[], &d, &net4, None, None);
        assert!(!any_fed(&r) && a.fed_rule.is_none());
        // Net 3 holds. Net 2 releases it.
        let (_, a) = run(
            &[],
            &d,
            &team(vec![syndra(6, 3)], Some(45)),
            Some(SYNDRA),
            None,
        );
        assert_eq!(a.fed_rule, Some(SYNDRA));
        let (r, a) = run(
            &[],
            &d,
            &team(vec![syndra(6, 4)], Some(45)),
            Some(SYNDRA),
            None,
        );
        assert!(!any_fed(&r) && a.fed_rule.is_none());
        // A prev_fed that is no foe of this game does nothing.
        let (_, a) = run(&[], &d, &net4, Some(999), None);
        assert_eq!(a.fed_rule, None);
    }

    #[test]
    fn the_fed_rule_needs_two_finished_items_or_two_levels() {
        let d = heimer();
        let one_item = scored_enemy(SYNDRA, &[3089], 9, 1, 0);
        // One finished item at your level (9): no fire.
        let (r, a) = run(&[], &d, &team(vec![one_item.clone()], Some(45)), None, None);
        assert!(!any_fed(&r) && a.fed_rule.is_none());
        // One level above: still no fire.
        let up1 = Player {
            level: 10,
            ..one_item.clone()
        };
        let (_, a) = run(&[], &d, &team(vec![up1], Some(45)), None, None);
        assert_eq!(a.fed_rule, None);
        // Two levels above: it fires.
        let up2 = Player {
            level: 11,
            ..one_item
        };
        let (r, a) = run(&[], &d, &team(vec![up2], Some(45)), None, None);
        assert_eq!(reason_of(&r, 3102), Some(vs_fed(SYNDRA, 9, 1)));
        assert_eq!(a.fed_rule, Some(SYNDRA));
    }

    #[test]
    fn a_tank_build_with_lifesteal_gets_spirit_visage_vs_a_fed_mage() {
        let c = team(vec![syndra(9, 1)], None);
        // Ravenous Hydra (lifesteal) in the core.
        let (r, _) = run(&[], &with_build("tank", &[3074], &[]), &c, None, None);
        assert_eq!(reason_of(&r, 3065), Some(vs_fed(SYNDRA, 9, 1)));
        assert_eq!(reason_of(&r, 4401), None);
        // In the later items, too.
        let (r, _) = run(&[], &with_build("tank", &[3089], &[3074]), &c, None, None);
        assert_eq!(reason_of(&r, 3065), Some(vs_fed(SYNDRA, 9, 1)));
        // Without lifesteal, the tank list as before: Force of Nature.
        let (r, _) = run(&[], &with_build("tank", &[3089], &[]), &c, None, None);
        assert_eq!(reason_of(&r, 4401), Some(vs_fed(SYNDRA, 9, 1)));
        assert_eq!(reason_of(&r, 3065), None);
    }

    #[test]
    fn a_fed_tank_never_fires_the_fed_rule() {
        let d = heimer();
        // Kaenic Rookern and Force of Nature: a tank build, with the role prior of a mage.
        let c = team(vec![scored_enemy(SYNDRA, &[2504, 4401], 9, 1, 0)], Some(45));
        assert_eq!(c.foes[0].kind, FoeKind::Tank(2));
        let (r, a) = run(&[], &d, &c, None, None);
        assert!(!any_fed(&r) && a.fed_rule.is_none());
    }

    #[test]
    fn boots_parts_and_items_with_many_tags_do_not_count_in_the_kind() {
        let sd = sd();
        let kind = |items: &[i64]| foe_kind(&sd, &enemy(SYNDRA, items));
        assert!(sd.item_info[&4403].tags > MAX_RULE_TAGS);
        // Sorcerer's Shoes (boots) and a Needlessly Large Rod (a part) are no AP items.
        assert_eq!(kind(&[3089, 1058, 3020]), FoeKind::None);
        assert_eq!(kind(&[3089, 4645, 3020, 1058]), FoeKind::Ap(2));
        // The Golden Spatula has 13 tags.
        assert_eq!(kind(&[4403, 4403, 3089]), FoeKind::None);
        assert_eq!(kind(&[4403, 4403]), FoeKind::None);
        // Each item counts once, under its first match: Lord Dominik's is crit, not AD.
        assert_eq!(kind(&CRIT_ITEMS), FoeKind::Crit(3));
        // A tie goes crit > ap > ad > tank.
        assert_eq!(kind(&[3031, 3036, 3089, 4645]), FoeKind::Crit(2));
        assert_eq!(kind(&[3089, 4645, 2504, 4401]), FoeKind::Ap(2));
        // Banshee's Veil and Zhonya's Hourglass have AP: no tank items.
        assert_eq!(kind(&[3102, 3157]), FoeKind::Ap(2));
        assert_eq!(kind(&[2504, 4401, 3143]), FoeKind::Tank(3));
        assert_eq!(kind(&[]), FoeKind::None);
    }

    #[test]
    fn the_turkish_threat_texts_have_no_suffix_after_a_name() {
        for key in [
            Key::ReasonVsFed,
            Key::ReasonVsFedCell,
            Key::ReasonVsKiller,
            Key::ReasonVsKillerN,
            Key::ReasonVsKillerCell,
            Key::CtxKindCrit,
            Key::CtxKindAp,
            Key::CtxKindAd,
            Key::CtxKindTank,
            Key::CtxKindHeals,
        ] {
            for lang in [Lang::En, Lang::Tr] {
                let text = lang.t(key);
                for (at, _) in text.match_indices("{0}") {
                    let after = text[at + 3..].chars().next();
                    assert!(
                        !matches!(after, Some('’' | '\'')),
                        "{lang:?} {text:?} has a suffix after {{0}}"
                    );
                }
            }
        }
    }

    // ---- F5: the killer.

    #[test]
    fn an_ad_killer_raises_armor_once_with_vs_ad() {
        let d = heimer();
        // Caitlyn is not fed (2/0), and the team is 70% AD.
        let c = team(vec![scored_enemy(CAITLYN, &[3031], 2, 0, 0)], Some(30));
        let killer = Killer {
            champion: CAITLYN,
            times: 3,
        };
        let (plain, _) = run(&[], &d, &c, None, None);
        let (killed, a) = run(&[], &d, &c, None, Some(killer));
        assert_eq!(reason_of(&plain, 3157), Some(Reason::VsAd(70)));
        assert_eq!(
            reason_of(&killed, 3157),
            Some(Reason::VsKiller {
                champion: CAITLYN,
                times: 3
            })
        );
        // The resist points come once: the killer adds none on top of VsAd.
        assert!(score_of(&plain, 3157).is_some());
        assert_eq!(score_of(&killed, 3157), score_of(&plain, 3157));
        // The killer rule is no fed rule.
        assert_eq!(a.fed_rule, None);
    }

    #[test]
    fn an_ap_killer_brings_magic_resist_with_no_mix_rule() {
        let d = heimer();
        let c = team(vec![scored_enemy(SYNDRA, &[3089], 1, 0, 0)], Some(45));
        let killer = Killer {
            champion: SYNDRA,
            times: 1,
        };
        let (r, _) = run(&[], &d, &c, None, Some(killer));
        assert_eq!(
            reason_of(&r, 3102),
            Some(Reason::VsKiller {
                champion: SYNDRA,
                times: 1
            })
        );
    }

    #[test]
    fn a_mixed_or_unknown_killer_gives_no_hit() {
        let sd = sd();
        let d = heimer();
        let mixed = context(
            &roster(vec![scored_enemy(SYNDRA, &[], 1, 0, 0)]),
            &sd,
            &HashMap::from([(SYNDRA, 0.5)]),
        );
        let is_killer = |x: Reason| matches!(x, Reason::VsKiller { .. });
        let killer = |champion| Killer { champion, times: 2 };
        let (r, _) = run(&[], &d, &mixed, None, Some(killer(SYNDRA)));
        assert!(!has_reason(&r, is_killer));
        // A killer that is no foe of the context.
        let c = team(vec![scored_enemy(SYNDRA, &[3089], 1, 0, 0)], Some(45));
        let (r, _) = run(&[], &d, &c, None, Some(killer(CAITLYN)));
        assert!(!has_reason(&r, is_killer));
    }

    #[test]
    fn the_killer_and_the_fed_enemy_both_inject_and_the_killer_wins_its_item() {
        let d = heimer();
        // Caitlyn killed you; Syndra is fed. Each gives its own resist.
        let c = team(
            vec![scored_enemy(CAITLYN, &[3031], 2, 0, 0), syndra(9, 1)],
            Some(50),
        );
        let killer = Killer {
            champion: CAITLYN,
            times: 1,
        };
        let (r, a) = run(&[], &d, &c, None, Some(killer));
        assert_eq!(
            reason_of(&r, 3157),
            Some(Reason::VsKiller {
                champion: CAITLYN,
                times: 1
            })
        );
        assert_eq!(reason_of(&r, 3102), Some(vs_fed(SYNDRA, 9, 1)));
        assert_eq!(a.fed_rule, Some(SYNDRA));
        // When the killer is the fed enemy, the killer reason wins the tie on the same item.
        let (r, _) = run(
            &[],
            &d,
            &c,
            None,
            Some(Killer {
                champion: SYNDRA,
                times: 2,
            }),
        );
        assert_eq!(
            reason_of(&r, 3102),
            Some(Reason::VsKiller {
                champion: SYNDRA,
                times: 2
            })
        );
    }

    #[test]
    fn no_context_means_no_threat_rule() {
        let sd = sd();
        let d = heimer();
        let mut i = input_of(&[], &d, &sd);
        i.prev_fed = Some(SYNDRA);
        i.killer = Some(Killer {
            champion: SYNDRA,
            times: 3,
        });
        let r = rank(&i);
        assert!(r.fed_rule.is_none());
        assert!(!r.scored.iter().any(|s| s.reason.is_rule()));
    }

    // ---- The hysteresis of the team rules.

    fn rules(c: &GameContext, prev: Option<RuleState>) -> Rules {
        Rules::of(Some(c), prev)
    }

    const AP_ON: RuleState = RuleState {
        vs_ap: true,
        vs_ad: false,
        tanks: false,
        healers: false,
    };

    #[test]
    fn vs_ap_switches_on_at_60() {
        assert_eq!(rules(&ctx(Some(60), 0, 0), None).vs_ap, Some(60));
        assert_eq!(
            rules(&ctx(Some(60), 0, 0), Some(RuleState::default())).vs_ap,
            Some(60)
        );
        assert_eq!(rules(&ctx(Some(55), 0, 0), None).vs_ap, None);
    }

    #[test]
    fn vs_ap_stays_on_at_55_when_it_was_on() {
        assert_eq!(rules(&ctx(Some(55), 0, 0), Some(AP_ON)).vs_ap, Some(55));
        assert_eq!(rules(&ctx(Some(50), 0, 0), Some(AP_ON)).vs_ap, Some(50));
    }

    #[test]
    fn vs_ap_goes_off_at_49_even_when_it_was_on() {
        assert_eq!(rules(&ctx(Some(49), 0, 0), Some(AP_ON)).vs_ap, None);
        assert_eq!(rules(&ctx(None, 0, 0), Some(AP_ON)).vs_ap, None);
    }

    #[test]
    fn vs_ap_stays_off_at_55_when_it_was_off() {
        assert_eq!(
            rules(&ctx(Some(55), 0, 0), Some(RuleState::default())).vs_ap,
            None
        );
    }

    #[test]
    fn vs_ad_has_the_same_hysteresis() {
        let ad_on = RuleState {
            vs_ad: true,
            ..Default::default()
        };
        assert_eq!(rules(&ctx(Some(40), 0, 0), None).vs_ad, Some(60));
        assert_eq!(rules(&ctx(Some(45), 0, 0), Some(ad_on)).vs_ad, Some(55));
        assert_eq!(rules(&ctx(Some(45), 0, 0), None).vs_ad, None);
        assert_eq!(rules(&ctx(Some(51), 0, 0), Some(ad_on)).vs_ad, None);
        // VsAp on does not hold VsAd, and the other way round.
        assert_eq!(rules(&ctx(Some(45), 0, 0), Some(AP_ON)).vs_ad, None);
        assert_eq!(rules(&ctx(Some(55), 0, 0), Some(ad_on)).vs_ap, None);
    }

    #[test]
    fn tanks_stay_on_with_one_less_and_one_calm_healer_does_not() {
        let all_on = RuleState {
            vs_ap: false,
            vs_ad: false,
            tanks: true,
            healers: true,
        };
        let on = rules(&ctx(None, 2, 2), None);
        assert_eq!((on.healers, on.tanks), (Some(2), Some(2)));
        // One tank holds the tank rule. One healer that is not ahead does not hold the healer rule.
        let held = rules(&ctx(None, 1, 1), Some(all_on));
        assert_eq!((held.healers, held.tanks), (None, Some(1)));
        let off = rules(&ctx(None, 1, 1), Some(RuleState::default()));
        assert_eq!((off.healers, off.tanks), (None, None));
        let none = rules(&ctx(None, 0, 0), Some(all_on));
        assert_eq!((none.healers, none.tanks), (None, None));
    }

    #[test]
    fn the_one_healer_rule_releases_when_the_healer_is_no_longer_fed() {
        const SORAKA: i64 = 6;
        let healers_on = RuleState {
            healers: true,
            ..Default::default()
        };
        // Soraka is fed (8/1) and the only healer, so the rule fires.
        let soraka = |k: u16, d: u16| scored_enemy(SORAKA, &[], k, d, 0);
        let fed = team(vec![soraka(8, 1)], None);
        assert_eq!(fed.healers, vec![SORAKA]);
        assert_eq!(fed.fed.map(|f| f.champion), Some(SORAKA));
        assert_eq!(rules(&fed, None).healers, Some(1));
        // 8/4: no longer fed, but net 4 holds the rule of the last pass.
        let ahead = team(vec![soraka(8, 4)], None);
        assert!(ahead.fed.is_none());
        assert_eq!(rules(&ahead, Some(healers_on)).healers, Some(1));
        assert_eq!(rules(&ahead, None).healers, None);
        // 8/6: net 2, the rule goes off and does not latch.
        let behind = team(vec![soraka(8, 6)], None);
        assert_eq!(rules(&behind, Some(healers_on)).healers, None);
    }

    #[test]
    fn the_rule_state_feeds_the_next_pass() {
        let sd = sd();
        let d = heimer();
        let hot = ctx(Some(60), 0, 0);
        let warm = ctx(Some(55), 0, 0);
        let mut i = input_of(&[], &d, &sd);
        i.ctx = Some(&hot);
        let first = advise(&i);
        assert_eq!(first.rules, AP_ON);
        assert_eq!(rules(&hot, None).state(), AP_ON);
        // The mix drops to 55: the rule holds with the state of the last pass, and not without it.
        i.ctx = Some(&warm);
        i.prev_rules = Some(first.rules);
        let second = advise(&i);
        assert!(second.rules.vs_ap);
        assert!(has_reason(&rank(&i), |x| x == Reason::VsAp(55)));
        i.prev_rules = None;
        assert!(!advise(&i).rules.vs_ap);
    }
}
