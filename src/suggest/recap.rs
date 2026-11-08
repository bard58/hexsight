//! The death recap and the nemesis: "Zed killed you 3 times" (F5).
//!
//! The source is the kill deltas of `playerlist` only. Names never enter this module: the roster
//! holds none.
//!
//! Owner: package B (the engine). F0 holds the final types and signatures as stubs that find
//! nothing, so no recap and no killer reach `Input` yet.

use super::Roster;
use std::time::Instant;

/// The enemies that killed you in your last death: their champion ids, sorted by id. Empty is no
/// recap: `death_recap` then gives `None`.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Recap {
    pub killers: Vec<i64>,
}

/// The enemy that the killer rule (`Reason::VsKiller`) is about, and how often it killed you.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Killer {
    pub champion: i64,
    pub times: u8,
}

/// The recap of a death between two rosters. All must hold:
/// - you are alive in `before` and dead in `now`;
/// - `now.me().deaths == before.me().deaths + 1`;
/// - at least one enemy has more kills in `now` than in `before`.
///
/// The killers are the enemies whose kills rose, sorted by id.
///
/// F0 stub: always `None`.
#[allow(dead_code)] // F0 stub: package B calls it from `Tracker::observe`.
pub fn death_recap(_before: &Roster, _now: &Roster) -> Option<Recap> {
    None
}

/// The recap state of one game. The worker keeps one in `Shared`, calls `observe` on each new
/// roster, and makes a new one in `end_game`.
#[derive(Clone, Debug, Default)]
#[allow(dead_code)] // F0 stub: package B fills and reads the fields.
pub struct Tracker {
    /// The last roster with you alive, and when it came.
    last_alive: Option<(Roster, Instant)>,
    /// How often each enemy was the only killer of a death of yours: (champion, count).
    killed_by: Vec<(i64, u8)>,
    /// The recap of your current death. `None` while you are alive.
    current: Option<Recap>,
}

#[allow(dead_code)] // F0 stub: package B wires it in worker.rs.
impl Tracker {
    /// Take a new roster.
    /// - With you alive: it replaces `last_alive` and clears `current`.
    /// - On the edge to dead: `current = death_recap(last_alive, r)`, but no recap when
    ///   `last_alive` is older than 6 s. Exactly one killer adds 1 to its `killed_by` count.
    ///
    /// F0 stub: does nothing.
    pub fn observe(&mut self, _r: &Roster, _now: Instant) {}

    /// The recap of your current death, for `Input::recap`. `None` while you are alive.
    pub fn recap(&self) -> Option<&Recap> {
        self.current.as_ref()
    }

    /// The killer for the rule, for `Input::killer`:
    /// - while dead with exactly one killer: that champion, with `times` = its `killed_by` count;
    /// - otherwise the nemesis: the enemy with a count of 2 or more and `count * 2 >= me.deaths`.
    ///
    /// F0 stub: always `None`.
    pub fn killer(&self) -> Option<Killer> {
        None
    }
}
