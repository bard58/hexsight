//! Background threads. The UI thread never waits for the network or OCR.
//! - poll thread: League client state and live game state, about once a second.
//! - data thread: static data and the stats index (network, with retries).
//! - fetch thread: per-champion stats on demand.
//! - icon thread: icon downloads.
//! - ocr thread: reads the offered augment cards while a pick is due.
//! - key thread: global hotkeys.
//! - history thread: saves the games of the player (waits without work most of the time).

use crate::data::{self, LoadError, NoStats, Stats};
use crate::history::{self, Game};
use crate::i18n::{Key, Lang};
use crate::lcu::{self, ChampSelect, Lcu, LiveClient, Phase, TradeCall};
use crate::model::{ChampStats, StaticData, is_mayhem};
use crate::ocr::{
    self, CaptureHealth, CaptureWatch, Card, Desk, Geometry, Line, Matcher, Ocr, ScanRecord,
};
use crate::suggest::{self, Advice, Roster};
use crate::{cache, update, win};
use std::collections::{HashMap, HashSet, VecDeque};
use std::path::Path;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, TryRecvError, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// The levels of the pick screens in ARAM Mayhem. A screen that shows at
/// another level is still read: the heartbeat scan finds it.
pub const PICK_LEVELS: [i64; 4] = [3, 7, 11, 15];

/// A pick screen that OCR never saw: stop waiting for it after this time.
const WAIT_GIVE_UP: Duration = Duration::from_secs(45);

/// While picks are left, capture the screen this often even when no pick is
/// due. A pick screen at an unexpected level is then still seen.
///
/// This was 5 s while each heartbeat ran a full OCR read (about 200 ms on a
/// frame without cards). Now a heartbeat capture first runs the cheap
/// `ocr::cards_visible` check (about 5 us) and reads only when cards show, so
/// the average cost per second goes down even at this faster rate. With c ms
/// for a capture and 200 ms for a read: before, (c + 200) / 5 per second; now,
/// c / 1.5 + 200 / 30 (the safety read below). That is lower for any capture
/// under about 70 ms, and a 1080p capture takes a few ms. A late pick screen
/// is seen after 1.5 s at most, not 5 s.
const HEARTBEAT: Duration = Duration::from_millis(1500);

/// A heartbeat capture without cards is not read. But a full read still runs
/// at least this often, so a screen layout that the check does not know (a
/// window shape or a color filter that nobody tested) is still seen.
const FULL_READ_EVERY: Duration = Duration::from_secs(30);

/// A band signature (see `ocr::band_signature`) may stand in for an OCR read
/// for this long. Then one real read runs, so a change that the signature
/// missed (a glow that hides a small text change) shows after 10 s at most.
/// The real read costs about 90 ms: under 1 % of one core at this rate.
const SIG_REUSE_MAX: Duration = Duration::from_secs(10);

/// Pause after a scan while cards show. 600 ms is the old rate. After a scan
/// that the signature answered (the band did not change), the next scan costs
/// only a capture and a signature (about 5 ms, not 95 ms), so the pause halves
/// to 300 ms and a reroll shows 300 ms sooner. Numbers, with a 5 ms capture
/// and a 90 ms read: before, 95 / (600 + 95) = 13.7 % of one core while cards
/// show. After a read the pause stays 600 ms, so a band that changes all the
/// time (an animation) costs the same as before. A steady band costs
/// 5.3 / 305 = 1.7 % per scan, and 2.5 % over 10 s with the forced read.
const CARDS_PAUSE: Duration = Duration::from_millis(600);
const CARDS_PAUSE_STEADY: Duration = Duration::from_millis(300);

/// While the screen is locked there is nothing to read: look again this often.
const LOCKED_PAUSE: Duration = Duration::from_secs(1);
/// Most pick screens in one game. Four is normal. Headliners (patch 26.21,
/// for example K/DA) can add more, so the reader keeps a margin.
const MAX_PICKS: usize = 6;

/// The last scan (one image) stays in memory this long after a game, then
/// Hexsight frees it. The Settings page can save it until then.
const SCAN_KEEP: Duration = Duration::from_secs(10 * 60);

/// Pauses before each try to read a finished game from the match history. The
/// game can show up late, so the five tries spread over about two minutes.
const GAME_TRIES: [Duration; 5] = [
    Duration::from_secs(4),
    Duration::from_secs(10),
    Duration::from_secs(20),
    Duration::from_secs(30),
    Duration::from_secs(60),
];

/// A toast shows at least this long before the next message takes its place.
const TOAST_MIN: Duration = Duration::from_millis(1500);

/// At most this many messages wait for their turn.
const TOAST_QUEUE: usize = 3;

/// The first import tries again after this pause (the client may not be signed
/// in yet), at most `IMPORT_TRIES` times.
const IMPORT_PAUSE: Duration = Duration::from_secs(30);
const IMPORT_TRIES: usize = 10;

/// The first import takes this many Mayhem games.
const IMPORT_GAMES: usize = 20;

/// The session read in game stops after this many tries without a game id.
const GAME_ID_TRIES: u32 = 30;

/// The stats of a champion that did not load (`NoStats::Transient`) are tried
/// again after this pause. A missing champion waits for the next stats index.
const STATS_RETRY: Duration = Duration::from_secs(60);

/// With no request, the fetch thread looks at the own champion this often. Its
/// stats then load in every phase, also while the window draws nothing.
const FETCH_TICK: Duration = Duration::from_secs(5);

/// The player list (for the item suggestions) is read this often, and at once after a purchase.
const ROSTER_EVERY: Duration = Duration::from_secs(4);

/// A drop of this much gold between two passes is a purchase: the player list is read at once.
const PURCHASE_DROP: u32 = 250;

#[derive(Default, Clone)]
pub struct Live {
    pub level: i64,
    /// Your gold (`currentGold` of the Live Client). `None` when the answer lacks it.
    pub gold: Option<u32>,
}

impl Live {
    /// Pick screens due at this level (0..=4). 0 while the level is unknown
    /// and below the first pick level.
    pub fn picks_due(&self) -> usize {
        PICK_LEVELS.iter().filter(|&&l| self.level >= l).count()
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    /// Offered augments, left to right: the id and the place of the title line
    /// of the card (fractions of the captured area, 0 to 1).
    pub cards: Vec<Card>,
    /// Pick screen number in this game (1..=4).
    pub screen: usize,
    /// Where the scan read the cards, in screen pixels. The badges on the cards
    /// need it. `None` when the scan did not read the game window.
    pub geometry: Option<Geometry>,
}

impl Offer {
    /// The augment ids only. The place of a card can move by a pixel between scans.
    pub fn augment_ids(&self) -> Vec<i64> {
        self.cards.iter().map(|c| c.id).collect()
    }
}

/// Where Hexsight got the own champion: the champ select session, or the live game.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ChampSource {
    ChampSelect,
    LiveClient,
}

#[derive(Default)]
pub struct Shared {
    pub phase: Phase,
    pub queue: Option<i64>,
    pub champ_select: Option<ChampSelect>,
    /// Last known own champion (champ select, then in game).
    pub my_champion: Option<i64>,
    pub locale: String,
    /// The language of the interface. The window sets it. The messages that
    /// the worker threads write (`say`) use it.
    pub lang: Lang,
    pub static_data: Option<Arc<StaticData>>,
    pub stats: Option<Arc<Stats>>,
    pub details: HashMap<i64, Arc<ChampStats>>,
    /// Details from the stats index before the last refresh. The UI shows them
    /// until the new details of that champion arrive.
    pub old_details: HashMap<i64, Arc<ChampStats>>,
    /// `WindowMode` of League (`game.cfg`): 0 Fullscreen, 1 Windowed, 2 Borderless.
    pub window_mode: Option<i32>,
    /// The setting "Check for new versions". The window sets it. The data thread
    /// sends the update request only while it is on.
    pub update_check: bool,
    /// The tag of a newer release (for example "v0.2.0"). `None` while this build
    /// is current, or no check has run.
    pub update: Option<String>,
    /// Champions whose stats did not load: the time of the last try, and why.
    /// `no_stats` turns it into the text of the window. A stats refresh clears it.
    pub failed: HashMap<i64, (Instant, NoStats)>,
    /// Why the last data load failed. The window writes it in its language.
    pub error: Option<LoadError>,
    pub icons: HashMap<String, egui::load::Bytes>,
    pub live: Live,
    pub offer: Option<Offer>,
    /// Pick screens that closed in this game.
    pub picks_done: usize,
    /// The wait gave up on a pick screen in this game, so `picks_done` is a guess.
    pub picks_guessed: bool,
    /// The champion whose item sets are in the League folder now. `None` when
    /// none are: the setting is off, the write failed, or the folder is unknown.
    pub item_sets_for: Option<i64>,
    /// The last clear of the item sets did not run (no League folder yet) or
    /// failed. The window tries again in the next champ select.
    pub item_sets_clear_failed: bool,
    /// A note for the Settings page and the overlay (a key, so it follows the language).
    pub ocr_note: Option<Key>,
    pub scan_always: bool,
    /// Hotkeys as Win32 virtual keys. The key thread reads them on each pass,
    /// so a change on the Settings page needs no restart.
    pub keys: Keys,
    /// The Settings page waits for a new hotkey. The key thread then ignores
    /// the hotkeys.
    pub rebinding: bool,
    /// Hotkey presses not yet handled by the UI.
    pub overlay_presses: u32,
    pub lock_presses: u32,
    pub scan_presses: u32,
    pub last_action: Option<String>,
    /// The raw error behind `last_action`, for example "http status: 404". The
    /// toast shows it only on hover.
    pub last_detail: Option<String>,
    /// When `last_action` changed. The UI shows it as a short toast.
    pub action_at: Option<Instant>,
    /// Messages that wait for their turn, oldest first, each with its raw error.
    /// `say` and `next_toast` keep them.
    pub later: VecDeque<(String, Option<String>)>,
    /// The setting for any suggestion is on (the item tips, the summary, or the fit mark).
    /// The window sets it. The poll thread reads the player list only while it is on.
    pub suggest_on: bool,
    /// The ten players of the game, from the last read of the player list. Hexsight keeps no
    /// name. It lives for the game and is never written or sent.
    pub roster: Option<Roster>,
    /// The suggestions for the window. The poll thread stores it only when it changes.
    pub advice: Option<Arc<Advice>>,
    /// Language tag of the OCR engine, for example "en-US". `None` until it starts.
    pub ocr_language: Option<String>,
    /// The League game window exists. `None` outside a game.
    pub game_window: Option<bool>,
    /// Where the own champion came from, the last time it was known.
    pub champ_source: Option<ChampSource>,
    /// The last scan of the card reader. One image at most: a new scan
    /// replaces it, and `SCAN_KEEP` after a game it goes away.
    pub scan: Option<Arc<ScanRecord>>,
    /// Screen rect of the main window (left, top, right, bottom, in pixels)
    /// while it shows in a game. A scan that covers it is not kept as the
    /// saved scan, because it would show Hexsight and not the game.
    pub window_rect: Option<[i32; 4]>,
    /// Screen rect of the overlay (left, top, right, bottom, in pixels) while it
    /// shows in recordings. The card reader drops the text lines inside it.
    pub overlay_rect: Option<[i32; 4]>,
    /// The cards that each pick screen offered in this game: screen number and
    /// augment ids. Hexsight cannot know which card the player took.
    pub offered: Vec<(usize, Vec<i64>)>,
    /// The saved games of the player, newest first. The history thread
    /// replaces the list when it adds a game.
    pub history: Arc<Vec<Game>>,
    /// `history.json` exists, or the first import ran. A sync that finds no new
    /// game then does not write the file again.
    pub history_ready: bool,
    /// The history thread reads the match list now (or waits to try again).
    /// My games says "Reading your last 20 games…" while this is true and the list is empty.
    pub importing: bool,
    /// The League install folder is known (the client runs, or a known place has it).
    pub league_found: bool,
    /// The Windows OCR engine can start: `None` while Hexsight looks for it.
    pub ocr_probe: Option<bool>,
    /// How the last scan of the game window looked: fine, black, or frozen.
    pub capture: CaptureHealth,
}

impl Shared {
    /// Show a short message (a toast). While the last message has shown for
    /// less than `TOAST_MIN`, the new one waits behind it, so a result is never
    /// hidden at once by the next one (a trade, then the item set of the new champion).
    pub fn say(&mut self, msg: String) {
        self.say_with_detail(msg, None);
    }

    /// Like `say`, with the raw error that the toast shows on hover.
    pub fn say_with_detail(&mut self, msg: String, detail: Option<String>) {
        if self.action_at.is_some_and(|t| t.elapsed() < TOAST_MIN) {
            self.later.push_back((msg, detail));
            // A long line of messages shows old news. The oldest ones go.
            while self.later.len() > TOAST_QUEUE {
                self.later.pop_front();
            }
        } else {
            self.last_action = Some(msg);
            self.last_detail = detail;
            self.action_at = Some(Instant::now());
        }
    }

    /// Put the next waiting message in place, when the shown one had `TOAST_MIN`.
    /// The window calls it for each frame. Returns true when the toast changed.
    pub fn next_toast(&mut self) -> bool {
        if self.later.is_empty() || self.action_at.is_some_and(|t| t.elapsed() < TOAST_MIN) {
            return false;
        }
        let (msg, detail) = self.later.pop_front().unzip();
        self.last_action = msg;
        self.last_detail = detail.flatten();
        self.action_at = Some(Instant::now());
        true
    }

    /// Remember the cards of a pick screen. The pairs hint on the later screens
    /// uses them. A new scan of the same screen replaces the old entry.
    pub fn note_offered(&mut self, screen: usize, ids: &[i64]) {
        match self.offered.iter_mut().find(|o| o.0 == screen) {
            Some(o) if o.1 == ids => {}
            Some(o) => o.1 = ids.to_vec(),
            None => self.offered.push((screen, ids.to_vec())),
        }
    }

    /// Why the window has no stats for `id` now: the source has none
    /// (`Missing`), or the last try failed (`Transient`: the window says that
    /// Hexsight tries again). It stays `Transient` during the next try, so the
    /// text does not go back to "Loading" once a minute. `None` while there are
    /// details (fresh or old), and while the first try runs.
    pub fn no_stats(&self, id: i64) -> Option<NoStats> {
        if self.details.contains_key(&id) || self.old_details.contains_key(&id) {
            return None;
        }
        self.failed.get(&id).map(|&(_, why)| why)
    }

