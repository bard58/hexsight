//! Numbers for the "My games" page: period and filters, the profile summary, the form line, and
//! the augment and champion counts. Pure functions over the saved games (newest first), unit
//! tested below. The page code only draws what these functions return.
//!
//! Policy: the augment counts hold picks and wins only. There is no rate, no sort by wins, and
//! no advice. Small samples prove little.

use crate::history::{self, Game};
use std::cmp::Reverse;
use std::collections::HashMap;

/// A champion needs this many games to show under "Most played" or "Most wins".
pub const MIN_CHAMPION_GAMES: u32 = 3;
/// An augment row with fewer picks than this shows dimmed.
pub const DIM_BELOW_PICKS: u32 = 5;
/// The form line looks at this many games at most.
pub const FORM_GAMES: usize = 50;
/// Each point of the form line is the win rate of this many games.
pub const FORM_WINDOW: usize = 10;
/// The form line needs this many games to show.
pub const MIN_FORM_GAMES: usize = 10;
/// The streak shows from this length.
pub const MIN_STREAK: u32 = 3;
/// The profile header shows this many last results.
pub const LAST_RESULTS: usize = 10;

/// The time span that drives every number on the page.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum Period {
    /// The local day of now.
    Today,
    /// The last 7 local days, today included.
    Days7,
    /// The last 30 local days, today included.
    Days30,
    #[default]
    All,
}

impl Period {
    /// Number of local days, or `None` for all time.
    fn days(self) -> Option<i64> {
        match self {
            Period::Today => Some(1),
            Period::Days7 => Some(7),
            Period::Days30 => Some(30),
            Period::All => None,
        }
    }
}

/// Filters from a tap on a champion or an augment. Both can be on at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct Filter {
    pub champion: Option<i64>,
    pub augment: Option<i64>,
}

impl Filter {
    fn matches(&self, g: &Game) -> bool {
        self.champion.is_none_or(|c| g.champion == c)
            && self.augment.is_none_or(|a| g.augments.contains(&a))
    }
}

/// The indexes (into `games`, so in the same newest first order) of the games that pass the
/// period and the filter. `now` is the Unix time in seconds and `zone` gives the local zone's
/// offset from UTC in seconds at a UTC instant (a parameter, so tests are deterministic). Each
/// game uses the offset of its own instant, like the date in the game list, so a clock change
/// does not move a game to another day than the list shows. A period
/// starts at local midnight. A game with no readable date shows under `All` only. A game that
/// lies after `now` counts as today.
///
/// For the Champions tab pass a filter with the augment only, and for the Augments tab a filter
/// with the champion only, so each list stays whole for its own kind.
pub fn select(
    games: &[Game],
    period: Period,
    filter: Filter,
    now: i64,
    zone: &dyn Fn(i64) -> i64,
) -> Vec<usize> {
    let first_day = period
        .days()
        .map(|n| (now + zone(now)).div_euclid(86_400) - n + 1);
    games
        .iter()
        .enumerate()
        .filter(|(_, g)| {
            let in_period = match first_day {
                None => true,
                Some(first) => history::parse_iso(&g.date)
                    .is_some_and(|at| (at + zone(at)).div_euclid(86_400) >= first),
            };
            in_period && filter.matches(g)
        })
        .map(|(i, _)| i)
        .collect()
}

/// The games at these indexes, in the same order.
pub fn pick<'a>(games: &'a [Game], idx: &[usize]) -> Vec<&'a Game> {
    idx.iter().filter_map(|&i| games.get(i)).collect()
}

/// The profile header in numbers.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Profile {
    pub games: u32,
    pub wins: u32,
    pub losses: u32,
    /// 0 to 1. Zero without games.
    pub win_rate: f32,
    /// The current run of equal results: positive for wins, negative for losses, 0 without games.
    /// Use `shown_streak` to apply the minimum length.
    pub streak: i32,
    pub avg_kills: f32,
    pub avg_deaths: f32,
    pub avg_assists: f32,
    /// (kills + assists) / max(deaths, 1) over all games.
    pub kda: f32,
    /// Average length in seconds over the games that have a length. `None` when none has.
    pub avg_secs: Option<u32>,
    /// The newest results first (`true` = win), up to `LAST_RESULTS`.
    pub last: Vec<bool>,
}

