//! The threat rules: which rules fire for the enemy team, the fed carry (F2, `Reason::VsFed`), the
//! killer (F5, `Reason::VsKiller`), and the build kind of each enemy (`FoeKind`).
//!
//! Owner: package A. F0 moved the rules of the game context here unchanged (`Rules::of`), and holds
//! the hooks of `rank` as stubs that fire nothing: no kind, no trigger, no hit.
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

use super::{
    FoeKind, GameContext, Hit, MIX_RULE_PCT, Pass, Player, RULE_MIN_COUNT, RuleState, Trigger,
};
use crate::model::{ItemInfo, StaticData};

/// The rules that fire, from the game context. The value of a rule is the number for its reason.
#[derive(Clone, Copy, Default)]
pub(super) struct Rules {
    pub(super) vs_ap: Option<u8>,
    pub(super) vs_ad: Option<u8>,
    pub(super) healers: Option<u8>,
    pub(super) tanks: Option<u8>,
}

impl Rules {
    /// The rules of this pass. `prev` is the state of the last pass (`Input::prev_rules`), for a
    /// hysteresis: a rule that fired may stay on a little below its limit.
    ///
    /// F0: `prev` is not read yet. Package A adds the hysteresis.
    pub(super) fn of(ctx: Option<&GameContext>, _prev: Option<RuleState>) -> Self {
        let Some(c) = ctx else { return Self::default() };
        Rules {
            vs_ap: c.ap_pct.filter(|&ap| ap >= MIX_RULE_PCT),
            vs_ad: c.ap_pct.map(|ap| 100 - ap).filter(|&ad| ad >= MIX_RULE_PCT),
            // Two healers, or one healer that is the fed enemy.
            healers: Some(c.healers.len())
                .filter(|&n| {
                    n >= RULE_MIN_COUNT
                        || (n >= 1 && c.fed.is_some_and(|f| c.healers.contains(&f.champion)))
                })
                .map(|n| n.min(255) as u8),
            tanks: Some(c.tanks.len())
                .filter(|&n| n >= RULE_MIN_COUNT)
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
///
/// F0 stub: always `FoeKind::None`.
pub(super) fn foe_kind(_sd: &StaticData, _p: &Player) -> FoeKind {
    FoeKind::None
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

/// The killer and fed triggers of this pass.
/// - VsKiller (priority `PRIO_KILLER`): `Input::killer` is a foe with an AP share of 0.60 or more
///   (MR), or 0.40 or less (armor). A mixed killer gives no trigger.
/// - VsFed (priority `PRIO_FED`): `ctx.fed`, or `Input::prev_fed` while that foe still has
///   kills - deaths >= 3. It fires only when the foe has 2 finished items or more, or is 2 levels
///   or more above you. AP share >= 0.60 gives MR; <= 0.40 or kind Crit gives armor. Kind Tank
///   never fires it.
///
/// The AP share of a foe is `ap_gold / (ap_gold + ad_gold)` (`Foe`).
///
/// F0 stub: no trigger, no fed rule.
pub(super) fn triggers(_p: &Pass) -> Threat {
    Threat::default()
}

/// Extra hits for one candidate that are no resist trigger (they add to the score and can give the
/// reason). The resist hits of the triggers do not come from here.
///
/// F0 stub: none.
pub(super) fn hits(_p: &Pass, _id: i64, _item: &ItemInfo) -> Vec<Hit> {
    Vec::new()
}