    /// Should the fetch thread load the stats of `id` now? Not while there is no
    /// stats index or the details are there, not for a missing champion, and not
    /// before `STATS_RETRY` after a failed try. Id 0 is "no champion yet" in champ
    /// select: it has no file.
    fn wants_fetch(&self, id: i64) -> bool {
        id > 0
            && self.stats.is_some()
            && !self.details.contains_key(&id)
            && match self.failed.get(&id) {
                None => true,
                Some(&(_, NoStats::Missing)) => false,
                Some(&(at, NoStats::Transient)) => at.elapsed() >= STATS_RETRY,
            }
    }

    /// The next champion for the fetch thread. The own champion goes first, in
    /// every phase, until its details load. The others come from `queue` in order.
    /// Ids that need no fetch leave the queue. Without a stats index nothing
    /// leaves it: the requests wait for the index.
    fn next_fetch(&self, queue: &mut VecDeque<i64>) -> Option<i64> {
        self.stats.as_ref()?;
        if let Some(c) = self.my_champion.filter(|&c| self.wants_fetch(c)) {
            queue.retain(|&q| q != c);
            return Some(c);
        }
        while let Some(id) = queue.pop_front() {
            if self.wants_fetch(id) {
                return Some(id);
            }
        }
        None
    }

    /// Keep the result of one fetch. `from` is the stats index of the download.
    /// When the index changed during the download, nothing is kept and the answer
    /// is false: the caller asks again with the new index. A load that worked can
    /// mean that the network is back, so the own champion does not wait for its pause.
    fn store_fetch(
        &mut self,
        from: &Arc<Stats>,
        id: i64,
        got: Result<ChampStats, NoStats>,
    ) -> bool {
        if !self.stats.as_ref().is_some_and(|x| Arc::ptr_eq(x, from)) {
            return false;
        }
        match got {
            Ok(cs) => {
                self.details.insert(id, Arc::new(cs));
                self.old_details.remove(&id);
                self.failed.remove(&id);
                if let Some(c) = self.my_champion
                    && matches!(self.failed.get(&c), Some((_, NoStats::Transient)))
                {
                    self.failed.remove(&c);
                }
            }
            Err(why) => {
                self.failed.insert(id, (Instant::now(), why));
            }
        }
        true
    }

    /// Keep new advice. Returns true when it differs from the old one, so the window draws again
    /// only for a change. `None` clears it.
    pub fn store_advice(&mut self, new: Option<Advice>) -> bool {
        if self.advice.as_deref() == new.as_ref() {
            return false;
        }
        self.advice = new.map(Arc::new);
        true
    }

    fn end_game(&mut self) {
        self.roster = None;
        self.advice = None;
        self.offer = None;
        self.picks_done = 0;
        self.picks_guessed = false;
        self.offered.clear();
        self.live = Live::default();
        self.capture = CaptureHealth::Fine;
    }
}

pub enum Cmd {
    BenchSwap(i64),
    /// A call on the champion trade with this id (`trades[].id` of the session).
    Trade(i64, TradeCall),
    Reload,
    /// Write the item sets of this champion (one for each build) into the League
    /// folder. The flag is the setting "Hide win rates": the titles of the blocks
    /// then carry no win rate.
    WriteItemSet(i64, Arc<ChampStats>, bool),
    /// Delete the item sets of Hexsight from the League folder (the setting is off).
    ClearItemSets,
    /// Set League to Borderless through the client. Only the button "Fix" sends it.
    FixBorderless,
}

enum DataCmd {
    Locale(String),
    Reload,
}

/// Work for the history thread.
pub enum HistoryJob {
    /// Read the newest 20 games and add the Mayhem games that `history.json`
    /// lacks. It runs at each client start, so the games that you play while
    /// Hexsight is closed are not lost.
    Sync,
    /// Read one finished game (its id) from the match history.
    Game(i64),
}

pub struct Handles {
    pub shared: Arc<Mutex<Shared>>,
    pub cmd: Sender<Cmd>,
    pub icon: Sender<String>,
    pub want: Sender<i64>,
}

/// The two hotkeys, as Win32 virtual keys.
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Keys {
    pub overlay: i32,
    pub scan: i32,
}

pub fn start(ctx: egui::Context, keys: Keys, scan_always: bool, last_locale: String) -> Handles {
    let saved = history::load(&history::path());
    let history_ready = saved.is_some();
    let shared = Arc::new(Mutex::new(Shared {
        scan_always,
        keys,
        history: Arc::new(saved.unwrap_or_default()),
        history_ready,
        ..Default::default()
    }));
    let (cmd_tx, cmd_rx) = channel();
    let (hist_tx, hist_rx) = channel();
    let (icon_tx, icon_rx) = channel();
    let (warm_tx, warm_rx) = channel();
    let (want_tx, want_rx) = channel();
    let (data_tx, data_rx) = channel();
    // Each thread has a name. A line of crash.log says which thread failed.
    let spawn = |name: &str, f: Box<dyn FnOnce() + Send>| {
        let _ = std::thread::Builder::new().name(name.to_string()).spawn(f);
    };
    // The data thread sends the augment icons to warm up. They have their own
    // queue, so a request from the UI never waits behind 249 downloads. The
    // empty URL wakes the icon thread when it sleeps.
    let wake = icon_tx.clone();
    let warm: Box<dyn Fn(Vec<String>) + Send> = Box::new(move |urls| {
        for url in urls {
            let _ = warm_tx.send(url);
        }
        let _ = wake.send(String::new());
    });
    let (s, c, w, d) = (shared.clone(), ctx.clone(), want_tx.clone(), data_tx);
    spawn(
        "poll",
        Box::new(move || poll_loop(s, c, cmd_rx, w, d, hist_tx)),
    );
    let (s, c, w) = (shared.clone(), ctx.clone(), want_tx.clone());
    spawn(
        "data",
        Box::new(move || data_loop(s, c, data_rx, w, warm, last_locale)),
    );
    let (s, c) = (shared.clone(), ctx.clone());
    spawn("fetch", Box::new(move || fetch_loop(s, c, want_rx)));
    let (s, c) = (shared.clone(), ctx.clone());
    spawn("icon", Box::new(move || icon_loop(s, c, icon_rx, warm_rx)));
    let (s, c) = (shared.clone(), ctx.clone());
    spawn("ocr", Box::new(move || ocr_loop(s, c)));
    let (s, c) = (shared.clone(), ctx.clone());
    spawn("key", Box::new(move || key_loop(s, c)));
    let (s, c) = (shared.clone(), ctx.clone());
    spawn("history", Box::new(move || history_loop(s, c, hist_rx)));
    // One short look at the OCR engine, for the first-run checklist. The thread ends at once.
    let (s, c) = (shared.clone(), ctx);
    spawn(
        "ocr-probe",
        Box::new(move || {
            unsafe {
                let _ = windows::Win32::System::WinRT::RoInitialize(
                    windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
                );
            }
            let found = Ocr::new().is_some();
            s.lock().unwrap().ocr_probe = Some(found);
            c.request_repaint();
        }),
    );
    Handles {
        shared,
        cmd: cmd_tx,
        icon: icon_tx,
        want: want_tx,
    }
}

/// Static data (client language) and the stats index. Runs apart from the
/// poll thread, so slow downloads never freeze champ select. After the static
/// data loads, it hands all augment icon URLs to `warm` once, so the first
/// pick screen shows icons and not grey squares.
fn data_loop(
    shared: Arc<Mutex<Shared>>,
    ctx: egui::Context,
    rx: Receiver<DataCmd>,
    want: Sender<i64>,
    warm: Box<dyn Fn(Vec<String>) + Send>,
    first_locale: String,
) {
    let web = cache::web_agent();
    cache::cleanup(Duration::from_secs(45 * 24 * 3600));
    let mut want_loc = if first_locale.is_empty() {
        "en_US".to_string()
    } else {
        first_locale
    };
    let mut loaded_loc: Option<String> = None;
    let mut static_retry = Instant::now();
    let mut next_stats = Instant::now();
    // The next update request. One per day, and none while the setting is off.
    let mut next_update = Instant::now();
    let mut forced = false;
    let mut first = true;
    let mut warmed: HashSet<String> = HashSet::new();
    loop {
        // Wake on a command, else check every 5 seconds.
        let wait = if first {
            Duration::ZERO
        } else {
            Duration::from_secs(5)
        };
        first = false;
        match rx.recv_timeout(wait) {
            Ok(DataCmd::Locale(l)) => {
                if l != want_loc {
                    want_loc = l;
                    static_retry = Instant::now();
                }
            }
            Ok(DataCmd::Reload) => {
                cache::force_refresh(true);
                forced = true;
                loaded_loc = None;
                static_retry = Instant::now();
                next_stats = Instant::now();
            }
            Err(RecvTimeoutError::Timeout) => {}
            Err(RecvTimeoutError::Disconnected) => return,
        }
        let now = Instant::now();
        if loaded_loc.as_deref() != Some(want_loc.as_str()) && now >= static_retry {
            match data::load_static(&web, &want_loc) {
                Ok(sd) => {
                    let urls = sd
                        .augments
                        .values()
                        .map(|a| a.icon.clone())
                        .filter(|u| !u.is_empty() && warmed.insert(u.clone()));
                    warm(urls.collect());
                    shared.lock().unwrap().static_data = Some(Arc::new(sd));
                    loaded_loc = Some(want_loc.clone());
                }
                Err(e) => {
                    shared.lock().unwrap().error = Some(e);
                    static_retry = now + Duration::from_secs(60);
                }
            }
            ctx.request_repaint();
        }
        if now >= next_stats {
            match Stats::load(&web) {
                Ok(st) => {
                    let mut s = shared.lock().unwrap();
                    s.stats = Some(Arc::new(st));
                    // Keep the old details. The UI shows them until the new
                    // ones arrive, so the overlay never flashes "Loading".
                    let old = std::mem::take(&mut s.details);
                    s.old_details.extend(old);
                    s.failed.clear();
                    s.error = None;
                    next_stats = now + Duration::from_secs(3 * 3600);
                    if let Some(c) = s.my_champion {
                        let _ = want.send(c);
                    }
                }
                Err(e) => {
                    shared.lock().unwrap().error = Some(e);
                    next_stats = now + Duration::from_secs(60);
                }
            }
            ctx.request_repaint();
        }
        if now >= next_update
            && update::repo_ready(update::REPO)
            && shared.lock().unwrap().update_check
        {
            next_update = now + update::CHECK_EVERY;
            // A plain GET of the public release data. Nothing else leaves the PC. The cache file also keeps a restart from asking again within the day.
            let found = cache::get_json(&web, &update::api_url(update::REPO), update::CHECK_EVERY)
                .and_then(|r| update::newer_tag(&r, update::VERSION));
            shared.lock().unwrap().update = found;
            ctx.request_repaint();
        }
        if forced && loaded_loc.is_some() && next_stats > now {
            cache::force_refresh(false);
            forced = false;
        }
    }
}

