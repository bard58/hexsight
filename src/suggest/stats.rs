//! Your own armor and magic resist, from the `activeplayer` read that already happens (F4).
//!
//! Owner: package B. The worker parses the stats on each read of `activeplayer` (no extra call),
//! keeps them in `Live.stats` and passes them into `Input.stats`. `factor` lowers the resist bonus
//! when you already have much of that resist, so no tip says "buy armor" at 170 armor.

use serde_json::Value;

/// Your own stats, from `activeplayer.championStats`. Every field is optional: the Live Client can
/// leave one out. Only the maximum values count, never `currentHealth`.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct MyStats {
    pub armor: Option<f32>,
    pub mr: Option<f32>,
    pub max_health: Option<f32>,
}

/// Your stats from the answer of `activeplayer`: `championStats.armor`, `.magicResist` and
/// `.maxHealth`. `None` without `championStats`. A missing field, or a value that is not a finite
/// number of 0 or more, is `None` in the result.
pub fn parse_my_stats(activeplayer: &Value) -> Option<MyStats> {
    let s = activeplayer.get("championStats")?.as_object()?;
    let field = |k: &str| {
        s.get(k)
            .and_then(Value::as_f64)
            .map(|x| x as f32)
            .filter(|x| x.is_finite() && *x >= 0.0)
    };
    Some(MyStats {
        armor: field("armor"),
        mr: field("magicResist"),
        max_health: field("maxHealth"),
    })
}

/// The share of a resist bonus that is left when you already have `x` of that resist:
/// `x.map_or(1.0, |x| ((160.0 - x) / 100.0).clamp(0.0, 1.0))`. 60 or less gives 1.0, 110 gives 0.5,
/// and 160 or more gives 0.0. `rank` takes `min(shrink(resist, n), round(resist * factor))`, and a
/// bonus of 0 gives no hit and no inject for that resist.
pub fn factor(x: Option<f32>) -> f32 {
    x.map_or(1.0, |x| ((160.0 - x) / 100.0).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::super::testkit::*;
    use super::super::{LATER_BASE, RESIST_BASE, RESIST_CARRY, Reason, rank};
    use super::*;

    const ACTIVEPLAYER: &str = include_str!("../../tests/fixtures/activeplayer-min.json");

    #[test]
    fn the_stats_come_from_the_fixture() {
        let v: Value = serde_json::from_str(ACTIVEPLAYER).unwrap();
        assert_eq!(
            parse_my_stats(&v),
            Some(MyStats {
                armor: Some(72.5),
                mr: Some(48.25),
                max_health: Some(1688.0),
            })
        );
    }

    #[test]
    fn no_champion_stats_gives_none() {
        assert_eq!(parse_my_stats(&serde_json::json!({ "level": 9 })), None);
        assert_eq!(parse_my_stats(&Value::Null), None);
        assert_eq!(
            parse_my_stats(&serde_json::json!({ "championStats": 3 })),
            None
        );
    }

    #[test]
    fn partial_fields_give_partial_stats_and_never_the_current_health() {
        let v = serde_json::json!({
            "championStats": { "armor": 40, "currentHealth": 100.0, "magicResist": "x" }
        });
        assert_eq!(
            parse_my_stats(&v),
            Some(MyStats {
                armor: Some(40.0),
                mr: None,
                max_health: None,
            })
        );
        // An empty object still is stats, with no field.
        let v = serde_json::json!({ "championStats": {} });
        assert_eq!(parse_my_stats(&v), Some(MyStats::default()));
        // A negative value is no stat.
        let v = serde_json::json!({ "championStats": { "armor": -5.0 } });
        assert_eq!(parse_my_stats(&v).unwrap().armor, None);
    }

    #[test]
    fn the_factor_is_full_to_60_half_at_110_and_none_from_160() {
        assert_eq!(factor(None), 1.0);
        assert_eq!(factor(Some(0.0)), 1.0);
        assert_eq!(factor(Some(60.0)), 1.0);
        assert!((factor(Some(110.0)) - 0.5).abs() < 1e-6);
        assert_eq!(factor(Some(160.0)), 0.0);
        assert_eq!(factor(Some(250.0)), 0.0);
    }

    #[test]
    fn much_armor_lowers_the_armor_bonus_and_leaves_magic_resist_alone() {
        let sd = sd();
        // Thornmail (armor) is a later item. A team with 70 % attack damage.
        let d = with_build("ap", &[2503, 6653, 3089], &[3075]);
        let ad = ctx(Some(30), 0, 0);
        let mut i = input_of(&[], &d, &sd);
        i.ctx = Some(&ad);
        let with_armor = |i: &mut super::super::Input, armor: f32| {
            i.stats = Some(MyStats {
                armor: Some(armor),
                ..Default::default()
            });
        };
        let today = rank(&i);
        assert_eq!(score_of(&today, 3075), Some(LATER_BASE + RESIST_CARRY));
        assert_eq!(reason_of(&today, 3075), Some(Reason::VsAd(70)));
        // 40 armor: the same as without stats.
        with_armor(&mut i, 40.0);
        assert_eq!(score_of(&rank(&i), 3075), Some(LATER_BASE + RESIST_CARRY));
        // 110 armor: half.
        with_armor(&mut i, 110.0);
        assert_eq!(
            score_of(&rank(&i), 3075),
            Some(LATER_BASE + RESIST_CARRY / 2)
        );
        // 170 armor: no armor bonus.
        with_armor(&mut i, 170.0);
        let r = rank(&i);
        assert_eq!(score_of(&r, 3075), Some(LATER_BASE));
        assert_ne!(reason_of(&r, 3075), Some(Reason::VsAd(70)));

        // With no armor item in the build, the rule adds Zhonya's Hourglass, but not at 170 armor.
        let bare = with_build("ap", &[2503, 6653, 3089], &[]);
        let mut i = input_of(&[], &bare, &sd);
        i.ctx = Some(&ad);
        assert!(score_of(&rank(&i), 3157).is_some());
        with_armor(&mut i, 170.0);
        assert_eq!(score_of(&rank(&i), 3157), None);

        // Against an AP team, 170 armor leaves the magic resist rule as it was.
        let ap = ctx(Some(70), 0, 0);
        i.ctx = Some(&ap);
        i.stats = None;
        let before = score_of(&rank(&i), 3102);
        assert_eq!(before, Some(RESIST_BASE + RESIST_CARRY));
        with_armor(&mut i, 170.0);
        assert_eq!(score_of(&rank(&i), 3102), before);
        assert_eq!(reason_of(&rank(&i), 3102), Some(Reason::VsAp(70)));
    }
}