impl Profile {
    /// The streak length and `true` for a win streak, when it is long enough to show.
    pub fn shown_streak(&self) -> Option<(u32, bool)> {
        let len = self.streak.unsigned_abs();
        (len >= MIN_STREAK).then_some((len, self.streak > 0))
    }
}

/// The profile of these games (newest first).
pub fn profile(games: &[&Game]) -> Profile {
    let n = games.len() as u32;
    if n == 0 {
        return Profile::default();
    }
    let wins = games.iter().filter(|g| g.win).count() as u32;
    let (k, d, a) = games.iter().fold((0u64, 0u64, 0u64), |t, g| {
        (
            t.0 + u64::from(g.kills),
            t.1 + u64::from(g.deaths),
            t.2 + u64::from(g.assists),
        )
    });
    let per = |sum: u64| sum as f32 / n as f32;
    let first_win = games[0].win;
    let run = games.iter().take_while(|g| g.win == first_win).count() as i32;
    let timed: Vec<u64> = games
        .iter()
        .filter(|g| g.secs > 0)
        .map(|g| u64::from(g.secs))
        .collect();
    let avg_secs = if timed.is_empty() {
        None
    } else {
        let total: u64 = timed.iter().sum();
        Some((total as f64 / timed.len() as f64).round() as u32)
    };
    Profile {
        games: n,
        wins,
        losses: n - wins,
        win_rate: wins as f32 / n as f32,
        streak: if first_win { run } else { -run },
        avg_kills: per(k),
        avg_deaths: per(d),
        avg_assists: per(a),
        kda: (k + a) as f32 / d.max(1) as f32,
        avg_secs,
        last: games.iter().take(LAST_RESULTS).map(|g| g.win).collect(),
    }
}

/// The form line: the rolling win rate (0 to 1) over the newest `max_games` games, oldest point
/// first. Point `i` is the win rate of the `window` games that end at game `i` (fewer at the
/// start). Empty when there are fewer than `MIN_FORM_GAMES` games, or when `window` is 0.
pub fn form_series(games: &[&Game], max_games: usize, window: usize) -> Vec<f32> {
    let take = games.len().min(max_games);
    if take < MIN_FORM_GAMES || window == 0 {
        return Vec::new();
    }
    // Oldest first.
    let wins: Vec<bool> = games[..take].iter().rev().map(|g| g.win).collect();
    (0..take)
        .map(|i| {
            let from = (i + 1).saturating_sub(window);
            let slice = &wins[from..=i];
            slice.iter().filter(|&&w| w).count() as f32 / slice.len() as f32
        })
        .collect()
}

/// Picks and wins of one of your augments.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AugmentCount {
    pub id: i64,
    /// Games where you took this augment (once per game).
    pub picks: u32,
    /// Those games that you won.
    pub wins: u32,
    /// Position of the newest such game in the input (0 = newest).
    pub last: usize,
}

impl AugmentCount {
    /// Rows with few picks show dimmed.
    pub fn dim(&self) -> bool {
        self.picks < DIM_BELOW_PICKS
    }
}

/// Counts per augment: most picks first, then the most recent use, then the lowest id.
pub fn augment_counts(games: &[&Game]) -> Vec<AugmentCount> {
    let mut map: HashMap<i64, AugmentCount> = HashMap::new();
    for (pos, g) in games.iter().enumerate() {
        let mut seen: Vec<i64> = Vec::with_capacity(g.augments.len());
        for &id in &g.augments {
            if seen.contains(&id) {
                continue;
            }
            seen.push(id);
            let c = map.entry(id).or_insert(AugmentCount {
                id,
                picks: 0,
                wins: 0,
                last: pos,
            });
            c.picks += 1;
            c.wins += u32::from(g.win);
            c.last = c.last.min(pos);
        }
    }
    let mut v: Vec<AugmentCount> = map.into_values().collect();
    v.sort_by(|a, b| {
        b.picks
            .cmp(&a.picks)
            .then(a.last.cmp(&b.last))
            .then(a.id.cmp(&b.id))
    });
    v
}

/// Games and wins of one champion.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ChampionCount {
    pub id: i64,
    pub games: u32,
    pub wins: u32,
    /// Position of the newest game in the input (0 = newest).
    pub last: usize,
}

