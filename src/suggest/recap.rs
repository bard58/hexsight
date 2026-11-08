//! The death recap and the nemesis: "Zed killed you 3 times" (F5).
//!
//! The source is the kill deltas of `playerlist` only. Names never enter this module: the roster
//! holds none.
//!
//! Owner: package B (the engine). The worker keeps one `Tracker` in `Shared`, calls `observe` on
//! each new roster, and passes `killer` and `recap` into `Input`.

use super::Roster;
use std::time::{Duration, Instant};

/// An alive roster older than this gives no recap: the kills between the two reads can belong to
/// another fight.
const FRESH: Duration = Duration::from_secs(6);

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
/// The killers are the enemies whose kills rose, sorted by id. An enemy counts only with a known
/// champion that is in both rosters.
pub fn death_recap(before: &Roster, now: &Roster) -> Option<Recap> {
    let (was, is) = (before.me(), now.me());
    if was.dead || !is.dead || is.deaths != was.deaths.saturating_add(1) {
        return None;
    }
    let mut killers: Vec<i64> = now
        .enemies()
        .filter(|e| e.champion > 0)
        .filter(|e| {
            before
                .enemies()
                .find(|b| b.champion == e.champion)
                .is_some_and(|b| e.kills > b.kills)
        })
        .map(|e| e.champion)
        .collect();
    killers.sort_unstable();
    killers.dedup();
    (!killers.is_empty()).then_some(Recap { killers })
}

/// The recap state of one game. The worker keeps one in `Shared`, calls `observe` on each new
/// roster, and makes a new one in `end_game`.
#[derive(Clone, Debug, Default)]
pub struct Tracker {
    /// The last roster with you alive, and when it came. The first dead roster after it takes it,
    /// so one death is counted once.
    last_alive: Option<(Roster, Instant)>,
    /// How often each enemy was the only killer of a death of yours: (champion, count).
    killed_by: Vec<(i64, u8)>,
    /// The recap of your current death. `None` while you are alive.
    current: Option<Recap>,
    /// Your deaths in the last roster.
    deaths: u16,
}

impl Tracker {
    /// Take a new roster.
    /// - With you alive: it replaces `last_alive` and clears `current`.
    /// - On the edge to dead: `current = death_recap(last_alive, r)`, but no recap when
    ///   `last_alive` is older than 6 s. Exactly one killer adds 1 to its `killed_by` count.
    pub fn observe(&mut self, r: &Roster, now: Instant) {
        self.deaths = r.me().deaths;
        if !r.me().dead {
            self.last_alive = Some((r.clone(), now));
            self.current = None;
            return;
        }
        // Dead: only the first dead roster after an alive one is the edge.
        let Some((before, at)) = self.last_alive.take() else {
            return;
        };
        if now.saturating_duration_since(at) > FRESH {
            return;
        }
        self.current = death_recap(&before, r);
        if let Some([one]) = self.current.as_ref().map(|c| c.killers.as_slice()) {
            match self.killed_by.iter_mut().find(|(c, _)| c == one) {
                Some((_, n)) => *n = n.saturating_add(1),
                None => self.killed_by.push((*one, 1)),
            }
        }
    }

    /// The recap of your current death, for `Input::recap`. `None` while you are alive.
    pub fn recap(&self) -> Option<&Recap> {
        self.current.as_ref()
    }

    /// The killer for the rule, for `Input::killer`:
    /// - while dead with exactly one killer: that champion, with `times` = its `killed_by` count;
    /// - otherwise the nemesis: the enemy with a count of 2 or more and `count * 2 >= me.deaths`
    ///   (the highest count, then the lowest id).
    pub fn killer(&self) -> Option<Killer> {
        if let Some([one]) = self.current.as_ref().map(|c| c.killers.as_slice()) {
            return Some(Killer {
                champion: *one,
                times: self.count(*one).max(1),
            });
        }
        self.killed_by
            .iter()
            .filter(|&&(_, n)| n >= 2 && u16::from(n) * 2 >= self.deaths)
            .max_by_key(|&&(c, n)| (n, std::cmp::Reverse(c)))
            .map(|&(champion, times)| Killer { champion, times })
    }

