//! The build-path tracker: "On your Malignance path" (F1), and the plan-ahead line (F7).
//!
//! Owner: package C. F0 holds the hooks of `rank` and `advise` as stubs that find no path, so
//! today's Step-1 candidates (core, later, other builds) run and the plan is empty.
//!
//! Policy: no path weight, no `w` and no rate leaves this module. `Advice` gets only the index of
//! the path, an item id, and the plan ids.

use super::{Input, Pass, Reason};
use crate::model::Build;

/// The path that drives the card in this pass. Package C may add private fields (the weights, the
/// later mix) for `plan`: `rank` reads only the fields below, and nothing of it reaches `Advice`
/// except `path`, `item` and the ids.
#[allow(dead_code)] // F0 stub: package C builds it in `data_cands`.
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
}

/// The candidates of the top path, for the items that you own. `None` makes `rank` run today's
/// Step-1 loops: with no finished item, with no path for the style `p.build_at`, or when the
/// pseudo-path "other" has the largest weight. `Input::prev_path` is the hysteresis: keep it unless
/// the new top has w >= 0.55 and w >= w[prev] + 0.15.
///
/// F0 stub: always `None`.
pub(super) fn data_cands(_p: &Pass) -> Option<PathPick> {
    None
}

/// The plan after the picks (F7), at most 2 ids. With owned' = your items and the non-boots
/// `picks`: the rest of the core of the top path (of `build.items` with no path), then the later
/// mix with a share of 0.15 or more, without owned' and the picks. Only items in the store. The
/// plan ignores rules and boots, and it is not in `Advice::signature`.
///
/// F0 stub: always empty.
pub(super) fn plan(
    _input: &Input,
    _build: &Build,
    _pick: Option<&PathPick>,
    _picks: &[i64],
) -> Vec<i64> {
    Vec::new()
}