fn poll_loop(
    shared: Arc<Mutex<Shared>>,
    ctx: egui::Context,
    cmds: Receiver<Cmd>,
    want: Sender<i64>,
    data: Sender<DataCmd>,
    hist: Sender<HistoryJob>,
) {
    let mut lcu = Lcu::new();
    let live = LiveClient::new();
    let mut phase = Phase::Offline;
    let mut misses = 0;
    let mut queue: Option<i64> = None;
    let mut queue_read = false;
    let mut locale_sent = false;
    let mut champ_known = false;
    // The own champion of the last champ select. In the game after it, it wins
    // over the answer of the live game, which only has names.
    let mut picked: Option<i64> = None;
    let mut window_mode: Option<i32> = None;
    // The League folder whose game.cfg gave `window_mode`.
    let mut mode_read_in: Option<std::path::PathBuf> = None;
    // The id of the game, from the gameflow session. The game that runs: its id
    // and queue. The history thread reads that game after it ends.
    let mut game_id: Option<i64> = None;
    let mut id_tries = 0;
    let mut live_game: Option<(i64, i64)> = None;
    // The history sync of this client connection was asked for.
    let mut sync_sent = false;
    // Memory trim: once 20 seconds after start-up, and once after a game ends.
    // More trims would only show a smaller number: the pages come back at the next scan.
    let mut trim_at = Some(Instant::now() + Duration::from_secs(20));
    // The player list for the suggestions: when it was read last, and the gold of the last pass.
    let mut last_roster: Option<Instant> = None;
    let mut last_gold: Option<u32> = None;

    loop {
        let tick = Instant::now();

        // 1. Client phase. One failed request keeps the last phase.
        if !lcu.connected() {
            lcu.refresh_lock();
            locale_sent = false;
        }
        let mut now_phase = if lcu.connected() {
            match lcu::phase(&mut lcu) {
                Some(p) => {
                    misses = 0;
                    p
                }
                None => {
                    misses += 1;
                    if misses >= 4 { Phase::Offline } else { phase }
                }
            }
        } else {
            Phase::Offline
        };
        // The client can restart during a game. The game API still answers.
        if now_phase == Phase::Offline && phase == Phase::InGame && live.get("gamestats").is_some()
        {
            now_phase = Phase::InGame;
        }
        if now_phase != phase {
            queue_read = false;
            if now_phase == Phase::InGame {
                // A new game: forget the id of the last one.
                game_id = None;
                id_tries = 0;
            }
        }
        // The player can change the window mode between games: read it again at each
        // phase change. Read it also as soon as the League folder is known, so the
        // first-run card knows it before the client starts.
        if let Some(dir) = lcu.install_dir()
            && (now_phase != phase || mode_read_in.as_deref() != Some(dir))
        {
            window_mode = read_window_mode(dir);
            mode_read_in = Some(dir.to_path_buf());
        }
        if !queue_read {
            let wants_queue =
                matches!(now_phase, Phase::Lobby | Phase::ChampSelect | Phase::InGame);
            let session = if wants_queue && lcu.connected() {
                lcu::game_session(&mut lcu)
            } else {
                None
            };
            let read = session.and_then(|s| s.queue);
            queue = next_queue(now_phase, read, queue);
            if let Some(id) = session.and_then(|s| s.game_id) {
                game_id = Some(id);
            }
            // In game, read again until the game id is known (a few tries at most).
            let in_game = now_phase == Phase::InGame;
            if in_game && session.is_some() {
                id_tries += 1;
            }
            // Only a real answer ends the reading: a queue kept from before does not.
            queue_read = !wants_queue
                || (read.is_some() && (!in_game || game_id.is_some() || id_tries >= GAME_ID_TRIES));
        }
        if now_phase == Phase::InGame
            && let (Some(id), Some(q)) = (game_id, queue)
        {
            live_game = Some((id, q));
        }
        if lcu.connected()
            && !locale_sent
            && let Some(l) = lcu::locale(&mut lcu)
        {
            shared.lock().unwrap().locale = l.clone();
            let _ = data.send(DataCmd::Locale(l));
            locale_sent = true;
            // The language of the interface (Auto) follows the locale: draw the window again.
            ctx.request_repaint();
        }
        let mut cs = if now_phase == Phase::ChampSelect {
            lcu::champ_select(&mut lcu)
        } else {
            None
        };
        // The first step of a Mayhem champ select: the player has no champion yet, and the
        // client offers some. One more request, only while there is none, and only in a
        // Mayhem queue: other queues have no such step, so they get no extra request.
        if let Some(c) = cs.as_mut().filter(|c| wants_choices(c.my_champion, queue)) {
            c.choices = lcu::subset_champions(&mut lcu);
        }
        picked = keep_picked(now_phase, cs.as_ref().map(|c| c.my_champion), picked);

        // 2. In game: level, and the champion until it is known (when no champ
        // select gave it). Also whether the game window exists (the card reader needs it).
        let mut live_champ = None;
        let mut live_state = None;
        let mut game_window = None;
        let mut new_roster = None;
        if now_phase == Phase::InGame {
            let (sd, suggest_on) = {
                let s = shared.lock().unwrap();
                (s.static_data.clone(), s.suggest_on)
            };
            let ask = RosterAsk {
                on: suggest_runs(now_phase, queue, suggest_on),
                since_last: last_roster.map(|t| t.elapsed()),
                gold_before: last_gold,
            };
            let read = read_live(&live, sd.as_deref(), !champ_known && picked.is_none(), &ask);
            (live_champ, live_state, new_roster) = (read.champion, read.state, read.roster);
            if read.asked {
                last_roster = Some(Instant::now());
            }
            last_gold = live_state.as_ref().and_then(|l| l.gold);
            if live_champ.is_some() {
                champ_known = true;
            }
            game_window = Some(ocr::game_window_found());
        } else {
            (last_roster, last_gold) = (None, None);
        }

        // 3. Publish. Repaint only on change.
        {
            let league_found = lcu.install_dir().is_some();
            let mut s = shared.lock().unwrap();
            let mut changed = s.phase != now_phase
                || s.queue != queue
                || s.window_mode != window_mode
                || s.game_window != game_window
                || s.league_found != league_found;
            s.window_mode = window_mode;
            s.game_window = game_window;
            s.league_found = league_found;
            if now_phase != s.phase {
                if now_phase == Phase::InGame {
                    s.end_game();
                    champ_known = false;
                    live_game = None;
                } else if s.phase == Phase::InGame && now_phase != Phase::Offline {
                    // A real end of the game (not a lost connection). The OCR
                    // thread frees its engine within a second, so trim a bit later.
                    s.end_game();
                    trim_at = Some(Instant::now() + Duration::from_secs(5));
                    // A Mayhem game: the history thread saves it.
                    if let Some((id, _)) = live_game.take().filter(|g| is_mayhem(g.1)) {
                        let _ = hist.send(HistoryJob::Game(id));
                    }
                }
            }
            // The champ select of this game decides. The live game names the champion
            // only when Hexsight saw no champ select (it started during the game).
            let mine = picked.or(live_champ);
            let entered_select = now_phase == Phase::ChampSelect && s.phase != Phase::ChampSelect;
            let champion =
                next_champion(now_phase, entered_select, cs.is_some(), mine, s.my_champion);
            if champion != s.my_champion {
                s.my_champion = champion;
                changed = true;
            }
            if mine.is_some() {
                let source = if picked.is_some() {
                    ChampSource::ChampSelect
                } else {
                    ChampSource::LiveClient
                };
                if s.champ_source != Some(source) {
                    s.champ_source = Some(source);
                    changed = true;
                }
            } else if champion.is_none() && s.champ_source.take().is_some() {
                changed = true;
            }
            // The last scan (an image) must not stay in memory for hours after a game.
            if now_phase != Phase::InGame
                && s.scan.as_ref().is_some_and(|r| r.at.elapsed() > SCAN_KEEP)
            {
                s.scan = None;
                changed = true;
            }
            let key = |c: &Option<ChampSelect>| {
                c.as_ref().map(|c| {
                    (
                        c.my_champion,
                        c.bench.clone(),
                        c.team.clone(),
                        c.trades.clone(),
                        c.choices.clone(),
                    )
                })
            };
            if key(&s.champ_select) != key(&cs) {
                changed = true;
                // Prefetch the champions I can swap to.
                for id in cs
                    .iter()
                    .flat_map(|c| std::iter::once(c.my_champion).chain(c.bench.iter().copied()))
                {
                    let _ = want.send(id);
                }
            }
            // The first read of a game or a new read of the player list. A pass that reads none keeps the last one.
            if let Some(r) = new_roster {
                s.roster = Some(r);
            }
            match live_state {
                Some(l) => {
                    if l.level != s.live.level {
                        changed = true;
                    }
                    s.live = l;
                }
                // No answer from the game: stop the pick logic until it answers.
                None if now_phase == Phase::InGame => s.live = Live::default(),
                None => {}
            }
            // Ask for the own champion on each pass, in champ select and in the game.
            // The fetch thread drops a request that needs no download.
            if let Some(c) = s.my_champion {
                let _ = want.send(c);
            }
            s.phase = now_phase;
            s.queue = queue;
            s.champ_select = cs;
            if changed {
                ctx.request_repaint();
            }
        }
        phase = now_phase;

        // 3b. The suggestions. They run in a Mayhem game only, while a setting asks for them, and once the
        // data of the game and of the own champion is there. The work runs outside the lock.
        suggest_pass(&shared, &ctx, now_phase, queue);

        // At each client start: read the newest 20 games and add the Mayhem games that
        // Hexsight missed. The next start of the client asks again.
        if !lcu.connected() {
            sync_sent = false;
        } else if !sync_sent && now_phase != Phase::InGame {
            sync_sent = true;
            shared.lock().unwrap().importing = true;
            let _ = hist.send(HistoryJob::Sync);
        }

        // 4. Commands from the UI.
        while let Ok(cmd) = cmds.try_recv() {
            match cmd {
                Cmd::BenchSwap(id) => {
                    let r = lcu::bench_swap(&mut lcu, id);
                    let mut s = shared.lock().unwrap();
                    let lang = s.lang;
                    match r {
                        Ok(()) => s.say(lang.t(Key::SwapSent).into()),
                        // The raw error is English ("http status: 404"). The toast names the reason, and shows the raw text on hover.
                        Err(e) => s.say_with_detail(
                            lang.fmt(Key::SwapFailed, &[&lang.client_error(&e)]),
                            Some(e),
                        ),
                    }
                    drop(s);
                    ctx.request_repaint();
                }
                Cmd::Trade(id, call) => {
                    let r = lcu::trade(&mut lcu, id, call);
                    let mut s = shared.lock().unwrap();
                    let lang = s.lang;
                    match r {
                        Ok(()) => s.say(
                            lang.t(match call {
                                TradeCall::Request => Key::TradeRequested,
                                TradeCall::Accept => Key::TradeAccepted,
                                TradeCall::Decline => Key::TradeDeclined,
                                TradeCall::Cancel => Key::TradeCancelled,
                            })
                            .into(),
                        ),
                        Err(e) => s.say_with_detail(
                            lang.fmt(Key::TradeFailed, &[&lang.client_error(&e)]),
                            Some(e),
                        ),
                    }
                    drop(s);
                    ctx.request_repaint();
                }
                Cmd::Reload => {
                    let _ = data.send(DataCmd::Reload);
                    let mut s = shared.lock().unwrap();
                    let lang = s.lang;
                    s.say(lang.t(Key::Reloading).into());
                    drop(s);
                    ctx.request_repaint();
                }
                Cmd::WriteItemSet(id, cs, hide_rates) => {
                    let (sd, lang) = {
                        let s = shared.lock().unwrap();
                        (s.static_data.clone(), s.lang)
                    };
                    // The window asks only after the data loaded (`item_set_due`). A request that comes
                    // earlier says so, not that League is missing.
                    let r = match (lcu.install_dir(), sd) {
                        (Some(dir), Some(sd)) => {
                            crate::itemset::write(lang, dir, id, &cs, &sd, hide_rates)
                        }
                        (None, _) => Err(lang.t(Key::ErrLeagueFolder).into()),
                        (Some(_), None) => Err(lang.t(Key::ErrDataLoading).into()),
                    };
                    // The build card shows a success (a check mark and a line), so it needs no toast.
                    let mut s = shared.lock().unwrap();
                    match r {
                        Ok(_) => s.item_sets_for = Some(id),
                        Err(e) => {
                            s.item_sets_for = None;
                            s.say(lang.fmt(Key::ItemSetFailed, &[&e]));
                        }
                    }
                    drop(s);
                    ctx.request_repaint();
                }
                Cmd::FixBorderless => {
                    let r = lcu::set_borderless(&mut lcu);
                    // The client writes `game.cfg` a moment after the save. Read it again to show the result.
                    let after = match (&r, lcu.install_dir()) {
                        (Ok(()), Some(dir)) => mode_after_save(dir),
                        _ => None,
                    };
                    if after.is_some() {
                        window_mode = after;
                    }
                    let mut s = shared.lock().unwrap();
                    let lang = s.lang;
                    match fix_outcome(&r, after) {
                        FixOutcome::Done => s.say(lang.t(Key::FixDone).into()),
                        FixOutcome::Still => s.say(lang.t(Key::FixStill).into()),
                        FixOutcome::Sent => s.say(lang.t(Key::FixSent).into()),
                        FixOutcome::Failed => {
                            let e = r.err().unwrap_or_default();
                            s.say_with_detail(
                                lang.fmt(Key::FixFailed, &[&lang.client_error(&e)]),
                                Some(e),
                            );
                        }
                    }
                    drop(s);
                    ctx.request_repaint();
                }
                Cmd::ClearItemSets => {
                    shared.lock().unwrap().item_sets_for = None;
                    ctx.request_repaint();
                    // The folder may not be known yet. That is no error to show, but the window must try again later.
                    let result = lcu.install_dir().map(crate::itemset::clear);
                    let mut s = shared.lock().unwrap();
                    s.item_sets_clear_failed = !matches!(result, Some(Ok(())));
                    if let Some(Err(e)) = result {
                        let lang = s.lang;
                        s.say(lang.fmt(Key::ItemSetFailed, &[&e]));
                    }
                    drop(s);
                    ctx.request_repaint();
                }
            }
        }

        if trim_at.is_some_and(|t| Instant::now() >= t) {
            win::trim_memory();
            trim_at = None;
        }

        // Champ select changes fast. Other phases do not.
        let period = match phase {
            Phase::ChampSelect => Duration::from_millis(700),
            Phase::InGame => Duration::from_millis(1000),
            Phase::Offline => Duration::from_secs(3),
            _ => Duration::from_millis(1500),
        };
        if let Some(rest) = period.checked_sub(tick.elapsed()) {
            std::thread::sleep(rest);
        }
    }
}

/// `WindowMode` of League: 0 Fullscreen, 1 Windowed, 2 Borderless. It is in
/// `<League>\Config\game.cfg`. `None` when the file or the key is missing.
fn read_window_mode(league: &Path) -> Option<i32> {
    let bytes = std::fs::read(league.join("Config").join("game.cfg")).ok()?;
    parse_window_mode(&String::from_utf8_lossy(&bytes))
}

/// Value of the key `WindowMode` in the `[General]` section of a `game.cfg` text.
fn parse_window_mode(cfg: &str) -> Option<i32> {
    let mut general = false;
    for line in cfg.trim_start_matches('\u{feff}').lines() {
        let line = line.trim();
        if let Some(section) = line.strip_prefix('[').and_then(|l| l.strip_suffix(']')) {
            general = section.eq_ignore_ascii_case("General");
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        if general && key.trim().eq_ignore_ascii_case("WindowMode") {
            return value.trim().parse().ok();
        }
    }
    None
}

/// What the button "Fix" reports.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum FixOutcome {
    /// The client took it, and `game.cfg` now says Borderless.
    Done,
    /// The client took it, but `game.cfg` still says another mode.
    Still,
    /// The client took it, and `game.cfg` cannot be read, so Hexsight cannot confirm.
    Sent,
    /// The client refused, or did not answer.
    Failed,
}

/// The result of the fix from the answer of the client and a new read of `game.cfg`.
fn fix_outcome(sent: &Result<(), String>, mode_after: Option<i32>) -> FixOutcome {
    match (sent, mode_after) {
        (Err(_), _) => FixOutcome::Failed,
        (Ok(()), Some(2)) => FixOutcome::Done,
        (Ok(()), Some(_)) => FixOutcome::Still,
        (Ok(()), None) => FixOutcome::Sent,
    }
}

/// `WindowMode` after a save. The client can take a moment to write the file, so
/// the read tries again a few times while it still shows another mode.
fn mode_after_save(league: &Path) -> Option<i32> {
    let mut mode = None;
    for i in 0..4 {
        mode = read_window_mode(league);
        if mode == Some(2) {
            break;
        }
        if i < 3 {
            std::thread::sleep(Duration::from_millis(250));
        }
    }
    mode
}

/// Do the suggestions run? In a Mayhem game only, and only while a setting asks for them.
pub(crate) fn suggest_runs(phase: Phase, queue: Option<i64>, on: bool) -> bool {
    on && phase == Phase::InGame && queue.is_some_and(is_mayhem)
}

/// Is the player list (the larger request of the Live Client) due in this pass? Only while the
/// suggestions run (`on`). It is read at once after the first pass of a game (`since_last` is
/// `None`), every `ROSTER_EVERY`, and at once after a purchase: a fall of `PURCHASE_DROP` gold or
/// more between two passes.
pub(crate) fn roster_due(
    on: bool,
    since_last: Option<Duration>,
    gold_before: Option<u32>,
    gold_now: Option<u32>,
) -> bool {
    if !on {
        return false;
    }
    let bought = matches!((gold_before, gold_now), (Some(before), Some(now)) if before >= now.saturating_add(PURCHASE_DROP));
    since_last.is_none_or(|d| d >= ROSTER_EVERY) || bought
}

/// What `read_live` needs to know about the player list.
struct RosterAsk {
    /// The suggestions run (`suggest_runs`).
    on: bool,
    /// The time since the last read of the player list.
    since_last: Option<Duration>,
    /// The gold of the pass before.
    gold_before: Option<u32>,
}

/// What one pass of the Live Client gave.
#[derive(Default)]
struct LiveRead {
    champion: Option<i64>,
    state: Option<Live>,
    /// The player list was asked for in this pass.
    asked: bool,
    /// The players, when the list came and the player is in it.
    roster: Option<Roster>,
}