/// Counts per champion: most games first, then most wins, then the most recent, then the lowest id.
pub fn champion_counts(games: &[&Game]) -> Vec<ChampionCount> {
    let mut map: HashMap<i64, ChampionCount> = HashMap::new();
    for (pos, g) in games.iter().enumerate() {
        let c = map.entry(g.champion).or_insert(ChampionCount {
            id: g.champion,
            games: 0,
            wins: 0,
            last: pos,
        });
        c.games += 1;
        c.wins += u32::from(g.win);
        c.last = c.last.min(pos);
    }
    let mut v: Vec<ChampionCount> = map.into_values().collect();
    v.sort_by(|a, b| {
        b.games
            .cmp(&a.games)
            .then(b.wins.cmp(&a.wins))
            .then(a.last.cmp(&b.last))
            .then(a.id.cmp(&b.id))
    });
    v
}

/// The two champion chips. Both only look at champions with `MIN_CHAMPION_GAMES` games or more.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Highlights {
    /// The most games. Ties go to more wins, then the more recent champion.
    pub most_played: Option<ChampionCount>,
    /// The most wins (at least one). Ties go to more games, then the more recent champion.
    pub most_wins: Option<ChampionCount>,
}

pub fn highlights(counts: &[ChampionCount]) -> Highlights {
    let eligible = || counts.iter().filter(|c| c.games >= MIN_CHAMPION_GAMES);
    let most_played = eligible()
        .min_by_key(|c| (Reverse(c.games), Reverse(c.wins), c.last, c.id))
        .copied();
    let most_wins = eligible()
        .filter(|c| c.wins > 0)
        .min_by_key(|c| (Reverse(c.wins), Reverse(c.games), c.last, c.id))
        .copied();
    Highlights {
        most_played,
        most_wins,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DAY: i64 = 86_400;

    fn at(day: i64, secs_in_day: i64) -> String {
        history::iso_date((day * DAY + secs_in_day) * 1000)
    }

    fn game(id: i64, date: &str, champion: i64, augments: &[i64], win: bool) -> Game {
        Game {
            id,
            date: date.to_string(),
            queue: 2400,
            champion,
            augments: augments.to_vec(),
            win,
            kills: 0,
            deaths: 0,
            assists: 0,
            secs: 0,
        }
    }

    fn kda(mut g: Game, k: u32, d: u32, a: u32, secs: u32) -> Game {
        g.kills = k;
        g.deaths = d;
        g.assists = a;
        g.secs = secs;
        g
    }

    fn refs(v: &[Game]) -> Vec<&Game> {
        v.iter().collect()
    }

    /// 2026-10-23 as a day number.
    fn today() -> i64 {
        history::days_from_civil(2026, 10, 23)
    }

    #[test]
    fn empty_list_gives_empty_numbers() {
        let none: Vec<&Game> = Vec::new();
        assert_eq!(profile(&none), Profile::default());
        assert!(form_series(&none, FORM_GAMES, FORM_WINDOW).is_empty());
        assert!(augment_counts(&none).is_empty());
        assert!(champion_counts(&none).is_empty());
        assert_eq!(highlights(&[]), Highlights::default());
        assert!(select(&[], Period::Today, Filter::default(), 0, &|_| 0).is_empty());
        assert_eq!(Profile::default().shown_streak(), None);
    }

    #[test]
    fn all_period_keeps_every_game_even_without_a_date() {
        let games = vec![
            game(2, &at(today(), 100), 1, &[], true),
            game(1, "", 1, &[], false),
        ];
        let now = today() * DAY + 1000;
        assert_eq!(
            select(&games, Period::All, Filter::default(), now, &|_| 0),
            vec![0, 1]
        );
        // A game with no date is in no timed period.
        assert_eq!(
            select(&games, Period::Days30, Filter::default(), now, &|_| 0),
            vec![0]
        );
    }

    #[test]
    fn today_starts_at_local_midnight() {
        let d = today();
        // Zone UTC+3: local midnight is 21:00 UTC of the day before.
        let off = 3 * 3600;
        let now = d * DAY + 12 * 3600;
        let games = vec![
            game(4, &at(d, 12 * 3600), 1, &[], true),
            game(3, &at(d - 1, 21 * 3600), 1, &[], true), // exactly local midnight: today
            game(2, &at(d - 1, 21 * 3600 - 1), 1, &[], true), // one second before: yesterday
            game(1, &at(d - 5, 0), 1, &[], true),
        ];
        assert_eq!(
            select(&games, Period::Today, Filter::default(), now, &|_| off),
            vec![0, 1]
        );
        // In UTC the second game is yesterday.
        assert_eq!(
            select(&games, Period::Today, Filter::default(), now, &|_| 0),
            vec![0]
        );
    }

    #[test]
    fn each_game_uses_the_offset_of_its_own_instant() {
        let d = today();
        let change = d * DAY - 1800;
        // The offset is 0 before the clock change and three hours after it.
        let zone = move |at: i64| if at < change { 0 } else { 3 * 3600 };
        let now = d * DAY + 12 * 3600;
        let games = vec![
            game(2, &at(d, 60), 1, &[], true),
            // 23:00 local yesterday, as the game list shows it. The offset of now would
            // make it 02:00 today.
            game(1, &at(d - 1, DAY - 3600), 1, &[], true),
        ];
        assert_eq!(
            select(&games, Period::Today, Filter::default(), now, &zone),
            vec![0]
        );
    }

    #[test]
    fn negative_offset_moves_the_boundary_the_other_way() {
        let d = today();
        let off = -5 * 3600;
        // Now is 02:00 UTC, which is 21:00 of the day before in the zone.
        let now = d * DAY + 2 * 3600;
        let games = vec![
            game(3, &at(d, 60), 1, &[], true),
            game(2, &at(d - 1, 5 * 3600), 1, &[], true), // local midnight: today
            game(1, &at(d - 1, 5 * 3600 - 1), 1, &[], true), // one second before
        ];
        assert_eq!(
            select(&games, Period::Today, Filter::default(), now, &|_| off),
            vec![0, 1]
        );
    }

    #[test]
    fn seven_and_thirty_day_edges() {
        let d = today();
        let now = d * DAY + 10 * 3600;
        let games = vec![
            game(5, &at(d, 0), 1, &[], true),
            game(4, &at(d - 6, 0), 1, &[], true), // 7th day: in
            game(3, &at(d - 7, 86_399), 1, &[], true), // 8th day: out of 7
            game(2, &at(d - 29, 0), 1, &[], true), // 30th day: in
            game(1, &at(d - 30, 86_399), 1, &[], true), // out of 30
        ];
        assert_eq!(
            select(&games, Period::Days7, Filter::default(), now, &|_| 0),
            vec![0, 1]
        );
        assert_eq!(
            select(&games, Period::Days30, Filter::default(), now, &|_| 0),
            vec![0, 1, 2, 3]
        );
    }

    #[test]
    fn future_game_counts_as_today() {
        let d = today();
        let now = d * DAY + 100;
        let games = vec![game(1, &at(d + 2, 0), 1, &[], true)];
        assert_eq!(
            select(&games, Period::Today, Filter::default(), now, &|_| 0),
            vec![0]
        );
    }

    #[test]
    fn filters_combine_with_period() {
        let d = today();
        let now = d * DAY + 100;
        let games = vec![
            game(5, &at(d, 10), 7, &[100, 200], true),
            game(4, &at(d, 5), 8, &[100], true),
            game(3, &at(d - 3, 0), 7, &[100], false),
            game(2, &at(d - 40, 0), 7, &[100, 200], true),
            game(1, &at(d - 40, 0), 7, &[300], true),
        ];
        let by_champ = Filter {
            champion: Some(7),
            augment: None,
        };
        let by_aug = Filter {
            champion: None,
            augment: Some(100),
        };
        let both = Filter {
            champion: Some(7),
            augment: Some(100),
        };
        assert_eq!(
            select(&games, Period::All, by_champ, now, &|_| 0),
            vec![0, 2, 3, 4]
        );
        assert_eq!(
            select(&games, Period::All, by_aug, now, &|_| 0),
            vec![0, 1, 2, 3]
        );
        assert_eq!(
            select(&games, Period::All, both, now, &|_| 0),
            vec![0, 2, 3]
        );
        assert_eq!(select(&games, Period::Days7, both, now, &|_| 0), vec![0, 2]);
        assert_eq!(select(&games, Period::Today, both, now, &|_| 0), vec![0]);
        let none = Filter {
            champion: Some(99),
            augment: None,
        };
        assert!(select(&games, Period::All, none, now, &|_| 0).is_empty());
        let idx = select(&games, Period::All, both, now, &|_| 0);
        let ids: Vec<i64> = pick(&games, &idx).iter().map(|g| g.id).collect();
        assert_eq!(ids, vec![5, 3, 2]);
    }

    #[test]
    fn profile_numbers() {
        let games = vec![
            kda(game(3, "", 1, &[], true), 10, 2, 5, 1200),
            kda(game(2, "", 1, &[], false), 2, 8, 4, 0), // no length: skipped
            kda(game(1, "", 1, &[], true), 6, 0, 3, 1800),
        ];
        let p = profile(&refs(&games));
        assert_eq!((p.games, p.wins, p.losses), (3, 2, 1));
        assert!((p.win_rate - 2.0 / 3.0).abs() < 1e-6);
        assert!((p.avg_kills - 6.0).abs() < 1e-6);
        assert!((p.avg_deaths - 10.0 / 3.0).abs() < 1e-6);
        assert!((p.avg_assists - 4.0).abs() < 1e-6);
        assert!((p.kda - 3.0).abs() < 1e-6);
        assert_eq!(p.avg_secs, Some(1500));
        assert_eq!(p.last, vec![true, false, true]);
        assert_eq!(p.streak, 1);
        assert_eq!(p.shown_streak(), None);
    }

    #[test]
    fn kda_with_no_deaths_does_not_divide_by_zero() {
        let games = vec![kda(game(1, "", 1, &[], true), 4, 0, 6, 900)];
        assert!((profile(&refs(&games)).kda - 10.0).abs() < 1e-6);
    }

    #[test]
    fn length_is_none_when_no_game_has_one() {
        let games = vec![game(1, "", 1, &[], true), game(2, "", 1, &[], false)];
        assert_eq!(profile(&refs(&games)).avg_secs, None);
    }

    #[test]
    fn streak_signs_and_threshold() {
        let wins: Vec<Game> = (0..4).map(|i| game(i, "", 1, &[], true)).collect();
        let p = profile(&refs(&wins));
        assert_eq!(p.streak, 4);
        assert_eq!(p.shown_streak(), Some((4, true)));

        let mut mixed = vec![
            game(9, "", 1, &[], false),
            game(8, "", 1, &[], false),
            game(7, "", 1, &[], false),
            game(6, "", 1, &[], true),
        ];
        let p = profile(&refs(&mixed));
        assert_eq!(p.streak, -3);
        assert_eq!(p.shown_streak(), Some((3, false)));

        mixed.remove(0);
        assert_eq!(profile(&refs(&mixed)).shown_streak(), None);
    }

    #[test]
    fn last_results_cap_at_ten_newest_first() {
        let games: Vec<Game> = (0..15).map(|i| game(i, "", 1, &[], i < 3)).collect();
        let p = profile(&refs(&games));
        assert_eq!(p.last.len(), LAST_RESULTS);
        assert_eq!(&p.last[..4], &[true, true, true, false]);
    }

    #[test]
    fn form_needs_enough_games() {
        let games: Vec<Game> = (0..9).map(|i| game(i, "", 1, &[], true)).collect();
        assert!(form_series(&refs(&games), FORM_GAMES, FORM_WINDOW).is_empty());
        let games: Vec<Game> = (0..10).map(|i| game(i, "", 1, &[], true)).collect();
        let s = form_series(&refs(&games), FORM_GAMES, FORM_WINDOW);
        assert_eq!(s.len(), 10);
        assert!(s.iter().all(|&v| v == 1.0));
        assert!(form_series(&refs(&games), FORM_GAMES, 0).is_empty());
    }

    #[test]
    fn form_is_oldest_first_and_rolling() {
        // Newest first: the 5 newest are wins, the 5 oldest are losses.
        let games: Vec<Game> = (0..10).map(|i| game(i, "", 1, &[], i < 5)).collect();
        let s = form_series(&refs(&games), FORM_GAMES, 4);
        assert_eq!(s.len(), 10);
        assert_eq!(s[0], 0.0);
        // Oldest first: L L L L L W W W W W. Point 5 covers L L L W (index 2..=5).
        assert_eq!(s[5], 0.25);
        assert_eq!(s[9], 1.0);
        assert!(s.windows(2).all(|w| w[1] >= w[0]));
    }

    #[test]
    fn form_caps_at_max_games() {
        let games: Vec<Game> = (0..80).map(|i| game(i, "", 1, &[], true)).collect();
        assert_eq!(
            form_series(&refs(&games), FORM_GAMES, FORM_WINDOW).len(),
            50
        );
    }

    #[test]
    fn augment_counts_sort_by_picks_then_recency() {
        let games = vec![
            game(5, "", 1, &[10, 20], true),
            game(4, "", 1, &[30], false),
            game(3, "", 1, &[20, 40], true),
            game(2, "", 1, &[10, 40], false),
            game(1, "", 1, &[50], true),
        ];
        let c = augment_counts(&refs(&games));
        let ids: Vec<i64> = c.iter().map(|x| x.id).collect();
        // 10, 20 and 40 have 2 picks (last use 0, 0, 2). Then 30 and 50 with 1 pick (1, 4).
        assert_eq!(ids, vec![10, 20, 40, 30, 50]);
        assert_eq!((c[0].picks, c[0].wins), (2, 1));
        assert_eq!((c[1].picks, c[1].wins), (2, 2));
        assert_eq!(c[2].last, 2);
        assert!(c.iter().all(AugmentCount::dim));
    }

    #[test]
    fn augment_counts_count_a_repeat_once_and_dim_below_five() {
        let mut games: Vec<Game> = (0..5)
            .map(|i| game(i, "", 1, &[7, 7], i % 2 == 0))
            .collect();
        let c = augment_counts(&refs(&games));
        assert_eq!(c.len(), 1);
        assert_eq!((c[0].picks, c[0].wins), (5, 3));
        assert!(!c[0].dim());
        games.pop();
        assert!(augment_counts(&refs(&games))[0].dim());
    }

    #[test]
    fn augment_ties_end_on_id() {
        let games = vec![game(1, "", 1, &[9, 3], true)];
        let ids: Vec<i64> = augment_counts(&refs(&games)).iter().map(|x| x.id).collect();
        assert_eq!(ids, vec![3, 9]);
    }

    #[test]
    fn champion_counts_and_highlights() {
        // Newest first. Champion 1: 4 games, 1 win. Champion 2: 3 games, 3 wins. Champion 3: 2 games.
        let plan = [
            (2, true),
            (1, false),
            (3, true),
            (2, true),
            (1, true),
            (2, true),
            (1, false),
            (3, false),
            (1, false),
        ];
        let games: Vec<Game> = plan
            .iter()
            .enumerate()
            .map(|(i, (c, w))| game(100 - i as i64, "", *c, &[], *w))
            .collect();
        let counts = champion_counts(&refs(&games));
        assert_eq!(counts[0].id, 1);
        assert_eq!((counts[0].games, counts[0].wins), (4, 1));
        assert_eq!(counts[1].id, 2);
        let h = highlights(&counts);
        assert_eq!(h.most_played.unwrap().id, 1);
        assert_eq!(h.most_wins.unwrap().id, 2);
    }

    #[test]
    fn highlights_need_three_games_and_one_win() {
        let games = vec![
            game(2, "", 1, &[], true),
            game(1, "", 1, &[], true),
            game(0, "", 2, &[], true),
        ];
        assert_eq!(
            highlights(&champion_counts(&refs(&games))),
            Highlights::default()
        );

        let games: Vec<Game> = (0..3).map(|i| game(i, "", 5, &[], false)).collect();
        let h = highlights(&champion_counts(&refs(&games)));
        assert_eq!(h.most_played.unwrap().id, 5);
        assert!(h.most_wins.is_none());
    }

    #[test]
    fn highlight_ties_prefer_wins_then_recency() {
        // Champions 1 and 2 both have 3 games. Champion 2 has more wins.
        let games = vec![
            game(6, "", 1, &[], false),
            game(5, "", 1, &[], true),
            game(4, "", 1, &[], false),
            game(3, "", 2, &[], true),
            game(2, "", 2, &[], true),
            game(1, "", 2, &[], false),
        ];
        let h = highlights(&champion_counts(&refs(&games)));
        assert_eq!(h.most_played.unwrap().id, 2);
        // Equal games and wins: the more recent champion wins.
        let games = vec![
            game(6, "", 1, &[], true),
            game(5, "", 1, &[], false),
            game(4, "", 1, &[], false),
            game(3, "", 2, &[], true),
            game(2, "", 2, &[], false),
            game(1, "", 2, &[], false),
        ];
        let h = highlights(&champion_counts(&refs(&games)));
        assert_eq!(h.most_played.unwrap().id, 1);
        assert_eq!(h.most_wins.unwrap().id, 1);
    }

    #[test]
    fn short_real_game_counts() {
        // Remakes never reach the file (`history::is_remake`), so a 301 second game is a game.
        let games = vec![kda(game(1, "", 1, &[], true), 1, 1, 1, 301)];
        assert_eq!(profile(&refs(&games)).games, 1);
    }
}