    fn count(&self, champion: i64) -> u8 {
        self.killed_by
            .iter()
            .find(|(c, _)| *c == champion)
            .map_or(0, |&(_, n)| n)
    }
}

#[cfg(test)]
mod tests {
    use super::super::parse_players;
    use super::super::testkit::*;
    use super::*;
    use serde_json::Value;

    const CAITLYN: i64 = 51;
    const AMUMU: i64 = 32;
    const ZED: i64 = 4;

    /// The real player list, you are Player1 (Ashe, alive).
    fn real() -> Roster {
        let list: Value = serde_json::from_str(PLAYERLIST_REAL).unwrap();
        let r = parse_players(&list, "Player1", &sd().champions).unwrap();
        assert!(!r.me().dead);
        r
    }

    fn kill(r: &Roster, champion: i64) -> Roster {
        let mut r = r.clone();
        for p in r.players.iter_mut().filter(|p| p.champion == champion) {
            p.kills += 1;
        }
        r
    }

    /// You die once: dead, one more death.
    fn die(r: &Roster) -> Roster {
        let mut r = r.clone();
        let me = r.me;
        r.players[me].dead = true;
        r.players[me].deaths += 1;
        r
    }

    fn respawn(r: &Roster) -> Roster {
        let mut r = r.clone();
        let me = r.me;
        r.players[me].dead = false;
        r
    }

    #[test]
    fn one_enemy_kill_rise_names_the_killer() {
        let before = real();
        let now = die(&kill(&before, CAITLYN));
        assert_eq!(
            death_recap(&before, &now),
            Some(Recap {
                killers: vec![CAITLYN]
            })
        );
        let mut t = Tracker::default();
        let t0 = Instant::now();
        t.observe(&before, t0);
        t.observe(&now, t0 + Duration::from_secs(1));
        assert_eq!(t.recap().map(|r| r.killers.clone()), Some(vec![CAITLYN]));
        assert_eq!(
            t.killer(),
            Some(Killer {
                champion: CAITLYN,
                times: 1
            })
        );
        // The next dead rosters keep the recap and count nothing more.
        t.observe(&now, t0 + Duration::from_secs(2));
        assert_eq!(t.killer().map(|k| k.times), Some(1));
    }

    #[test]
    fn two_risers_give_no_named_killer() {
        let before = real();
        let now = die(&kill(&kill(&before, CAITLYN), AMUMU));
        assert_eq!(
            death_recap(&before, &now),
            Some(Recap {
                killers: vec![AMUMU, CAITLYN]
            })
        );
        let mut t = Tracker::default();
        let t0 = Instant::now();
        t.observe(&before, t0);
        t.observe(&now, t0 + Duration::from_secs(1));
        assert_eq!(t.recap().map(|r| r.killers.len()), Some(2));
        assert_eq!(t.killer(), None);
    }

    #[test]
    fn no_kill_rise_gives_none() {
        let before = real();
        let now = die(&before);
        assert_eq!(death_recap(&before, &now), None);
        // An ally kill rise is no killer either.
        let mut ally = die(&before);
        ally.players[1].kills += 1;
        assert_eq!(death_recap(&before, &ally), None);
        // Two deaths at once (a missed read) give none.
        let twice = die(&die(&kill(&before, CAITLYN)));
        assert_eq!(death_recap(&before, &twice), None);
        // Dead with no new death gives none.
        let mut stale = kill(&before, CAITLYN);
        let me = stale.me;
        stale.players[me].dead = true;
        assert_eq!(death_recap(&before, &stale), None);
    }