/// Level and gold from the Live Client API. With `want_champ`, also the own champion (riot id
/// match in the player list, a larger request). The same request gives the roster of the
/// suggestions when it is due (`roster_due`), so a pass asks for the player list at most once.
fn read_live(
    live: &LiveClient,
    sd: Option<&StaticData>,
    want_champ: bool,
    ask: &RosterAsk,
) -> LiveRead {
    let Some(me) = live.get("activeplayer") else {
        return LiveRead::default();
    };
    let gold = me["currentGold"].as_f64().map(|g| g.max(0.0) as u32);
    let state = Some(Live {
        level: me["level"].as_i64().unwrap_or(0),
        gold,
    });
    let due = roster_due(ask.on, ask.since_last, ask.gold_before, gold);
    let Some(sd) = sd.filter(|_| want_champ || due) else {
        return LiveRead {
            state,
            ..Default::default()
        };
    };
    let my_id = me["riotId"]
        .as_str()
        .or(me["summonerName"].as_str())
        .unwrap_or("")
        .to_string();
    let mut champ = None;
    let players = live.get("playerlist");
    if want_champ {
        for p in players.iter().flat_map(|l| l.as_array()).flatten() {
            let rid = p["riotId"]
                .as_str()
                .or(p["summonerName"].as_str())
                .unwrap_or("");
            if rid != my_id {
                continue;
            }
            let raw = p["rawChampionName"].as_str().unwrap_or("");
            let name = p["championName"].as_str().unwrap_or("");
            champ = data::live_champion(&sd.champions, raw, name);
        }
    }
    let roster = players
        .as_ref()
        .filter(|_| due)
        .and_then(|l| suggest::parse_players(l, &my_id, &sd.champions));
    LiveRead {
        champion: champ,
        state,
        asked: due,
        roster,
    }
}

/// One pass of the suggestions: from the roster of the game, the own champion and the offers so
/// far, make the advice, and store it when it changed. Outside a Mayhem game, or with every setting
/// off, the advice is cleared. The work runs outside the lock. The summary of the enemy team needs
/// the roster and the static data only. While the stats of the champion are still loading, the pass
/// waits. When they are not there (the champion is unknown, or the load failed), the advice has the
/// summary and no items: it says that there is no build.
fn suggest_pass(shared: &Mutex<Shared>, ctx: &egui::Context, phase: Phase, queue: Option<i64>) {
    let work = {
        let s = shared.lock().unwrap();
        let details = s
            .my_champion
            .and_then(|c| s.details.get(&c).or(s.old_details.get(&c)).cloned());
        let stats_gone = s.my_champion.is_none_or(|c| s.failed.contains_key(&c));
        match (
            suggest_runs(phase, queue, s.suggest_on),
            s.static_data.clone(),
            s.roster.clone(),
        ) {
            (true, Some(sd), Some(roster)) if details.is_some() || stats_gone => Some((
                sd,
                details,
                roster,
                s.live.gold,
                s.offered
                    .iter()
                    .flat_map(|o| o.1.iter().copied())
                    .collect::<Vec<i64>>(),
            )),
            _ => None,
        }
    };
    let advice = work.map(|(sd, details, roster, gold, offered)| {
        let summary = suggest::context(&roster, &sd);
        let no_stats = ChampStats::default();
        // The Live Client does not give the augments that the player took, so none is known.
        let input = suggest::Input {
            me: roster.me(),
            gold,
            ctx: Some(&summary),
            details: details.as_deref().unwrap_or(&no_stats),
            sd: &sd,
            offered: &offered,
            known: &[],
        };
        suggest::advise(&input)
    });
    if shared.lock().unwrap().store_advice(advice) {
        ctx.request_repaint();
    }
}

/// The own champion of the last champ select, after one pass of the poll loop.
/// `session` is the champion that the champ select session names for the player
/// (0 before the player has one), or `None` when the session did not answer.
/// Champ select sets it: a session with champion 0 means that the player has none
/// yet, and a pass without an answer keeps the last one. The game after it keeps it,
/// also while the client is offline (the client can restart during a game). The
/// lobby, the end of the game, and the idle client forget it, so the next game
/// starts clean.
pub(crate) fn keep_picked(phase: Phase, session: Option<i64>, picked: Option<i64>) -> Option<i64> {
    match phase {
        Phase::ChampSelect => match session {
            Some(c) => Some(c).filter(|&c| c > 0),
            None => picked,
        },
        Phase::InGame | Phase::Offline => picked,
        Phase::Lobby | Phase::Idle | Phase::PostGame => None,
    }
}

/// The queue after one pass of the queue read in the poll loop. `read` is what the gameflow
/// session named in this pass: `None` when the call failed or the session has no queue yet.
/// In a lobby, a champ select and a game, a real value replaces the queue, and a pass without
/// one keeps it, so one failed call does not drop it (a Ranked champ select would look like an
/// unknown queue for a pass). The end of the game keeps the queue of the game: the window
/// picks its result screen with it, and a Ranked game has no After card. A client with no game
/// (idle or offline) forgets it.
pub(crate) fn next_queue(phase: Phase, read: Option<i64>, queue: Option<i64>) -> Option<i64> {
    match phase {
        Phase::Lobby | Phase::ChampSelect | Phase::InGame => read.or(queue),
        Phase::PostGame => queue,
        Phase::Idle | Phase::Offline => None,
    }
}

/// The own champion that the window shows after one pass of the poll loop. A known
/// champion (`mine`) wins. Without one, a champ select forgets the champion of the
/// game before: at the start of the champ select (`entered_select`), and whenever the
/// session answers (`session_read`) and names none. Item sets are written only for
/// a champion of this champ select, so the old one must not stay. Other phases keep
/// the last champion (the result card of the game that ended shows it).
pub(crate) fn next_champion(
    phase: Phase,
    entered_select: bool,
    session_read: bool,
    mine: Option<i64>,
    current: Option<i64>,
) -> Option<i64> {
    match mine {
        Some(c) => Some(c),
        None if phase == Phase::ChampSelect && (entered_select || session_read) => None,
        None => current,
    }
}

/// Does the poll loop ask for the offered champions? Only in the first step of a
/// Mayhem champ select: the player has no champion (id 0) and the queue is Mayhem.
pub(crate) fn wants_choices(session_champion: i64, queue: Option<i64>) -> bool {
    session_champion == 0 && queue.is_some_and(is_mayhem)
}

/// Loads the stats of one champion at a time. The own champion goes first and
/// stays wanted until its details load: the thread looks at it every
/// `FETCH_TICK`, so a failed try runs again after `STATS_RETRY` in every phase
/// (the own champion stays until the next champ select), without a request
/// from the window.
fn fetch_loop(shared: Arc<Mutex<Shared>>, ctx: egui::Context, rx: Receiver<i64>) {
    let web = cache::web_agent();
    // Requests in order. A request that is in the queue already is not added again.
    let mut queue: VecDeque<i64> = VecDeque::new();
    let add = |queue: &mut VecDeque<i64>, id: i64| {
        if !queue.contains(&id) {
            queue.push_back(id);
        }
    };
    loop {
        for id in rx.try_iter() {
            add(&mut queue, id);
        }
        let next = {
            let s = shared.lock().unwrap();
            s.next_fetch(&mut queue).zip(s.stats.clone())
        };
        let Some((id, stats)) = next else {
            // Nothing to load: wait for a request, or look at the own champion again.
            match rx.recv_timeout(FETCH_TICK) {
                Ok(id) => add(&mut queue, id),
                Err(RecvTimeoutError::Timeout) => {}
                Err(RecvTimeoutError::Disconnected) => return,
            }
            continue;
        };
        let got = stats.champion(&web, id);
        let kept = shared.lock().unwrap().store_fetch(&stats, id, got);
        if !kept {
            // The index changed during the download. Ask again with the new one.
            queue.push_front(id);
            continue;
        }
        ctx.request_repaint();
    }
}

fn icon_loop(
    shared: Arc<Mutex<Shared>>,
    ctx: egui::Context,
    rx: Receiver<String>,
    warm_rx: Receiver<String>,
) {
    let web = cache::web_agent();
    // Last try per URL. A failed download can try again after 60 seconds.
    let mut tried: HashMap<String, Instant> = HashMap::new();
    // Icons to load before the UI asks. They wait while a request from the UI waits.
    let mut warm: VecDeque<String> = VecDeque::new();
    loop {
        warm.extend(warm_rx.try_iter());
        let url = match rx.try_recv() {
            Ok(url) => url,
            Err(TryRecvError::Disconnected) => return,
            Err(TryRecvError::Empty) => match warm.pop_front() {
                Some(url) => url,
                // Nothing to load: sleep until the next request or the next warm-up.
                None => match rx.recv() {
                    Ok(url) => url,
                    Err(_) => return,
                },
            },
        };
        if url.is_empty()
            || tried
                .get(&url)
                .is_some_and(|t| t.elapsed() < Duration::from_secs(60))
        {
            continue;
        }
        if shared.lock().unwrap().icons.contains_key(&url) {
            continue;
        }
        tried.insert(url.clone(), Instant::now());
        if let Some(bytes) = cache::get(&web, &url, Duration::from_secs(30 * 24 * 3600)) {
            shared
                .lock()
                .unwrap()
                .icons
                .insert(url, egui::load::Bytes::from(bytes));
            ctx.request_repaint();
        }
    }
}

/// One job of the history thread and the time of its next try.
struct Todo {
    job: HistoryJob,
    due: Instant,
    tries: usize,
}

/// Saves the games of the player. It waits for a job without any work. A job
/// that cannot finish yet (the game is not in the match history, or the client
/// is not signed in) tries again later. It has its own client connection, so
/// the poll thread never waits for it.
fn history_loop(shared: Arc<Mutex<Shared>>, ctx: egui::Context, rx: Receiver<HistoryJob>) {
    let mut lcu = Lcu::new();
    let mut todo: Vec<Todo> = Vec::new();
    loop {
        let next = todo.iter().map(|t| t.due).min();
        let job = match next {
            None => match rx.recv() {
                Ok(job) => Some(job),
                Err(_) => return,
            },
            Some(due) => match rx.recv_timeout(due.saturating_duration_since(Instant::now())) {
                Ok(job) => Some(job),
                Err(RecvTimeoutError::Timeout) => None,
                Err(RecvTimeoutError::Disconnected) => return,
            },
        };
        if let Some(job) = job {
            let wait = match job {
                HistoryJob::Sync => {
                    // A new start of the client replaces a sync that still waits.
                    todo.retain(|t| !matches!(t.job, HistoryJob::Sync));
                    Duration::ZERO
                }
                HistoryJob::Game(_) => GAME_TRIES[0],
            };
            todo.push(Todo {
                job,
                due: Instant::now() + wait,
                tries: 0,
            });
            continue;
        }
        let now = Instant::now();
        let mut i = 0;
        while i < todo.len() {
            if todo[i].due > now {
                i += 1;
                continue;
            }
            let done = history_try(&mut lcu, &todo[i].job, &shared, &ctx);
            let t = &mut todo[i];
            t.tries += 1;
            let (limit, pause) = match t.job {
                HistoryJob::Sync => (IMPORT_TRIES, IMPORT_PAUSE),
                HistoryJob::Game(_) => (
                    GAME_TRIES.len(),
                    GAME_TRIES[t.tries.min(GAME_TRIES.len() - 1)],
                ),
            };
            if done || t.tries >= limit {
                // The sync is over (it worked, or it gave up): My games stops saying "Reading…".
                if matches!(todo.remove(i).job, HistoryJob::Sync) {
                    shared.lock().unwrap().importing = false;
                    ctx.request_repaint();
                }
            } else {
                t.due = Instant::now() + pause;
                i += 1;
            }
        }
    }
}

/// One try of a job. Returns true when the job is done and needs no more tries.
fn history_try(
    lcu: &mut Lcu,
    job: &HistoryJob,
    shared: &Mutex<Shared>,
    ctx: &egui::Context,
) -> bool {
    if !lcu.connected() && !lcu.refresh_lock() {
        return false;
    }
    let Some(puuid) = lcu::puuid(lcu) else {
        return false;
    };
    match job {
        HistoryJob::Game(id) => {
            let Some(game) = lcu::match_game(lcu, *id) else {
                return false;
            };
            if history::is_remake(&game) {
                return true;
            }
            match history::parse_game(&game, &puuid) {
                Some(g) if is_mayhem(g.queue) => {
                    save_games(shared, ctx, vec![g], Some(Key::GameSaved));
                    true
                }
                // Another queue: nothing to save.
                Some(_) => true,
                // The player is not in the data yet. Try again later.
                None => false,
            }
        }
        HistoryJob::Sync => {
            let Some(list) = lcu::match_list(lcu, &puuid) else {
                return false;
            };
            // The games that the file has need no call.
            let known: HashSet<i64> = shared
                .lock()
                .unwrap()
                .history
                .iter()
                .map(|g| g.id)
                .collect();
            let mut games = Vec::new();
            for e in history::missing(history::parse_list(&list), &known, IMPORT_GAMES) {
                // The whole game first. The list entry holds the own participant
                // too, so it stands in when the whole game is not there.
                let full = lcu::match_game(lcu, e.id);
                let src = full
                    .as_ref()
                    .filter(|g| history::parse_game(g, &puuid).is_some())
                    .unwrap_or(e.raw);
                if history::is_remake(src) {
                    continue;
                }
                games.extend(history::parse_game(src, &puuid));
            }
            // An empty list is saved too, so the file exists after the first sync.
            save_games(shared, ctx, games, None);
            true
        }
    }
}

/// Add games to the list in memory and in `history.json`, then wake the window.
/// `toast` is a short message for the window.
fn save_games(shared: &Mutex<Shared>, ctx: &egui::Context, new: Vec<Game>, toast: Option<Key>) {
    let (mut list, ready) = {
        let s = shared.lock().unwrap();
        (s.history.as_ref().clone(), s.history_ready)
    };
    let added = history::merge(&mut list, new);
    // A sync that finds nothing new leaves the file as it is.
    if added == 0 && ready {
        return;
    }
    // The file write runs without the lock.
    let saved = history::save(&history::path(), &list);
    let mut s = shared.lock().unwrap();
    s.history = Arc::new(list);
    s.history_ready = true;
    let lang = s.lang;
    match (saved, toast) {
        (Ok(()), Some(t)) if added > 0 => s.say(lang.t(t).to_string()),
        (Err(e), _) => s.say(lang.fmt(Key::GamesSaveFailed, &[&e])),
        _ => {}
    }
    drop(s);
    ctx.request_repaint();
}

