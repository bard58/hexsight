//! Your own armor and magic resist, from the `activeplayer` read that already happens (F4).
//!
//! Owner: package B. F0 holds the final signatures as stubs that change nothing: no stats, and a
//! factor of 1.0, so the resist rules give their full points.

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
/// `.maxHealth`. `None` without `championStats`. A missing field is `None` in the result.
///
/// F0 stub: always `None`. Package B fills it in and stores the result in `Live.stats`.
#[allow(dead_code)] // F0 stub: package B calls it from worker.rs.
pub fn parse_my_stats(_activeplayer: &Value) -> Option<MyStats> {
    None
}

/// The share of a resist bonus that is left when you already have `x` of that resist:
/// `x.map_or(1.0, |x| ((160.0 - x) / 100.0).clamp(0.0, 1.0))`. 60 or less gives 1.0, 110 gives 0.5,
/// and 160 or more gives 0.0. `rank` takes `min(shrink(resist, n), round(resist * factor))`, and a
/// bonus of 0 gives no hit and no inject for that resist.
///
/// F0 stub: always 1.0, so the points are the same as before.
pub fn factor(_x: Option<f32>) -> f32 {
    1.0
}