    #[test]
    fn dead_in_both_snapshots_gives_none() {
        let dead = die(&real());
        let now = die(&kill(&dead, CAITLYN));
        assert_eq!(death_recap(&dead, &now), None);
        let mut t = Tracker::default();
        let t0 = Instant::now();
        t.observe(&dead, t0);
        t.observe(&now, t0 + Duration::from_secs(1));
        assert_eq!((t.recap(), t.killer()), (None, None));
    }

    #[test]
    fn a_stale_alive_snapshot_gives_none() {
        let before = real();
        let now = die(&kill(&before, CAITLYN));
        let mut t = Tracker::default();
        let t0 = Instant::now();
        t.observe(&before, t0);
        t.observe(&now, t0 + Duration::from_secs(7));
        assert_eq!((t.recap(), t.killer()), (None, None));
        // At 6 s it still counts.
        let mut t = Tracker::default();
        t.observe(&before, t0);
        t.observe(&now, t0 + Duration::from_secs(6));
        assert!(t.recap().is_some());
    }

    #[test]
    fn respawn_clears_current() {
        let before = real();
        let now = die(&kill(&before, CAITLYN));
        let mut t = Tracker::default();
        let t0 = Instant::now();
        t.observe(&before, t0);
        t.observe(&now, t0 + Duration::from_secs(1));
        assert!(t.recap().is_some());
        t.observe(&respawn(&now), t0 + Duration::from_secs(20));
        assert_eq!(t.recap(), None);
        // One kill by Caitlyn of five deaths is no nemesis.
        assert_eq!(t.killer(), None);
    }

    /// A roster of you and Zed, Amumu and Caitlyn, with you alive and no death.
    fn duel() -> Roster {
        roster(vec![
            enemy(ZED, &[]),
            enemy(AMUMU, &[]),
            enemy(CAITLYN, &[]),
        ])
    }

    /// Die `n` times to these killers, one death each, with a respawn after each.
    fn deaths(t: &mut Tracker, start: &Roster, killers: &[i64]) -> Roster {
        let mut r = start.clone();
        let mut at = Instant::now();
        t.observe(&r, at);
        for &k in killers {
            at += Duration::from_secs(2);
            r = die(&kill(&r, k));
            t.observe(&r, at);
            at += Duration::from_secs(2);
            r = respawn(&r);
            t.observe(&r, at);
        }
        r
    }

    #[test]
    fn three_deaths_two_by_zed_make_zed_the_nemesis() {
        let mut t = Tracker::default();
        let r = deaths(&mut t, &duel(), &[ZED, CAITLYN, ZED]);
        assert_eq!(r.me().deaths, 3);
        assert_eq!(t.recap(), None);
        assert_eq!(
            t.killer(),
            Some(Killer {
                champion: ZED,
                times: 2
            })
        );
        // While dead to Caitlyn, she is the killer of the rule, with her own count.
        let at = Instant::now() + Duration::from_secs(60);
        t.observe(&r, at);
        t.observe(&die(&kill(&r, CAITLYN)), at + Duration::from_secs(1));
        assert_eq!(
            t.killer(),
            Some(Killer {
                champion: CAITLYN,
                times: 2
            })
        );
    }

    #[test]
    fn two_of_six_deaths_is_no_nemesis() {
        let mut t = Tracker::default();
        let mut start = duel();
        let me = start.me;
        // Four deaths that no read saw (for example under a turret).
        start.players[me].deaths = 4;
        deaths(&mut t, &start, &[ZED, ZED]);
        assert_eq!(t.killer(), None);
        // With two earlier deaths, 2 of 4 makes the nemesis.
        let mut t = Tracker::default();
        start.players[me].deaths = 2;
        deaths(&mut t, &start, &[ZED, ZED]);
        assert_eq!(t.killer().map(|k| (k.champion, k.times)), Some((ZED, 2)));
    }

    #[test]
    fn a_new_tracker_knows_nothing() {
        let t = Tracker::default();
        assert_eq!((t.recap(), t.killer()), (None, None));
    }
}