/// Reads the cards only in ARAM Mayhem games. Scan rate: every 0.6 s while
/// cards show (every 0.3 s while the title band does not change, because such
/// a scan skips OCR), every 1.2 s while a pick screen is due, and every 1.5 s as a
/// heartbeat while picks are left. The heartbeat reads a capture only when the
/// cheap check sees cards (or every 30 s, as a safety). The scan hotkey (F9 at
/// first) scans at any time. On a locked screen the loop does not capture.
fn ocr_loop(shared: Arc<Mutex<Shared>>, ctx: egui::Context) {
    unsafe {
        let _ = windows::Win32::System::WinRT::RoInitialize(
            windows::Win32::System::WinRT::RO_INIT_MULTITHREADED,
        );
    }
    let mut ocr: Option<Ocr> = None;
    let mut matcher: Option<(Arc<StaticData>, Matcher)> = None;
    let mut seen_presses = 0;
    let mut misses = 0;
    // The cards and the screen number of the last screen that closed in this game.
    let mut last_closed: Option<(Vec<i64>, usize)> = None;
    // Since when a pick is due but no cards showed. Gives up after `WAIT_GIVE_UP`.
    let mut waiting: Option<Instant> = None;
    let mut last_scan: Option<Instant> = None;
    // The last full OCR read. A heartbeat capture without cards skips the read.
    let mut last_read: Option<Instant> = None;
    // Judges the scans of one game: a black or a frozen capture.
    let mut watch = CaptureWatch::default();
    // The last OCR read of the title band. An equal band is not read again.
    let mut memo: Option<BandMemo> = None;
    loop {
        let (phase, queue, live, sd, has_offer, presses, always, picks_done) = {
            let s = shared.lock().unwrap();
            (
                s.phase,
                s.queue,
                s.live.clone(),
                s.static_data.clone(),
                s.offer.is_some(),
                s.scan_presses,
                s.scan_always,
                s.picks_done,
            )
        };
        let manual = presses != seen_presses;
        seen_presses = presses;
        let in_game = phase == Phase::InGame;
        if !in_game && !manual {
            // Free the OCR engine outside games.
            ocr = None;
            waiting = None;
            // The first screen of the next game must not match the last screen of this one.
            last_closed = None;
            watch = CaptureWatch::default();
            memo = None;
            std::thread::sleep(Duration::from_secs(1));
            continue;
        }
        let due = live.picks_due();
        let pending = due > picks_done;
        if pending && !has_offer {
            if waiting.get_or_insert_with(Instant::now).elapsed() > WAIT_GIVE_UP {
                // A screen that OCR never saw (picked fast, or another UI scale), or
                // cards that the player opens later. The count is a guess from now on.
                let mut s = shared.lock().unwrap();
                s.picks_done = due;
                s.picks_guessed = true;
                drop(s);
                waiting = None;
                continue;
            }
        } else {
            waiting = None;
        }
        let mayhem = queue.is_some_and(is_mayhem);
        let armed = in_game && mayhem && live.level > 0;
        let likely = armed && (always || pending || has_offer);
        let since_scan = last_scan.map(|t| t.elapsed());
        if !(manual || scan_wanted(armed, always, pending, has_offer, picks_done, since_scan)) {
            std::thread::sleep(Duration::from_millis(500));
            continue;
        }
        let Some(sd) = sd else {
            std::thread::sleep(Duration::from_secs(1));
            continue;
        };
        // A locked screen, a secure prompt, or a disconnected remote session shows
        // nothing to capture. Say so, and skip the capture and the read.
        let desk = ocr::desk_state();
        if desk == Desk::Locked {
            watch = CaptureWatch::default();
            memo = None;
            set_capture(&shared, &ctx, CaptureHealth::Locked);
            std::thread::sleep(LOCKED_PAUSE);
            continue;
        }
        if matcher.as_ref().is_none_or(|(k, _)| !Arc::ptr_eq(k, &sd)) {
            // New names: an old reading no longer holds.
            memo = None;
            let names = sd
                .augments
                .iter()
                .flat_map(|(id, a)| [(*id, a.name.as_str()), (*id, a.name_en.as_str())]);
            matcher = Some((sd.clone(), Matcher::new(names)));
        }
        if ocr.is_none() {
            ocr = Ocr::new();
            if ocr.is_none() {
                shared.lock().unwrap().ocr_note = Some(Key::OcrUnavailable);
                std::thread::sleep(Duration::from_secs(30));
                continue;
            }
            shared.lock().unwrap().ocr_language = ocr.as_ref().map(|e| e.language().to_string());
        }
        let (Some(engine), Some((_, m))) = (&ocr, &matcher) else {
            continue;
        };
        let (overlay, window) = {
            let s = shared.lock().unwrap();
            (s.overlay_rect, s.window_rect)
        };
        // Only a plain heartbeat may skip the read: not a pick that is due, not
        // an offer that shows, not "all game", and not the scan hotkey.
        let must_read = manual || always || pending || has_offer;
        let (rec, kind) = scan_once(
            engine,
            m,
            manual,
            overlay,
            may_skip_read(must_read, last_read.map(|t| t.elapsed())),
            &mut memo,
        );
        last_scan = Some(rec.at);
        // A capture of the screen (F9 without a game window) says nothing about the game.
        let health = rec
            .shot
            .as_ref()
            .filter(|s| s.game_window)
            .map(|s| ocr::capture_state(desk, watch.step(&s.bgra, live.level)));
        if kind == ScanKind::Skipped {
            // No cards on this capture: keep the last read, update only the health of the capture.
            if let Some(h) = health {
                set_capture(&shared, &ctx, h);
            }
            std::thread::sleep(scan_pause(false, false, false));
            continue;
        }
        // A reused reading is no new full read: the safety timer of the heartbeat counts real reads.
        if kind == ScanKind::Read {
            last_read = Some(rec.at);
        }
        let found = rec.matched.clone();
        let ids: Vec<i64> = found.iter().map(|c| c.id).collect();
        let geometry = rec.shot.as_ref().and_then(|s| s.geometry());
        // The main window shows over the game: this scan would show Hexsight.
        // Keep the saved scan from before.
        let covered = window
            .zip(rec.shot.as_ref())
            .is_some_and(|(r, shot)| ocr::crop_overlaps(shot.crop, r));
        {
            let mut s = shared.lock().unwrap();
            if !covered {
                s.scan = Some(Arc::new(rec));
            }
            if let Some(h) = health.filter(|&h| h != s.capture) {
                s.capture = h;
                ctx.request_repaint();
            }
            if found.len() >= 2 {
                misses = 0;
                // The screen that shows keeps its number. Cards that came back (closed
                // without a pick, or a reroll) are the screen that closed.
                let open = s.offer.as_ref().map(|o| o.screen);
                let closed = last_closed.as_ref().map(|(c, n)| (c.as_slice(), *n));
                let screen = offer_screen(open, closed, &ids, s.picks_done, s.picks_guessed, due);
                // A move of the game window changes the badges, so it wakes the window too.
                if s.offer.as_ref().map(|o| (o.augment_ids(), o.geometry)) != Some((ids, geometry))
                {
                    ctx.request_repaint();
                }
                s.offer = Some(Offer {
                    cards: found,
                    screen,
                    geometry,
                });
            } else if s.offer.is_some() {
                misses += 1;
                // Cards gone: the player picked. Two misses avoid flicker.
                if misses >= 2 {
                    if let Some(o) = s.offer.take() {
                        s.picks_done = count_closed(s.picks_done, o.screen);
                        last_closed = Some((o.augment_ids(), o.screen));
                    }
                    ctx.request_repaint();
                }
            }
            if manual {
                let lang = s.lang;
                let msg = match &s.offer {
                    Some(o) => lang.fmt(Key::ScanRead, &[&o.cards.len()]),
                    None => lang.t(Key::ScanNoCards).into(),
                };
                s.say(msg);
                ctx.request_repaint();
            }
        }
        let cards = shared.lock().unwrap().offer.is_some();
        std::thread::sleep(scan_pause(
            cards,
            likely || manual,
            kind == ScanKind::Reused,
        ));
    }
}

/// Publish the capture state when it changed.
fn set_capture(shared: &Mutex<Shared>, ctx: &egui::Context, health: CaptureHealth) {
    let mut s = shared.lock().unwrap();
    if s.capture != health {
        s.capture = health;
        drop(s);
        ctx.request_repaint();
    }
}

/// The last OCR read of the title band, kept so that an equal band is not read
/// again (see `ocr::band_signature`).
struct BandMemo {
    sig: u64,
    /// The capture area and the overlay rect of that read. The positions of the
    /// cards and the dropped lines depend on them.
    crop: ocr::Crop,
    overlay: Option<[i32; 4]>,
    read_at: Instant,
    lines: Vec<Line>,
    dropped: usize,
    matched: Vec<Card>,
}

impl BandMemo {
    /// May this reading answer for a capture with signature `sig`? The band, the
    /// capture area and the overlay must be the same, and the read must be younger
    /// than `SIG_REUSE_MAX` (`age`).
    fn answers(&self, sig: u64, crop: ocr::Crop, overlay: Option<[i32; 4]>, age: Duration) -> bool {
        self.sig == sig && self.crop == crop && self.overlay == overlay && age < SIG_REUSE_MAX
    }
}

/// What one scan did with its capture.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ScanKind {
    /// No cards on the capture (the cheap check): not read.
    Skipped,
    /// The OCR engine read it.
    Read,
    /// The title band did not change: the last reading stands, no OCR ran.
    Reused,
}

/// One scan: capture the game area and read the cards (see `Ocr::read_cards`).
/// `overlay` is the screen rect of the overlay while it shows in recordings.
/// With `may_skip`, a capture that the cheap check says has no cards is not
/// read. The answer says what happened to the capture (`ScanKind`). A capture
/// that failed is never skipped: its record says so, as before.
///
/// Before OCR, the signature of the title band is compared with the one of
/// `memo`, the last read. An equal band is the same pick screen, so the last
/// reading is reused and OCR does not run. A reroll or a pick changes the band
/// and is read at once. The scan hotkey (`fallback`) never reuses: it reads.
/// Only a reading with two or more cards, from the band alone, is kept.
fn scan_once(
    engine: &Ocr,
    m: &Matcher,
    fallback: bool,
    overlay: Option<[i32; 4]>,
    may_skip: bool,
    memo: &mut Option<BandMemo>,
) -> (ScanRecord, ScanKind) {
    let shot = ocr::capture_shot(fallback);
    if may_skip
        && shot
            .as_ref()
            .is_some_and(|s| !ocr::cards_visible(&s.bgra, s.crop.w as u32, s.crop.h as u32))
    {
        let rec = ScanRecord {
            at: Instant::now(),
            shot,
            lines: Vec::new(),
            dropped: 0,
            matched: Vec::new(),
            engine_failed: false,
        };
        return (rec, ScanKind::Skipped);
    }
    let crop = shot.as_ref().map(|s| s.crop);
    let sig = shot
        .as_ref()
        .and_then(|s| ocr::band_signature(&s.bgra, s.crop.w as u32, s.crop.h as u32));
    if let (Some(sig), Some(crop), false) = (sig, crop, fallback)
        && let Some(old) = memo
            .as_ref()
            .filter(|o| o.answers(sig, crop, overlay, o.read_at.elapsed()))
    {
        let rec = ScanRecord {
            at: Instant::now(),
            shot,
            lines: old.lines.clone(),
            dropped: old.dropped,
            matched: old.matched.clone(),
            engine_failed: false,
        };
        return (rec, ScanKind::Reused);
    }
    let reading = shot.as_ref().map(|s| engine.read_cards(s, m, overlay));
    // Keep this reading for the next scan, or forget the old one.
    *memo = match (&reading, sig, crop) {
        (Some(r), Some(sig), Some(crop))
            if r.from_band && !r.engine_failed && r.matched.len() >= 2 =>
        {
            Some(BandMemo {
                sig,
                crop,
                overlay,
                read_at: Instant::now(),
                lines: r.lines.clone(),
                dropped: r.dropped,
                matched: r.matched.clone(),
            })
        }
        _ => None,
    };
    let (lines, dropped, matched, engine_failed) = match reading {
        Some(r) => (r.lines, r.dropped, r.matched, r.engine_failed),
        None => (Vec::new(), 0, Vec::new(), false),
    };
    (
        ScanRecord {
            at: Instant::now(),
            shot,
            lines,
            dropped,
            matched,
            engine_failed,
        },
        ScanKind::Read,
    )
}

/// May the loop skip the read of a capture that shows no cards? Only a plain
/// heartbeat may (`must_read` is false), and only while the last full read is
/// younger than `FULL_READ_EVERY`. With no read yet, the answer is no.
fn may_skip_read(must_read: bool, since_read: Option<Duration>) -> bool {
    !must_read && since_read.is_some_and(|t| t < FULL_READ_EVERY)
}

/// Should the card reader scan now? `armed` means a Mayhem game with a known
/// level. While a pick is due, cards show, or the setting "all game" is on, the
/// answer is yes. Else the answer is yes once every `HEARTBEAT`, while picks
/// are left: a pick screen can come at a level that nobody confirmed.
fn scan_wanted(
    armed: bool,
    always: bool,
    pending: bool,
    has_offer: bool,
    picks_done: usize,
    since_scan: Option<Duration>,
) -> bool {
    if !armed {
        return false;
    }
    always
        || pending
        || has_offer
        || (picks_done < MAX_PICKS && since_scan.is_none_or(|t| t >= HEARTBEAT))
}

/// Pause after a scan: while cards show, 0.6 s, or 0.3 s when the last scan
/// was answered by the band signature (`steady`, see `CARDS_PAUSE_STEADY`).
/// Else 1.2 s while a pick is due (or after the scan hotkey), else the 0.5 s of
/// the idle check. A heartbeat scan then waits there until the next heartbeat.
fn scan_pause(cards: bool, busy: bool, steady: bool) -> Duration {
    if cards {
        if steady {
            CARDS_PAUSE_STEADY
        } else {
            CARDS_PAUSE
        }
    } else if busy {
        Duration::from_millis(1200)
    } else {
        Duration::from_millis(500)
    }
}

/// The number of the pick screen whose cards show now (1..=`MAX_PICKS`).
/// - `open` is the screen of the cards that show already. It keeps its number
///   while they show, also when the level goes up.
/// - `closed` is the last screen that closed: its cards and its number. When two
///   or more of these cards come back, the player closed the cards without a
///   pick, or a reroll hid them for two scans. It is the same screen.
/// - Else it is the screen after `done`. After a give-up in this game
///   (`guessed`), the count can be one too high, because the player can open
///   the cards late. The number then stays at or below `due`, the screens due by level.
fn offer_screen(
    open: Option<usize>,
    closed: Option<(&[i64], usize)>,
    ids: &[i64],
    done: usize,
    guessed: bool,
    due: usize,
) -> usize {
    if let Some(screen) = open {
        return screen;
    }
    if let Some((_, screen)) =
        closed.filter(|(cards, _)| ids.iter().filter(|i| cards.contains(i)).count() >= 2)
    {
        return screen;
    }
    let cap = if guessed && due > 0 { due } else { MAX_PICKS };
    (done + 1).min(cap).clamp(1, MAX_PICKS)
}

/// The count after the cards of `screen` closed. A screen that the count has
/// already (cards opened late after a give-up, or cards that came back) does
/// not count again.
fn count_closed(done: usize, screen: usize) -> usize {
    done.max(screen)
}

/// Finds the new presses of the two hotkeys. A press is a key that goes down.
#[derive(Default)]
struct KeyEdges {
    keys: Keys,
    was: (bool, bool),
}

impl KeyEdges {
    /// Feed the key states of one pass. Returns which hotkeys were pushed.
    /// When the hotkeys change, a new key can be down already (the user just
    /// pushed it to bind it). That is no press.
    fn step(&mut self, keys: Keys, now: (bool, bool)) -> (bool, bool) {
        if keys != self.keys {
            self.keys = keys;
            self.was = now;
        }
        let pressed = (now.0 && !self.was.0, now.1 && !self.was.1);
        self.was = now;
        pressed
    }
}

fn key_loop(shared: Arc<Mutex<Shared>>, ctx: egui::Context) {
    let mut edges = KeyEdges::default();
    loop {
        // The Settings page can change the keys at any time.
        let (keys, rebinding) = {
            let s = shared.lock().unwrap();
            (s.keys, s.rebinding)
        };
        let ctrl = win::key_down(0x11);
        let (a, b) = edges.step(
            keys,
            (win::key_down(keys.overlay), win::key_down(keys.scan)),
        );
        let in_game = {
            let mut s = shared.lock().unwrap();
            // While the Settings page waits for a new key, a push is no command.
            if a && !rebinding {
                if ctrl {
                    s.lock_presses += 1
                } else {
                    s.overlay_presses += 1
                }
                ctx.request_repaint();
            }
            if b && !rebinding {
                s.scan_presses += 1;
            }
            s.phase == Phase::InGame
        };
        // Fast in game, slow elsewhere.
        std::thread::sleep(Duration::from_millis(if in_game { 40 } else { 100 }));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn picks_due_follows_level() {
        let at = |level| {
            Live {
                level,
                ..Default::default()
            }
            .picks_due()
        };
        assert_eq!(at(0), 0);
        // The first pick is at level 3, the same as the pick card says.
        assert_eq!(at(1), 0);
        assert_eq!(at(2), 0);
        assert_eq!(at(3), 1);
        assert_eq!(at(6), 1);
        assert_eq!(at(7), 2);
        assert_eq!(at(11), 3);
        assert_eq!(at(18), 4);
    }

    #[test]
    fn scan_waits_for_a_mayhem_game_then_follows_the_pick_state() {
        let ago = |s: u64| Some(Duration::from_secs(s));
        // Outside a Mayhem game with a known level nothing scans.
        assert!(!scan_wanted(false, true, true, true, 0, None));
        // A pick is due, or cards show, or "all game" is on: scan.
        assert!(scan_wanted(true, false, true, false, 0, ago(0)));
        assert!(scan_wanted(true, false, false, true, 2, ago(0)));
        assert!(scan_wanted(true, true, false, false, 4, ago(0)));
        // Nothing due: a heartbeat every 1.5 seconds while picks are left.
        assert!(!scan_wanted(
            true,
            false,
            false,
            false,
            1,
            Some(Duration::from_millis(1400))
        ));
        assert!(scan_wanted(
            true,
            false,
            false,
            false,
            1,
            Some(Duration::from_millis(1500))
        ));
        assert!(scan_wanted(true, false, false, false, 1, None));
        // A Headliner can add a fifth pick, so the heartbeat continues.
        assert!(scan_wanted(true, false, false, false, 4, ago(60)));
        // After the sixth pick the heartbeat stops.
        assert!(!scan_wanted(true, false, false, false, MAX_PICKS, ago(60)));
    }

    #[test]
    fn scan_pause_keeps_the_known_rates() {
        assert_eq!(scan_pause(true, true, false), Duration::from_millis(600));
        assert_eq!(scan_pause(true, false, false), Duration::from_millis(600));
        assert_eq!(scan_pause(false, true, false), Duration::from_millis(1200));
        assert_eq!(scan_pause(false, false, false), Duration::from_millis(500));
        assert_eq!(HEARTBEAT, Duration::from_millis(1500));
        assert_eq!(WAIT_GIVE_UP, Duration::from_secs(45));
    }

    #[test]
    fn the_pause_halves_only_while_cards_show_and_the_band_is_steady() {
        // A scan that the band signature answered: the next one comes in 300 ms.
        assert_eq!(scan_pause(true, true, true), Duration::from_millis(300));
        assert_eq!(scan_pause(true, false, true), Duration::from_millis(300));
        // A real read keeps the old 600 ms, so an animated band costs what it cost before.
        assert_eq!(scan_pause(true, true, false), Duration::from_millis(600));
        // Without cards, the steady flag changes nothing.
        assert_eq!(scan_pause(false, true, true), Duration::from_millis(1200));
        assert_eq!(scan_pause(false, false, true), Duration::from_millis(500));
        assert_eq!(SIG_REUSE_MAX, Duration::from_secs(10));
    }

    #[test]
    fn the_faster_pause_costs_less_than_the_old_rate_for_a_steady_band() {
        // CPU share of one core while cards show, with a capture of `c` ms, a signature of `s` ms
        // and a read of 90 ms (debug build, 1536x712).
        let (s, read) = (0.5, 90.0);
        let span = SIG_REUSE_MAX.as_secs_f64() * 1000.0;
        let (slow, fast) = (
            CARDS_PAUSE.as_secs_f64() * 1000.0,
            CARDS_PAUSE_STEADY.as_secs_f64() * 1000.0,
        );
        for c in [2.0, 5.0, 20.0, 50.0] {
            let old = (c + read) / (slow + c + read);
            // New, over one reuse window: one real read and its 600 ms pause, then scans every 300 ms
            // that cost only a capture and a signature.
            let first = c + s + read + slow;
            let hits = ((span - first) / (fast + c + s)).floor();
            let new = (c + s + read + hits * (c + s)) / span;
            assert!(
                new < old,
                "capture {c} ms: new {:.1} %, old {:.1} %",
                new * 100.0,
                old * 100.0
            );
        }
        // A band that never repeats (an animation) reads each time with the old pause. The only
        // extra cost is the signature: well under 0.1 % of one core.
        let c = 5.0;
        let old = (c + read) / (slow + c + read);
        let animated = (c + s + read) / (slow + c + s + read);
        assert!(animated - old < 0.001, "{:.4} against {:.4}", animated, old);
    }

    /// A memo of a read made now, for the capture area `crop`.
    fn memo_of(sig: u64, crop: ocr::Crop, overlay: Option<[i32; 4]>) -> BandMemo {
        BandMemo {
            sig,
            crop,
            overlay,
            read_at: Instant::now(),
            lines: Vec::new(),
            dropped: 0,
            matched: Vec::new(),
        }
    }

    #[test]
    fn an_equal_band_reuses_the_last_reading_and_a_changed_one_does_not() {
        let crop = ocr::Crop {
            x: 192,
            y: 130,
            w: 1536,
            h: 712,
        };
        let memo = memo_of(42, crop, None);
        let young = Duration::from_secs(1);
        assert!(memo.answers(42, crop, None, young));
        // A reroll or a pick: another signature, read at once.
        assert!(!memo.answers(43, crop, None, young));
        // The game window moved or changed size: the positions differ.
        assert!(!memo.answers(42, ocr::Crop { x: 200, ..crop }, None, young));
        // The overlay moved over or away from the capture: the dropped lines can differ.
        assert!(!memo.answers(42, crop, Some([0, 0, 100, 100]), young));
        assert!(!memo_of(42, crop, Some([0, 0, 100, 100])).answers(
            42,
            crop,
            Some([0, 0, 120, 100]),
            young
        ));
        assert!(memo_of(42, crop, Some([0, 0, 100, 100])).answers(
            42,
            crop,
            Some([0, 0, 100, 100]),
            young
        ));
    }

    #[test]
    fn a_reading_is_reused_for_ten_seconds_at_most() {
        let crop = ocr::Crop {
            x: 0,
            y: 0,
            w: 1536,
            h: 712,
        };
        let memo = memo_of(7, crop, None);
        assert!(memo.answers(7, crop, None, Duration::from_millis(9_999)));
        assert!(!memo.answers(7, crop, None, SIG_REUSE_MAX));
        assert!(!memo.answers(7, crop, None, Duration::from_secs(60)));
    }

    #[test]
    fn only_a_plain_heartbeat_may_skip_the_read() {
        let ago = |s: u64| Some(Duration::from_secs(s));
        // A heartbeat after a recent read: skip when there are no cards.
        assert!(may_skip_read(false, ago(0)));
        assert!(may_skip_read(false, ago(29)));
        // A pick that is due, an offer, "all game" or the hotkey (must_read): always read.
        assert!(!may_skip_read(true, ago(0)));
        assert!(!may_skip_read(true, ago(29)));
        // The safety: a full read at least every 30 seconds, and at the first heartbeat.
        assert!(!may_skip_read(false, ago(30)));
        assert!(!may_skip_read(false, ago(600)));
        assert!(!may_skip_read(false, None));
    }

    #[test]
    fn the_faster_heartbeat_costs_less_than_the_old_one() {
        // Per second, in ms of CPU, with a capture of `c` ms and a read of 200 ms.
        // Old: capture and read each 5 s. New: capture each 1.5 s, one read each 30 s.
        let (read, old_s, new_s, safety_s) = (
            200.0,
            5.0,
            HEARTBEAT.as_secs_f64(),
            FULL_READ_EVERY.as_secs_f64(),
        );
        for c in [2.0, 5.0, 20.0, 50.0] {
            let old = (c + read) / old_s;
            let new = c / new_s + read / safety_s;
            assert!(
                new < old,
                "capture {c} ms: new {new:.1} ms/s, old {old:.1} ms/s"
            );
        }
    }

    #[test]
    fn reads_window_mode_from_game_cfg() {
        // Shape of a real game.cfg: a blank first line, several sections.
        let cfg = "\n[FloatingText]\nWindowMode=7\n\n[General]\nCfgVersion=16.21.829.8620\nHeight=1080\nWindowMode=2\nWidth=1920\n";
        assert_eq!(parse_window_mode(cfg), Some(2));
        assert_eq!(
            parse_window_mode("[General]\r\nWindowMode = 0\r\n"),
            Some(0)
        );
        assert_eq!(
            parse_window_mode("\u{feff}[general]\nwindowmode=1\n"),
            Some(1)
        );
    }

    #[test]
    fn window_mode_needs_the_general_section() {
        assert_eq!(parse_window_mode(""), None);
        assert_eq!(parse_window_mode("[General]\nHeight=1080\n"), None);
        // The key in another section does not count.
        assert_eq!(
            parse_window_mode("[Other]\nWindowMode=0\n[General]\nHeight=1\n"),
            None
        );
        assert_eq!(parse_window_mode("[General]\nWindowMode=abc\n"), None);
    }

    #[test]
    fn offer_ids_ignore_the_card_position() {
        let card = |id, x| Card {
            id,
            x,
            y: 0.48,
            h: 0.03,
        };
        let o = Offer {
            cards: vec![card(5, 0.2), card(7, 0.5), card(9, 0.8)],
            screen: 2,
            geometry: None,
        };
        assert_eq!(o.augment_ids(), vec![5, 7, 9]);
    }

    #[test]
    fn a_finished_game_gets_five_tries_over_two_minutes() {
        assert_eq!(GAME_TRIES.len(), 5);
        // The last try runs after the sum of all pauses: more than two minutes.
        assert!(GAME_TRIES.iter().sum::<Duration>() >= Duration::from_secs(120));
        // The pauses grow, so a late game still has a try.
        assert!(GAME_TRIES.windows(2).all(|w| w[0] < w[1]));
        // The first import waits for a signed-in client for five minutes.
        assert_eq!(IMPORT_PAUSE * IMPORT_TRIES as u32, Duration::from_secs(300));
        assert_eq!(IMPORT_GAMES, 20);
    }

    #[test]
    fn a_hotkey_press_is_a_key_that_goes_down() {
        let keys = Keys {
            overlay: 0x77,
            scan: 0x78,
        };
        let mut e = KeyEdges::default();
        // The first pass learns the keys. F8 is down already: no press.
        assert_eq!(e.step(keys, (true, false)), (false, false));
        // F8 goes up, then down: one press. Held: no second press.
        assert_eq!(e.step(keys, (false, false)), (false, false));
        assert_eq!(e.step(keys, (true, false)), (true, false));
        assert_eq!(e.step(keys, (true, false)), (false, false));
        // The user binds the overlay to F5, and F5 is still down: no press.
        let moved = Keys {
            overlay: 0x74,
            scan: 0x78,
        };
        assert_eq!(e.step(moved, (true, false)), (false, false));
        assert_eq!(e.step(moved, (true, false)), (false, false));
        // Up and down again: both keys give a press.
        assert_eq!(e.step(moved, (false, false)), (false, false));
        assert_eq!(e.step(moved, (true, true)), (true, true));
    }

    #[test]
    fn a_new_message_waits_while_the_last_one_has_just_shown() {
        let ago = |d: Duration| Instant::now().checked_sub(d);
        let mut s = Shared::default();
        s.say("trade accepted".into());
        s.say("build sent".into());
        // The first message shows for 1.5 seconds at least. The second one waits.
        assert_eq!(s.last_action.as_deref(), Some("trade accepted"));
        assert_eq!(s.later.len(), 1);
        assert!(!s.next_toast());
        s.action_at = ago(TOAST_MIN);
        assert!(s.next_toast());
        assert_eq!(s.last_action.as_deref(), Some("build sent"));
        // Nothing waits any more, and the new message has its own 1.5 seconds.
        assert!(!s.next_toast());
        assert!(s.action_at.is_some_and(|t| t.elapsed() < TOAST_MIN));
        // After a pause, a message shows at once.
        s.action_at = ago(Duration::from_secs(10));
        s.say("later".into());
        assert_eq!(
            (s.last_action.as_deref(), s.later.len()),
            (Some("later"), 0)
        );
    }

    #[test]
    fn only_a_few_messages_wait() {
        let mut s = Shared::default();
        for n in 0..10 {
            s.say(format!("message {n}"));
        }
        // The first one shows. The newest `TOAST_QUEUE` of the others wait, in order.
        assert_eq!(s.last_action.as_deref(), Some("message 0"));
        let waiting: Vec<&str> = s.later.iter().map(|(m, _)| m.as_str()).collect();
        assert_eq!(waiting, vec!["message 7", "message 8", "message 9"]);
    }

    #[test]
    fn a_raw_error_goes_with_its_message_and_only_with_it() {
        let ago = |d: Duration| Instant::now().checked_sub(d);
        let mut s = Shared::default();
        s.say_with_detail(
            "swap failed: League didn't allow it".into(),
            Some("http status: 404".into()),
        );
        assert_eq!(s.last_detail.as_deref(), Some("http status: 404"));
        // The next message waits, and it has no raw error. The detail goes with its own message.
        s.say("build sent".into());
        assert_eq!(s.last_detail.as_deref(), Some("http status: 404"));
        s.action_at = ago(TOAST_MIN);
        assert!(s.next_toast());
        assert_eq!(
            (s.last_action.as_deref(), s.last_detail.as_deref()),
            (Some("build sent"), None)
        );
        // A waiting message keeps its detail too.
        s.say_with_detail("trade failed".into(), Some("timeout".into()));
        s.action_at = ago(TOAST_MIN);
        assert!(s.next_toast());
        assert_eq!(
            (s.last_action.as_deref(), s.last_detail.as_deref()),
            (Some("trade failed"), Some("timeout"))
        );
    }

    #[test]
    fn the_cards_of_a_screen_are_noted_once_and_cleared_at_game_end() {
        let mut s = Shared::default();
        s.note_offered(1, &[10, 11, 12]);
        s.note_offered(2, &[20, 21, 22]);
        // The same cards again: nothing changes.
        s.note_offered(2, &[20, 21, 22]);
        assert_eq!(
            s.offered,
            vec![(1, vec![10, 11, 12]), (2, vec![20, 21, 22])]
        );
        // A new scan of the same screen replaces its entry.
        s.note_offered(1, &[10, 13, 12]);
        assert_eq!(
            s.offered,
            vec![(1, vec![10, 13, 12]), (2, vec![20, 21, 22])]
        );
        s.picks_guessed = true;
        s.end_game();
        assert!(s.offered.is_empty());
        assert!(!s.picks_guessed);
    }

    // ---- Stats of the own champion: retries and the text of the window.

    /// Shared state with a stats index and this own champion.
    fn with_stats(mine: Option<i64>) -> Shared {
        Shared {
            stats: Some(Arc::new(Stats::of_champions(HashMap::new()))),
            my_champion: mine,
            ..Default::default()
        }
    }

    fn ago(d: Duration) -> Instant {
        Instant::now().checked_sub(d).unwrap()
    }

    #[test]
    fn no_stats_means_a_recent_failure_and_no_details() {
        let mut s = with_stats(Some(74));
        assert_eq!(s.no_stats(74), None);
        // A failed try: the window says that Hexsight tries again.
        s.failed.insert(74, (Instant::now(), NoStats::Transient));
        assert_eq!(s.no_stats(74), Some(NoStats::Transient));
        // After the pause a new try runs. The text stays: no "Loading" once a minute
        // while the network is down.
        s.failed.insert(74, (ago(STATS_RETRY), NoStats::Transient));
        assert!(s.wants_fetch(74));
        assert_eq!(s.no_stats(74), Some(NoStats::Transient));
        // A missing champion stays missing until the next stats index.
        s.failed
            .insert(74, (ago(Duration::from_secs(3600)), NoStats::Missing));
        assert_eq!(s.no_stats(74), Some(NoStats::Missing));
        // Old details from before a refresh still show, so nothing is missing.
        s.old_details.insert(74, Arc::new(ChampStats::default()));
        assert_eq!(s.no_stats(74), None);
    }

    #[test]
    fn the_own_champion_loads_first_and_again_after_the_pause() {
        let mut s = with_stats(Some(74));
        let mut queue = VecDeque::from([5, 74, 6]);
        assert_eq!(s.next_fetch(&mut queue), Some(74));
        // The own champion left the queue. The others follow in order.
        assert_eq!(queue, VecDeque::from([5, 6]));
        // A failed try waits for `STATS_RETRY`. The bench champions go on meanwhile.
        s.failed.insert(74, (Instant::now(), NoStats::Transient));
        assert_eq!(s.next_fetch(&mut queue), Some(5));
        s.failed.insert(74, (ago(STATS_RETRY), NoStats::Transient));
        assert_eq!(s.next_fetch(&mut queue), Some(74));
        // A missing champion and a champion with details need no fetch, so they leave the queue.
        s.failed.insert(74, (ago(STATS_RETRY), NoStats::Missing));
        s.details.insert(6, Arc::new(ChampStats::default()));
        assert_eq!(s.next_fetch(&mut queue), None);
        assert!(queue.is_empty());
    }

    #[test]
    fn requests_wait_for_the_stats_index() {
        let mut s = Shared {
            my_champion: Some(74),
            ..Default::default()
        };
        let mut queue = VecDeque::from([5]);
        assert_eq!(s.next_fetch(&mut queue), None);
        assert_eq!(queue, VecDeque::from([5]));
        s.stats = Some(Arc::new(Stats::of_champions(HashMap::new())));
        assert_eq!(s.next_fetch(&mut queue), Some(74));
        // The own champion stays first until its try has a result.
        assert_eq!(s.next_fetch(&mut queue), Some(74));
        let index = s.stats.clone().unwrap();
        assert!(s.store_fetch(&index, 74, Ok(ChampStats::default())));
        assert_eq!(s.next_fetch(&mut queue), Some(5));
    }

    #[test]
    fn a_result_from_an_old_index_is_dropped_without_a_mark() {
        let mut s = with_stats(Some(74));
        let old = s.stats.clone().unwrap();
        // The stats refresh replaces the index during the download.
        s.stats = Some(Arc::new(Stats::of_champions(HashMap::new())));
        assert!(!s.store_fetch(&old, 74, Err(NoStats::Transient)));
        assert!(!s.store_fetch(&old, 5, Ok(ChampStats::default())));
        assert!(s.failed.is_empty() && s.details.is_empty());
        // The champion is still wanted, so the fetch thread asks again with the new index.
        let mut queue = VecDeque::from([5]);
        assert_eq!(s.next_fetch(&mut queue), Some(74));
        let new = s.stats.clone().unwrap();
        assert!(s.store_fetch(&new, 74, Err(NoStats::Transient)));
        assert_eq!(s.next_fetch(&mut queue), Some(5));
    }

    #[test]
    fn a_load_that_works_lets_the_own_champion_try_again_at_once() {
        let mut s = with_stats(Some(74));
        let index = s.stats.clone().unwrap();
        assert!(s.store_fetch(&index, 74, Err(NoStats::Transient)));
        assert_eq!(s.no_stats(74), Some(NoStats::Transient));
        assert!(!s.wants_fetch(74));
        // A bench champion loads: the network can be back.
        assert!(s.store_fetch(&index, 5, Ok(ChampStats::default())));
        assert!(s.wants_fetch(74));
        assert_eq!(s.no_stats(74), None);
        // A missing own champion stays missing.
        assert!(s.store_fetch(&index, 74, Err(NoStats::Missing)));
        assert!(s.store_fetch(&index, 6, Ok(ChampStats::default())));
        assert_eq!(s.no_stats(74), Some(NoStats::Missing));
        // Its own details clear its mark and the old details.
        s.old_details.insert(74, Arc::new(ChampStats::default()));
        assert!(s.store_fetch(&index, 74, Ok(ChampStats::default())));
        assert!(s.failed.is_empty() && s.old_details.is_empty() && s.details.contains_key(&74));
    }

    #[test]
    fn the_champ_select_champion_stays_for_its_game() {
        // Champ select sets it, and a swap changes it. A pass without an answer keeps it.
        let mut picked = keep_picked(Phase::ChampSelect, Some(74), None);
        assert_eq!(picked, Some(74));
        picked = keep_picked(Phase::ChampSelect, Some(6), picked);
        assert_eq!(picked, Some(6));
        picked = keep_picked(Phase::ChampSelect, None, picked);
        assert_eq!(picked, Some(6));
        // The game keeps it, also while the client restarts.
        picked = keep_picked(Phase::InGame, None, picked);
        picked = keep_picked(Phase::Offline, None, picked);
        assert_eq!(picked, Some(6));
        // After the game, and in a lobby, it is gone.
        for phase in [Phase::PostGame, Phase::Lobby, Phase::Idle] {
            assert_eq!(keep_picked(phase, None, Some(6)), None);
        }
        // A game without a champ select (Hexsight started during it) has none.
        assert_eq!(keep_picked(Phase::InGame, None, None), None);
    }

    #[test]
    fn a_session_without_a_champion_clears_the_champion() {
        // The player has no champion yet (champion 0): none is picked, even with one in memory.
        assert_eq!(keep_picked(Phase::ChampSelect, Some(0), None), None);
        assert_eq!(keep_picked(Phase::ChampSelect, Some(0), Some(76)), None);
        // A session that does not answer keeps the last one. The first pass has none to keep.
        assert_eq!(keep_picked(Phase::ChampSelect, None, Some(76)), Some(76));
        assert_eq!(keep_picked(Phase::ChampSelect, None, None), None);
        // After the player picks, the champion is back.
        assert_eq!(keep_picked(Phase::ChampSelect, Some(51), None), Some(51));
    }

    #[test]
    fn a_new_champ_select_forgets_the_champion_of_the_game_before() {
        // The last game was Swain (50). The lobby forgets the picked champion, and champ select starts.
        let swain = Some(50);
        // The first pass of the champ select: the session may not answer yet. The old champion goes at once.
        assert_eq!(
            next_champion(Phase::ChampSelect, true, false, None, swain),
            None
        );
        // The session answers with champion 0: none, and it stays none on the next passes.
        assert_eq!(
            next_champion(Phase::ChampSelect, false, true, None, swain),
            None
        );
        assert_eq!(
            next_champion(Phase::ChampSelect, false, true, None, None),
            None
        );
        // One failed read in the middle of the champ select keeps what is there.
        assert_eq!(
            next_champion(Phase::ChampSelect, false, false, None, Some(51)),
            Some(51)
        );
        // The player picks Camille (164): the new champion shows.
        assert_eq!(
            next_champion(Phase::ChampSelect, false, true, Some(164), None),
            Some(164)
        );
        assert_eq!(
            next_champion(Phase::ChampSelect, true, true, Some(164), swain),
            Some(164)
        );
        // Outside the champ select the last champion stays (the result card of the game, the live game).
        for phase in [
            Phase::InGame,
            Phase::PostGame,
            Phase::Lobby,
            Phase::Idle,
            Phase::Offline,
        ] {
            assert_eq!(
                next_champion(phase, false, false, None, swain),
                swain,
                "{phase:?}"
            );
        }
        assert_eq!(
            next_champion(Phase::InGame, false, false, Some(7), swain),
            Some(7)
        );
    }

    #[test]
    fn a_failed_session_read_keeps_the_queue_and_a_real_value_replaces_it() {
        // A Ranked game from the lobby on: one pass whose session call fails (`None`) keeps 420.
        let mut queue = next_queue(Phase::Lobby, Some(420), None);
        assert_eq!(queue, Some(420));
        queue = next_queue(Phase::ChampSelect, None, queue);
        assert_eq!(queue, Some(420));
        queue = next_queue(Phase::ChampSelect, Some(420), queue);
        queue = next_queue(Phase::InGame, None, queue);
        assert_eq!(queue, Some(420));
        // Each phase with a game: a failed read keeps the queue, a real value replaces it, and a first failed read stays unknown.
        for phase in [Phase::Lobby, Phase::ChampSelect, Phase::InGame] {
            assert_eq!(next_queue(phase, None, Some(420)), Some(420), "{phase:?}");
            assert_eq!(
                next_queue(phase, Some(2400), Some(420)),
                Some(2400),
                "{phase:?}"
            );
            assert_eq!(next_queue(phase, Some(2400), None), Some(2400), "{phase:?}");
            assert_eq!(next_queue(phase, None, None), None, "{phase:?}");
        }
    }

    #[test]
    fn the_queue_of_a_game_stays_through_the_end_of_the_game_and_goes_with_the_idle_client() {
        // A Ranked game ends: the end-of-game phase reads no session, and the window still knows the queue.
        // Else the result card waits for a result that never comes (only a Mayhem game is saved).
        let mut queue = next_queue(Phase::InGame, Some(420), None);
        queue = next_queue(Phase::PostGame, None, queue);
        assert_eq!(queue, Some(420));
        assert!(!is_mayhem(queue.unwrap()));
        // A Mayhem game keeps its queue too, and the next lobby (another queue) replaces it by its own read.
        let mut queue = next_queue(Phase::PostGame, None, Some(2400));
        assert_eq!(queue, Some(2400));
        queue = next_queue(Phase::Lobby, Some(420), queue);
        assert_eq!(queue, Some(420));
        // The end of a game that Hexsight did not see has no queue to keep.
        assert_eq!(next_queue(Phase::PostGame, None, None), None);
        // A client with no game (idle, closed) forgets the queue, whatever it holds or reads.
        for phase in [Phase::Idle, Phase::Offline] {
            assert_eq!(next_queue(phase, None, Some(420)), None, "{phase:?}");
            assert_eq!(next_queue(phase, Some(420), Some(420)), None, "{phase:?}");
        }
    }

    #[test]
    fn the_offered_champions_are_asked_for_only_in_the_first_step_of_a_mayhem_champ_select() {
        assert!(wants_choices(0, Some(2400)));
        assert!(wants_choices(0, Some(3270)));
        // A champion is picked: no request.
        assert!(!wants_choices(164, Some(2400)));
        // Another queue (draft, normal ARAM) has no such step: no extra request.
        assert!(!wants_choices(0, Some(450)));
        assert!(!wants_choices(0, Some(420)));
        assert!(!wants_choices(0, None));
    }

    #[test]
    fn the_retry_pauses_are_short_enough_for_a_game() {
        assert_eq!(STATS_RETRY, Duration::from_secs(60));
        // The own champion is looked at much more often than it is tried.
        assert!(FETCH_TICK * 10 <= STATS_RETRY);
    }

    /// The pick count of `ocr_loop` without a screen: the level, the wait that
    /// gives up, and cards that show and close. It calls the same functions as the loop.
    #[derive(Default)]
    struct Game {
        level: i64,
        done: usize,
        guessed: bool,
        offer: Option<(Vec<i64>, usize)>,
        closed: Option<(Vec<i64>, usize)>,
    }

    impl Game {
        fn due(&self) -> usize {
            Live {
                level: self.level,
                ..Default::default()
            }
            .picks_due()
        }

        /// `WAIT_GIVE_UP` passes without cards.
        fn wait(&mut self) {
            if self.due() > self.done && self.offer.is_none() {
                self.done = self.due();
                self.guessed = true;
            }
        }

        /// A scan reads these cards. Returns the screen number.
        fn show(&mut self, ids: [i64; 3]) -> usize {
            let closed = self.closed.as_ref().map(|(c, n)| (c.as_slice(), *n));
            let screen = offer_screen(
                self.offer.as_ref().map(|o| o.1),
                closed,
                &ids,
                self.done,
                self.guessed,
                self.due(),
            );
            self.offer = Some((ids.to_vec(), screen));
            screen
        }

        /// Two scans without cards.
        fn close(&mut self) {
            if let Some((ids, screen)) = self.offer.take() {
                self.done = count_closed(self.done, screen);
                self.closed = Some((ids, screen));
            }
        }

        /// The cards show, then close. Returns the screen number.
        fn pick(&mut self, ids: [i64; 3]) -> usize {
            let screen = self.show(ids);
            self.close();
            screen
        }
    }

    #[test]
    fn a_normal_game_counts_four_screens() {
        let mut g = Game::default();
        let mut screens = Vec::new();
        for (level, ids) in [
            (3, [1, 2, 3]),
            (7, [4, 5, 6]),
            (11, [7, 8, 9]),
            (15, [10, 11, 12]),
        ] {
            g.level = level;
            screens.push(g.pick(ids));
        }
        assert_eq!(screens, vec![1, 2, 3, 4]);
        assert_eq!(g.done, 4);
    }

    #[test]
    fn cards_opened_late_after_the_give_up_are_the_screen_that_was_given_up() {
        // The games of 24 October: at level 7 the cards open after 53 s, the wait gave up at 45 s.
        let mut g = Game {
            level: 3,
            ..Default::default()
        };
        assert_eq!(g.pick([1, 2, 3]), 1);
        g.level = 7;
        g.wait();
        assert_eq!(g.done, 2);
        assert_eq!(g.pick([4, 5, 6]), 2);
        // The late screen does not count again.
        assert_eq!(g.done, 2);
        g.level = 11;
        assert_eq!(g.pick([7, 8, 9]), 3);
        g.level = 15;
        assert_eq!(g.pick([10, 11, 12]), 4);
        assert_eq!(g.done, 4);
    }

    #[test]
    fn a_screen_that_nobody_saw_still_moves_the_count() {
        let mut g = Game {
            level: 3,
            ..Default::default()
        };
        g.pick([1, 2, 3]);
        // Picked fast at level 7: no scan saw the cards.
        g.level = 7;
        g.wait();
        g.level = 11;
        assert_eq!(g.pick([7, 8, 9]), 3);
        g.level = 15;
        g.wait();
        assert_eq!(g.done, 4);
    }

    #[test]
    fn cards_that_come_back_keep_their_screen() {
        // A reroll hides the cards for two scans. They come back with one new card.
        let mut g = Game {
            level: 3,
            ..Default::default()
        };
        g.pick([1, 2, 3]);
        g.level = 7;
        assert_eq!(g.pick([4, 5, 6]), 2);
        assert_eq!(g.pick([4, 5, 13]), 2);
        assert_eq!(g.done, 2);
        g.level = 11;
        assert_eq!(g.pick([7, 8, 9]), 3);
        // The same after a give-up: the late cards come back as the same screen.
        let mut g = Game {
            level: 3,
            ..Default::default()
        };
        g.pick([1, 2, 3]);
        g.level = 7;
        g.wait();
        assert_eq!(g.pick([4, 5, 6]), 2);
        assert_eq!(g.pick([4, 5, 6]), 2);
        assert_eq!(g.done, 2);
        g.level = 11;
        assert_eq!(g.pick([7, 8, 9]), 3);
    }

    #[test]
    fn a_screen_keeps_its_number_while_the_level_goes_up() {
        let mut g = Game {
            level: 3,
            ..Default::default()
        };
        g.pick([1, 2, 3]);
        g.level = 7;
        g.wait();
        g.level = 10;
        assert_eq!(g.show([4, 5, 6]), 2);
        g.level = 11;
        assert_eq!(g.show([4, 5, 6]), 2);
        g.close();
        assert_eq!(g.done, 2);
    }

    #[test]
    fn cards_of_level_7_opened_at_level_11_cost_one_screen() {
        // Two screens wait at level 11. The first gets the next number, the
        // second the number of the level. The count is right again after them.
        let mut g = Game {
            level: 3,
            ..Default::default()
        };
        g.pick([1, 2, 3]);
        g.level = 7;
        g.wait();
        g.level = 11;
        assert_eq!(g.pick([4, 5, 6]), 3);
        assert_eq!(g.pick([7, 8, 9]), 3);
        assert_eq!(g.done, 3);
        g.level = 15;
        assert_eq!(g.pick([10, 11, 12]), 4);
    }

    #[test]
    fn extra_screens_count_as_before_until_a_give_up() {
        // A screen at level 9 (a Headliner can add one): a game without a give-up counts it.
        let mut g = Game::default();
        let mut screens = Vec::new();
        for (level, ids) in [
            (3, [1, 2, 3]),
            (7, [4, 5, 6]),
            (9, [20, 21, 22]),
            (11, [7, 8, 9]),
            (15, [10, 11, 12]),
        ] {
            g.level = level;
            screens.push(g.pick(ids));
        }
        assert_eq!(screens, vec![1, 2, 3, 4, 5]);
        // After a give-up the count is a guess: the extra screen shares the number of the level.
        let mut g = Game {
            level: 3,
            ..Default::default()
        };
        g.pick([1, 2, 3]);
        g.level = 7;
        g.wait();
        g.level = 9;
        assert_eq!(g.pick([20, 21, 22]), 2);
        g.level = 11;
        assert_eq!(g.pick([7, 8, 9]), 3);
    }

    #[test]
    fn a_late_start_counts_the_screens_before_it() {
        // Hexsight starts at level 12. The level 11 cards still wait and open late.
        let mut g = Game {
            level: 12,
            ..Default::default()
        };
        g.wait();
        assert_eq!(g.done, 3);
        assert_eq!(g.pick([7, 8, 9]), 3);
        g.level = 15;
        assert_eq!(g.pick([10, 11, 12]), 4);
    }

    #[test]
    fn offer_screen_edges() {
        let ids = [1, 2, 3];
        // No level from the game (due 0): no cap, the screen after the count.
        assert_eq!(offer_screen(None, None, &ids, 2, true, 0), 3);
        assert_eq!(offer_screen(None, None, &ids, 0, false, 0), 1);
        // One card in common is a new screen, not cards that came back.
        assert_eq!(
            offer_screen(None, Some((&[1, 8, 9], 2)), &ids, 2, false, 3),
            3
        );
        assert_eq!(
            offer_screen(None, Some((&[1, 2, 9], 2)), &ids, 2, false, 3),
            2
        );
        // Never more than `MAX_PICKS`.
        assert_eq!(
            offer_screen(None, None, &ids, MAX_PICKS, false, 4),
            MAX_PICKS
        );
        assert_eq!(count_closed(MAX_PICKS, MAX_PICKS), MAX_PICKS);
    }

    #[test]
    fn the_fix_reports_what_the_client_took_and_what_game_cfg_says_after() {
        let ok: Result<(), String> = Ok(());
        // Borderless in the file: done. Another mode: the client took it, but the file did not follow.
        assert_eq!(fix_outcome(&ok, Some(2)), FixOutcome::Done);
        assert_eq!(fix_outcome(&ok, Some(0)), FixOutcome::Still);
        assert_eq!(fix_outcome(&ok, Some(1)), FixOutcome::Still);
        // The file cannot be read: sent, not confirmed.
        assert_eq!(fix_outcome(&ok, None), FixOutcome::Sent);
        // A refused call is a failure whatever the file says (it may still hold an old Borderless).
        for raw in [
            crate::lcu::NOT_CONNECTED,
            "http status: 400",
            "timeout: global",
        ] {
            assert_eq!(
                fix_outcome(&Err(raw.to_string()), Some(2)),
                FixOutcome::Failed,
                "{raw}"
            );
        }
    }

    #[test]
    fn the_mode_after_a_save_is_read_from_the_file_again() {
        let dir = std::env::temp_dir().join(format!("hexsight-test-fix-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(dir.join("Config")).unwrap();
        std::fs::write(
            dir.join("Config").join("game.cfg"),
            "[General]
WindowMode=2
",
        )
        .unwrap();
        assert_eq!(mode_after_save(&dir), Some(2));
        // No file: nothing to confirm.
        std::fs::remove_file(dir.join("Config").join("game.cfg")).unwrap();
        assert_eq!(mode_after_save(&dir), None);
        let _ = std::fs::remove_dir_all(&dir);
    }

    // ---- The suggestions: the player list, the advice, and the end of the game.

    #[test]
    fn the_roster_is_asked_every_four_seconds_and_at_once_after_a_purchase() {
        let secs = |s: u64| Some(Duration::from_secs(s));
        // The first pass of a game asks.
        assert!(roster_due(true, None, None, Some(500)));
        // After it, every four seconds.
        assert!(!roster_due(true, secs(3), Some(500), Some(520)));
        assert!(roster_due(true, secs(4), Some(500), Some(520)));
        assert_eq!(ROSTER_EVERY, Duration::from_secs(4));
        // A fall of 250 gold or more between two passes is a purchase: ask at once.
        assert!(roster_due(true, secs(1), Some(1300), Some(1050)));
        assert!(!roster_due(true, secs(1), Some(1300), Some(1051)));
        assert_eq!(PURCHASE_DROP, 250);
        // Gold that rises, or gold that is not known, is no purchase.
        assert!(!roster_due(true, secs(1), Some(100), Some(900)));
        assert!(!roster_due(true, secs(1), None, Some(10)));
        assert!(!roster_due(true, secs(1), Some(1000), None));
    }

    #[test]
    fn no_roster_is_asked_when_the_suggestions_are_off() {
        // With every suggestion setting off, no pass asks, whatever the time and the gold say.
        for since in [None, Some(Duration::from_secs(60))] {
            assert!(!roster_due(false, since, Some(2000), Some(10)));
        }
        assert!(!suggest_runs(Phase::InGame, Some(2400), false));
    }

    #[test]
    fn suggestions_run_only_in_a_mayhem_game() {
        for queue in [2400, 2401, 2403, 2405, 2410, 2450, 3240, 3270, 3280] {
            assert!(suggest_runs(Phase::InGame, Some(queue), true), "{queue}");
        }
        // Another queue, an unknown queue, and the other phases.
        assert!(!suggest_runs(Phase::InGame, Some(450), true));
        assert!(!suggest_runs(Phase::InGame, None, true));
        for phase in [
            Phase::Offline,
            Phase::Idle,
            Phase::Lobby,
            Phase::ChampSelect,
            Phase::PostGame,
        ] {
            assert!(!suggest_runs(phase, Some(2400), true), "{phase:?}");
        }
    }

    #[test]
    fn end_game_clears_the_roster_and_the_advice() {
        let mut s = Shared {
            roster: Some(Roster {
                players: vec![suggest::Player::default()],
                me: 0,
            }),
            ..Default::default()
        };
        assert!(s.store_advice(Some(Advice {
            style: "ap".into(),
            ..Default::default()
        })));
        s.live = Live {
            level: 9,
            gold: Some(1200),
        };
        s.end_game();
        assert!(s.roster.is_none() && s.advice.is_none());
        // The gold goes with the level.
        assert_eq!((s.live.level, s.live.gold), (0, None));
    }

    #[test]
    fn advice_is_stored_only_when_it_changes() {
        let mut s = Shared::default();
        let a = || Advice {
            style: "ap".into(),
            ..Default::default()
        };
        // Nothing to clear.
        assert!(!s.store_advice(None));
        assert!(s.store_advice(Some(a())));
        // The same advice again: no change, so no new drawing.
        let first = s.advice.clone().unwrap();
        assert!(!s.store_advice(Some(a())));
        assert!(Arc::ptr_eq(&first, s.advice.as_ref().unwrap()));
        // A change of the items, of a ready flag, or of the summary is stored.
        let other = Advice {
            me_dead: true,
            ..a()
        };
        assert!(s.store_advice(Some(other)));
        assert!(!Arc::ptr_eq(&first, s.advice.as_ref().unwrap()));
        // Clearing is a change once.
        assert!(s.store_advice(None));
        assert!(!s.store_advice(None));
    }

    #[test]
    fn the_summary_does_not_wait_for_the_stats_of_the_champion() {
        let ctx = egui::Context::default();
        let s = Shared {
            suggest_on: true,
            static_data: Some(Arc::new(StaticData::default())),
            roster: Some(Roster {
                players: vec![suggest::Player::default()],
                me: 0,
            }),
            my_champion: Some(74),
            ..Default::default()
        };
        let shared = Mutex::new(s);
        // The stats are still loading: no advice yet, so the window says that the data arrives.
        suggest_pass(&shared, &ctx, Phase::InGame, Some(2400));
        assert!(shared.lock().unwrap().advice.is_none());
        // The stats are not there (the load failed): the advice has the summary and says that there is no build.
        shared
            .lock()
            .unwrap()
            .failed
            .insert(74, (Instant::now(), NoStats::Missing));
        suggest_pass(&shared, &ctx, Phase::InGame, Some(2400));
        let a = shared
            .lock()
            .unwrap()
            .advice
            .clone()
            .expect("advice without stats");
        assert!(a.no_build && a.items.is_empty() && a.ctx.is_some());
        // The champion is not known either: the same.
        {
            let mut s = shared.lock().unwrap();
            s.failed.clear();
            s.my_champion = None;
        }
        suggest_pass(&shared, &ctx, Phase::InGame, Some(2400));
        assert!(
            shared
                .lock()
                .unwrap()
                .advice
                .as_ref()
                .is_some_and(|a| a.no_build && a.ctx.is_some())
        );
        // The stats arrive: the advice has a build again (an empty one is still no build, and the items follow the data).
        shared.lock().unwrap().my_champion = Some(74);
        shared
            .lock()
            .unwrap()
            .details
            .insert(74, Arc::new(ChampStats::default()));
        suggest_pass(&shared, &ctx, Phase::InGame, Some(2400));
        assert!(shared.lock().unwrap().advice.is_some());
        // Outside a Mayhem game the advice goes.
        suggest_pass(&shared, &ctx, Phase::InGame, Some(450));
        assert!(shared.lock().unwrap().advice.is_none());
    }
}
