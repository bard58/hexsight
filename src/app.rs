//! UI: a small frameless window that follows the game phase, and a compact click-through overlay for in game.

use crate::badges;
use crate::data::{LoadError, NoStats};
use crate::history::{self, Game};
use crate::i18n::{Key, Lang, LangPref};
use crate::kit::{self, fmt_n};
use crate::lcu::{Phase, TradeCall, TradeState};
use crate::model::*;
use crate::ocr::{CaptureHealth, Card, ScanRecord};
use crate::suggest::{self, Advice, GameContext, ItemPick, Reason};
use crate::theme::{self, glyph, *};
use crate::update;
use crate::win::key_label;
use crate::worker::{self, ChampSource, Cmd, Handles, Keys, Offer};
use egui::{Align2, Color32, CornerRadius, FontId, Rect, Sense, Ui, Vec2, pos2, vec2};
use serde::{Deserialize, Serialize};
use std::sync::Arc;

#[derive(Serialize, Deserialize, Clone)]
#[serde(default)]
pub struct Settings {
    pub pos: Option<[f32; 2]>,
    /// Win32 virtual keys. 0x77 = F8 (overlay), 0x78 = F9 (read cards now). The
    /// Settings page changes them (button "Change").
    pub overlay_key: i32,
    pub scan_key: i32,
    /// Where the overlay sits, in points at zoom 1.0. A change of the overlay
    /// size then keeps it on the same spot of the screen.
    pub overlay_pos: Option<[f32; 2]>,
    /// Overlay size: 0 small (zoom 0.9), 1 medium (1.0), 2 large (1.2). The row
    /// "Overlay size" in Settings > Overlay sets it.
    pub overlay_scale: u8,
    /// Show the overlay in recordings and streams. Off hides it from screen
    /// captures. On shows it, and the card reader skips the text of the overlay.
    /// The switch "Hide the panel from recordings and screenshots" in Settings >
    /// Panel sets it (the switch is on when this is false).
    pub overlay_in_captures: bool,
    /// Badges on the game cards (beta): a second window over the game draws a
    /// tier badge and the game count above each offered card. With `top_highlight`
    /// on, every top-tier card also gets a green edge.
    pub card_badges: bool,
    /// "Highlight top-tier cards". Off by default: Riot's rules for third-party
    /// apps. It turns on the green frame, the "top" label and the top-tier line in
    /// the pick header (overlay strip, pick card and badges).
    pub top_highlight: bool,
    /// "Show pair hints". Off by default, for the same reason. It turns on the
    /// pairs line of the pick card and the pairs mark of the overlay strip.
    pub pair_hints: bool,
    /// "Mark augments that fit your build". Off by default, for the same reason. It adds the
    /// chip "fits" and a line to a card of the pick card when the data ties the augment to an
    /// item of your build.
    pub fit_marks: bool,
    /// "Suggest the next items": the card "Next items" in the main window and the item cells of
    /// the overlay card. The tips come from build data and Hexsight's own rules.
    pub suggest_items: bool,
    /// "Show the game summary": the damage mix of the enemy team, the fed enemy, the healers and
    /// the tanks. Hexsight's own estimate.
    pub suggest_context: bool,
    /// Rows with fewer games show dim, and a card needs this many games to count
    /// for the top tier. Only the settings file changes it.
    pub min_games: u32,
    /// Show the overlay when a game starts, hide it after.
    pub auto_overlay: bool,
    /// Read the cards all game long, not only when a pick is likely. Only the
    /// settings file changes it.
    pub scan_always: bool,
    /// No effect since 2026-10-29: the extended frame let the native caption buttons show
    /// through. The main window never uses glass now. The field stays so an old settings file
    /// loads and saves as it was.
    pub glass: bool,
    /// Write the build as an in-game shop item set in champ select.
    pub item_sets: bool,
    /// Bring the window back when champ select starts. Only the settings file changes it.
    pub show_on_champ_select: bool,
    /// Old setting "Hide win rates" (the field is from "Show tiers only"). It has no
    /// effect and no switch any more: Hexsight shows no augment win rate, and the
    /// shop item sets carry no rate. The field stays so an old settings file loads
    /// and saves as it was.
    pub tiers_only: bool,
    /// The first-run checklist asked "Start with Windows?" once. It never asks again.
    pub autostart_asked: bool,
    /// Ask GitHub once a day for the latest version, and show a link when a newer
    /// one exists. Off sends no request at all.
    pub check_updates: bool,
    /// Client language seen last time. Skips an English load at start. The
    /// language of the interface (Auto) also uses it until the client answers.
    pub last_locale: String,
    /// Language of the interface: Auto follows the client language.
    pub language: LangPref,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            pos: None,
            overlay_key: 0x77,
            scan_key: 0x78,
            overlay_pos: None,
            overlay_scale: 1,
            overlay_in_captures: false,
            card_badges: false,
            top_highlight: false,
            pair_hints: false,
            fit_marks: false,
            suggest_items: true,
            suggest_context: true,
            min_games: 300,
            auto_overlay: true,
            scan_always: false,
            glass: true,
            item_sets: true,
            show_on_champ_select: true,
            tiers_only: false,
            check_updates: true,
            autostart_asked: false,
            last_locale: String::new(),
            language: LangPref::Auto,
        }
    }
}

impl Settings {
    /// Any suggestion is on: the poll thread then reads the player list in a Mayhem game.
    pub fn suggests(&self) -> bool {
        self.suggest_items || self.suggest_context || self.fit_marks
    }

    fn path() -> std::path::PathBuf {
        crate::cache::app_dir().join("settings.json")
    }
    pub fn load() -> Self {
        let mut s: Self = std::fs::read(Self::path())
            .ok()
            .and_then(|b| serde_json::from_slice(&b).ok())
            .unwrap_or_default();
        // A hand-edited size must stay one of S, M and L.
        s.overlay_scale = s.overlay_scale.min(2);
        s
    }
    fn save(&self) {
        if let Ok(b) = serde_json::to_vec_pretty(self) {
            let tmp = Self::path().with_extension("tmp");
            if std::fs::write(&tmp, b).is_ok() {
                let _ = std::fs::rename(&tmp, Self::path());
            }
        }
    }
}

/// Width of the main window, in points. The height follows the content.
pub const MAIN_W: f32 = 300.0;

/// The window height before the first content measure, in points.
const MAIN_START_H: f32 = 380.0;

/// The tallest the main window grows. A longer page scrolls.
const MAIN_MAX_H: f32 = 680.0;

/// Space at the left and the right of the content of the main window, in points.
const MARGIN: f32 = 14.0;

/// Height of the title bar of the main window, in points.
const TITLE_H: f32 = 44.0;

/// Height of the footer of the main window, in points.
const FOOTER_H: f32 = 28.0;

/// Space above the line over the footer, in points.
const FOOTER_GAP: f32 = 8.0;

/// Overlay width while it shows the pick strip. The card is 8 points narrower, and the
/// window adds the room for the shadow (`OVERLAY_WINDOW_W`).
const OVERLAY_STRIP_W: f32 = 340.0;

/// The widest the overlay gets with a hint under the pill.
const OVERLAY_HINT_W: f32 = 290.0;

/// Where the overlay starts when it has no saved place, in points at zoom 1.0.
const OVERLAY_HOME: [f32; 2] = [12.0, 96.0];

/// Zoom factors of the overlay sizes S, M and L.
const OVERLAY_ZOOMS: [f32; 3] = [0.9, 1.0, 1.2];

/// How long the Settings page waits for a new hotkey.
const REBIND_WAIT: std::time::Duration = std::time::Duration::from_secs(15);

/// The overlay strip stays this long after the cards go away. A pick that you
/// make or a missed scan then does not make it flicker.
const CARD_HOLD: std::time::Duration = std::time::Duration::from_millis(1500);

/// The size of the overlay pill, in points.
const PILL_W: f32 = 162.0;
const PILL_H: f32 = 40.0;

/// The fonts of the pill. The name is bold (before semibold), and the word has the
/// full white of the labels (before the grey LABEL_2).
fn pill_name_font() -> FontId {
    FontId::new(13.0, bold())
}
fn pill_word_font() -> FontId {
    FontId::proportional(12.5)
}

/// The word after "Hexsight" on the pill. In a game with a known level it says when the next
/// pick screen comes ("pick at level 7"), or that all picks are done. Outside a game, and
/// before the level is known, it says "watching". The pill grows with a longer word.
fn pill_word(lang: Lang, phase: Phase, level: i64, picks_done: usize) -> String {
    if phase != Phase::InGame || level <= 0 {
        return lang.t(Key::OverlayWatching).to_string();
    }
    match next_pick(level, picks_done) {
        Some(i) => lang.fmt(Key::OverlayNextPick, &[&PICK_LEVELS[i]]),
        None => lang.t(Key::OverlayPicksDone).to_string(),
    }
}

/// The plate of the overlay (the strip, the pill, the hint box): near opaque so the
/// game never shows through the text (alpha 250 of 255, before 230). It is fully
/// opaque while the overlay is unlocked (before 250).
fn overlay_fill(unlocked: bool) -> Color32 {
    Color32::from_rgba_unmultiplied(22, 22, 26, if unlocked { 255 } else { 250 })
}

/// The thin light outline of the overlay (white at 80 of 255, before 22).
fn overlay_edge() -> egui::Stroke {
    egui::Stroke::new(1.0, Color32::from_white_alpha(80))
}

/// The shadow under the overlay: soft, 6 points down, 18 points of blur, black at 110
/// of 255. Before, the overlay painted a solid black rect 2 or 3 points lower, and it
/// showed as a grey band under the plate. Paint it with `as_shape` before the plate.
fn overlay_shadow() -> egui::epaint::Shadow {
    egui::epaint::Shadow {
        offset: [0, 6],
        blur: 18,
        spread: 0,
        color: Color32::from_black_alpha(110),
    }
}

/// The room between a card of the overlay and the edge of its window, in points. The soft
/// shadow reaches half its blur (9 points) past the plate, and 6 more down. If the window
/// were closer, the shadow would end in a hard line at its edge. The top needs the least:
/// the shadow sits lower than the plate.
const OVERLAY_SIDE: f32 = 10.0;
const OVERLAY_TOP: f32 = 4.0;
const OVERLAY_BOTTOM: f32 = 16.0;

/// The width of the overlay window while it shows a card of `OVERLAY_STRIP_W - 8` points.
const OVERLAY_WINDOW_W: f32 = OVERLAY_STRIP_W - 8.0 + 2.0 * OVERLAY_SIDE;

/// The best augment pairs. The pairs hint on a pick screen uses them.
const TOP_COMBOS: usize = 8;

/// The My games page lists this many champions. The switch "All champions" lists every champion with a game.
const MY_CHAMPIONS: usize = 5;

/// "Recent games" on the My games page: it lists the newest `GAMES_FIRST` games, and "Show all"
/// lists `GAMES_MAX` at most (the file keeps 500).
const GAMES_FIRST: usize = 10;
const GAMES_MAX: usize = 100;

/// A row of "Recent games": its height, the champion disc, and the room between the disc and the
/// texts, in points.
const GAME_ROW_H: f32 = 44.0;
const GAME_ICON: f32 = 32.0;
const GAME_TEXT_GAP: f32 = 10.0;

/// The augment icons of a row (in pick order, six at most): size, the space between them, and
/// the radius of their corners, in points. They are the icons of the card after a game, smaller.
const GAME_AUG: f32 = 18.0;
const GAME_AUG_GAP: f32 = 3.0;
const GAME_AUG_RADIUS: u8 = 5;
const GAME_AUGMENTS: usize = 6;

/// The build card lists this many builds.
const MAX_BUILDS: usize = 3;

/// The space between the column of the styles and the first item of a build row, before
/// the 4 points of space between items. The items of every row start at the same place.
const BUILD_STYLE_GAP: f32 = 8.0;

/// The bench grid: champions in a row, and the size of a champion icon, in points.
/// Five icons of 40 share the 246 points of a card (272, less the margins and the
/// edge of `kit::group`) with even gaps of 11.5, so a bench of 10 takes two rows
/// and the green glow of the best swap has room.
const BENCH_COLS: usize = 5;
const BENCH_ICON: f32 = 40.0;

/// The tier badge on the corner of a bench icon, in points.
const BENCH_BADGE: f32 = 20.0;

/// The button under the bench grid that swaps to the best champion: its height in points,
/// and the strength of its green fill at rest and under the mouse.
const SWAP_BUTTON_H: f32 = 32.0;
const SWAP_BUTTON_REST: f32 = 0.18;
const SWAP_BUTTON_HOVER: f32 = 0.28;

/// The badge reaches this far out of its icon, and its ring of the card color is this
/// wide. Together (3.5 points) they stay clear of the glow of the next icon
/// (`kit::GLOW_REACH`, 7.5) in a gap of 11.5.
const BENCH_BADGE_OUT: f32 = 2.0;
const BENCH_BADGE_RING: f32 = 1.5;

/// The champions of the first step of a champ select: icons in a row, the size of an
/// icon, the height of the two lines under an icon (the name at 12 points, the games at
/// 11), and the height of one row, in points. Three icons of 56 stand in three equal
/// columns of the 246 points of a card.
const CHOICE_COLS: usize = 3;
const CHOICE_ICON: f32 = 56.0;
const CHOICE_TEXT_H: f32 = 30.0;
const CHOICE_ROW_H: f32 = CHOICE_ICON + 4.0 + CHOICE_TEXT_H + 10.0;

/// The font of the champion name under an icon of the first step.
fn choice_name_font() -> FontId {
    FontId::new(12.0, semibold())
}

/// The tier badge on the corner of an icon of the first step, and how far it reaches
/// out of the icon, in points.
const CHOICE_BADGE: f32 = 22.0;
const CHOICE_BADGE_OUT: f32 = 4.0;
const CHOICE_BADGE_RING: f32 = 2.0;

/// Room at the left, the right and the bottom of the main page in its scroll area,
/// so the glow of a card (`kit::glow`, 7.5 points) is not cut.
const PAGE_BLEED: i8 = 8;

/// After a click on a trade button, the buttons of that trade stay off for
/// this long. The session shows the new state of the trade within a second.
/// Until then, a second click would send the call again and fail.
const TRADE_PAUSE: std::time::Duration = std::time::Duration::from_millis(1500);

/// The height of the buttons of a trade request, in points.
const TRADE_BUTTON_H: f32 = 28.0;

/// After a click on the button "Fix", it stays off for this long, so a second
/// click cannot send the change again while the client saves it.
const FIX_PAUSE: std::time::Duration = std::time::Duration::from_secs(3);

/// The size of a summoner spell icon on the spell card, in points.
const SPELL_ICON: f32 = 26.0;

/// The levels of the pick screens in ARAM Mayhem.
const PICK_LEVELS: [i64; 4] = worker::PICK_LEVELS;

/// Games behind a stat for a high confidence (three bars). A placeholder: tune
/// it on the real spread of the game counts. The count is the one the cell
/// shows: the stats of the pick screen when it has them, else the stats of all
/// picks. The first is a part of the second, so the same augment can show fewer
/// bars on a later screen.
const CONF_HIGH: u32 = 8000;

/// Games behind a stat for a medium confidence (two bars). A placeholder, as `CONF_HIGH`.
const CONF_MID: u32 = 3000;

/// The pages of the main window. The title bar has a way back from the others.
#[derive(PartialEq, Clone, Copy, Debug)]
enum Page {
    /// What the game phase asks for (`Screen`).
    Main,
    Settings,
    Games,
}

/// The two hotkeys that the Settings page can change.
#[derive(PartialEq, Clone, Copy, Debug)]
enum Hotkey {
    Overlay,
    Scan,
}

pub struct App {
    h: Handles,
    settings: Settings,
    page: Page,
    overlay: bool,
    /// Overlay takes mouse input (to move it) when unlocked.
    overlay_unlocked: bool,
    overlay_auto: bool,
    last_phase: Phase,
    seen_overlay: u32,
    seen_lock: u32,
    item_set_for: Option<i64>,
    /// The item sets of Hexsight were deleted for the setting that is off.
    item_sets_cleared: bool,
    hwnd: Option<isize>,
    /// Debug capture (env HEXSIGHT_SHOT): start time and whether it was requested.
    started: std::time::Instant,
    shot_sent: bool,
    debug_overlay: bool,
    /// Started by Windows (--background): minimize on the first frame.
    start_minimized: bool,
    autostart: bool,
    /// Settings changed at this time. Saved 500 ms later.
    dirty_at: Option<std::time::Instant>,
    /// Last stats request per champion.
    wanted: std::collections::HashMap<i64, std::time::Instant>,
    /// The auto overlay check ran for this game.
    auto_done: bool,
    /// Last overlay size sent to the window.
    overlay_size: Option<Vec2>,
    /// Last size sent to the main window. The content decides it.
    main_size: Option<Vec2>,
    /// The height of the main page in the last frame, without a scroll area. Above the
    /// room between the title bar and the footer, the page scrolls (`main_area`).
    main_h: Option<f32>,
    debug_offer: Option<Offer>,
    /// Debug capture (env HEXSIGHT_PICKS): the cards of the earlier pick screens. The window shows them
    /// whatever the game says, so a capture in the phase InGame keeps them (a new game clears the shared ones).
    debug_offered: Option<Vec<(usize, Vec<i64>)>>,
    /// Debug capture (env HEXSIGHT_SCROLL): scroll offset of the Settings and My games pages, and of the main page when it scrolls.
    debug_scroll: Option<f32>,
    /// The end of the Settings page (largest scroll offset) in the last frame.
    debug_scroll_end: f32,
    /// Debug capture (env HEXSIGHT_SAVE_SCAN): seconds after the start when the
    /// last scan is saved, as the button on the Settings page does.
    debug_save_scan: Option<f32>,
    /// The window waits for the zoom factor to become active, then it gets its
    /// size and place. A command sent earlier would use the old zoom.
    place_window: bool,
    /// The window got its place in this frame. Its rect is not the new one yet.
    just_placed: bool,
    /// The Settings page waits for a new hotkey: which one, and since when.
    rebinding: Option<(Hotkey, std::time::Instant)>,
    /// The client phase of this frame. `last_phase` follows it later in the same frame.
    cur_phase: Phase,
    /// Whether the overlay window lets the mouse pass, as last sent to the window.
    /// `None` until the state is sent. It passes the mouse only in a game.
    pass_sent: Option<bool>,
    /// The offer that the overlay strip shows. After the cards go away, the strip
    /// stays for `CARD_HOLD`: the second part is the time when they went away.
    held_offer: Option<(Offer, Option<std::time::Instant>)>,
    /// Debug capture (env HEXSIGHT_CAPTURE_FAULT=black|frozen): the capture health
    /// that the overlay and Settings show, without a game.
    debug_capture: Option<CaptureHealth>,
    /// Debug capture (env HEXSIGHT_FIRST_RUN=1): ask "Start with Windows?" (on the card
    /// after a game) even when Windows starts Hexsight already or the question was answered.
    debug_first_run: bool,
    /// The language of the interface: the setting, with Auto resolved. The
    /// worker threads get the same value (`Shared::lang`).
    lang: Lang,
    /// The window over the game that draws the badges on the cards (beta).
    badge_win: badges::Window,
    /// Debug capture (env HEXSIGHT_VIEW=badges): show the badges in the main window.
    debug_badges: bool,
    /// The last click on a trade button: the trade id, and when. See `TRADE_PAUSE`.
    trade_clicked: Option<(i64, std::time::Instant)>,
    /// Debug capture (env HEXSIGHT_TRADE=<request|accept|decline|cancel>:<trade id>:<seconds>):
    /// the call, the trade id, and the seconds after the start. The call is sent
    /// at that time, as a click on the button would.
    debug_trade: Option<(TradeCall, i64, f32)>,
    /// The card reading tools of the Settings page are open.
    tools_open: bool,
    /// The last click on the button "Fix". See `FIX_PAUSE`.
    fix_clicked: Option<std::time::Instant>,
    /// The id of the newest saved game while no game ended. After a game, a newer
    /// one is the game that just ended.
    seen_game: Option<i64>,
    /// The signature of the advice (`Advice::signature`) and when it last changed. The overlay
    /// shows its advice card for `ADVICE_SHOW` after a change.
    advice_seen: Option<(u64, std::time::Instant)>,
    /// Where the window is after the X of the main page (`hide_step`). The toast "Hexsight keeps
    /// running" waits for the window to come back: a minimized window shows no toast.
    hide: Hide,
    /// The toast "Hexsight keeps running" showed. It shows once in a run.
    hidden_told: bool,
    /// My games lists every saved game (up to `GAMES_MAX`), not only the newest `GAMES_FIRST`.
    /// It goes back to the short list when the page closes.
    games_all: bool,
    /// My games lists every champion with a game, not only the `MY_CHAMPIONS` most played.
    champions_all: bool,
    /// For tests: the time that My games reads dates against, as (Unix seconds in UTC, the offset of
    /// the local zone in seconds). `None` reads the clock and the time zone of this PC.
    debug_clock: Option<(i64, i64)>,
}

/// One offered card with its stats.
pub struct OfferStat {
    pub id: i64,
    /// Center of the card on the screen: 0 is the left edge, 1 the right edge.
    pub x: f32,
    /// Center of the title line, as a fraction of the capture.
    pub y: f32,
    pub stat: Option<AugStat>,
}

/// The offered cards with the stats of this pick screen when present, in screen
/// order, left to right. A card without stats has `stat: None`.
pub fn offer_stats(
    cs: &ChampStats,
    offer: &[Card],
    screen: usize,
    min_games: u32,
) -> Vec<OfferStat> {
    let stage = cs
        .stages
        .get(screen.saturating_sub(1))
        .filter(|s| !s.is_empty());
    let mut v: Vec<OfferStat> = offer
        .iter()
        .map(|c| {
            let staged = stage
                .and_then(|s| s.iter().find(|a| a.augment_id == c.id))
                .filter(|a| a.games >= min_games);
            OfferStat {
                id: c.id,
                x: c.x,
                y: c.y,
                stat: staged.or(cs.get(c.id)).cloned(),
            }
        })
        .collect();
    v.sort_by(|a, b| a.x.total_cmp(&b.x));
    v
}

/// The cards that Hexsight highlights on a pick screen.
#[derive(Debug, Default, PartialEq)]
pub struct TopTier {
    /// Every eligible card of the best tier. Empty when no eligible card has a worse tier.
    pub ids: Vec<i64>,
    /// Two or more eligible cards, and all have the same tier.
    pub equal: bool,
}

/// The rule for the highlight, for the pick views and for the badges. A card is
/// eligible when it has stats and at least `min_games` games (the stats of this
/// pick screen, else the stats of all picks, as in `offer_stats`). Hexsight
/// highlights every eligible card of the best tier, but only when another
/// eligible card has a worse tier. When all eligible cards have the same tier,
/// nothing is highlighted and `equal` says so.
pub fn top_tier(cs: &ChampStats, offer: &[Card], screen: usize, min_games: u32) -> TopTier {
    let eligible: Vec<(i64, u8)> = offer_stats(cs, offer, screen, min_games)
        .into_iter()
        .filter_map(|r| {
            r.stat
                .filter(|a| a.games >= min_games)
                .map(|a| (r.id, a.tier))
        })
        .collect();
    let (Some(best), Some(worst)) = (
        eligible.iter().map(|e| e.1).min(),
        eligible.iter().map(|e| e.1).max(),
    ) else {
        return TopTier::default();
    };
    if best == worst {
        return TopTier {
            ids: Vec::new(),
            equal: eligible.len() >= 2,
        };
    }
    TopTier {
        ids: eligible
            .iter()
            .filter(|e| e.1 == best)
            .map(|e| e.0)
            .collect(),
        equal: false,
    }
}

/// How much data stands behind the numbers of a card. The pick views show it as
/// three bars and a word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Confidence {
    Low,
    Mid,
    High,
}

impl Confidence {
    /// The confidence for a count of games (`CONF_MID` and `CONF_HIGH` are the limits).
    fn of(games: u32) -> Self {
        if games >= CONF_HIGH {
            Confidence::High
        } else if games >= CONF_MID {
            Confidence::Mid
        } else {
            Confidence::Low
        }
    }

    /// Bars that light up, 1 to 3.
    fn bars(self) -> u8 {
        match self {
            Confidence::Low => 1,
            Confidence::Mid => 2,
            Confidence::High => 3,
        }
    }

    fn key(self) -> Key {
        match self {
            Confidence::Low => Key::ConfLow,
            Confidence::Mid => Key::ConfMid,
            Confidence::High => Key::ConfHigh,
        }
    }
}

/// Which marks the pick views draw. All are off by default, because Riot's rules
/// for third-party apps bar apps that draw conclusions during play. The player
/// turns each one on in Settings. Tier badges, game counts and the confidence
/// bars do not depend on them.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Marks {
    /// The green frame, the "top" label and the top-tier line in the header.
    pub top: bool,
    /// The pairs line and the pairs mark.
    pub pairs: bool,
    /// The chip "fits" and the line "Fits your build" on the pick card.
    pub fit: bool,
}

impl Settings {
    /// The marks that the player turned on.
    pub fn marks(&self) -> Marks {
        Marks {
            top: self.top_highlight,
            pairs: self.pair_hints,
            fit: self.fit_marks,
        }
    }
}

/// One offered card as the pick views show it: where it sits, how many games
/// stand behind its stats, and whether it is in the top tier.
#[derive(Debug, PartialEq)]
struct PickCell {
    id: i64,
    /// Center of the card on the screen: 0 is the left edge, 1 the right edge.
    x: f32,
    tier: Option<u8>,
    games: Option<u32>,
    top: bool,
    /// The card pairs well with a card of an earlier pick screen.
    pair: Option<PairHint>,
    /// The data ties the card to this item of your build (the mark "fits your build").
    fit: Option<i64>,
}

impl PickCell {
    fn confidence(&self) -> Option<Confidence> {
        self.games.map(Confidence::of)
    }
}

/// The cells of a pick screen in screen order, and the header line about the top
/// tier. `top_tier` decides the highlight, as for the badges. The pairs hint does
/// not change it. Without stats (`cs` is `None`), every cell is empty.
/// With `marks.top` off, no cell is on top and there is no header line (`None`).
/// With `marks.pairs` off, no cell has a pair.
fn pick_cells(
    cs: Option<&ChampStats>,
    offer: &Offer,
    earlier: &[i64],
    min_games: u32,
    marks: Marks,
) -> (Vec<PickCell>, Option<TopLine>) {
    let none = ChampStats::default();
    let cs = cs.unwrap_or(&none);
    let top = top_tier(cs, &offer.cards, offer.screen, min_games);
    let cells = offer_stats(cs, &offer.cards, offer.screen, min_games)
        .into_iter()
        .map(|r| PickCell {
            id: r.id,
            x: r.x,
            tier: r.stat.as_ref().map(|a| a.tier),
            games: r.stat.as_ref().map(|a| a.games),
            top: marks.top && top.ids.contains(&r.id),
            pair: if marks.pairs {
                pairs_hint(cs, r.id, earlier, min_games)
            } else {
                None
            },
            fit: None,
        })
        .collect();
    (cells, marks.top.then(|| top_line(&top)))
}

/// Add the mark "fits your build" to the cells: the item that the data ties to the card, when
/// that item is among the items you could still want (`Advice::wanted`). Without the mark, without
/// stats or without advice, no cell has a fit. The mark has no number, and it changes neither the
/// order of the cells nor the highlight.
fn fit_cells(
    cells: &mut [PickCell],
    cs: Option<&ChampStats>,
    advice: Option<&Advice>,
    marks: Marks,
) {
    let (Some(cs), Some(advice), true) = (cs, advice, marks.fit) else {
        return;
    };
    for c in cells {
        c.fit = suggest::fit_item(cs, &advice.wanted, c.id);
    }
}

/// What the header of a pick view says about the top tier.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum TopLine {
    /// This many cards are in the top tier.
    Count(usize),
    /// Every card has the same tier.
    Equal,
    /// Not enough data to tell.
    NoData,
}

fn top_line(top: &TopTier) -> TopLine {
    match top.ids.len() {
        0 if top.equal => TopLine::Equal,
        0 => TopLine::NoData,
        n => TopLine::Count(n),
    }
}

/// The words of the header line, and their color: green when cards are on top.
fn top_line_text(lang: Lang, line: TopLine) -> (String, Color32) {
    match line {
        TopLine::Count(1) => (lang.t(Key::TopOne).to_string(), GREEN),
        TopLine::Count(n) => (lang.fmt(Key::TopMany, &[&n]), GREEN),
        TopLine::Equal => (lang.t(Key::TierEqual).to_string(), LABEL_3),
        TopLine::NoData => (lang.t(Key::NoData).to_string(), LABEL_3),
    }
}

/// What the main window shows. The game phase and the card reader decide.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Screen {
    /// No game yet: the first-run checklist.
    Ready,
    /// Champ select: your champion, the bench, and the build.
    Select,
    /// In a game between pick screens: the next pick, and the build.
    Waiting,
    /// A pick screen: the offered cards.
    Pick,
    /// The game is over: its result.
    After,
    /// A game of another mode: there are no pick screens and no saved result. Also a
    /// champ select of a queue without a bench (Ranked, Normal).
    Other,
}

/// The screen for a phase. `other_mode` is true when the queue is known and is
/// not a Mayhem queue. Champ select stays the same for every ARAM queue, because the
/// bench works there. `non_aram` is true when the queue is known and is not an ARAM
/// queue (`is_aram`): its champ select has no bench and no Mayhem page. A missing
/// queue and a custom game (queue 0 or less) keep the Select page.
fn screen_for(phase: Phase, has_offer: bool, other_mode: bool, non_aram: bool) -> Screen {
    match phase {
        _ if has_offer => Screen::Pick,
        Phase::ChampSelect if non_aram => Screen::Other,
        Phase::ChampSelect => Screen::Select,
        Phase::InGame | Phase::PostGame if other_mode => Screen::Other,
        Phase::InGame => Screen::Waiting,
        Phase::PostGame => Screen::After,
        Phase::Offline | Phase::Idle | Phase::Lobby => Screen::Ready,
    }
}

/// Is the queue known, and does it have no ARAM bench (Ranked, Normal)? A missing queue
/// and a custom game (queue 0 or less) are not: their champ select keeps the Select page.
fn non_aram_queue(queue: Option<i64>) -> bool {
    queue.is_some_and(|q| q > 0 && !is_aram(q))
}

/// Does champ select bring the main window up? Only with the setting on, while the
/// overlay is not open, and not in a queue without a bench: a Ranked champ select has
/// nothing for Hexsight to show.
fn raises_on_champ_select(setting: bool, overlay: bool, queue: Option<i64>) -> bool {
    setting && !overlay && !non_aram_queue(queue)
}

/// Does a champ select send the window back to the main page? Only in a queue with a bench,
/// where the page has the bench and the build. A Ranked champ select has nothing to show, so
/// a page that the player has open (Settings, My games) stays.
fn champ_select_resets_page(queue: Option<i64>) -> bool {
    !non_aram_queue(queue)
}

/// Does the change of the phase `from` -> `to` send the window back to the main page? A
/// game that starts or ends has its own screen (the next pick, the result), and a page
/// that the player left open (Settings, My games) would hide it. The first phase after
/// the start of Hexsight (`from` is Offline) keeps the page that is open.
fn phase_resets_page(from: Phase, to: Phase) -> bool {
    matches!(to, Phase::InGame | Phase::PostGame) && from != Phase::Offline
}

/// The main window around the X of the main page, which minimizes it (`hide_window`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Hide {
    /// Not hidden by the X.
    Shown,
    /// The X asked for the minimize, and the window has not reported it yet.
    Asked,
    /// The window reported that it is minimized.
    Minimized,
}

/// One frame of the hide state: the next state, and whether the window just came back. Only a
/// window that was seen minimized comes back, so the frame right after the X (the window is
/// not minimized yet) is not a return.
fn hide_step(state: Hide, minimized: bool) -> (Hide, bool) {
    match (state, minimized) {
        (Hide::Asked, true) => (Hide::Minimized, false),
        (Hide::Minimized, false) => (Hide::Shown, true),
        (state, _) => (state, false),
    }
}

/// Does Esc send a page back to the main page? Not on the main page, and not while a
/// hotkey waits for a key: there Esc cancels the wait.
fn esc_goes_back(page: Page, rebinding: bool, esc: bool) -> bool {
    esc && !rebinding && page != Page::Main
}

/// The index in `PICK_LEVELS` of the next pick screen. `None` when all are
/// past. `done` is the number of pick screens that closed in this game. A pick
/// screen that Hexsight missed (it started late in the game) is past once the level is higher.
fn next_pick(level: i64, done: usize) -> Option<usize> {
    let past = PICK_LEVELS.iter().filter(|&&l| l < level).count();
    Some(done.max(past)).filter(|&i| i < PICK_LEVELS.len())
}

/// The row of the next-pick card: its height, and the size and the distance of its four
/// dots (center to center), in points.
const NEXT_PICK_ROW_H: f32 = 32.0;
const NEXT_PICK_DOT_D: f32 = 6.0;
const NEXT_PICK_DOT_STEP: f32 = 10.0;

/// The colors of the four dots of the next-pick card for the next pick `next` (an index in
/// `PICK_LEVELS`, `None` when all are past): green for the next one, grey for the past
/// ones, faint white for the ones that are still to come.
fn next_pick_dots(next: Option<usize>) -> [Color32; PICK_LEVELS.len()] {
    std::array::from_fn(|i| match next {
        Some(n) if i == n => GREEN,
        Some(n) if i > n => Color32::from_white_alpha(46),
        _ => LABEL_3,
    })
}

/// The tallest the main window grows on a monitor of height `monitor_h`:
/// `MAIN_MAX_H`, or less on a small screen (80 points stay free, 320 at least).
fn max_height_on(monitor_h: Option<f32>) -> f32 {
    monitor_h.map_or(MAIN_MAX_H, |h| MAIN_MAX_H.min(h - 80.0).max(320.0))
}

/// The new height of the main window for content of height `used`, or `None`
/// when the window keeps its height. The height stays between 120 and `max`. A
/// change of 2 points or less is no change, so the window cannot resize in a loop.
fn fit_height(used: f32, max: f32, last: Option<f32>) -> Option<f32> {
    let h = used.clamp(120.0, max).ceil();
    last.is_none_or(|l| (l - h).abs() > 2.0).then_some(h)
}

/// The bench champion that is worth a swap: the best tier among the bench
/// champions that have `min_games` games, when it beats your own tier. A tie
/// goes to the higher win rate. `None` when you have no stats to compare with.
fn best_swap(
    mine: Option<ChampRow>,
    bench: &[(i64, Option<ChampRow>)],
    min_games: u32,
) -> Option<i64> {
    let mine = mine?;
    bench
        .iter()
        .filter_map(|&(id, r)| Some((id, r?)))
        .filter(|(_, r)| r.games >= min_games && r.tier < mine.tier)
        .min_by(|a, b| {
            a.1.tier
                .cmp(&b.1.tier)
                .then(b.1.win_rate.total_cmp(&a.1.win_rate))
        })
        .map(|(id, _)| id)
}

/// The bench in the order of the bench card: the best tier first, and the higher
/// win rate first inside a tier. A champion without stats goes last.
fn bench_order(a: Option<ChampRow>, b: Option<ChampRow>) -> std::cmp::Ordering {
    match (a, b) {
        (Some(a), Some(b)) => a.tier.cmp(&b.tier).then(b.win_rate.total_cmp(&a.win_rate)),
        (Some(_), None) => std::cmp::Ordering::Less,
        (None, Some(_)) => std::cmp::Ordering::Greater,
        (None, None) => std::cmp::Ordering::Equal,
    }
}

/// One champion of the bench grid: its stats, and whether it is the best swap.
#[derive(Clone, Copy, Debug)]
struct BenchCell {
    id: i64,
    row: Option<ChampRow>,
    best: bool,
}

/// The cells of the bench grid in the order of `bench_order`. The champion of
/// `best_swap` against your stats (`mine`) is marked.
fn bench_cells(
    mut bench: Vec<(i64, Option<ChampRow>)>,
    mine: Option<ChampRow>,
    min_games: u32,
) -> Vec<BenchCell> {
    bench.sort_by(|a, b| bench_order(a.1, b.1));
    let best = best_swap(mine, &bench, min_games);
    bench
        .into_iter()
        .map(|(id, row)| BenchCell {
            id,
            row,
            best: best == Some(id),
        })
        .collect()
}

/// The place of each icon of the bench grid in a card that is `width` points wide,
/// from its top left corner: `BENCH_COLS` icons in a row, and the gap between the
/// rows is the gap between the icons.
fn bench_grid(width: f32, n: usize) -> Vec<Rect> {
    let cols = BENCH_COLS as f32;
    let gap = ((width - cols * BENCH_ICON) / (cols - 1.0)).max(4.0);
    (0..n)
        .map(|i| {
            let (row, col) = ((i / BENCH_COLS) as f32, (i % BENCH_COLS) as f32);
            Rect::from_min_size(
                pos2(col * (BENCH_ICON + gap), row * (BENCH_ICON + gap)),
                Vec2::splat(BENCH_ICON),
            )
        })
        .collect()
}

/// The tier badge of the bench icon at `icon`, on its lower right corner.
fn bench_badge(icon: Rect) -> Rect {
    Rect::from_min_size(
        icon.max - Vec2::splat(BENCH_BADGE - BENCH_BADGE_OUT),
        Vec2::splat(BENCH_BADGE),
    )
}

/// The part of a bench icon that takes the hover and the click: the icon, and its
/// badge with the ring, so a click on the badge asks for the swap too.
fn bench_hit(icon: Rect) -> Rect {
    icon.union(bench_badge(icon).expand(BENCH_BADGE_RING))
}

/// Paint the tier badge of a champion on the lower right corner of its 40 point icon
/// (`icon`), on a ring of the card color so it reads over the portrait. A champion
/// without stats gets the muted dash. The bench grid and the trade card use it.
fn paint_corner_badge(p: &egui::Painter, icon: Rect, row: Option<ChampRow>) {
    let badge = bench_badge(icon);
    p.rect_filled(
        badge.expand(BENCH_BADGE_RING),
        CornerRadius::same((BENCH_BADGE * 0.28) as u8 + BENCH_BADGE_RING as u8),
        SURFACE,
    );
    match row {
        Some(r) => kit::paint_tier(p, badge, Some(r.tier)),
        None => kit::paint_no_stats(p, badge),
    }
}

/// The place of each icon of the first step in a card that is `width` points wide, from
/// its top left corner: `CHOICE_COLS` icons in a row, each one centered in its column.
fn choice_grid(width: f32, n: usize) -> Vec<Rect> {
    let col_w = width / CHOICE_COLS as f32;
    (0..n)
        .map(|i| {
            let (row, col) = ((i / CHOICE_COLS) as f32, (i % CHOICE_COLS) as f32);
            Rect::from_min_size(
                pos2(
                    col * col_w + (col_w - CHOICE_ICON) / 2.0,
                    row * CHOICE_ROW_H,
                ),
                Vec2::splat(CHOICE_ICON),
            )
        })
        .collect()
}

/// The tier badge of the icon of the first step at `icon`, on its lower right corner.
fn choice_badge(icon: Rect) -> Rect {
    Rect::from_min_size(
        icon.max - Vec2::splat(CHOICE_BADGE - CHOICE_BADGE_OUT),
        Vec2::splat(CHOICE_BADGE),
    )
}

/// The tier of a champion as its letters ("S+", "C"), or "—" without stats.
fn tier_text(row: Option<ChampRow>) -> &'static str {
    row.map_or("—", |r| TIER_NAMES[r.tier.min(5) as usize])
}

/// The line under the bench grid in four parts: text, the bench champion, text,
/// and your champion. English reads "Best swap: Lux (S) → your Garen (C)".
fn best_swap_parts(lang: Lang, theirs: &str, yours: &str) -> [String; 4] {
    let template = lang.t(Key::BenchBestSwap);
    let (head, rest) = template.split_once("{0}").unwrap_or((template, ""));
    let (mid, tail) = rest.split_once("{1}").unwrap_or((rest, ""));
    [
        head.to_string(),
        theirs.to_string(),
        mid.to_string(),
        format!("{yours}{tail}"),
    ]
}

/// The champion whose item sets the window asks for now, or `None`. Once per
/// champion in a Mayhem champ select, and only when the data is `ready` (the
/// stats of the champion and the static data): before that, the worker could
/// not write the sets.
fn item_set_due(
    on: bool,
    phase: Phase,
    queue: Option<i64>,
    mine: Option<i64>,
    ready: bool,
    asked: Option<i64>,
) -> Option<i64> {
    let c = mine
        .filter(|_| on && ready && phase == Phase::ChampSelect && queue.is_some_and(is_mayhem))?;
    (asked != Some(c)).then_some(c)
}

/// The newest saved game, when it is not the one that was newest before the
/// game ended (`seen`). The history thread saves a game some seconds after it ends.
fn fresh_game(history: &[Game], seen: Option<i64>) -> Option<&Game> {
    history.first().filter(|g| Some(g.id) != seen)
}

/// A game length as "21:40". `None` for a length that is not known (0).
fn clock(secs: u32) -> Option<String> {
    (secs > 0).then(|| format!("{}:{:02}", secs / 60, secs % 60))
}

/// The mark of a value that is not known: a game with no length (0) or no date.
const UNKNOWN: &str = "–";

/// A game length as "21:40", or "–" when the length is not known (0).
fn length_text(secs: u32) -> String {
    clock(secs).unwrap_or_else(|| UNKNOWN.to_string())
}

/// A UTC instant as the clock and the calendar of a zone show it.
#[derive(Debug, PartialEq)]
struct Moment {
    /// The local day as a number (days since 1970-01-01). Two moments of the same local day share it.
    day_no: i64,
    year: i64,
    month: u32,
    day: u32,
    hour: u32,
    minute: u32,
}

/// The `Moment` of a UTC instant (Unix seconds) in a zone that is `offset` seconds ahead of UTC.
fn moment(utc: i64, offset: i64) -> Moment {
    let local = utc + offset;
    let day_no = local.div_euclid(86_400);
    let (year, month, day) = history::civil_from_days(day_no);
    let rest = local.rem_euclid(86_400);
    Moment {
        day_no,
        year,
        month,
        day,
        hour: (rest / 3600) as u32,
        minute: (rest % 3600 / 60) as u32,
    }
}

/// The short date of a game for its row, in local time. `date` is the UTC text that the file
/// keeps ("2026-10-23T09:20:31Z"), `now` is the Unix time in seconds, and `zone` gives the
/// offset of the local zone at a UTC instant (daylight saving changes it during the year).
///
/// - the same local day as now: "Today 15:31" ("Bugün 15:31")
/// - the day before: "Yesterday 21:04" ("Dün 21:04")
/// - older: "Oct 26" ("26 Eki"), with the year ("Oct 26, 2025") when it is not this year
/// - a game with no date: "–"
///
/// A game that lies after now (the clock of the PC went back) reads as today.
fn game_date(lang: Lang, date: &str, now: i64, zone: &dyn Fn(i64) -> i64) -> String {
    let Some(at) = history::parse_iso(date) else {
        return UNKNOWN.to_string();
    };
    let (then, today) = (moment(at, zone(at)), moment(now, zone(now)));
    let time = format!("{:02}:{:02}", then.hour, then.minute);
    match today.day_no - then.day_no {
        ..=0 => lang.fmt(Key::GameToday, &[&time]),
        1 => lang.fmt(Key::GameYesterday, &[&time]),
        _ if then.year == today.year => {
            lang.fmt(Key::GameDate, &[&then.day, &lang.month(then.month)])
        }
        _ => lang.fmt(
            Key::GameDateYear,
            &[&then.day, &lang.month(then.month), &then.year],
        ),
    }
}

/// How many rows "Recent games" lists: `GAMES_FIRST`, or with "Show all" up to `GAMES_MAX`.
/// Never more than the games there are.
fn games_listed(total: usize, all: bool) -> usize {
    total.min(if all { GAMES_MAX } else { GAMES_FIRST })
}

/// The Unix time now, in seconds.
fn unix_now() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs() as i64)
}

/// The line under the result of a game: champion, length, and the augments. The
/// length stays out when it is not known.
fn result_line(lang: Lang, champion: &str, g: &Game) -> String {
    let augments = lang.fmt(
        if g.augments.len() == 1 {
            Key::CapAugment1
        } else {
            Key::CapAugments
        },
        &[&g.augments.len()],
    );
    let parts = [Some(champion.to_string()), clock(g.secs), Some(augments)];
    parts.into_iter().flatten().collect::<Vec<_>>().join(" · ")
}

/// "Left", "Middle" or "Right" (Turkish: "Sol", "Orta", "Sağ") for the x center
/// of a card (0..1).
fn position_label(lang: Lang, x: f32) -> &'static str {
    lang.t(if x < 0.4 {
        Key::PosLeft
    } else if x > 0.6 {
        Key::PosRight
    } else {
        Key::PosMiddle
    })
}

/// Name of a hotkey on the screen. `key_label` has the names that read the
/// same in every language (F8, A, 5). Other codes show as "Key 45" / "Tuş 45".
fn key_text(lang: Lang, vk: i32) -> String {
    match vk {
        0x70..=0x7B | 0x30..=0x39 | 0x41..=0x5A => key_label(vk),
        _ => lang.fmt(Key::HotkeyCode, &[&vk]),
    }
}

/// The cards that the pick screens before `screen` offered, in screen order.
fn earlier_offers(offered: &[(usize, Vec<i64>)], screen: usize) -> Vec<i64> {
    let mut v: Vec<&(usize, Vec<i64>)> = offered.iter().filter(|o| o.0 < screen).collect();
    v.sort_by_key(|o| o.0);
    v.into_iter().flat_map(|o| o.1.iter().copied()).collect()
}

/// The top augment pairs of a champion: pairs with enough games, the first
/// `TOP_COMBOS` in the order of the source.
fn top_combos(cs: &ChampStats, min_games: u32) -> impl Iterator<Item = &Combo> {
    cs.combos
        .iter()
        .filter(move |c| c.games >= min_games)
        .take(TOP_COMBOS)
}

/// A top pair that links an offered augment to a card of an earlier screen.
#[derive(Debug, PartialEq)]
pub struct PairHint {
    /// The earlier card that the offered augment pairs with. The hint carries no
    /// number: Hexsight shows no augment win rate and nothing derived from one.
    pub with: i64,
}

/// The best top pair between `offered` and one of the `earlier` cards (the cards
/// that the earlier pick screens offered). Hexsight cannot know which card the
/// player took, so the pick views show this as a hint ("if picked earlier"). It
/// never takes part in the highlight. The win rate of a pair only picks the best
/// pair here. It is never shown.
pub fn pairs_hint(
    cs: &ChampStats,
    offered: i64,
    earlier: &[i64],
    min_games: u32,
) -> Option<PairHint> {
    let mut best: Option<PairHint> = None;
    let mut best_rate: Option<f32> = None;
    for c in top_combos(cs, min_games).filter(|c| c.augments.contains(&offered)) {
        let Some(&with) = earlier
            .iter()
            .find(|&&e| e != offered && c.augments.contains(&e))
        else {
            continue;
        };
        if best_rate.is_none_or(|r| c.win_rate > r) {
            best_rate = Some(c.win_rate);
            best = Some(PairHint { with });
        }
    }
    best
}

/// The line of the pairs hint in three parts: text, the name, and text. English
/// reads "Pairs well with <name> if picked earlier". Turkish puts words on both
/// sides of the name ("Önceden seçtiysen <name> ile iyi gider"), so the parts
/// come from the template (`Key::PairsLine`: `{0}` is the name). A template without
/// `{0}` (`Key::PairsLineEarlier`) comes back whole in the first part.
/// The line has no number. When the line is too wide (`fits` says no), the name
/// gets shorter and ends with "…". The rest stays, because "if picked earlier"
/// says that Hexsight does not know the pick.
fn pairs_line(template: &str, name: &str, fits: impl Fn(&str) -> bool) -> [String; 3] {
    let (head, tail) = template.split_once("{0}").unwrap_or((template, ""));
    let mut chars: Vec<char> = name.chars().collect();
    let mut shown = name.to_string();
    while !fits(&format!("{head}{shown}{tail}")) && chars.len() > 1 {
        chars.pop();
        shown = format!("{}…", chars.iter().collect::<String>().trim_end());
    }
    [head.to_string(), shown, tail.to_string()]
}

/// Does the overlay window let the mouse pass to the game? Only in a game, and
/// only while the overlay is locked. Outside a game (the overlay opened with the
/// hotkey), it takes clicks, so a click on the pill closes it. While it is
/// unlocked (to move it), it takes the mouse too.
fn passes_mouse(overlay: bool, phase: Phase, unlocked: bool) -> bool {
    overlay && phase == Phase::InGame && !unlocked
}

/// The offer that the overlay strip shows. While the reader has cards (`current`),
/// the strip shows them. After they go away, the strip stays `CARD_HOLD` longer with
/// the last offer: the first value is the offer to show, and the second is the
/// time that the hold has left (`None` while the cards show, and after the hold).
/// `held` keeps the last offer, and the time when the cards went away.
fn held_card(
    held: &mut Option<(Offer, Option<std::time::Instant>)>,
    current: Option<&Offer>,
    now: std::time::Instant,
) -> (Option<Offer>, Option<std::time::Duration>) {
    if let Some(o) = current {
        *held = Some((o.clone(), None));
        return (Some(o.clone()), None);
    }
    let Some((offer, gone)) = held else {
        return (None, None);
    };
    let since = *gone.get_or_insert(now);
    match CARD_HOLD
        .checked_sub(now.saturating_duration_since(since))
        .filter(|left| !left.is_zero())
    {
        Some(left) => (Some(offer.clone()), Some(left)),
        None => {
            *held = None;
            (None, None)
        }
    }
}

/// What the overlay shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum OverlayMode {
    /// The small pill "Hexsight watching": no pick screen is open.
    Pill,
    /// The pick strip: the reader found the cards.
    Strip,
    /// The advice card: the next items and the summary of the game. It shows for a short time
    /// (`advice_visible`).
    Advice,
}

/// The mode of the overlay. The strip shows whenever the reader has cards, and it wins over the
/// advice card. The advice card shows when `advice_visible` says so. Else the pill.
fn overlay_mode(has_offer: bool, show_advice: bool) -> OverlayMode {
    if has_offer {
        OverlayMode::Strip
    } else if show_advice {
        OverlayMode::Advice
    } else {
        OverlayMode::Pill
    }
}

/// How long the advice card of the overlay stays after a change of the advice.
const ADVICE_SHOW: std::time::Duration = std::time::Duration::from_secs(12);

/// Does the overlay show the advice card? Only with advice that has something to show (`has_content`).
/// Then: while you are dead (you can shop), for `ADVICE_SHOW` after the list of items changed
/// (`since_change`; a `ready` flag that flips with the gold does not count), and while the overlay
/// is unlocked (to drag it). Else the pill shows.
fn advice_visible(
    has_content: bool,
    dead: bool,
    unlocked: bool,
    since_change: Option<std::time::Duration>,
) -> bool {
    has_content && (dead || unlocked || since_change.is_some_and(|d| d < ADVICE_SHOW))
}

/// Zoom factor of an overlay size: 0 small, 1 medium, 2 large.
fn overlay_zoom(size: u8) -> f32 {
    OVERLAY_ZOOMS[(size as usize).min(2)]
}

/// A window place in points at the current zoom -> the place to save, in
/// points at zoom 1.0. The physical place is the same for each zoom.
fn place_to_saved(p: egui::Pos2, zoom: f32) -> [f32; 2] {
    [p.x * zoom, p.y * zoom]
}

/// The saved place -> points at the current zoom.
fn place_from_saved(saved: [f32; 2], zoom: f32) -> egui::Pos2 {
    pos2(saved[0] / zoom, saved[1] / zoom)
}

/// The two hotkeys after the user binds `vk` to `which`. `None` when `vk` is
/// the other hotkey already: two actions cannot share one key.
fn rebound(now: Keys, which: Hotkey, vk: i32) -> Option<Keys> {
    match which {
        Hotkey::Overlay if vk != now.scan => Some(Keys { overlay: vk, ..now }),
        Hotkey::Scan if vk != now.overlay => Some(Keys { scan: vk, ..now }),
        _ => None,
    }
}

/// What a look at the key events found, while the Settings page waits for a
/// new hotkey.
#[derive(Debug, PartialEq)]
enum Rebind {
    Wait,
    Cancel,
    Key(i32),
}

/// The first key press decides. Esc cancels. A key that cannot be a hotkey
/// (see `win::vk_for_key`) and a key repeat do not count.
fn rebind_from(events: &[egui::Event]) -> Rebind {
    for e in events {
        if let egui::Event::Key {
            key,
            pressed: true,
            repeat: false,
            ..
        } = e
        {
            if *key == egui::Key::Escape {
                return Rebind::Cancel;
            }
            if let Some(vk) = crate::win::vk_for_key(*key) {
                return Rebind::Key(vk);
            }
        }
    }
    Rebind::Wait
}

/// What Settings > Card reading tools shows, as plain values.
#[derive(Clone, Default)]
struct ReadView {
    in_game: bool,
    ocr_language: Option<String>,
    game_window: Option<bool>,
    window_mode: Option<i32>,
    champ_source: Option<ChampSource>,
    /// Seconds since the last scan. `None` without a scan.
    scan_age: Option<u64>,
    /// Text lines of the last scan, without the lines under the overlay.
    lines: usize,
    /// Lines of the last scan that sat under the overlay.
    dropped: usize,
    matched: usize,
    engine_failed: bool,
    /// Pick screens due at this level, and the ones done.
    due: usize,
    done: usize,
    /// How the capture of the game window looks: fine, black, or frozen.
    capture: CaptureHealth,
}

/// Time since an event in short words: "12 s ago", "3 min ago", "2 h ago".
/// Turkish: "12 sn önce", "3 dk önce", "2 sa önce".
fn age_text(lang: Lang, secs: u64) -> String {
    match secs {
        0..60 => lang.fmt(Key::AgeSeconds, &[&secs]),
        60..3600 => lang.fmt(Key::AgeMinutes, &[&(secs / 60)]),
        _ => lang.fmt(Key::AgeHours, &[&(secs / 3600)]),
    }
}

/// "Borderless", "Windowed" or "Fullscreen": the `WindowMode` of League.
fn window_mode_label(lang: Lang, mode: Option<i32>) -> &'static str {
    lang.t(match mode {
        Some(2) => Key::ModeBorderless,
        Some(1) => Key::ModeWindowed,
        Some(0) => Key::ModeFullscreen,
        _ => Key::ModeUnknown,
    })
}

/// The word for a capture that is fine, black, or frozen.
fn capture_key(h: CaptureHealth) -> Key {
    match h {
        CaptureHealth::Fine => Key::CaptureOk,
        CaptureHealth::Black => Key::CaptureBlack,
        CaptureHealth::Frozen => Key::CaptureFrozen,
        CaptureHealth::Locked => Key::CaptureLocked,
        CaptureHealth::Remote => Key::CaptureRemote,
    }
}

/// The one-line hint for a black or a frozen capture. `None` for a fine capture.
fn capture_hint(lang: Lang, h: CaptureHealth) -> Option<&'static str> {
    match h {
        CaptureHealth::Fine => None,
        CaptureHealth::Black => Some(lang.t(Key::CaptureBlackHint)),
        CaptureHealth::Frozen => Some(lang.t(Key::CaptureFrozenHint)),
        CaptureHealth::Locked => Some(lang.t(Key::CaptureLockedHint)),
        CaptureHealth::Remote => Some(lang.t(Key::CaptureRemoteHint)),
    }
}

/// The rows of Settings > Card reading tools: label, value, and whether the value
/// is a warning. The saved text file starts with the same rows (always in English).
fn read_rows(r: &ReadView, lang: Lang) -> Vec<(&'static str, String, bool)> {
    let outside = || lang.t(Key::ReadNotInGame).to_string();
    let no_scan = || lang.t(Key::ReadNoScan).to_string();
    let scanned = |text: String| {
        if r.scan_age.is_some() {
            text
        } else {
            no_scan()
        }
    };
    let lines = if r.engine_failed {
        lang.t(Key::ReadEngineFailed).to_string()
    } else if r.dropped > 0 {
        lang.fmt(Key::ReadLinesUnder, &[&(r.lines + r.dropped), &r.dropped])
    } else {
        r.lines.to_string()
    };
    let source = lang.t(match r.champ_source {
        Some(ChampSource::ChampSelect) => Key::SourceChampSelect,
        Some(ChampSource::LiveClient) => Key::SourceLive,
        None => Key::SourceUnknown,
    });
    let window = match r.game_window {
        Some(true) => lang.t(Key::ReadFound).to_string(),
        Some(false) => lang.t(Key::ReadNotFound).to_string(),
        None => outside(),
    };
    vec![
        // The name of the language, not its tag: "English (United States)".
        (
            lang.t(Key::ReadOcrLanguage),
            r.ocr_language.as_deref().map_or_else(
                || lang.t(Key::ReadStartsInGame).into(),
                |tag| lang.language_name(tag),
            ),
            false,
        ),
        (
            lang.t(Key::ReadGameWindow),
            window,
            r.game_window == Some(false),
        ),
        (
            lang.t(Key::ReadWindowMode),
            window_mode_label(lang, r.window_mode).to_string(),
            r.window_mode == Some(0),
        ),
        (lang.t(Key::ReadChampSource), source.to_string(), false),
        (
            lang.t(Key::ReadCapture),
            // A bad capture shows at once. A fine one needs a scan to say so.
            if r.capture == CaptureHealth::Fine {
                scanned(lang.t(Key::CaptureOk).to_string())
            } else {
                lang.t(capture_key(r.capture)).to_string()
            },
            r.capture != CaptureHealth::Fine,
        ),
        (
            lang.t(Key::ReadLastScan),
            r.scan_age.map_or_else(no_scan, |s| age_text(lang, s)),
            false,
        ),
        (lang.t(Key::ReadLines), scanned(lines), r.engine_failed),
        (
            lang.t(Key::ReadMatched),
            scanned(r.matched.to_string()),
            false,
        ),
        (
            lang.t(Key::ReadPickScreens),
            if r.in_game {
                lang.fmt(Key::ReadPickScreensValue, &[&r.due, &r.done])
            } else {
                outside()
            },
            false,
        ),
    ]
}

/// 0 to 1 over `secs` when `key` shows for the first time. egui starts the
/// animation of a new id at its end, so the first frame asks for "off".
fn appear(ctx: &egui::Context, key: egui::Id, secs: f32) -> f32 {
    let seen = ctx.data_mut(|d| std::mem::replace(d.get_temp_mut_or(key, false), true));
    if !seen {
        ctx.request_repaint();
    }
    ctx.animate_bool_with_time_and_easing(key, seen, secs, egui::emath::easing::cubic_out)
}

/// Games that a win rate needs before it gets a color. With fewer, the rate says little
/// (one win in one game is 100 %), so it stays white.
const RATE_COLOR_GAMES: u32 = 10;

/// Color of one of your win rates over `games` games: green from 55 %, red up to 45 %,
/// and white (`LABEL`) with fewer than `RATE_COLOR_GAMES` games.
fn rate_color(rate: f32, games: u32) -> Color32 {
    if games < RATE_COLOR_GAMES {
        LABEL
    } else if rate >= 0.55 {
        GREEN
    } else if rate <= 0.45 {
        RED
    } else {
        LABEL
    }
}

/// The test hook `HEXSIGHT_TRADE=accept:101:8`: the call, the trade id, and the
/// seconds after the start.
fn parse_debug_trade(text: &str) -> Option<(TradeCall, i64, f32)> {
    let mut parts = text.split(':');
    let call = TradeCall::parse(parts.next()?)?;
    let id = parts.next()?.trim().parse().ok()?;
    let seconds = parts
        .next()
        .and_then(|s| s.trim().parse().ok())
        .unwrap_or(5.0);
    Some((call, id, seconds))
}

/// The marks of the last ten results: a win is a filled green disc, a loss is a red
/// ring with no fill, so the result does not depend on the color alone.
const RESULT_DOT_STEP: f32 = 14.0;
const WIN_DOT_R: f32 = 5.0;
const LOSS_DOT_R: f32 = 4.25;
const LOSS_DOT_STROKE: f32 = 1.5;

/// Paint the result mark of one game with its center at `at`.
fn paint_result_dot(p: &egui::Painter, at: egui::Pos2, win: bool) {
    if win {
        p.circle_filled(at, WIN_DOT_R, GREEN);
    } else {
        p.circle_stroke(at, LOSS_DOT_R, egui::Stroke::new(LOSS_DOT_STROKE, RED));
    }
}

/// Two arrows that point both ways: a swap. A drawing, so no font is needed.
fn paint_swap_arrows(p: &egui::Painter, center: egui::Pos2, color: Color32) {
    let stroke = egui::Stroke::new(1.6, color);
    // The upper arrow points right, the lower one points left.
    for (dy, dir) in [(-4.5, 1.0), (4.5, -1.0)] {
        let from = pos2(center.x - 7.0 * dir, center.y + dy);
        let to = pos2(center.x + 7.0 * dir, center.y + dy);
        p.line_segment([from, to], stroke);
        p.line_segment([to, pos2(to.x - 3.5 * dir, to.y - 3.0)], stroke);
        p.line_segment([to, pos2(to.x - 3.5 * dir, to.y + 3.0)], stroke);
    }
}

/// The look of a `block_button`.
struct BlockButton<'a> {
    text: &'a str,
    /// The fill color. It keeps `rest` of its strength at rest and `hover` under the mouse.
    color: Color32,
    ink: Color32,
    font: FontId,
    radius: u8,
    rest: f32,
    hover: f32,
}

/// A button of a set size, for the places where buttons share a row in equal parts or fill
/// it (`kit::accent_button` is as wide as its text). The text sits in the middle and ends
/// with "…" when it is too wide.
fn block_button(ui: &mut Ui, size: Vec2, b: BlockButton) -> egui::Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    ui.painter().rect_filled(
        rect,
        CornerRadius::same(b.radius),
        b.color.gamma_multiply(b.rest + (b.hover - b.rest) * t),
    );
    let mut job = egui::text::LayoutJob::single_section(
        b.text.to_string(),
        egui::TextFormat::simple(b.font, b.ink),
    );
    job.wrap.max_width = (rect.width() - 16.0).max(10.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let g = ui.painter().layout_job(job);
    ui.painter()
        .galley(rect.center() - g.size() / 2.0, g, b.ink);
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp
}

/// A trade button of a set size. It shows dim and does nothing while `on` is false (a
/// click is on its way). Returns true on a click.
fn trade_button(ui: &mut Ui, size: Vec2, text: &str, color: Color32, on: bool) -> bool {
    let (rest, hover) = if on { (0.85, 1.0) } else { (0.34, 0.34) };
    let look = BlockButton {
        text,
        color,
        ink: Color32::WHITE,
        font: FontId::new(12.5, semibold()),
        radius: 255,
        rest,
        hover,
    };
    block_button(ui, size, look).clicked() && on
}

/// The width of a keycap with this text: the key of a hotkey row, in points.
fn keycap_width(ui: &Ui, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_string(), FontId::new(12.0, semibold()), LABEL)
        .size()
        .x
        + 16.0
}

/// A hotkey as a keycap: a thin edge, no fill, small corners (radius 6), 12 points
/// semibold, so it does not look like the button "Change" beside it. While the row waits
/// for a new key (`waiting`), the edge and the text are orange.
fn keycap(ui: &mut Ui, text: &str, waiting: bool) -> egui::Response {
    let (ink, edge) = if waiting {
        (ORANGE, ORANGE)
    } else {
        (LABEL, LABEL_3)
    };
    let g = ui
        .painter()
        .layout_no_wrap(text.to_string(), FontId::new(12.0, semibold()), ink);
    let (rect, resp) =
        ui.allocate_exact_size(vec2(g.size().x + 16.0, g.size().y + 8.0), Sense::hover());
    ui.painter().rect_stroke(
        rect,
        CornerRadius::same(6),
        egui::Stroke::new(1.0, edge),
        egui::StrokeKind::Inside,
    );
    ui.painter().galley(rect.center() - g.size() / 2.0, g, ink);
    resp
}

/// The section header with a note at its right end, on the same line. The header looks like
/// `kit::section` (11 points, semibold, `LABEL_3`, 6 points above and 2 below). The note is
/// a text with its color, for example the build style or "In shop". A note that starts with
/// an icon glyph (`glyph::CHECK`) draws the glyph in the icon font, 4 points before the text.
fn section_note(ui: &mut Ui, text: &str, note: Option<(&str, Color32)>) {
    ui.add_space(6.0);
    let font = FontId::new(11.0, semibold());
    let g = ui
        .painter()
        .layout_no_wrap(text.to_string(), font.clone(), LABEL_3);
    let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), g.size().y), Sense::hover());
    ui.painter().galley(row.left_top(), g, LABEL_3);
    if let Some((note, color)) = note {
        // A glyph of the icon font sits in the private use area of Unicode.
        let mut chars = note.chars();
        let glyph = chars
            .next()
            .filter(|c| ('\u{E000}'..='\u{F8FF}').contains(c));
        let body = if glyph.is_some() {
            chars.as_str().trim_start()
        } else {
            note
        };
        let n = ui
            .painter()
            .layout_no_wrap(body.to_string(), font.clone(), color);
        let icon = glyph.map(|c| {
            ui.painter()
                .layout_no_wrap(c.to_string(), FontId::new(11.0, icons()), color)
        });
        let icon_w = icon.as_ref().map_or(0.0, |i| i.size().x + 4.0);
        let left = (row.right() - n.size().x - icon_w).max(row.left());
        if let Some(i) = icon {
            ui.painter()
                .galley(pos2(left, row.center().y - i.size().y / 2.0), i, color);
        }
        ui.painter().galley(
            pos2(left + icon_w, row.center().y - n.size().y / 2.0),
            n,
            color,
        );
    }
    ui.add_space(2.0);
}

/// How far a first-run check is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Check {
    /// Done: a green check mark.
    Ok,
    /// It needs the player: an orange exclamation mark.
    Warn,
    /// No answer yet (the client has not started, the data loads): three grey dots.
    Wait,
}

/// What the first-run checklist looks at.
struct FirstRun {
    league_found: bool,
    /// `WindowMode` of League (`game.cfg`): 0 Fullscreen, 1 Windowed, 2 Borderless.
    window_mode: Option<i32>,
    /// The OCR engine can start. `None` while Hexsight looks for it.
    ocr_probe: Option<bool>,
    data_loaded: bool,
    data_failed: bool,
    /// The two hotkeys (Win32 virtual keys), and whether no other app holds each one.
    hotkeys: [(i32, bool); 2],
}

/// The checks of the first-run card, in order: League found, Borderless,
/// OCR engine, data loaded, hotkeys free. Each has its state and its text. The
/// window mode has no row while it is unknown (no game.cfg yet): that is no
/// problem, and the overlay warns in a game that runs in Fullscreen.
fn first_run_checks(lang: Lang, f: &FirstRun) -> Vec<(Check, String)> {
    let t = |c: Check, k: Key| (c, lang.t(k).to_string());
    let league = if f.league_found {
        t(Check::Ok, Key::CheckLeagueOk)
    } else {
        t(Check::Warn, Key::CheckLeagueNo)
    };
    let window = match f.window_mode {
        Some(2) => Some(t(Check::Ok, Key::CheckBorderlessOk)),
        Some(1) => Some(t(Check::Ok, Key::CheckWindowedOk)),
        Some(0) => Some(t(Check::Warn, Key::CheckBorderlessNo)),
        _ => None,
    };
    let ocr = match f.ocr_probe {
        Some(true) => t(Check::Ok, Key::CheckOcrOk),
        Some(false) => t(Check::Warn, Key::CheckOcrNo),
        None => t(Check::Wait, Key::CheckOcrWait),
    };
    let data = if f.data_loaded {
        t(Check::Ok, Key::CheckDataOk)
    } else if f.data_failed {
        t(Check::Warn, Key::CheckDataNo)
    } else {
        t(Check::Wait, Key::CheckDataWait)
    };
    let hotkeys = match f.hotkeys.iter().find(|(_, free)| !free) {
        Some((vk, _)) => (
            Check::Warn,
            lang.fmt(Key::CheckHotkeyNo, &[&key_text(lang, *vk)]),
        ),
        None => (
            Check::Ok,
            lang.fmt(
                Key::CheckHotkeyOk,
                &[
                    &key_text(lang, f.hotkeys[0].0),
                    &key_text(lang, f.hotkeys[1].0),
                ],
            ),
        ),
    };
    [Some(league), window, Some(ocr), Some(data), Some(hotkeys)]
        .into_iter()
        .flatten()
        .collect()
}

/// The rows the first-run card shows: one line `all_ok` when every check
/// passes, else only the checks that wait or need the player, in their order.
fn checks_to_show(rows: Vec<(Check, String)>, all_ok: &str) -> Vec<(Check, String)> {
    if rows.iter().all(|r| r.0 == Check::Ok) {
        return vec![(Check::Ok, all_ok.to_string())];
    }
    rows.into_iter().filter(|r| r.0 != Check::Ok).collect()
}

/// The buttons that the first-run card shows under the rows, one for each check that
/// the player can fix with a click.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
struct FirstRunFixes {
    /// The data did not load: "Try again" asks the worker for it again.
    retry: bool,
    /// The OCR engine cannot start: "Language settings" opens the Windows page for it.
    language: bool,
    /// Another app holds a hotkey: "Change" opens Settings.
    hotkey: bool,
}

fn first_run_fixes(f: &FirstRun) -> FirstRunFixes {
    FirstRunFixes {
        retry: !f.data_loaded && f.data_failed,
        language: f.ocr_probe == Some(false),
        hotkey: f.hotkeys.iter().any(|(_, free)| !free),
    }
}

/// Whether the card after a game asks "Start with Windows?". It asks once, when
/// Windows does not start Hexsight already. `debug` (HEXSIGHT_FIRST_RUN) asks
/// again. A portable copy never asks: its users rarely want a registry entry.
/// The switch in Settings stays.
fn asks_autostart(place: crate::cache::Place, debug: bool, asked: bool, on: bool) -> bool {
    !place.portable() && (debug || (!asked && !on))
}

/// The question "Start with Windows in the background?" with its two buttons. Yes is the
/// default: it has the color. Returns whether the player clicked Yes, and whether No.
fn autostart_question(ui: &mut Ui, lang: Lang) -> (bool, bool) {
    ui.add(egui::Label::new(strong(lang.t(Key::AskAutostart), 12.5)).wrap());
    ui.add_space(2.0);
    let (mut yes, mut no) = (false, false);
    ui.horizontal(|ui| {
        yes = kit::accent_button(ui, lang.t(Key::AskYes), BLUE).clicked();
        no = kit::accent_button(ui, lang.t(Key::AskNo), Color32::from_white_alpha(40)).clicked();
    });
    (yes, no)
}

/// The extra line of the About group in portable mode. The normal mode has none.
fn portable_line(place: crate::cache::Place) -> Option<Key> {
    match place {
        crate::cache::Place::Local => None,
        crate::cache::Place::Portable => Some(Key::AboutPortable),
        crate::cache::Place::PortableFallback => Some(Key::AboutPortableFallback),
    }
}

/// Open an address in the default browser. Only a click of the player calls it.
fn open_url(url: &str) {
    let _ = std::process::Command::new("explorer").arg(url).spawn();
}

/// The text of the button "Fix" and whether it works now. With no League Client
/// (the phase is Offline) it says to open League and does nothing. For `FIX_PAUSE`
/// after a click it does nothing, so the change is not sent twice.
fn fix_label(phase: Phase, since_click: Option<std::time::Duration>) -> (Key, bool) {
    if phase == Phase::Offline {
        return (Key::FixOpenLeague, false);
    }
    (
        Key::FixBorderless,
        since_click.is_none_or(|t| t >= FIX_PAUSE),
    )
}

/// What the feedback page gets: the version, Windows, and the main settings, in
/// English. Never a Riot ID, a path, or anything from a game.
fn feedback_of(
    s: &Settings,
    place: crate::cache::Place,
    windows_build: u32,
) -> update::Feedback<'static> {
    let on = |b: bool| if b { "on" } else { "off" }.to_string();
    let language = match s.language {
        LangPref::Auto => "auto",
        LangPref::English => "English",
        LangPref::Turkish => "Turkish",
    };
    let size = ["small", "medium", "large"][usize::from(s.overlay_scale.min(2))];
    update::Feedback {
        version: update::VERSION,
        windows: update::windows_label(windows_build),
        settings: vec![
            ("Language", language.to_string()),
            ("Show the overlay when a game starts", on(s.auto_overlay)),
            (
                "Hide the panel from recordings and screenshots",
                on(!s.overlay_in_captures),
            ),
            ("Highlight top-tier cards", on(s.top_highlight)),
            ("Show pair hints", on(s.pair_hints)),
            ("Badges on augment cards (beta)", on(s.card_badges)),
            ("Send the build to the in-game shop", on(s.item_sets)),
            ("Overlay size", size.to_string()),
            ("Check for new versions", on(s.check_updates)),
            ("Portable mode", on(place.portable())),
        ],
    }
}

/// The summoner spell pair to show in champ select: the pair that players take most
/// (`data::top_spell_pair`) when the names and icons of both spells are known. The
/// card never picks a spell for the player.
fn spell_suggestion(cs: &ChampStats, sd: &StaticData, min_games: u32) -> Option<[i64; 2]> {
    crate::data::top_spell_pair(&cs.spells, min_games)
        .filter(|pair| pair.iter().all(|id| sd.spells.contains_key(id)))
}

// ---------------------------------------------------------------------------
// The suggestions: the words of a reason, the summary of the game, and the card sizes.

/// Height of a row of the card "Next items", in points.
const ITEM_ROW_H: f32 = 40.0;

/// Room at the right of an item row for the chip (the check mark or the missing gold), in points.
const ITEM_CHIP_W: f32 = 44.0;

/// The icon of an item in the card "Next items", and of a champion in the summary card.
const ITEM_ICON: f32 = 32.0;
const SUMMARY_ICON: f32 = 20.0;

/// The most champions that the summary card shows for healers and for tanks.
const SUMMARY_ICONS: usize = 3;

/// The augment icons of the card after a game: their size, the space between them, and the
/// radius of their corners, in points.
const TOOK_ICON: f32 = 24.0;
const TOOK_GAP: f32 = 4.0;
const TOOK_RADIUS: u8 = 6;

/// How many augment icons (`TOOK_ICON`, `TOOK_GAP` apart) fit in `room` points.
fn took_icons_that_fit(room: f32) -> usize {
    ((room + TOOK_GAP) / (TOOK_ICON + TOOK_GAP))
        .floor()
        .max(0.0) as usize
}

/// The words of a reason, in the language. A name that Hexsight does not know (an augment) reads as "an earlier pick".
fn reason_text(lang: Lang, sd: &StaticData, r: Reason) -> String {
    match r {
        Reason::Core(n) => lang.fmt(Key::ReasonCore, &[&n]),
        Reason::Boots => lang.t(Key::ReasonBoots).to_string(),
        Reason::Later => lang.t(Key::ReasonLater).to_string(),
        Reason::Slot(n) => lang.fmt(Key::ReasonSlot, &[&n]),
        Reason::OtherBuild => lang.t(Key::ReasonOtherBuild).to_string(),
        Reason::VsAp(p) => lang.fmt(Key::ReasonVsAp, &[&lang.pct(f32::from(p), 0)]),
        Reason::VsAd(p) => lang.fmt(Key::ReasonVsAd, &[&lang.pct(f32::from(p), 0)]),
        Reason::AntiHeal(n) => lang.fmt(Key::ReasonAntiHeal, &[&n]),
        Reason::VsTanks(n) => lang.fmt(Key::ReasonVsTanks, &[&n]),
        Reason::Pairs { augment, .. } => sd.augments.get(&augment).map_or_else(
            || lang.t(Key::ReasonPairsEarlier).to_string(),
            |a| lang.fmt(Key::ReasonPairs, &[&a.name]),
        ),
    }
}

/// The tooltip line of a reason: who it comes from. A rule says that it is Hexsight's own rule and no
/// statistics. A pair with an offered augment says that Hexsight cannot know if you took it.
fn reason_tip(lang: Lang, r: Reason) -> &'static str {
    match r {
        r if r.is_rule() => lang.t(Key::TipReasonRule),
        Reason::Pairs { known: false, .. } => lang.t(Key::TipPairsOffered),
        _ => lang.t(Key::TipReasonData),
    }
}

/// The score line of an enemy or of a game of yours: "9/2/4".
fn score_line(
    k: impl std::fmt::Display,
    d: impl std::fmt::Display,
    a: impl std::fmt::Display,
) -> String {
    format!("{k}/{d}/{a}")
}

/// The damage mix as words: "70% AP · 30% AD" ("%70 AP · %30 AD" in Turkish).
fn mix_text(lang: Lang, ap_pct: u8) -> String {
    lang.fmt(
        Key::CtxMix,
        &[
            &lang.pct(f32::from(ap_pct), 0),
            &lang.pct(f32::from(100 - ap_pct.min(100)), 0),
        ],
    )
}

/// The champion name for a summary, or an empty text when Hexsight does not know it.
fn champion_name(sd: &StaticData, id: i64) -> &str {
    sd.champions.get(&id).map_or("", |c| c.name.as_str())
}

/// The context line of the overlay card: "70% AP · Fed: Zed 9/2/4 · 2 healers". A part that does not
/// exist is left out. `None` when no part exists. One healer is no part: the line says "healers" in
/// the plural only, and the rule that uses healers starts at two.
fn context_line(lang: Lang, sd: &StaticData, c: &GameContext) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(ap) = c.ap_pct {
        parts.push(mix_text(lang, ap));
    }
    if let Some(f) = c.fed {
        parts.push(lang.fmt(
            Key::CtxFed,
            &[
                &champion_name(sd, f.champion),
                &score_line(f.kills, f.deaths, f.assists),
            ],
        ));
    }
    if c.healers.len() >= 2 {
        parts.push(lang.fmt(Key::CtxHealersN, &[&c.healers.len()]));
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Has the summary card anything to show?
fn summary_has_rows(c: &GameContext) -> bool {
    c.ap_pct.is_some() || c.fed.is_some() || !c.healers.is_empty() || !c.tanks.is_empty()
}

/// Has the overlay card anything to show, with these settings? The item cells need the setting for
/// the item tips and at least one item. The context line needs the setting for the summary.
fn overlay_has_content(lang: Lang, sd: &StaticData, s: &Settings, a: &Advice) -> bool {
    (s.suggest_items && !a.items.is_empty())
        || (s.suggest_context
            && a.ctx
                .as_ref()
                .is_some_and(|c| context_line(lang, sd, c).is_some()))
}

/// One consistent copy of the shared state per frame.
struct View {
    phase: Phase,
    queue: Option<i64>,
    cs: Option<crate::lcu::ChampSelect>,
    mine: Option<i64>,
    sd: Option<Arc<StaticData>>,
    stats: Option<Arc<crate::data::Stats>>,
    details: Option<Arc<ChampStats>>,
    /// The details come from before the last stats refresh. New ones are on the way.
    stale: bool,
    /// Why there are no stats for this champion now (`Shared::no_stats`). `None`
    /// while they load or show.
    no_data: Option<NoStats>,
    offer: Option<Offer>,
    error: Option<LoadError>,
    note: Option<Key>,
    /// `WindowMode` of League: 0 Fullscreen, 1 Windowed, 2 Borderless.
    window_mode: Option<i32>,
    /// Last action message and its age in seconds.
    action: Option<(String, f32)>,
    /// The raw error behind the last message. The toast shows it on hover.
    action_detail: Option<String>,
    /// The cards that each pick screen offered so far in this game.
    offered: Vec<(usize, Vec<i64>)>,
    /// The last scan of the card reader, for the button "Save the last scan".
    scan: Option<Arc<ScanRecord>>,
    /// The numbers of Settings > Card reading tools. Empty while another page shows.
    read: ReadView,
    /// The saved games of the player, newest first.
    history: Arc<Vec<Game>>,
    /// The first read of the match history runs (or waits to try again).
    importing: bool,
    /// The League install folder is known.
    league_found: bool,
    /// The OCR engine can start. `None` while Hexsight looks for it.
    ocr_probe: Option<bool>,
    /// How the capture of the game window looks (black or frozen shows a hint).
    capture: CaptureHealth,
    /// Your level in the game. 0 while it is not known.
    level: i64,
    /// Pick screens that closed in this game.
    picks_done: usize,
    /// The item sets of this champion are in the shop folder.
    item_sets_for: Option<i64>,
    /// The tag of a newer release (for example "v0.2.0"). `None` when this build is
    /// current, or the setting "Check for new versions" is off.
    update: Option<String>,
    /// The suggestions of the poll thread. `None` outside a Mayhem game, while the data is not there,
    /// or with every suggestion setting off.
    advice: Option<Arc<Advice>>,
    /// Your gold in the game. The chip of an item shows how much is missing.
    gold: Option<u32>,
}

/// Paint one line of text, cut with "…" at `max_w`.
fn text_at(ui: &Ui, pos: egui::Pos2, text: &str, font: FontId, color: Color32, max_w: f32) -> Rect {
    let mut job = egui::text::LayoutJob::single_section(
        text.to_string(),
        egui::TextFormat::simple(font, color),
    );
    job.wrap.max_width = max_w.max(10.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let g = ui.painter().layout_job(job);
    let r = Rect::from_min_size(pos, g.size());
    ui.painter().galley(pos, g, color);
    r
}

/// Like `text_at`, with several colored parts on one line.
fn parts_at(
    ui: &Ui,
    pos: egui::Pos2,
    parts: &[(String, Color32)],
    font: FontId,
    max_w: f32,
) -> Rect {
    let mut job = egui::text::LayoutJob::default();
    for (text, color) in parts {
        job.append(text, 0.0, egui::TextFormat::simple(font.clone(), *color));
    }
    job.wrap.max_width = max_w.max(10.0);
    job.wrap.max_rows = 1;
    job.wrap.break_anywhere = true;
    let g = ui.painter().layout_job(job);
    let r = Rect::from_min_size(pos, g.size());
    ui.painter().galley(pos, g, LABEL);
    r
}

impl App {
    pub fn new(cc: &eframe::CreationContext<'_>, settings: Settings) -> Self {
        egui_extras::install_image_loaders(&cc.egui_ctx);
        theme::setup(&cc.egui_ctx);
        let hwnd = {
            use raw_window_handle::{HasWindowHandle, RawWindowHandle};
            cc.window_handle().ok().and_then(|h| match h.as_raw() {
                RawWindowHandle::Win32(w) => Some(w.hwnd.get()),
                _ => None,
            })
        };
        if let Some(h) = hwnd {
            // No glass: the extended frame of the acrylic backdrop let the caption buttons of Windows show through.
            crate::win::style_window(h, false);
        }
        let keys = Keys {
            overlay: settings.overlay_key,
            scan: settings.scan_key,
        };
        cc.egui_ctx.options_mut(|o| o.reduce_texture_memory = true);
        let h = worker::start(
            cc.egui_ctx.clone(),
            keys,
            settings.scan_always,
            settings.last_locale.clone(),
        );
        // The client has not answered yet: Auto uses the language of the last run.
        let lang = settings.language.resolve(&settings.last_locale);
        let seen_game = {
            let mut s = h.shared.lock().unwrap();
            s.lang = lang;
            s.history.first().map(|g| g.id)
        };
        let mut app = Self::assemble(h, settings, hwnd, lang, seen_game);
        app.debug_setup();
        app
    }

    /// The fields of a new app. It starts no thread and no window, and reads no debug capture.
    fn assemble(
        h: Handles,
        settings: Settings,
        hwnd: Option<isize>,
        lang: Lang,
        seen_game: Option<i64>,
    ) -> Self {
        Self {
            h,
            settings,
            page: Page::Main,
            overlay: false,
            overlay_unlocked: false,
            overlay_auto: false,
            last_phase: Phase::Offline,
            seen_overlay: 0,
            seen_lock: 0,
            item_set_for: None,
            item_sets_cleared: false,
            hwnd,
            started: std::time::Instant::now(),
            shot_sent: false,
            debug_overlay: false,
            start_minimized: std::env::args().any(|a| a == "--background"),
            autostart: crate::win::autostart_enabled(),
            dirty_at: None,
            wanted: Default::default(),
            auto_done: false,
            overlay_size: None,
            main_size: None,
            main_h: None,
            debug_offer: None,
            debug_offered: None,
            debug_scroll: std::env::var("HEXSIGHT_SCROLL")
                .ok()
                .and_then(|s| s.parse().ok()),
            debug_scroll_end: f32::MAX,
            debug_save_scan: std::env::var("HEXSIGHT_SAVE_SCAN")
                .ok()
                .and_then(|s| s.parse().ok()),
            place_window: false,
            just_placed: false,
            rebinding: None,
            cur_phase: Phase::Offline,
            pass_sent: None,
            held_offer: None,
            debug_capture: match std::env::var("HEXSIGHT_CAPTURE_FAULT").as_deref() {
                Ok("black") => Some(CaptureHealth::Black),
                Ok("frozen") => Some(CaptureHealth::Frozen),
                Ok("locked") => Some(CaptureHealth::Locked),
                Ok("remote") => Some(CaptureHealth::Remote),
                _ => None,
            },
            debug_first_run: std::env::var_os("HEXSIGHT_FIRST_RUN").is_some(),
            lang,
            badge_win: badges::Window::default(),
            debug_badges: false,
            trade_clicked: None,
            debug_trade: std::env::var("HEXSIGHT_TRADE")
                .ok()
                .and_then(|s| parse_debug_trade(&s)),
            tools_open: false,
            fix_clicked: None,
            seen_game,
            advice_seen: None,
            hide: Hide::Shown,
            hidden_told: false,
            games_all: false,
            champions_all: false,
            debug_clock: None,
        }
    }

    fn bytes(&self, url: &str) -> Option<egui::load::Bytes> {
        if url.is_empty() {
            return None;
        }
        let b = self.h.shared.lock().unwrap().icons.get(url).cloned();
        if b.is_none() {
            let _ = self.h.icon.send(url.to_string());
        }
        b
    }

    /// Paint an icon into a rect. A soft placeholder shows while it downloads.
    fn paint_icon(&self, ui: &Ui, url: &str, rect: Rect, radius: u8) {
        match self.bytes(url) {
            Some(b) => egui::Image::from_bytes(format!("bytes://{url}"), b)
                .corner_radius(radius)
                .paint_at(ui, rect),
            None => {
                ui.painter()
                    .rect_filled(rect, CornerRadius::same(radius), SURFACE_2);
            }
        }
    }

    fn icon(&self, ui: &mut Ui, url: &str, size: f32, radius: u8) -> egui::Response {
        let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
        self.paint_icon(ui, url, rect, radius);
        resp
    }

    fn mark_dirty(&mut self) {
        self.dirty_at = Some(std::time::Instant::now());
    }

    /// The zoom factor of the current mode: the overlay size in the overlay,
    /// 1.0 in the main window.
    fn wanted_zoom(&self) -> f32 {
        if self.overlay {
            overlay_zoom(self.settings.overlay_scale)
        } else {
            1.0
        }
    }

    fn set_overlay(&mut self, ctx: &egui::Context, on: bool) {
        if on == self.overlay {
            return;
        }
        self.overlay = on;
        self.overlay_unlocked = false;
        self.overlay_size = None;
        self.held_offer = None;
        // No acrylic blur, in the overlay or in the main window (see `Settings::glass`).
        // The overlay is also hidden from screen captures, unless the user
        // wants it in recordings. Then the card reader skips its text.
        // It shows without taking the game focus.
        if let Some(h) = self.hwnd {
            crate::win::style_window(h, false);
            crate::win::exclude_from_capture(h, on && !self.settings.overlay_in_captures);
            if on {
                crate::win::show_no_activate(h);
            }
        }
        if on {
            if let Some(r) = ctx.input(|i| i.viewport().outer_rect) {
                self.settings.pos = Some([r.min.x, r.min.y]);
            }
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::AlwaysOnTop,
            ));
            // The mouse passes through the overlay in a game only. Outside a game, the
            // overlay takes clicks, and a click on the pill closes it (`sync_passthrough`).
            self.pass_sent = None;
            self.sync_passthrough(ctx);
        } else {
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(false));
            self.pass_sent = Some(false);
            ctx.send_viewport_cmd(egui::ViewportCommand::WindowLevel(
                egui::WindowLevel::Normal,
            ));
        }
        // The overlay has its own zoom (S, M, L). A window command uses the
        // zoom of its frame, and the new zoom is active one frame later. The
        // window gets its size and place then (`place`).
        ctx.set_zoom_factor(self.wanted_zoom());
        self.place_window = true;
        ctx.request_repaint();
        self.mark_dirty();
    }

    /// Send the size and the place of the window for the current mode. Call it
    /// when the zoom factor of the mode is active: the commands use it.
    fn place(&mut self, ctx: &egui::Context) {
        if self.overlay {
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(1.0, 1.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(vec2(
                OVERLAY_WINDOW_W,
                120.0,
            )));
            let at = place_from_saved(
                self.settings.overlay_pos.unwrap_or(OVERLAY_HOME),
                ctx.zoom_factor(),
            );
            ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(at));
        } else {
            // The content sets the height (`fit_main`). The last height stands in until it does.
            let size = self.main_size.unwrap_or(vec2(MAIN_W, MAIN_START_H));
            self.main_size = Some(size);
            ctx.send_viewport_cmd(egui::ViewportCommand::MinInnerSize(vec2(MAIN_W, 100.0)));
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
            if let Some([x, y]) = self.settings.pos {
                ctx.send_viewport_cmd(egui::ViewportCommand::OuterPosition(pos2(x, y)));
            }
        }
        // The overlay sends its size again, in the points of this zoom.
        self.overlay_size = None;
        self.just_placed = true;
    }

    /// Send the mouse state of the overlay window when it changed: it lets the
    /// mouse pass in a game, and takes clicks outside a game or while unlocked.
    fn sync_passthrough(&mut self, ctx: &egui::Context) {
        if !self.overlay {
            return;
        }
        let want = passes_mouse(self.overlay, self.cur_phase, self.overlay_unlocked);
        if self.pass_sent != Some(want) {
            self.pass_sent = Some(want);
            ctx.send_viewport_cmd(egui::ViewportCommand::MousePassthrough(want));
        }
    }

    /// The overlay strip stays `CARD_HOLD` after the cards go away. While it
    /// stays, `v.offer` holds the last offer, so the strip does not flicker.
    fn hold_offer(&mut self, ctx: &egui::Context, v: &mut View) {
        let (offer, left) = held_card(
            &mut self.held_offer,
            v.offer.as_ref(),
            std::time::Instant::now(),
        );
        if let Some(left) = left {
            // The strip closes at the end of the hold, with no other event to wake the window.
            ctx.request_repaint_after(left);
        }
        v.offer = offer;
    }

    fn set_overlay_unlocked(&mut self, ctx: &egui::Context, unlocked: bool) {
        self.overlay_unlocked = unlocked;
        self.sync_passthrough(ctx);
        if !unlocked && let Some(r) = ctx.input(|i| i.viewport().outer_rect) {
            self.settings.overlay_pos = Some(place_to_saved(r.min, ctx.zoom_factor()));
            self.mark_dirty();
        }
    }

    /// Start or stop waiting for a new hotkey. The key thread ignores the
    /// hotkeys meanwhile.
    fn set_rebinding(&mut self, which: Option<Hotkey>) {
        self.rebinding = which.map(|w| (w, std::time::Instant::now()));
        self.h.shared.lock().unwrap().rebinding = which.is_some();
    }

    /// The user pushed `vk` while Settings waited for a new hotkey.
    fn bind_key(&mut self, which: Hotkey, vk: i32) {
        self.set_rebinding(None);
        let lang = self.lang;
        let now = Keys {
            overlay: self.settings.overlay_key,
            scan: self.settings.scan_key,
        };
        let Some(keys) = rebound(now, which, vk) else {
            self.h
                .shared
                .lock()
                .unwrap()
                .say(lang.fmt(Key::HotTaken, &[&key_text(lang, vk)]));
            return;
        };
        (self.settings.overlay_key, self.settings.scan_key) = (keys.overlay, keys.scan);
        let mut s = self.h.shared.lock().unwrap();
        s.keys = keys;
        s.say(lang.fmt(Key::HotSet, &[&key_text(lang, vk)]));
        drop(s);
        self.mark_dirty();
    }

    /// The button "Fix" next to the Fullscreen warning. It shows only while League
    /// runs in Fullscreen (the caller checks). While League is closed it is dim and says
    /// to open League. Returns true on a click that can run.
    fn fix_button(&self, ui: &mut Ui, v: &View) -> bool {
        let lang = self.lang;
        let (key, on) = fix_label(v.phase, self.fix_clicked.map(|t| t.elapsed()));
        let text = lang.t(key);
        let resp = kit::accent_button(ui, text, if on { BLUE } else { BLUE.gamma_multiply(0.4) })
            .on_hover_text(lang.t(Key::FixTip));
        on && resp.clicked()
    }

    /// Ask the worker to set League to Borderless. Only a click calls it.
    fn fix_borderless(&mut self, ctx: &egui::Context) {
        let _ = self.h.cmd.send(Cmd::FixBorderless);
        self.fix_clicked = Some(std::time::Instant::now());
        ctx.request_repaint_after(FIX_PAUSE + std::time::Duration::from_millis(50));
    }

    /// Write `last-scan.png` and `last-scan.txt` in the data folder. This is the
    /// only time that Hexsight writes a capture to disk. The text file is always in
    /// English, so a helper can read it, whatever the language of the interface.
    fn save_scan(&self, v: &View) -> Result<(), String> {
        let rec = v
            .scan
            .as_deref()
            .ok_or_else(|| self.lang.t(Key::ScanNone).to_string())?;
        let facts: Vec<(String, String)> = read_rows(&v.read, Lang::En)
            .into_iter()
            .map(|(label, value, _)| (label.to_string(), value))
            .collect();
        let name_of = |id: i64| {
            v.sd.as_ref()
                .and_then(|sd| sd.augments.get(&id))
                .map_or_else(|| "unknown augment".to_string(), |a| a.name.clone())
        };
        let text = crate::ocr::scan_text(rec, &facts, &name_of);
        crate::ocr::save_scan(&crate::cache::app_dir(), rec, &text)
    }
}

impl eframe::App for App {
    fn clear_color(&self, _v: &egui::Visuals) -> [f32; 4] {
        [0.0, 0.0, 0.0, 0.0]
    }

    /// Runs before each frame and also while the window is minimized, so the
    /// hotkeys, the auto overlay and champ select work from the background.
    fn logic(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        let (
            phase,
            queue,
            mine,
            mine_details,
            overlay_presses,
            lock_presses,
            locale,
            offer,
            static_loaded,
        ) = {
            let s = self.h.shared.lock().unwrap();
            let details_of = |c: i64| s.details.get(&c).or(s.old_details.get(&c)).cloned();
            let d = s.my_champion.and_then(details_of);
            (
                s.phase,
                s.queue,
                s.my_champion,
                d,
                s.overlay_presses,
                s.lock_presses,
                s.locale.clone(),
                s.offer.clone(),
                s.static_data.is_some(),
            )
        };
        self.cur_phase = phase;
        if !locale.is_empty() && locale != self.settings.last_locale {
            self.settings.last_locale = locale;
            self.mark_dirty();
        }
        // The language of the interface. Auto follows the client. Until the client
        // answers, the saved locale of the last run stands in for it.
        let lang = self.settings.language.resolve(&self.settings.last_locale);
        if lang != self.lang {
            self.lang = lang;
            self.h.shared.lock().unwrap().lang = lang;
            ctx.request_repaint();
        }
        // A debug capture (env HEXSIGHT_OFFER) shows its cards in every phase.
        let offer = self.debug_offer.clone().or(offer);
        // The cards of this pick screen. The pairs hint on the later screens uses them.
        let offered = offer.as_ref().map(|o| (o.screen, o.augment_ids()));

        // Hotkeys.
        if overlay_presses != self.seen_overlay {
            self.seen_overlay = overlay_presses;
            self.overlay_auto = false;
            self.set_overlay(ctx, !self.overlay);
        }
        if lock_presses != self.seen_lock {
            self.seen_lock = lock_presses;
            if self.overlay {
                self.set_overlay_unlocked(ctx, !self.overlay_unlocked);
            }
        }

        // Phase changes.
        if phase != self.last_phase {
            if phase == Phase::InGame {
                self.auto_done = false;
            } else if self.last_phase == Phase::InGame && self.overlay_auto {
                self.overlay_auto = false;
                self.set_overlay(ctx, false);
            }
            // A game that starts or ends has its own screen. Settings and My games go back to it.
            if phase_resets_page(self.last_phase, phase) {
                self.page = Page::Main;
            }
            if phase == Phase::ChampSelect {
                if champ_select_resets_page(queue) {
                    self.page = Page::Main;
                }
                self.item_set_for = None;
                // A clear that did not run or failed gets one more try in this champ select.
                if std::mem::take(&mut self.h.shared.lock().unwrap().item_sets_clear_failed) {
                    self.item_sets_cleared = false;
                }
                // A hidden (minimized) window comes back: `show_no_activate` restores it without
                // taking the focus. Not in a queue without a bench (Ranked, Normal).
                if raises_on_champ_select(self.settings.show_on_champ_select, self.overlay, queue)
                    && let Some(h) = self.hwnd
                {
                    crate::win::show_no_activate(h);
                }
            }
            self.last_phase = phase;
        }
        // The overlay lets the mouse pass in a game, and takes clicks outside one.
        self.sync_passthrough(ctx);
        // Auto overlay once per game, when the queue is known to be Mayhem.
        if phase == Phase::InGame && !self.auto_done && queue.is_some() {
            self.auto_done = true;
            if self.settings.auto_overlay && queue.is_some_and(is_mayhem) && !self.overlay {
                self.overlay_auto = true;
                self.set_overlay(ctx, true);
            }
        }
        // Item sets for the shop: once per champion in a Mayhem champ select, after the data loaded.
        let ready = mine_details.is_some() && static_loaded;
        if let (Some(c), Some(d)) = (
            item_set_due(
                self.settings.item_sets,
                phase,
                queue,
                mine,
                ready,
                self.item_set_for,
            ),
            mine_details.clone(),
        ) {
            self.item_set_for = Some(c);
            let _ = self
                .h
                .cmd
                .send(Cmd::WriteItemSet(c, d, self.settings.tiers_only));
        }
        // With the setting off, no file of ours stays in the shop folder. Once per run
        // is enough for files from an older run. The switch itself deletes at once.
        if !self.settings.item_sets && !self.item_sets_cleared && phase == Phase::ChampSelect {
            self.item_sets_cleared = true;
            let _ = self.h.cmd.send(Cmd::ClearItemSets);
        }

        if std::mem::take(&mut self.debug_overlay) {
            self.set_overlay(ctx, true);
        }
        if std::mem::take(&mut self.start_minimized) {
            ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        }
        // The zoom of a new mode is active one frame after the request. The
        // window gets its size and place then.
        if self.place_window && ctx.zoom_factor() == self.wanted_zoom() {
            self.place_window = false;
            self.place(ctx);
        }
        // Waiting for a new hotkey ends when the Settings page goes away, or
        // after `REBIND_WAIT`. Else the hotkeys would stay off.
        if self.rebinding.is_some_and(|(_, since)| {
            self.page != Page::Settings || self.overlay || since.elapsed() > REBIND_WAIT
        }) {
            self.set_rebinding(None);
        }
        // Tell the worker threads what only the window knows.
        {
            let minimized = ctx.input(|i| i.viewport().minimized) == Some(true);
            // Before the lock below: the toast takes it too.
            self.tell_on_return(minimized);
            // The main window shows in a game: a scan that covers it would show Hexsight.
            let in_window = phase == Phase::InGame && !self.overlay && !minimized;
            // The overlay in recordings: the card reader skips the text inside it.
            let in_overlay = self.overlay && self.settings.overlay_in_captures;
            let rect = if in_window || in_overlay {
                self.hwnd.and_then(crate::win::window_rect)
            } else {
                None
            };
            let (window, overlay) = (rect.filter(|_| in_window), rect.filter(|_| in_overlay));
            let mut s = self.h.shared.lock().unwrap();
            s.window_rect = window;
            s.overlay_rect = overlay;
            s.update_check = self.settings.check_updates;
            s.suggest_on = self.settings.suggests();
            if let Some((screen, ids)) = &offered {
                s.note_offered(*screen, ids);
            }
        }
        // Badges on the game cards (beta): a window over the game, only while an
        // offer shows in a Mayhem game. egui closes the window when it is not shown.
        let in_mayhem = phase == Phase::InGame && queue.is_some_and(is_mayhem);
        let layer = match &offer {
            Some(o) if self.settings.card_badges && (in_mayhem || self.debug_offer.is_some()) => {
                badges::layer(
                    self.lang,
                    mine_details.as_deref(),
                    o,
                    self.settings.min_games,
                    self.settings.top_highlight,
                )
            }
            _ => None,
        };
        // A minimized main window runs no frame, and the badge window needs one to open.
        if layer.is_some() && ctx.input(|i| i.viewport().minimized) == Some(true) {
            ctx.send_viewport_cmd(egui::ViewportCommand::RequestPaintWhileHidden);
        }
        self.badge_win.update(ctx, layer);
        // Save settings once, 500 ms after the last change.
        if let Some(t) = self.dirty_at {
            if t.elapsed() >= std::time::Duration::from_millis(500) {
                self.settings.save();
                self.dirty_at = None;
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(520));
            }
        }
    }

    fn ui(&mut self, ui: &mut Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        let mut v = {
            let mut s = self.h.shared.lock().unwrap();
            // A message that waited behind another one takes its place now.
            s.next_toast();
            let mine = s.my_champion;
            // After a stats refresh, show the old details until the new ones arrive.
            let fresh = mine.and_then(|c| s.details.get(&c).cloned());
            View {
                phase: s.phase,
                queue: s.queue,
                cs: s.champ_select.clone(),
                mine,
                sd: s.static_data.clone(),
                stats: s.stats.clone(),
                stale: fresh.is_none(),
                details: fresh.or_else(|| mine.and_then(|c| s.old_details.get(&c).cloned())),
                no_data: mine.and_then(|c| s.no_stats(c)),
                // Cards show only during a game.
                offer: self.debug_offer.clone().or_else(|| {
                    if s.phase == Phase::InGame {
                        s.offer.clone()
                    } else {
                        None
                    }
                }),
                error: s.error.clone(),
                note: s.ocr_note,
                window_mode: s.window_mode,
                action: s
                    .last_action
                    .clone()
                    .zip(s.action_at.map(|t| t.elapsed().as_secs_f32())),
                action_detail: s.last_detail.clone(),
                offered: self
                    .debug_offered
                    .clone()
                    .unwrap_or_else(|| s.offered.clone()),
                scan: s.scan.clone(),
                read: if self.page == Page::Settings {
                    let scan = s.scan.as_deref();
                    ReadView {
                        in_game: s.phase == Phase::InGame,
                        ocr_language: s.ocr_language.clone(),
                        game_window: s.game_window,
                        window_mode: s.window_mode,
                        champ_source: s.champ_source,
                        scan_age: scan.map(|r| r.at.elapsed().as_secs()),
                        lines: scan.map_or(0, |r| r.lines.len()),
                        dropped: scan.map_or(0, |r| r.dropped),
                        matched: scan.map_or(0, |r| r.matched.len()),
                        engine_failed: scan.is_some_and(|r| r.engine_failed),
                        due: s.live.picks_due(),
                        done: s.picks_done,
                        capture: self.debug_capture.unwrap_or(s.capture),
                    }
                } else {
                    ReadView::default()
                },
                history: s.history.clone(),
                importing: s.importing,
                league_found: s.league_found,
                ocr_probe: s.ocr_probe,
                capture: self.debug_capture.unwrap_or(s.capture),
                level: s.live.level,
                picks_done: s.picks_done,
                item_sets_for: s.item_sets_for,
                update: s.update.clone().filter(|_| self.settings.check_updates),
                advice: s.advice.clone(),
                gold: s.live.gold,
            }
        };
        self.note_advice(&v);
        if self.overlay {
            self.hold_offer(&ctx, &mut v);
        }
        // The game that ends is the one that gets saved after this point.
        if v.phase != Phase::PostGame {
            self.seen_game = v.history.first().map(|g| g.id);
        }
        // Ask for stats at most once every 2 seconds per champion.
        if let Some(c) = v.mine
            && v.stale
            && self
                .wanted
                .get(&c)
                .is_none_or(|t| t.elapsed().as_secs_f32() > 2.0)
        {
            self.wanted.insert(c, std::time::Instant::now());
            let _ = self.h.want.send(c);
        }
        self.debug_shot(&ctx);
        if self
            .debug_save_scan
            .is_some_and(|t| self.started.elapsed().as_secs_f32() >= t)
        {
            self.debug_save_scan = None;
            let lang = self.lang;
            let msg = self.save_scan(&v).map_or_else(
                |e| lang.fmt(Key::ScanSaveFailed, &[&e]),
                |()| lang.t(Key::ScanSaved).into(),
            );
            self.h.shared.lock().unwrap().say(msg);
        }
        if let Some((call, id, at)) = self.debug_trade {
            if self.started.elapsed().as_secs_f32() >= at {
                self.debug_trade = None;
                self.trade(&ctx, id, call);
            } else {
                ctx.request_repaint_after(std::time::Duration::from_millis(250));
            }
        }
        if self.debug_badges {
            self.badge_preview(ui, &v);
            return;
        }
        if self.overlay {
            self.overlay_ui(ui, &v);
            let full = ui.max_rect();
            self.toast(ui, &v, full);
            return;
        }
        self.window_ui(ui, &v);
    }
}

// ---------------------------------------------------------------------------
// Debug capture for tests without a screen (the PC can be locked):
// HEXSIGHT_SHOT=<file.png> HEXSIGHT_VIEW=games|settings|rebind|overlay|badges
// HEXSIGHT_OFFER=<id,id,id> HEXSIGHT_PICKS=<id,id> (one earlier pick screen for each comma; a / lists several cards of one screen)
// HEXSIGHT_SHOT_DELAY=<seconds> HEXSIGHT_SCROLL=<points> (Settings, My games, and a main page that scrolls)
// HEXSIGHT_SAVE_SCAN=<seconds> (push "Save the last scan" at this time)
// HEXSIGHT_TRADE=<request|accept|decline|cancel>:<trade id>:<seconds> (send that trade call at this time)

impl App {
    fn debug_setup(&mut self) {
        match std::env::var("HEXSIGHT_VIEW").as_deref() {
            Ok("games") => self.page = Page::Games,
            Ok("badges") => self.debug_badges = true,
            Ok("settings") => self.page = Page::Settings,
            // Settings while it waits for a new overlay hotkey.
            Ok("rebind") => {
                self.page = Page::Settings;
                self.set_rebinding(Some(Hotkey::Overlay));
            }
            Ok("overlay") => self.debug_overlay = true,
            _ => {}
        }
        if let Ok(ids) = std::env::var("HEXSIGHT_OFFER") {
            // The cards sit left, middle and right. The game is 1920x1080 at the corner of the screen.
            let cards: Vec<Card> = ids
                .split(',')
                .filter_map(|s| s.trim().parse().ok())
                .enumerate()
                .map(|(i, id)| Card {
                    id,
                    x: [0.2, 0.5, 0.8][i.min(2)],
                    y: 0.48,
                    h: 0.03,
                })
                .collect();
            let game = crate::ocr::Crop {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            };
            let geometry = Some(crate::ocr::Geometry {
                game,
                crop: crate::ocr::crop_of(0, 0, 1920, 1080),
            });
            self.debug_offer = Some(Offer {
                cards,
                screen: 2,
                geometry,
            });
        }
        if let Ok(ids) = std::env::var("HEXSIGHT_PICKS") {
            // The cards of the earlier pick screens: the first part is screen 1.
            let offered: Vec<(usize, Vec<i64>)> = ids
                .split(',')
                .enumerate()
                .map(|(i, screen)| {
                    (
                        i + 1,
                        screen
                            .split('/')
                            .filter_map(|s| s.trim().parse().ok())
                            .collect(),
                    )
                })
                .collect();
            self.debug_offered = Some(offered);
        }
    }

    fn debug_shot(&mut self, ctx: &egui::Context) {
        let Ok(path) = std::env::var("HEXSIGHT_SHOT") else {
            return;
        };
        let delay: f32 = std::env::var("HEXSIGHT_SHOT_DELAY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(10.0);
        if self.shot_sent {
            return;
        }
        if self.started.elapsed().as_secs_f32() < delay {
            ctx.request_repaint_after(std::time::Duration::from_millis(250));
            return;
        }
        self.shot_sent = true;
        ctx.request_screenshot(move |img| {
            let [w, h] = img.size;
            let mut buf = Vec::with_capacity(w * h * 4);
            for p in &img.pixels {
                buf.extend_from_slice(&p.to_srgba_unmultiplied());
            }
            if let Some(im) = image::RgbaImage::from_raw(w as u32, h as u32, buf) {
                let _ = im.save(&path);
            }
            std::process::exit(0);
        });
    }
}

impl App {
    /// Debug capture (HEXSIGHT_VIEW=badges): the badges in the main window, over
    /// three fake cards. The window acts as the game. The same code places the
    /// badges as over the real game. The window over the game cannot be captured
    /// while the PC is locked, so this shows what it draws.
    fn badge_preview(&self, ui: &mut Ui, v: &View) {
        let full = ui.max_rect();
        let ppp = ui.ctx().pixels_per_point();
        ui.painter()
            .rect_filled(full, CornerRadius::ZERO, Color32::from_rgb(22, 26, 34));
        let (Some(sd), Some(offer)) = (&v.sd, &v.offer) else {
            return;
        };
        let (w, h) = ((full.width() * ppp) as i32, (full.height() * ppp) as i32);
        let geometry = crate::ocr::Geometry {
            game: crate::ocr::Crop { x: 0, y: 0, w, h },
            crop: crate::ocr::crop_of(0, 0, w, h),
        };
        let offer = Offer {
            geometry: Some(geometry),
            ..offer.clone()
        };
        let Some(layer) = badges::layer(
            self.lang,
            v.details.as_deref(),
            &offer,
            self.settings.min_games,
            self.settings.top_highlight,
        ) else {
            return;
        };
        let k = badges::badge_scale(h, ppp);
        let p = ui.painter();
        for c in &offer.cards {
            // A card: its title line sits at the place that Hexsight read.
            let (x0, y0) = geometry.crop.at(c.x - 0.135, c.y - 0.30);
            let (x1, y1) = geometry.crop.at(c.x + 0.135, c.y + 0.24);
            let card = Rect::from_min_max(pos2(x0 / ppp, y0 / ppp), pos2(x1 / ppp, y1 / ppp));
            p.rect_filled(card, CornerRadius::same(10), Color32::from_rgb(34, 40, 52));
            p.rect_stroke(
                card,
                CornerRadius::same(10),
                egui::Stroke::new(1.0, Color32::from_white_alpha(26)),
                egui::StrokeKind::Inside,
            );
            let (tx, ty) = geometry.crop.at(c.x, c.y);
            let name = sd
                .augments
                .get(&c.id)
                .map_or("Augment", |a| a.name.as_str());
            p.text(
                pos2(tx / ppp, ty / ppp),
                Align2::CENTER_CENTER,
                name,
                FontId::new(13.0 * k, semibold()),
                LABEL_2,
            );
        }
        badges::paint(p, full, &layer, (0, 0), ppp);
        p.text(
            full.left_top() + vec2(10.0, 8.0),
            Align2::LEFT_TOP,
            "Badge preview: the layer over the game, drawn in this window",
            FontId::proportional(11.0),
            LABEL_3,
        );
    }
}

// ---------------------------------------------------------------------------
// Main window

/// How many item icons of `size` points, with 4 points between them, fit in
/// `room` points. A row of the build card shows five at most.
fn items_that_fit(room: f32, size: f32) -> usize {
    (((room + 4.0) / (size + 4.0)).floor().max(0.0) as usize).min(5)
}

/// The line of the build card while your champion has no stats. Only a source
/// without stats for the champion says "no data". A failed try says that
/// Hexsight tries again, and a try that runs says "Loading".
fn stats_wait_text(no_data: Option<NoStats>) -> Key {
    match no_data {
        Some(NoStats::Missing) => Key::NoDataChampion,
        Some(NoStats::Transient) => Key::HeroRetrying,
        None => Key::HeroLoading,
    }
}

/// The height of the fade at the bottom of a page that scrolls, in points.
const SCROLL_FADE_H: f32 = 20.0;

/// Paint a fade into `rect`: clear at the top, the color of the window (`BG`) at the bottom.
/// It is a mesh of four vertices with a color each, so the fade needs no texture.
fn paint_scroll_fade(p: &egui::Painter, rect: Rect) {
    let mut mesh = egui::Mesh::default();
    mesh.colored_vertex(rect.left_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(rect.right_top(), Color32::TRANSPARENT);
    mesh.colored_vertex(rect.right_bottom(), BG);
    mesh.colored_vertex(rect.left_bottom(), BG);
    mesh.add_triangle(0, 1, 2);
    mesh.add_triangle(0, 2, 3);
    p.add(egui::Shape::mesh(mesh));
}

/// Throw this frame away and lay it out again at once. egui allows only a few passes
/// in one frame. When it declines, the next frame comes at once, so a cut frame does
/// not stay on the screen until the next event.
fn lay_out_again(ctx: &egui::Context, reason: &'static str) {
    ctx.request_discard(reason);
    if !ctx.will_discard() {
        ctx.request_repaint();
    }
}

impl App {
    /// The main window: the title bar, the page and the footer. Returns the height that
    /// the content uses. The window takes this height, `max_height` at most.
    fn window_ui(&mut self, ui: &mut Ui, v: &View) -> f32 {
        let ctx = ui.ctx().clone();
        let full = ui.max_rect();
        // The main window never uses glass (see `Settings::glass`): the extended frame of the
        // acrylic backdrop let the caption buttons of Windows show through the title bar.
        let glass = false;
        let bg = if glass {
            Color32::from_rgba_unmultiplied(16, 16, 18, 222)
        } else {
            BG
        };
        ui.painter().rect_filled(full, CornerRadius::same(8), bg);
        // Esc is the way back from Settings and My games. While a hotkey waits for a key,
        // Esc cancels the wait instead (`settings_ui`).
        if esc_goes_back(
            self.page,
            self.rebinding.is_some(),
            ctx.input(|i| i.key_pressed(egui::Key::Escape)),
        ) {
            self.page = Page::Main;
        }

        // The content lays out in a tall area. The window then takes the height that it used.
        let content = Rect::from_min_max(
            pos2(full.left() + MARGIN, full.top()),
            pos2(full.right() - MARGIN, full.top() + 4000.0),
        );
        let mut body = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(content)
                .layout(egui::Layout::top_down(egui::Align::Min)),
        );
        let ui = &mut body;

        self.title_bar(ui, v);
        // The lists of My games open short again each time the page opens.
        if self.page != Page::Games {
            self.games_all = false;
            self.champions_all = false;
        }
        match self.page {
            Page::Main => self.main_area(ui, v),
            Page::Settings => self.settings_ui(ui, v),
            Page::Games => self.games_ui(ui, v),
        }
        self.footer(ui, v);
        let used = ui.min_rect().bottom() - full.top();
        self.toast(ui, v, full);
        self.fit_main(&ctx, used);

        // Remember the window place for the next start. Not while the window
        // gets its place: its rect is then still the one of the other mode.
        let settling = std::mem::take(&mut self.just_placed) || self.place_window;
        if let Some(r) = ctx.input(|i| i.viewport().outer_rect) {
            let new = Some([r.min.x, r.min.y]);
            if self.settings.pos != new && !settling && ctx.input(|i| !i.pointer.any_down()) {
                self.settings.pos = new;
                self.mark_dirty();
            }
        }
        used
    }

    /// The tallest the main window grows: `MAIN_MAX_H`, or less on a small screen.
    fn max_height(ctx: &egui::Context) -> f32 {
        max_height_on(ctx.input(|i| i.viewport().monitor_size).map(|m| m.y))
    }

    /// Give the window the height of its content, once per change. Not while
    /// the zoom changes: the command would use the old zoom.
    fn fit_main(&mut self, ctx: &egui::Context, used: f32) {
        if self.place_window {
            return;
        }
        if let Some(h) = fit_height(used, Self::max_height(ctx), self.main_size.map(|s| s.y)) {
            let size = vec2(MAIN_W, h);
            self.main_size = Some(size);
            ctx.send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
    }

    /// The most that a page with a long list shows before it scrolls. The title bar,
    /// the footer, the gap and the line above the footer, and the item spacing after the
    /// title bar and after the page take the rest of the tallest window. With less, the
    /// window cuts the footer at the bottom.
    fn page_height(ctx: &egui::Context) -> f32 {
        let spacing = ctx.global_style().spacing.item_spacing.y;
        // The line over the footer (`kit::hairline`) takes 1 point.
        Self::max_height(ctx) - TITLE_H - FOOTER_GAP - 1.0 - FOOTER_H - 2.0 * spacing
    }

    /// The bar at the top. The main page shows the mark, the name and the status of
    /// the client, the gear, and the close button. The close button hides the window
    /// (it minimizes it) and Hexsight keeps running: "Quit Hexsight" in Settings > About
    /// ends it. The other pages show the way back and their title, and no close button.
    /// The first hide in a run shows the toast "Hexsight keeps running".
    fn title_bar(&mut self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), TITLE_H), Sense::hover());
        // The empty part of the bar moves the window.
        let drag = ui.interact(
            rect,
            ui.scope_id().with("titlebar"),
            Sense::click_and_drag(),
        );
        if drag.drag_started() {
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
        }
        let mut bar = ui.new_child(
            egui::UiBuilder::new()
                .max_rect(rect)
                .layout(egui::Layout::left_to_right(egui::Align::Center)),
        );
        if self.page == Page::Main {
            let (txt, col) = match v.phase {
                Phase::Offline => (Key::PhaseOffline, LABEL_3),
                Phase::Idle => (Key::PhaseClient, LABEL_2),
                Phase::Lobby => (Key::PhaseLobby, TEAL),
                // Orange stays for warnings only.
                Phase::ChampSelect => (Key::PhaseChampSelect, BLUE),
                Phase::InGame => (Key::PhaseInGame, GREEN),
                Phase::PostGame => (Key::PhasePostGame, LABEL_2),
            };
            let (m, _) = bar.allocate_exact_size(Vec2::splat(20.0), Sense::hover());
            kit::app_mark(&bar, m);
            bar.add_space(2.0);
            bar.label(strong("Hexsight", 13.5));
            bar.add_space(4.0);
            kit::pill(&mut bar, lang.t(txt), col, true);
        } else {
            if kit::icon_button(&mut bar, glyph::BACK, lang.t(Key::TipBack), false).clicked() {
                self.page = Page::Main;
            }
            bar.add_space(2.0);
            bar.label(title(
                lang.t(if self.page == Page::Settings {
                    Key::SettingsTitle
                } else {
                    Key::MyGames
                }),
                17.0,
            ));
        }
        // The other pages have only the way back: no close button there.
        if self.page != Page::Main {
            return;
        }
        bar.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            if kit::icon_button(ui, glyph::CLOSE, lang.t(Key::TipClose), false).clicked() {
                self.settings.save();
                self.hide_window(ui.ctx());
            }
            if kit::icon_button(ui, glyph::SETTINGS, lang.t(Key::TipSettings), false).clicked() {
                self.page = Page::Settings;
            }
        });
    }

    /// Hide the main window: minimize it. The app and the card reader keep running, and champ select
    /// (or the start of a game) brings the window back. The toast "Hexsight keeps running" does
    /// not show now, because nobody sees a toast in a minimized window: `tell_on_return` shows it.
    fn hide_window(&mut self, ctx: &egui::Context) {
        ctx.send_viewport_cmd(egui::ViewportCommand::Minimized(true));
        self.hide = Hide::Asked;
    }

    /// Runs once per `logic` pass. The first time in a run that the window comes back from a hide,
    /// it shows the toast "Hexsight keeps running".
    fn tell_on_return(&mut self, minimized: bool) {
        let (hide, back) = hide_step(self.hide, minimized);
        self.hide = hide;
        if back && !std::mem::replace(&mut self.hidden_told, true) {
            let toast = self.lang.t(Key::HiddenToast).to_string();
            self.h.shared.lock().unwrap().say(toast);
        }
    }

    /// Short message near the bottom. It fades in, stays 3 seconds, and fades out.
    fn toast(&self, ui: &mut Ui, v: &View, full: Rect) {
        let Some((msg, age)) = &v.action else { return };
        let life = 3.2;
        if *age > life {
            return;
        }
        let a = (age / 0.15).min((life - age) / 0.5).clamp(0.0, 1.0);
        // A long message (a Turkish text, or an error with details) wraps inside the window.
        let g = ui.painter().layout(
            msg.clone(),
            FontId::new(12.5, semibold()),
            LABEL.gamma_multiply(a),
            (full.width() - 56.0).max(80.0),
        );
        let rise = (1.0 - (age / 0.2).min(1.0)) * 8.0;
        let r = Rect::from_center_size(
            pos2(full.center().x, full.bottom() - 52.0 + rise),
            g.size() + vec2(28.0, 14.0),
        );
        let p = ui.painter();
        p.rect_filled(
            r.translate(vec2(0.0, 3.0)),
            CornerRadius::same(255),
            Color32::from_black_alpha((90.0 * a) as u8),
        );
        p.rect_filled(
            r,
            CornerRadius::same(255),
            Color32::from_rgba_unmultiplied(58, 58, 60, (240.0 * a) as u8),
        );
        p.galley(r.center() - g.size() / 2.0, g, LABEL);
        // A failed call names the reason. The raw error of the client shows only on hover.
        if let Some(detail) = &v.action_detail {
            ui.interact(r, ui.scope_id().with("toast"), Sense::hover())
                .on_hover_text(detail);
        }
        ui.ctx()
            .request_repaint_after(std::time::Duration::from_millis(30));
    }

    /// The footer: the patch of the numbers at the left (the name of the data source is in
    /// its tooltip), and how many games stand behind them at the right. On the Settings
    /// page it names the app and its version.
    fn footer(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        ui.add_space(FOOTER_GAP);
        kit::hairline(ui, 0.0);
        let (rect, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), FOOTER_H), Sense::hover());
        // The tooltip of the left text: the full error, or where the numbers come from.
        let mut tip = None;
        let (left, right, color) = if self.page == Page::Settings {
            (
                "Hexsight".to_string(),
                format!("v{}", env!("CARGO_PKG_VERSION")),
                LABEL_2,
            )
        } else if let Some(e) = &v.error {
            (
                format!("{} · {}", lang.t(Key::OfflineData), lang.load_error(e)),
                String::new(),
                ORANGE,
            )
        } else if let Some(st) = &v.stats {
            tip = Some(lang.fmt(Key::FooterSource, &[&st.source, &st.patch]));
            (
                lang.fmt(Key::FooterPatch, &[&st.patch]),
                lang.games(st.matches as u32, &fmt_n(lang, st.matches as u32)),
                LABEL_2,
            )
        } else {
            (lang.t(Key::LoadingData).to_string(), String::new(), LABEL_2)
        };
        // A newer version takes the right side as a link. On the Settings page the version moves to the left.
        let link = v.update.is_some().then(|| lang.t(Key::UpdateShort));
        let (left, right, color) = match (link, self.page == Page::Settings) {
            (Some(t), true) => (format!("{left} {right}"), t.to_string(), BLUE),
            (Some(t), false) => (left, t.to_string(), BLUE),
            (None, _) => (left, right, color),
        };
        let font = FontId::proportional(11.0);
        let right_w = ui
            .painter()
            .layout_no_wrap(right.clone(), font.clone(), color)
            .size()
            .x;
        let room = rect.width()
            - if right.is_empty() {
                0.0
            } else {
                right_w + 12.0
            };
        let shown = text_at(
            ui,
            pos2(rect.left(), rect.center().y - 7.0),
            &left,
            font.clone(),
            color,
            room,
        );
        // A text that is too long (an error with two causes) ends with "…", and the full text shows on hover.
        if v.error.is_some() && self.page != Page::Settings {
            tip = Some(left.clone());
        }
        if let Some(tip) = tip.filter(|_| self.page != Page::Settings) {
            ui.interact(shown, ui.scope_id().with("footer"), Sense::hover())
                .on_hover_text(tip);
        }
        let sized = ui.painter().text(
            pos2(rect.right(), rect.center().y),
            Align2::RIGHT_CENTER,
            right,
            font,
            color,
        );
        if let (Some(tag), Some(_)) = (&v.update, link) {
            let resp = ui.interact(
                sized.expand2(vec2(4.0, 4.0)),
                ui.scope_id().with("update"),
                Sense::click(),
            );
            if resp.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                ui.painter().line_segment(
                    [sized.left_bottom(), sized.right_bottom()],
                    egui::Stroke::new(1.0, color),
                );
            }
            if resp
                .on_hover_text(format!("{} {tag}", lang.t(Key::UpdateTip)))
                .clicked()
            {
                open_url(&update::releases_url(update::REPO));
            }
        }
    }

    /// The main page between the title bar and the footer. It scrolls only when it is
    /// taller than the room there (two trade requests, or a small screen), so the
    /// window never cuts the footer. The height of the last frame decides. A frame that
    /// crosses the limit lays out again at once. The scroll area has its own id, and
    /// room at the sides and the bottom for the glow of a card. While it scrolls, a fade of
    /// 20 points at the bottom edge of the area shows that the page goes on.
    fn main_area(&mut self, ui: &mut Ui, v: &View) {
        let room = Self::page_height(ui.ctx());
        let scroll = self.main_h.is_some_and(|h| h > room);
        let h = if scroll {
            let top = ui.cursor().top();
            let r = ui.max_rect();
            let bleed = f32::from(PAGE_BLEED);
            let outer = Rect::from_min_max(
                pos2(r.left() - bleed, top),
                pos2(r.right() + bleed, top + room),
            );
            let mut child = ui.new_child(
                egui::UiBuilder::new()
                    .max_rect(outer)
                    .layout(egui::Layout::top_down(egui::Align::Min)),
            );
            let mut area = egui::ScrollArea::vertical()
                .id_salt("main_page_scroll")
                .auto_shrink([false, false])
                .max_height(room)
                .content_margin(egui::Margin {
                    left: PAGE_BLEED,
                    right: PAGE_BLEED,
                    top: 0,
                    bottom: PAGE_BLEED,
                });
            if let Some(y) = self.debug_scroll {
                area = area.vertical_scroll_offset(y);
            }
            let h = area
                .show(&mut child, |ui| {
                    ui.scope(|ui| self.main_page(ui, v)).response.rect.height()
                })
                .inner;
            ui.advance_cursor_after_rect(Rect::from_min_max(
                pos2(r.left(), top),
                pos2(r.right(), top + room),
            ));
            // A fade at the bottom edge of the area shows that the page goes on below it.
            paint_scroll_fade(
                ui.painter(),
                Rect::from_min_max(
                    pos2(outer.left(), outer.bottom() - SCROLL_FADE_H),
                    outer.right_bottom(),
                ),
            );
            h
        } else {
            ui.scope(|ui| self.main_page(ui, v)).response.rect.height()
        };
        if (h > room) != scroll {
            lay_out_again(ui.ctx(), "the main page needs a scroll area, or no longer");
        }
        self.main_h = Some(h);
    }

    /// The main page. The game phase picks what it shows (`screen_for`).
    fn main_page(&mut self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let other_mode = v.queue.is_some_and(|q| !is_mayhem(q));
        match screen_for(
            v.phase,
            v.offer.is_some(),
            other_mode,
            non_aram_queue(v.queue),
        ) {
            Screen::Ready => {
                self.ready_card(ui, v);
                ui.add_space(8.0);
                kit::group(ui, |ui| self.games_link(ui));
            }
            Screen::Select => {
                if v.mine.is_none() {
                    // The first step: the player has no champion yet, so there is no champion row, bench, spells, or build.
                    self.choose_card(ui, v);
                    return;
                }
                self.champion_row(ui, v);
                self.trade_requests(ui, v);
                self.bench_card(ui, v);
                self.spells_card(ui, v);
                // The note at the right of the header says that the sets are in the shop.
                let shop = (v.mine.is_some() && v.item_sets_for == v.mine)
                    .then(|| format!("{} {}", glyph::CHECK, lang.t(Key::ShopReady)));
                section_note(
                    ui,
                    &lang.upper(lang.t(Key::SecBuild)),
                    shop.as_deref().map(|t| (t, GREEN)),
                );
                self.build_card(ui, v);
            }
            Screen::Waiting => {
                self.warning_banner(ui, v);
                self.champion_row(ui, v);
                if self.settings.suggest_items {
                    // The build style is the note at the right of the header.
                    let style = v
                        .advice
                        .as_deref()
                        .filter(|a| !a.no_build && !a.style.is_empty())
                        .map(|a| lang.fmt(Key::SuggestStyle, &[&lang.style(&a.style)]));
                    section_note(
                        ui,
                        &lang.upper(lang.t(Key::SecNextItems)),
                        style.as_deref().map(|t| (t, LABEL_3)),
                    );
                    self.items_card(ui, v);
                }
                if self.settings.suggest_context {
                    self.summary_card(ui, v);
                }
                kit::section(ui, &lang.upper(lang.t(Key::SecAugments)));
                self.next_pick_card(ui, v);
                kit::section(ui, &lang.upper(lang.t(Key::SecBuild)));
                self.build_card(ui, v);
                ui.add_space(10.0);
                kit::group(ui, |ui| self.games_link(ui));
            }
            Screen::Pick => {
                self.champion_row(ui, v);
                ui.add_space(8.0);
                self.pick_card(ui, v);
                kit::section(ui, &lang.upper(lang.t(Key::SecBuild)));
                self.build_card(ui, v);
            }
            Screen::After => {
                ui.add_space(4.0);
                self.after_card(ui, v);
            }
            Screen::Other => {
                ui.add_space(4.0);
                kit::group(ui, |ui| {
                    ui.label(title(lang.t(Key::OtherMode), 20.0));
                    ui.add(egui::Label::new(caption(lang.t(Key::OtherModeNote))).wrap());
                });
            }
        }
    }

    /// Your champion: the portrait, the name, the mode, and the tier of the champion.
    fn champion_row(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let champion = v.mine.and_then(|id| v.sd.as_ref()?.champions.get(&id));
        let (rect, _) = ui.allocate_exact_size(vec2(ui.available_width(), 52.0), Sense::hover());
        let portrait = Rect::from_min_size(rect.min + vec2(0.0, 2.0), Vec2::splat(48.0));
        let Some(c) = champion else {
            // The portrait waits for the champion. A grey tile holds its place.
            ui.painter()
                .rect_filled(portrait, CornerRadius::same(12), SURFACE_2);
            text_at(
                ui,
                pos2(portrait.right() + 12.0, rect.center().y - 9.0),
                lang.t(Key::NoChampion),
                FontId::new(15.0, semibold()),
                LABEL_2,
                rect.right() - portrait.right() - 12.0,
            );
            return;
        };
        self.paint_icon(ui, &c.icon, portrait, 12);
        let tier = v
            .stats
            .as_ref()
            .and_then(|s| s.champions.get(&c.id))
            .copied();
        let x = portrait.right() + 12.0;
        let room = rect.right() - x - if tier.is_some() { 40.0 } else { 0.0 };
        let mayhem = v.queue.is_some_and(is_mayhem);
        text_at(
            ui,
            pos2(x, rect.top() + if mayhem { 6.0 } else { 14.0 }),
            &c.name,
            FontId::new(20.0, bold()),
            LABEL,
            room,
        );
        if mayhem {
            text_at(
                ui,
                pos2(x, rect.top() + 32.0),
                &lang.upper(lang.t(Key::ModeName)),
                FontId::new(10.5, semibold()),
                PRISM,
                room,
            );
        }
        if let Some(r) = tier {
            let br = Rect::from_center_size(
                pos2(rect.right() - 14.0, rect.center().y),
                Vec2::splat(28.0),
            );
            let mut child = ui.new_child(egui::UiBuilder::new().max_rect(br));
            kit::tier_badge(&mut child, r.tier, 28.0, false)
                .on_hover_text(lang.t(Key::TipChampTier));
        }
    }

    /// A teammate asks for your champion: a card with the two champions (the tier badge
    /// on the corner of each icon) and the buttons Decline and Accept in equal parts
    /// across the bottom. The card is a calm surface with a blue edge: orange and a
    /// glow stay for warnings. It shows only while the request stands.
    fn trade_requests(&mut self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let (Some(sd), Some(cs)) = (&v.sd, &v.cs) else {
            return;
        };
        let row_of = |id: i64| v.stats.as_ref().and_then(|s| s.champions.get(&id)).copied();
        for t in cs.trades.iter().filter(|t| t.state == TradeState::Received) {
            let Some(mate) = cs.team.iter().find(|m| m.cell == t.cell) else {
                continue;
            };
            let (Some(theirs), Some(yours)) = (
                sd.champions.get(&mate.champion),
                sd.champions.get(&cs.my_champion),
            ) else {
                continue;
            };
            let on = self
                .trade_clicked
                .is_none_or(|(id, at)| id != t.id || at.elapsed() >= TRADE_PAUSE);
            let frame = egui::Frame::new()
                .fill(SURFACE)
                .corner_radius(CornerRadius::same(RADIUS))
                .stroke(egui::Stroke::new(1.5, BLUE))
                .inner_margin(egui::Margin::same(10));
            ui.add_space(8.0);
            frame.show(ui, |ui| {
                ui.set_width(ui.available_width());
                ui.horizontal(|ui| {
                    // Your champion goes to the teammate, and theirs comes to you.
                    let mine = self.icon(ui, &yours.icon, 40.0, 10).rect;
                    let (arrows, _) = ui.allocate_exact_size(vec2(20.0, 40.0), Sense::hover());
                    paint_swap_arrows(ui.painter(), arrows.center(), LABEL_2);
                    let other = self.icon(ui, &theirs.icon, 40.0, 10).rect;
                    // The badges come after both icons, so they paint over the art.
                    paint_corner_badge(ui.painter(), mine, row_of(cs.my_champion));
                    paint_corner_badge(ui.painter(), other, row_of(mate.champion));
                    ui.vertical(|ui| {
                        ui.add(egui::Label::new(strong(lang.t(Key::TradeWants), 13.0)).wrap());
                        ui.add(
                            egui::Label::new(caption(
                                lang.fmt(Key::TradeGetGive, &[&theirs.name, &yours.name]),
                            ))
                            .wrap(),
                        );
                    });
                });
                ui.add_space(4.0);
                // Decline at the left, Accept at the right, in equal parts of the row.
                let button = vec2(
                    (ui.available_width() - ui.spacing().item_spacing.x) / 2.0,
                    TRADE_BUTTON_H,
                );
                ui.horizontal(|ui| {
                    if trade_button(ui, button, lang.t(Key::TradeDecline), SURFACE_3, on) {
                        self.trade(ui.ctx(), t.id, TradeCall::Decline);
                    }
                    if trade_button(ui, button, lang.t(Key::TradeAccept), GREEN, on) {
                        self.trade(ui.ctx(), t.id, TradeCall::Accept);
                    }
                });
            });
        }
    }

    /// Send a call on a trade, as a click on its button does. The buttons of
    /// that trade stay off for `TRADE_PAUSE`, and the window wakes up when they come back.
    fn trade(&mut self, ctx: &egui::Context, id: i64, call: TradeCall) {
        let _ = self.h.cmd.send(Cmd::Trade(id, call));
        self.trade_clicked = Some((id, std::time::Instant::now()));
        ctx.request_repaint_after(TRADE_PAUSE);
    }

    /// The first step of a Mayhem champ select, while the player has no champion: the
    /// champions that the client offers, each with the tier badge, the name and the games
    /// from the stats index, in the order of the client. It shows no win rate, ranks
    /// nothing, and picks nothing: the player chooses in the League Client. The note
    /// "You choose in League" is the tooltip of the card. Without an offer, one line says
    /// where to choose.
    fn choose_card(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let choices = v.cs.as_ref().map_or(&[][..], |c| c.choices.as_slice());
        kit::section(ui, &lang.upper(lang.t(Key::SecChoose)));
        ui.scope(|ui| {
            kit::group(ui, |ui| {
                let Some(sd) = v.sd.as_ref().filter(|_| !choices.is_empty()) else {
                    let note = if choices.is_empty() {
                        Key::ChooseEmpty
                    } else {
                        Key::HeroLoading
                    };
                    ui.add(egui::Label::new(caption(lang.t(note))).wrap());
                    return;
                };
                let width = ui.available_width();
                let grid = choice_grid(width, choices.len());
                let height = grid.last().map_or(0.0, |r| r.bottom()) + 4.0 + CHOICE_TEXT_H;
                let (area, _) = ui.allocate_exact_size(vec2(width, height), Sense::hover());
                for (&id, at) in choices.iter().zip(&grid) {
                    let row = v.stats.as_ref().and_then(|s| s.champions.get(&id)).copied();
                    self.choice_cell(
                        ui,
                        sd,
                        id,
                        row,
                        at.translate(area.min.to_vec2()),
                        width / CHOICE_COLS as f32,
                    );
                }
            });
        })
        .response
        .on_hover_text(lang.t(Key::ChooseNote));
    }

    /// One offered champion: the icon with the tier badge on its lower right corner, the
    /// name under it (12 points, semibold, cut with "…" to the column), and the games under
    /// the name (11 points). The tooltip has the full name and the games. The cell takes no click.
    fn choice_cell(
        &self,
        ui: &mut Ui,
        sd: &StaticData,
        id: i64,
        row: Option<ChampRow>,
        rect: Rect,
        col_w: f32,
    ) {
        let lang = self.lang;
        let Some(c) = sd.champions.get(&id) else {
            return;
        };
        let badge = choice_badge(rect);
        let resp = ui.interact(
            rect.union(badge),
            ui.scope_id().with(("choice", id)),
            Sense::hover(),
        );
        let hover = ui
            .ctx()
            .animate_bool_with_time(resp.id, resp.hovered(), 0.1);
        self.paint_icon(ui, &c.icon, rect, RADIUS);
        let p = ui.painter();
        if hover > 0.0 {
            let edge = egui::Stroke::new(1.5, Color32::from_white_alpha((90.0 * hover) as u8));
            p.rect_stroke(
                rect,
                CornerRadius::same(RADIUS),
                edge,
                egui::StrokeKind::Inside,
            );
        }
        p.rect_filled(
            badge.expand(CHOICE_BADGE_RING),
            CornerRadius::same((CHOICE_BADGE * 0.28) as u8 + CHOICE_BADGE_RING as u8),
            SURFACE,
        );
        match row {
            Some(r) => kit::paint_tier(p, badge, Some(r.tier)),
            None => kit::paint_no_stats(p, badge),
        }
        // The name sits under the icon, centered, and ends with "…" in a column that is too narrow for it.
        let mut job = egui::text::LayoutJob::single_section(
            c.name.clone(),
            egui::TextFormat::simple(choice_name_font(), LABEL),
        );
        job.wrap.max_width = (col_w - 4.0).max(10.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let name = p.layout_job(job);
        let top = rect.bottom() + 4.0;
        let name_h = name.size().y;
        p.galley(
            pos2(rect.center().x - name.size().x / 2.0, top),
            name,
            LABEL,
        );
        let games = row.map_or_else(
            || lang.t(Key::NoData).to_string(),
            |r| lang.games(r.games, &fmt_n(lang, r.games)),
        );
        // The games sit under the name, centered. A text that does not fit the column drops its noun.
        let font = FontId::proportional(11.0);
        let mut shown = games.clone();
        if let Some(r) = row.filter(|_| {
            p.layout_no_wrap(games.clone(), font.clone(), LABEL_2)
                .size()
                .x
                > col_w
        }) {
            shown = fmt_n(lang, r.games);
        }
        p.text(
            pos2(rect.center().x, top + name_h.max(CHOICE_TEXT_H - 14.0)),
            Align2::CENTER_TOP,
            shown,
            font,
            LABEL_2,
        );
        resp.on_hover_text(format!("{}\n{games}", c.name));
    }

    /// The bench of an ARAM champ select as a grid of icons, `BENCH_COLS` in a row, in
    /// the order of `bench_order`. A badge on the corner of each icon shows the tier,
    /// or "—" without stats. The bench champion that beats yours (`best_swap`) has the
    /// green glow, and a button under the grid names it ("Swap to Yasuo (S)"). A click
    /// on the button asks for that swap, as a click on its icon does. The tooltip of the
    /// button has the whole line "Best swap: ... · yours is ...". A click on any icon asks
    /// for the swap too. The tooltip of an icon has the name and the games.
    fn bench_card(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let (Some(sd), Some(cs)) = (&v.sd, &v.cs) else {
            return;
        };
        if cs.bench.is_empty() {
            return;
        }
        let row_of = |id: i64| v.stats.as_ref().and_then(|s| s.champions.get(&id)).copied();
        let mine = row_of(cs.my_champion);
        let cells = bench_cells(
            cs.bench.iter().map(|&id| (id, row_of(id))).collect(),
            mine,
            self.settings.min_games,
        );
        kit::section(ui, &lang.upper(lang.t(Key::SecBench)));
        let mut asked = None;
        kit::group(ui, |ui| {
            let width = ui.available_width();
            let grid = bench_grid(width, cells.len());
            let (area, _) = ui.allocate_exact_size(
                vec2(width, grid.last().map_or(0.0, |r| r.bottom())),
                Sense::hover(),
            );
            for (cell, at) in cells.iter().zip(&grid) {
                if self.bench_cell(ui, sd, cell, at.translate(area.min.to_vec2())) {
                    asked = Some(cell.id);
                }
            }
            let best = cells.iter().find(|c| c.best);
            if let (Some(best), Some(yours)) = (best, sd.champions.get(&cs.my_champion)) {
                let theirs = sd.champions.get(&best.id).map_or("?", |c| c.name.as_str());
                let theirs_tier = format!("{theirs} ({})", tier_text(best.row));
                let line = best_swap_parts(
                    lang,
                    &theirs_tier,
                    &format!("{} ({})", yours.name, tier_text(mine)),
                )
                .concat();
                ui.add_space(4.0);
                // The whole line is the tooltip. A long name ends with "…" in the button.
                let look = BlockButton {
                    text: &lang.fmt(Key::BenchSwapButton, &[&theirs_tier]),
                    color: GREEN,
                    ink: GREEN,
                    font: FontId::new(13.0, semibold()),
                    radius: 10,
                    rest: SWAP_BUTTON_REST,
                    hover: SWAP_BUTTON_HOVER,
                };
                if block_button(ui, vec2(width, SWAP_BUTTON_H), look)
                    .on_hover_text(line)
                    .clicked()
                {
                    asked = Some(best.id);
                }
            }
        });
        if let Some(id) = asked {
            let _ = self.h.cmd.send(Cmd::BenchSwap(id));
        }
    }

    /// One icon of the bench grid, with the tier badge on its lower right corner.
    /// Returns true when the player clicks it to ask for the swap.
    fn bench_cell(&self, ui: &mut Ui, sd: &StaticData, cell: &BenchCell, rect: Rect) -> bool {
        let lang = self.lang;
        let Some(c) = sd.champions.get(&cell.id) else {
            return false;
        };
        let resp = ui.interact(
            bench_hit(rect),
            ui.scope_id().with(("bench", cell.id)),
            Sense::click(),
        );
        let hover = ui
            .ctx()
            .animate_bool_with_time(resp.id, resp.hovered(), 0.1);
        self.paint_icon(ui, &c.icon, rect, RADIUS);
        let p = ui.painter();
        if cell.best {
            kit::glow(ui, rect, GREEN);
        }
        if hover > 0.0 {
            // A light edge on hover. The best swap keeps its glow under it.
            let edge = egui::Stroke::new(1.5, Color32::from_white_alpha((90.0 * hover) as u8));
            p.rect_stroke(
                rect,
                CornerRadius::same(RADIUS),
                edge,
                egui::StrokeKind::Inside,
            );
        }
        // The badge sits on a ring of the card color, so it reads over the portrait.
        paint_corner_badge(p, rect, cell.row);
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        let games = cell.row.map_or_else(
            || lang.t(Key::NoData).to_string(),
            |r| lang.games(r.games, &fmt_n(lang, r.games)),
        );
        resp.on_hover_text(format!(
            "{}\n{games}\n{}",
            c.name,
            lang.t(Key::BenchClickToSwap)
        ))
        .clicked()
    }

    /// Champ select: the summoner spell pair that players take most with your
    /// champion, as two small icons with a short label and the names. It shows no
    /// rate, and it picks nothing: the player chooses the spells in the game. The
    /// card stays away without data or without the names of both spells.
    fn spells_card(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let (Some(sd), Some(cs)) = (&v.sd, &v.details) else {
            return;
        };
        let Some([a, b]) = spell_suggestion(cs, sd, self.settings.min_games) else {
            return;
        };
        let (Some(first), Some(second)) = (sd.spells.get(&a), sd.spells.get(&b)) else {
            return;
        };
        ui.add_space(8.0);
        kit::group(ui, |ui| {
            let (row, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), SPELL_ICON), Sense::hover());
            let icon2 = Rect::from_min_size(
                pos2(row.right() - SPELL_ICON, row.top()),
                Vec2::splat(SPELL_ICON),
            );
            let icon1 = icon2.translate(vec2(-SPELL_ICON - 4.0, 0.0));
            let label = ui.painter().layout_no_wrap(
                lang.t(Key::SpellsLabel).to_string(),
                FontId::proportional(13.5),
                LABEL,
            );
            let x = row.left() + label.size().x + 10.0;
            ui.painter().galley(
                pos2(row.left(), row.center().y - label.size().y / 2.0),
                label,
                LABEL,
            );
            // The names show when both fit. The tooltip always has them.
            let names = format!("{} + {}", first.name, second.name);
            let font = FontId::proportional(11.5);
            let room = icon1.left() - 8.0 - x;
            if ui
                .painter()
                .layout_no_wrap(names.clone(), font.clone(), LABEL_2)
                .size()
                .x
                <= room
            {
                text_at(
                    ui,
                    pos2(x, row.center().y - 7.0),
                    &names,
                    font,
                    LABEL_2,
                    room,
                );
            }
            self.paint_icon(ui, &first.icon, icon1, 7);
            self.paint_icon(ui, &second.icon, icon2, 7);
            ui.interact(row, ui.scope_id().with("spells"), Sense::hover())
                .on_hover_text(format!("{names}\n{}", lang.t(Key::SpellsTip)));
        });
    }

    /// The build of your champion: one row for each style (Tank, AP ...) with the
    /// first items of its build order. The style is plain text in a column as wide as
    /// the widest style, so the items of every row start at the same place. That the
    /// item sets are in the in-game shop shows in the header of the section (`section_note`).
    fn build_card(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        kit::group(ui, |ui| {
            let (Some(sd), Some(cs)) = (&v.sd, &v.details) else {
                ui.label(caption(lang.t(stats_wait_text(v.no_data))));
                return;
            };
            if cs.builds.is_empty() {
                ui.label(caption(lang.t(Key::NoBuildData)));
                return;
            }
            const ICON: f32 = 26.0;
            // The column of the styles is as wide as the widest style label.
            let font = FontId::new(12.0, semibold());
            let styles: Vec<String> = cs
                .builds
                .iter()
                .take(MAX_BUILDS)
                .map(|b| lang.style(&b.name))
                .collect();
            let column = styles
                .iter()
                .map(|s| {
                    ui.painter()
                        .layout_no_wrap(s.clone(), font.clone(), LABEL_2)
                        .size()
                        .x
                })
                .fold(0.0, f32::max)
                .ceil();
            for (i, (b, style)) in cs.builds.iter().zip(&styles).enumerate() {
                if i > 0 {
                    ui.add_space(6.0);
                }
                let width = ui.available_width();
                let (row, _) = ui.allocate_exact_size(vec2(width, ICON), Sense::hover());
                let mut line = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(row)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                line.spacing_mut().item_spacing.x = 4.0;
                text_at(
                    ui,
                    pos2(row.left(), row.center().y - 8.0),
                    style,
                    font.clone(),
                    LABEL_2,
                    column,
                );
                line.add_space(column + BUILD_STYLE_GAP);
                // As many items as the row holds.
                let room = width - column - BUILD_STYLE_GAP - 4.0;
                for id in b.items.iter().take(items_that_fit(room, ICON)) {
                    self.item(&mut line, sd, *id, ICON);
                }
            }
        });
    }

    fn item(&self, ui: &mut Ui, sd: &StaticData, id: i64, size: f32) {
        match sd.items.get(&id) {
            Some(it) => {
                self.icon(ui, &it.icon, size, 8).on_hover_text(&it.name);
            }
            None => {
                ui.label(caption(format!("#{id}")));
            }
        }
    }

    /// In a game between pick screens: one row. At the left, the level of the next pick
    /// ("Next pick: level 7"), or that no pick screen is left. At the right, four small dots
    /// for the four pick levels: the next one is green, the ones that are past are grey,
    /// the ones that are still to come are faint. The row says nothing about your own
    /// level (the dot and the text give the next pick), and Hexsight does not know what you
    /// took at a pick screen, so it says nothing about it either.
    fn next_pick_card(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let next = next_pick(v.level, v.picks_done);
        kit::group(ui, |ui| {
            let (row, _) =
                ui.allocate_exact_size(vec2(ui.available_width(), NEXT_PICK_ROW_H), Sense::hover());
            let p = ui.painter();
            let text = match next {
                Some(n) => lang.fmt(Key::NextPickAt, &[&PICK_LEVELS[n]]),
                None => lang.t(Key::PicksDone).to_string(),
            };
            p.text(
                row.left_center(),
                Align2::LEFT_CENTER,
                text,
                FontId::new(13.5, semibold()),
                LABEL,
            );
            let dots = next_pick_dots(next);
            let left =
                row.right() - (dots.len() - 1) as f32 * NEXT_PICK_DOT_STEP - NEXT_PICK_DOT_D / 2.0;
            for (i, ink) in dots.into_iter().enumerate() {
                p.circle_filled(
                    pos2(left + i as f32 * NEXT_PICK_DOT_STEP, row.center().y),
                    NEXT_PICK_DOT_D / 2.0,
                    ink,
                );
            }
        });
    }

    /// A pick screen in the main window (second monitor): the offered cards in screen
    /// order. A row shows the name, where the card sits and its games, the confidence
    /// in three bars, and the tier. The header shows the level. Two marks are off by
    /// default and the player turns each one on in Settings. "Highlight top-tier
    /// cards" gives every top-tier card the green frame and adds a header line that
    /// says how many are in the top tier, or that the tiers are equal. "Show pair
    /// hints" adds a line that says a card pairs well with an earlier card *if* the
    /// player took it: Hexsight never knows that. No view shows an augment win rate.
    fn pick_card(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let (Some(sd), Some(offer)) = (&v.sd, &v.offer) else {
            return;
        };
        let min_games = self.settings.min_games;
        let earlier = earlier_offers(&v.offered, offer.screen);
        let (mut cells, top) = pick_cells(
            v.details.as_deref(),
            offer,
            &earlier,
            min_games,
            self.settings.marks(),
        );
        fit_cells(
            &mut cells,
            v.details.as_deref(),
            v.advice.as_deref(),
            self.settings.marks(),
        );
        // The card fades in over 150 ms. It does not move, so the window height stays the same.
        let ctx = ui.ctx().clone();
        let t = appear(&ctx, egui::Id::unique(("offer", offer.augment_ids())), 0.15);
        ui.scope(|ui| {
            ui.set_opacity(t);
            kit::list(ui, |ui| {
                let (head, _) =
                    ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::hover());
                let p = ui.painter();
                let level = if v.level > 0 {
                    lang.fmt(Key::Level, &[&v.level])
                } else {
                    lang.t(Key::PickTitle).to_string()
                };
                p.text(
                    pos2(head.left() + 8.0, head.center().y),
                    Align2::LEFT_CENTER,
                    level,
                    FontId::new(14.0, semibold()),
                    LABEL,
                );
                // The top-tier line shows only when the player turned the highlight on.
                if let Some(line) = top {
                    let (line, ink) = top_line_text(lang, line);
                    let g = p.layout_no_wrap(line, FontId::new(12.0, semibold()), ink);
                    let left = head.right() - 8.0 - g.size().x;
                    p.circle_filled(pos2(left - 10.0, head.center().y), 3.0, ink);
                    p.galley(pos2(left, head.center().y - g.size().y / 2.0), g, ink);
                }
                for c in &cells {
                    self.pick_row(ui, sd, c);
                }
            });
        });
    }

    /// One row of the pick card.
    fn pick_row(&self, ui: &mut Ui, sd: &StaticData, c: &PickCell) {
        let lang = self.lang;
        let Some(aug) = sd.augments.get(&c.id) else {
            return;
        };
        let min_games = self.settings.min_games;
        let height = 52.0
            + if c.pair.is_some() { 16.0 } else { 0.0 }
            + if c.fit.is_some() { 16.0 } else { 0.0 };
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), height), Sense::hover());
        if c.top {
            kit::paint_top(ui.painter(), r, 9);
        }
        // The right side: the tier, and the confidence at the left of it.
        let badge = Rect::from_center_size(
            pos2(r.right() - 8.0 - 14.0, r.top() + 26.0),
            Vec2::splat(28.0),
        );
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(badge));
        match c.tier {
            Some(tier) => kit::tier_badge(
                &mut child,
                tier,
                28.0,
                c.games.is_some_and(|g| g < min_games),
            ),
            None => kit::unknown_badge(&mut child, 28.0),
        };
        let conf_right = badge.left() - 10.0;
        let conf = c.confidence();
        if let Some(conf) = conf {
            kit::paint_bars(
                ui.painter(),
                pos2(conf_right - kit::BARS_W, r.top() + 17.0),
                conf.bars(),
            );
            // Only a small sample gets a word ("few games"). The bars say the rest.
            if conf == Confidence::Low {
                ui.painter().text(
                    pos2(conf_right, r.top() + 34.0),
                    Align2::RIGHT_CENTER,
                    lang.t(conf.key()),
                    FontId::proportional(10.5),
                    LABEL_3,
                );
            }
        }
        let mut x = r.left() + 10.0;
        if c.top {
            ui.painter()
                .circle_filled(pos2(x + 3.5, r.top() + 19.0), 3.5, GREEN);
            x += 12.0;
        }
        let room = conf_right - kit::BARS_W - 6.0 - x;
        let name = text_at(
            ui,
            pos2(x, r.top() + 9.0),
            &aug.name,
            FontId::new(14.5, semibold()),
            LABEL,
            room,
        );
        // The mark "fits your build": a small chip after the name, when it fits in the row. The line under the
        // row names the item. The mark has no number, and it does not rank the cards.
        if c.fit.is_some() {
            let g = ui.painter().layout_no_wrap(
                lang.t(Key::FitMark).to_string(),
                FontId::new(10.5, semibold()),
                TEAL,
            );
            let chip = Rect::from_min_size(
                pos2(name.right() + 6.0, r.top() + 10.0),
                vec2(g.size().x + 12.0, g.size().y + 2.0),
            );
            if chip.right() <= x + room {
                ui.painter()
                    .rect_filled(chip, CornerRadius::same(255), TEAL.gamma_multiply(0.16));
                ui.painter().galley(
                    pos2(chip.left() + 6.0, chip.center().y - g.size().y / 2.0),
                    g,
                    TEAL,
                );
            }
        }
        let games = c.games.map_or_else(
            || lang.t(Key::NoData).to_string(),
            |g| lang.games(g, &fmt_n(lang, g)),
        );
        text_at(
            ui,
            pos2(x, r.top() + 29.0),
            &format!("{} · {games}", position_label(lang, c.x)),
            FontId::proportional(11.5),
            LABEL_2,
            room,
        );
        // The pairs hint: one line under the row. Hexsight does not know which card the
        // player took, so the line says "if picked earlier".
        if let Some(h) = &c.pair {
            // Without a name for the partner, a line of its own words takes the place of the template.
            let (template, partner) = sd
                .augments
                .get(&h.with)
                .map_or((lang.t(Key::PairsLineEarlier), ""), |a| {
                    (lang.t(Key::PairsLine), a.name.as_str())
                });
            let font = FontId::proportional(11.0);
            let room = r.right() - x - 8.0;
            let [pre, name, post] = pairs_line(template, partner, |text| {
                ui.painter()
                    .layout_no_wrap(text.to_string(), font.clone(), LABEL)
                    .size()
                    .x
                    <= room
            });
            let parts = [(pre, LABEL_2), (name, TEAL), (post, LABEL_2)];
            parts_at(ui, pos2(x, r.top() + 50.0), &parts, font, room);
        }
        if let Some(item) = c.fit {
            let item_name = sd
                .items
                .get(&item)
                .map_or_else(|| format!("#{item}"), |i| i.name.clone());
            let y = r.top() + 50.0 + if c.pair.is_some() { 16.0 } else { 0.0 };
            let line = text_at(
                ui,
                pos2(x, y),
                &lang.fmt(Key::FitLine, &[&item_name]),
                FontId::proportional(11.0),
                TEAL,
                r.right() - x - 8.0,
            );
            ui.interact(line, ui.scope_id().with(("fit", c.id)), Sense::hover())
                .on_hover_text(lang.t(Key::FitTip));
        }
    }

    /// A row that opens the My games page.
    fn games_link(&mut self, ui: &mut Ui) {
        let lang = self.lang;
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 30.0), Sense::click());
        let t = ui
            .ctx()
            .animate_bool_with_time(resp.id, resp.hovered(), 0.1);
        if t > 0.0 {
            ui.painter().rect_filled(
                r.expand2(vec2(6.0, 0.0)),
                CornerRadius::same(8),
                Color32::from_white_alpha((10.0 * t) as u8),
            );
        }
        let p = ui.painter();
        p.text(
            r.left_center(),
            Align2::LEFT_CENTER,
            lang.t(Key::MyGames),
            FontId::proportional(13.5),
            LABEL,
        );
        p.text(
            r.right_center(),
            Align2::RIGHT_CENTER,
            glyph::CHEVRON,
            FontId::new(11.0, icons()),
            LABEL_3,
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            self.page = Page::Games;
        }
    }

    /// After a game: its result, the augments that you took (in pick order, as small icons)
    /// with your K/D/A, and the way to My games. The history thread saves the game some
    /// seconds after it ends, so the card waits for it. The question "Start with Windows?"
    /// shows here, once: the player has seen Hexsight help for a game, and the question is not
    /// the loudest thing on the first screen. The card that still waits for the result does not
    /// ask: the question needs a saved Mayhem game under it.
    fn after_card(&mut self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let game = fresh_game(&v.history, self.seen_game);
        let ask = game.is_some()
            && asks_autostart(
                crate::cache::place(),
                self.debug_first_run,
                self.settings.autostart_asked,
                self.autostart,
            );
        let (mut yes, mut no) = (false, false);
        kit::group(ui, |ui| {
            match game {
                Some(g) => {
                    let (word, ink) = if g.win {
                        (Key::AfterWin, GREEN)
                    } else {
                        (Key::AfterLoss, RED)
                    };
                    ui.label(title(lang.t(word), 26.0).color(ink));
                    let name =
                        v.sd.as_ref()
                            .and_then(|sd| sd.champions.get(&g.champion))
                            .map_or("?", |c| c.name.as_str());
                    ui.label(caption(result_line(lang, name, g)));
                    self.took_row(ui, v.sd.as_deref(), g);
                }
                None => {
                    ui.label(title(lang.t(Key::AfterOver), 22.0));
                    ui.add(egui::Label::new(caption(lang.t(Key::AfterWaiting))).wrap());
                }
            }
            if ask {
                ui.add_space(8.0);
                (yes, no) = autostart_question(ui, lang);
            }
            ui.add_space(6.0);
            kit::hairline(ui, 0.0);
            self.games_link(ui);
        });
        self.answer_autostart(yes, no);
    }

    /// The row under the result of a game: the augments that you took, in pick order, as
    /// small icons at the left (their names show on hover), and the K/D/A at the right. Icons
    /// that do not fit beside the K/D/A are left out.
    fn took_row(&self, ui: &mut Ui, sd: Option<&StaticData>, g: &Game) {
        ui.add_space(2.0);
        let (row, _) =
            ui.allocate_exact_size(vec2(ui.available_width(), TOOK_ICON), Sense::hover());
        let kda = ui.painter().layout_no_wrap(
            score_line(g.kills, g.deaths, g.assists),
            FontId::new(12.5, semibold()),
            LABEL,
        );
        let kda_pos = pos2(
            row.right() - kda.size().x,
            row.center().y - kda.size().y / 2.0,
        );
        let room = kda_pos.x - 8.0 - row.left();
        let shown = took_icons_that_fit(room).min(g.augments.len());
        for (i, id) in g.augments.iter().take(shown).enumerate() {
            let rect = Rect::from_min_size(
                pos2(row.left() + i as f32 * (TOOK_ICON + TOOK_GAP), row.top()),
                Vec2::splat(TOOK_ICON),
            );
            match sd.and_then(|sd| sd.augments.get(id)) {
                Some(a) => {
                    self.paint_icon(ui, &a.icon, rect, TOOK_RADIUS);
                    ui.interact(rect, ui.scope_id().with(("took", i)), Sense::hover())
                        .on_hover_text(&a.name);
                }
                None => self.paint_icon(ui, "", rect, TOOK_RADIUS),
            }
        }
        ui.painter().galley(kda_pos, kda, LABEL);
    }

    /// The first-run card, for a window without a game: the checks, and a button under
    /// them for each check that the player can fix with a click (Fullscreen, data that did
    /// not load, no OCR language, a hotkey that another app holds).
    fn ready_card(&mut self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let keys = [self.settings.overlay_key, self.settings.scan_key];
        let facts = FirstRun {
            league_found: v.league_found,
            window_mode: v.window_mode,
            ocr_probe: v.ocr_probe,
            data_loaded: v.sd.is_some() && v.stats.is_some(),
            data_failed: v.error.is_some(),
            hotkeys: keys.map(|k| (k, crate::win::hotkey_free(k))),
        };
        let fixes = first_run_fixes(&facts);
        let (mut fix, mut retry, mut language, mut change) = (false, false, false, false);
        ui.add_space(4.0);
        kit::group(ui, |ui| {
            // While League is closed, the first thing to do is to open it.
            ui.label(title(
                lang.t(if v.phase == Phase::Offline {
                    Key::HeroOpenLeague
                } else {
                    Key::HeroReady
                }),
                20.0,
            ));
            ui.add_space(6.0);
            for (state, text) in
                checks_to_show(first_run_checks(lang, &facts), lang.t(Key::CheckAllOk))
            {
                Self::check_row(ui, state, &text);
            }
            // The Fullscreen warning has its fix: one click sets Borderless through League.
            if v.window_mode == Some(0) {
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    fix = self.fix_button(ui, v);
                });
            }
            // Each other check that fails has its button, under the rows, in line with their texts.
            for (shown, key, flag) in [
                (fixes.retry, Key::RetryButton, &mut retry),
                (fixes.language, Key::OpenLangSettings, &mut language),
                (fixes.hotkey, Key::HotChange, &mut change),
            ] {
                if shown {
                    ui.horizontal(|ui| {
                        ui.add_space(24.0);
                        *flag = kit::accent_button(ui, lang.t(key), BLUE).clicked();
                    });
                }
            }
            ui.add_space(4.0);
            ui.add(
                egui::Label::new(caption(lang.fmt(
                    Key::HeroStartGame,
                    &[&key_text(lang, self.settings.overlay_key)],
                )))
                .wrap(),
            );
        });
        if fix {
            let ctx = ui.ctx().clone();
            self.fix_borderless(&ctx);
        }
        if retry {
            let _ = self.h.cmd.send(Cmd::Reload);
        }
        if language {
            open_url("ms-settings:regionlanguage");
        }
        if change {
            self.page = Page::Settings;
        }
    }

    /// The player answered the question "Start with Windows?" (`autostart_question`). Yes
    /// turns it on. Either answer ends the question for good.
    fn answer_autostart(&mut self, yes: bool, no: bool) {
        if yes {
            self.autostart = if crate::win::set_autostart(true) {
                true
            } else {
                crate::win::autostart_enabled()
            };
        }
        if yes || no {
            self.settings.autostart_asked = true;
            self.debug_first_run = false;
            self.mark_dirty();
        }
    }

    /// One check of the first-run card: a round mark, and its text. A long text wraps.
    fn check_row(ui: &mut Ui, state: Check, text: &str) {
        let (tint, ink) = match state {
            Check::Ok => (GREEN, LABEL),
            Check::Warn => (ORANGE, ORANGE),
            Check::Wait => (LABEL_2, LABEL_2),
        };
        let g = ui.painter().layout(
            text.to_string(),
            FontId::proportional(12.5),
            ink,
            (ui.available_width() - 26.0).max(40.0),
        );
        let (r, _) = ui.allocate_exact_size(
            vec2(ui.available_width(), g.size().y.max(16.0) + 4.0),
            Sense::hover(),
        );
        let p = ui.painter();
        let c = pos2(r.left() + 8.0, r.top() + 10.0);
        match state {
            // A green disc with a check mark, drawn with two lines.
            Check::Ok => {
                let s = egui::Stroke::new(1.8, Color32::from_rgb(8, 28, 14));
                p.circle_filled(c, 7.5, tint);
                p.line_segment([c + vec2(-3.2, 0.2), c + vec2(-0.9, 2.6)], s);
                p.line_segment([c + vec2(-0.9, 2.6), c + vec2(3.4, -2.6)], s);
            }
            // An orange disc with an exclamation mark.
            Check::Warn => {
                let ink = Color32::from_rgb(40, 24, 0);
                p.circle_filled(c, 7.5, tint);
                p.line_segment(
                    [c + vec2(0.0, -3.6), c + vec2(0.0, 0.6)],
                    egui::Stroke::new(1.8, ink),
                );
                p.circle_filled(c + vec2(0.0, 3.2), 1.1, ink);
            }
            // A ring with three dots: the check has no answer yet.
            Check::Wait => {
                p.circle_stroke(c, 7.0, egui::Stroke::new(1.2, tint));
                for dx in [-3.0, 0.0, 3.0] {
                    p.circle_filled(c + vec2(dx, 0.0), 1.0, tint);
                }
            }
        }
        p.galley(pos2(r.left() + 24.0, r.top() + 2.0), g, ink);
    }
}

// ---------------------------------------------------------------------------
// Pages behind the main page: My games and Settings

impl App {
    fn loading(ui: &mut Ui, text: &str) {
        ui.add_space(30.0);
        ui.vertical_centered(|ui| {
            ui.label(caption(text));
        });
    }

    fn empty(ui: &mut Ui, title_text: &str, text: &str) {
        ui.add_space(26.0);
        ui.vertical_centered(|ui| {
            ui.label(strong(title_text, 15.0));
            ui.add_space(2.0);
            ui.add(egui::Label::new(caption(text)).wrap());
        });
    }

    /// A vertical area for a long page. It shrinks to its content, and it scrolls
    /// above `page_height`. For a capture (env HEXSIGHT_SCROLL) it starts at that offset.
    fn page_area(&self, ctx: &egui::Context, end: f32) -> egui::ScrollArea {
        let area = egui::ScrollArea::vertical()
            .auto_shrink([false, true])
            .max_height(Self::page_height(ctx));
        match self.debug_scroll {
            Some(y) => area.vertical_scroll_offset(y.min(end)),
            None => area,
        }
    }

    /// "Now" and the local time zone for the dates of My games: the clock and the time zone of
    /// this PC, or the fixed time of a test (`debug_clock`).
    fn date_clock(&self) -> (i64, Box<dyn Fn(i64) -> i64>) {
        match self.debug_clock {
            Some((now, offset)) => (now, Box::new(move |_| offset)),
            None => (unix_now(), Box::new(crate::win::local_offset())),
        }
    }

    /// One row of "Recent games", `GAME_ROW_H` high. The champion is a round icon at the left.
    /// The first line has the result (a green disc for a win, a red ring for a loss, and the
    /// letter W or L, G or M in Turkish, so the color is never the only cue), the K/D/A, and the
    /// date `when` at the right. The second line has the augments that you took, in pick order, as
    /// small icons, and the length of the game at the right. A hover names the champion and the
    /// augments. The icons paint only for a row in view: a list of 100 rows asks for the icons of
    /// the rows that you scroll to, not for all of them at once.
    fn game_row(&self, ui: &mut Ui, sd: &StaticData, g: &Game, when: &str) {
        let lang = self.lang;
        let (r, resp) =
            ui.allocate_exact_size(vec2(ui.available_width(), GAME_ROW_H), Sense::hover());
        let champion = sd.champions.get(&g.champion);
        let disc = Rect::from_center_size(
            pos2(r.left() + GAME_ICON / 2.0, r.center().y),
            Vec2::splat(GAME_ICON),
        );
        let x = disc.right() + GAME_TEXT_GAP;
        let (first, aug_top) = (r.top() + 13.0, r.top() + 22.0);
        let second = aug_top + GAME_AUG / 2.0;
        let p = ui.painter();
        paint_result_dot(p, pos2(x + WIN_DOT_R, first), g.win);
        let letter = lang.t(if g.win {
            Key::GameWinShort
        } else {
            Key::GameLossShort
        });
        let letter = p.text(
            pos2(x + 2.0 * WIN_DOT_R + 5.0, first),
            Align2::LEFT_CENTER,
            letter,
            FontId::new(12.5, semibold()),
            if g.win { GREEN } else { RED },
        );
        p.text(
            pos2(letter.right() + 8.0, first),
            Align2::LEFT_CENTER,
            score_line(g.kills, g.deaths, g.assists),
            FontId::new(12.5, semibold()),
            LABEL,
        );
        p.text(
            pos2(r.right() - 2.0, first),
            Align2::RIGHT_CENTER,
            when,
            FontId::proportional(11.5),
            LABEL_3,
        );
        p.text(
            pos2(r.right() - 2.0, second),
            Align2::RIGHT_CENTER,
            length_text(g.secs),
            FontId::proportional(11.5),
            LABEL_3,
        );
        if ui.is_rect_visible(r) {
            self.paint_icon(
                ui,
                champion.map_or("", |c| c.icon.as_str()),
                disc,
                (GAME_ICON / 2.0) as u8,
            );
            for (i, id) in g.augments.iter().take(GAME_AUGMENTS).enumerate() {
                let rect = Rect::from_min_size(
                    pos2(x + i as f32 * (GAME_AUG + GAME_AUG_GAP), aug_top),
                    Vec2::splat(GAME_AUG),
                );
                // An augment that Hexsight does not know keeps its place as a soft tile.
                self.paint_icon(
                    ui,
                    sd.augments.get(id).map_or("", |a| a.icon.as_str()),
                    rect,
                    GAME_AUG_RADIUS,
                );
            }
        }
        resp.on_hover_ui(|ui| {
            let result = lang.t(if g.win { Key::AfterWin } else { Key::AfterLoss });
            ui.label(strong(
                format!("{} · {result}", champion.map_or("?", |c| c.name.as_str())),
                12.5,
            ));
            for (i, id) in g.augments.iter().take(GAME_AUGMENTS).enumerate() {
                if let Some(a) = sd.augments.get(id) {
                    ui.label(caption(format!("{}. {}", i + 1, a.name)));
                }
            }
        });
    }

    /// "My games": your results from the saved games. The overall numbers (a win rate
    /// has a color from `RATE_COLOR_GAMES` games on), the last ten results (a win is a
    /// filled green disc, a loss a red ring, then the score as text), the recent games (one
    /// row each: the newest `GAMES_FIRST`, and "Show all" lists up to `GAMES_MAX`), and your
    /// most played champions (the switch "All champions" lists every champion with a game). The
    /// rows show the augments that you took and never a win rate of an augment. That the games
    /// stay on this PC is said in Settings > About.
    fn games_ui(&mut self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let Some(sd) = v.sd.clone() else {
            return Self::loading(ui, lang.t(Key::LoadingData));
        };
        if v.history.is_empty() {
            // The first read of the match history can take minutes (the client may not be signed in yet).
            return if v.importing {
                Self::empty(
                    ui,
                    lang.t(Key::GamesImporting),
                    lang.t(Key::GamesImportingBody),
                )
            } else {
                Self::empty(ui, lang.t(Key::GamesNone), lang.t(Key::GamesNoneBody))
            };
        }
        let sum = history::summarize(&v.history);
        let (now, zone) = self.date_clock();
        self.page_area(ui.ctx(), f32::MAX).show(ui, |ui| {
            kit::section(ui, &lang.upper(lang.t(Key::GamesOverall)));
            kit::group(ui, |ui| {
                let t = sum.total;
                ui.horizontal(|ui| {
                    ui.label(
                        title(lang.pct(t.rate() * 100.0, 0), 28.0)
                            .color(rate_color(t.rate(), t.games)),
                    );
                    ui.vertical(|ui| {
                        ui.label(strong(lang.t(Key::GamesWinRate), 13.5));
                        ui.label(caption(lang.totals(t.games, t.wins, t.games - t.wins)));
                    });
                });
                ui.add_space(6.0);
                // The last ten results, the oldest at the left and the newest at the right end (the
                // tooltip says so). A win is a filled disc and a loss a ring, so the result does not
                // depend on the color alone. The score follows the dots.
                let recent: Vec<bool> = v.history.iter().take(10).map(|g| g.win).rev().collect();
                let wins = recent.iter().filter(|w| **w).count();
                let losses = recent.len() - wins;
                ui.horizontal(|ui| {
                    ui.label(caption(lang.t(Key::GamesLast10)));
                    let (r, resp) = ui.allocate_exact_size(
                        vec2(recent.len() as f32 * RESULT_DOT_STEP, 16.0),
                        Sense::hover(),
                    );
                    for (i, win) in recent.iter().enumerate() {
                        paint_result_dot(
                            ui.painter(),
                            pos2(
                                r.left() + RESULT_DOT_STEP / 2.0 + i as f32 * RESULT_DOT_STEP,
                                r.center().y,
                            ),
                            *win,
                        );
                    }
                    resp.on_hover_text(lang.t(Key::GamesLast10Tip));
                    ui.label(caption(lang.fmt(Key::GamesLast10Score, &[&wins, &losses])));
                });
            });

            // The recent games, the newest first. The button at the end shows the older rows, and takes them back.
            kit::section(ui, &lang.upper(lang.t(Key::GamesRecent)));
            kit::group(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                for (i, g) in v
                    .history
                    .iter()
                    .take(games_listed(v.history.len(), self.games_all))
                    .enumerate()
                {
                    if i > 0 {
                        kit::hairline(ui, GAME_ICON + GAME_TEXT_GAP);
                    }
                    self.game_row(ui, &sd, g, &game_date(lang, &g.date, now, &*zone));
                }
                if v.history.len() > GAMES_FIRST {
                    kit::hairline(ui, 0.0);
                    if kit::text_button(
                        ui,
                        lang.t(if self.games_all {
                            Key::GamesShowLess
                        } else {
                            Key::GamesShowAll
                        }),
                    )
                    .clicked()
                    {
                        self.games_all = !self.games_all;
                    }
                }
            });

            kit::section(ui, &lang.upper(lang.t(Key::GamesYourChampions)));
            kit::group(ui, |ui| {
                ui.spacing_mut().item_spacing.y = 0.0;
                // The switch is the first row, so it stays in place while the list grows under it.
                if sum.champions.len() > MY_CHAMPIONS {
                    kit::toggle(ui, &mut self.champions_all, lang.t(Key::GamesAllChampions));
                    ui.add_space(4.0);
                    kit::hairline(ui, 0.0);
                }
                let listed = if self.champions_all {
                    sum.champions.len()
                } else {
                    MY_CHAMPIONS
                };
                let mut first = true;
                for (id, t) in sum.champions.iter().take(listed) {
                    let Some(c) = sd.champions.get(id) else {
                        continue;
                    };
                    if !std::mem::take(&mut first) {
                        kit::hairline(ui, 48.0);
                    }
                    let (r, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 48.0), Sense::hover());
                    // The icon sits at the left edge of the row.
                    let ir = Rect::from_center_size(
                        pos2(r.left() + 18.0, r.center().y),
                        Vec2::splat(36.0),
                    );
                    self.paint_icon(ui, &c.icon, ir, 9);
                    let x = ir.right() + 12.0;
                    text_at(
                        ui,
                        pos2(x, r.top() + 8.0),
                        &c.name,
                        FontId::new(14.0, semibold()),
                        LABEL,
                        r.right() - x - 52.0,
                    );
                    let line = format!(
                        "{} · {}",
                        lang.games(t.games, &t.games.to_string()),
                        lang.wins(t.wins)
                    );
                    text_at(
                        ui,
                        pos2(x, r.top() + 28.0),
                        &line,
                        FontId::proportional(11.5),
                        LABEL_3,
                        r.right() - x - 52.0,
                    );
                    ui.painter().text(
                        pos2(r.right() - 2.0, r.center().y),
                        Align2::RIGHT_CENTER,
                        lang.pct(t.rate() * 100.0, 0),
                        FontId::new(15.0, semibold()),
                        rate_color(t.rate(), t.games),
                    );
                }
            });
            ui.add_space(12.0);
        });
    }

    /// One hotkey row: what it does, its key, and the button "Change". While
    /// the page waits for a new key, the row shows "Push a key" and the button
    /// says "Cancel".
    fn hotkey_row(&mut self, ui: &mut Ui, what: &str, which: Hotkey) {
        let lang = self.lang;
        let key = if which == Hotkey::Overlay {
            self.settings.overlay_key
        } else {
            self.settings.scan_key
        };
        let waiting = self.rebinding.is_some_and(|(w, _)| w == which);
        let button = lang.t(if waiting {
            Key::HotCancel
        } else {
            Key::HotChange
        });
        let shown = if waiting {
            lang.t(Key::HotPress).to_string()
        } else {
            key_text(lang, key)
        };
        // The button and the key keep their size. The label takes the room that is left, and a
        // long label wraps to a second line (in a narrow window, or while the row waits for a key).
        let right = kit::accent_button_width(ui, button)
            + keycap_width(ui, &shown)
            + 2.0 * ui.spacing().item_spacing.x;
        let room = (ui.available_width() - right).max(60.0);
        let lines = ui
            .painter()
            .layout(what.to_string(), FontId::proportional(13.5), LABEL, room);
        let row = vec2(ui.available_width(), lines.size().y.max(26.0));
        ui.allocate_ui_with_layout(
            row,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.scope(|ui| {
                    ui.set_max_width(room);
                    ui.add(egui::Label::new(body(what)).wrap());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    if kit::accent_button(ui, button, SURFACE_3).clicked() {
                        self.set_rebinding((!waiting).then_some(which));
                        // The page wakes up at the end of the wait.
                        ui.ctx().request_repaint_after(
                            REBIND_WAIT + std::time::Duration::from_millis(100),
                        );
                    }
                    keycap(ui, &shown, waiting);
                });
            },
        );
    }

    /// The row of the unlock key: what it does, and the key as a keycap ("Ctrl + F8"). The
    /// key is the overlay key with Ctrl, so the row has no button: change the overlay key above.
    fn unlock_row(&self, ui: &mut Ui) {
        let lang = self.lang;
        let what = lang.t(Key::HotUnlock);
        let key = format!("Ctrl + {}", key_text(lang, self.settings.overlay_key));
        // The label takes the room that the key leaves, and a long label wraps to a second line.
        let room =
            (ui.available_width() - keycap_width(ui, &key) - ui.spacing().item_spacing.x).max(60.0);
        let lines = ui
            .painter()
            .layout(what.to_string(), FontId::proportional(13.5), LABEL, room);
        let row = vec2(ui.available_width(), lines.size().y.max(26.0));
        ui.allocate_ui_with_layout(
            row,
            egui::Layout::left_to_right(egui::Align::Center),
            |ui| {
                ui.scope(|ui| {
                    ui.set_max_width(room);
                    ui.add(egui::Label::new(body(what)).wrap());
                });
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    keycap(ui, &key, false);
                });
            },
        );
    }

    /// Settings: a warning when the card reader cannot work, language, start with
    /// Windows, the overlay (its marks, its keys, its size and place, the beta badges),
    /// the item sets, the card reading tools (folded), and About (with "Quit Hexsight").
    fn settings_ui(&mut self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let mut changed = false;
        let mut quit = false;
        // The next key press is the new hotkey. Esc cancels.
        if let Some((which, _)) = self.rebinding {
            match ui.ctx().input(|i| rebind_from(&i.events)) {
                Rebind::Wait => {}
                Rebind::Cancel => self.set_rebinding(None),
                Rebind::Key(vk) => self.bind_key(which, vk),
            }
        }
        // Debug capture: hold the offset, cut to the end of the page. The page can grow after the first frame.
        let page = self
            .page_area(ui.ctx(), self.debug_scroll_end)
            .show(ui, |ui| {
                // A black capture or League in Fullscreen: the warning is the first thing here.
                self.warning_banner(ui, v);
                kit::section(ui, &lang.upper(lang.t(Key::GroupGeneral)));
                kit::group(ui, |ui| {
                    // Auto follows the language of the client. The window uses the choice in the next frame.
                    ui.horizontal(|ui| {
                        ui.label(body(lang.t(Key::SetLanguage)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.allocate_ui(vec2(ui.available_width().min(180.0), 30.0), |ui| {
                                let items = [
                                    (LangPref::Auto, lang.t(Key::LangAuto)),
                                    (LangPref::English, lang.t(Key::LangEnglish)),
                                    (LangPref::Turkish, lang.t(Key::LangTurkish)),
                                ];
                                if kit::segmented(
                                    ui,
                                    "language",
                                    &mut self.settings.language,
                                    &items,
                                ) {
                                    ui.ctx().request_repaint();
                                    changed = true;
                                }
                            });
                        });
                    });
                    kit::hairline(ui, 0.0);
                    if kit::toggle(ui, &mut self.autostart, lang.t(Key::SetAutostart)) {
                        if !crate::win::set_autostart(self.autostart) {
                            self.autostart = crate::win::autostart_enabled();
                        }
                        // The player decided: the first-run card does not ask about it any more.
                        self.settings.autostart_asked = true;
                        changed = true;
                    }
                });

                kit::section(ui, &lang.upper(lang.t(Key::GroupOverlay)));
                kit::group(ui, |ui| {
                    changed |= kit::toggle(
                        ui,
                        &mut self.settings.auto_overlay,
                        lang.t(Key::SetAutoOverlay),
                    );
                    kit::hairline(ui, 0.0);
                    // The switch says "hide", and the setting says "show in captures": they are opposites.
                    // The new value works from the next time the panel opens (`set_overlay`).
                    let mut hide = !self.settings.overlay_in_captures;
                    if kit::toggle(ui, &mut hide, lang.t(Key::SetHideInCaptures)) {
                        self.settings.overlay_in_captures = !hide;
                        changed = true;
                    }
                    ui.add(egui::Label::new(caption(lang.t(Key::SetHideInCapturesNote))).wrap());
                    kit::hairline(ui, 0.0);
                    // Both marks are off by default: Riot's rules for third-party apps. One note covers the two.
                    changed |= kit::toggle(
                        ui,
                        &mut self.settings.top_highlight,
                        lang.t(Key::SetTopHighlight),
                    );
                    kit::hairline(ui, 0.0);
                    changed |=
                        kit::toggle(ui, &mut self.settings.pair_hints, lang.t(Key::SetPairHints));
                    kit::hairline(ui, 0.0);
                    changed |=
                        kit::toggle(ui, &mut self.settings.fit_marks, lang.t(Key::SetFitMarks));
                    ui.add(egui::Label::new(caption(lang.t(Key::SetMarksNote))).wrap());
                    kit::hairline(ui, 0.0);
                    self.hotkey_row(ui, lang.t(Key::HotOverlay), Hotkey::Overlay);
                    kit::hairline(ui, 0.0);
                    self.hotkey_row(ui, lang.t(Key::HotScan), Hotkey::Scan);
                    if self.rebinding.is_some() {
                        ui.add(egui::Label::new(caption(lang.t(Key::HotNoteWaiting))).wrap());
                    }
                    kit::hairline(ui, 0.0);
                    self.unlock_row(ui);
                    kit::hairline(ui, 0.0);
                    // The size of the overlay: three steps. It applies the next time the overlay opens.
                    ui.horizontal(|ui| {
                        ui.label(body(lang.t(Key::OverlaySize)));
                        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                            ui.allocate_ui(vec2(ui.available_width().min(180.0), 30.0), |ui| {
                                let items = [
                                    (0u8, lang.t(Key::SizeSmall)),
                                    (1, lang.t(Key::SizeMedium)),
                                    (2, lang.t(Key::SizeLarge)),
                                ];
                                if kit::segmented(
                                    ui,
                                    "overlay_size",
                                    &mut self.settings.overlay_scale,
                                    &items,
                                ) {
                                    changed = true;
                                }
                            });
                        });
                    });
                    kit::hairline(ui, 0.0);
                    if kit::accent_button(ui, lang.t(Key::SetResetOverlay), SURFACE_3).clicked() {
                        self.settings.overlay_pos = None;
                        changed = true;
                    }
                    kit::hairline(ui, 0.0);
                    changed |=
                        kit::toggle(ui, &mut self.settings.card_badges, lang.t(Key::SetBadges));
                    ui.add(egui::Label::new(caption(lang.t(Key::SetBadgesNote))).wrap());
                });

                kit::section(ui, &lang.upper(lang.t(Key::GroupBuild)));
                kit::group(ui, |ui| {
                    if kit::toggle(ui, &mut self.settings.item_sets, lang.t(Key::SetItemSets)) {
                        if self.settings.item_sets {
                            // Write the sets again in the next champ select.
                            self.item_set_for = None;
                            self.item_sets_cleared = false;
                        } else {
                            let _ = self.h.cmd.send(Cmd::ClearItemSets);
                            self.item_sets_cleared = true;
                        }
                        changed = true;
                    }
                });

                // The tips of the game: the next items and the summary. Both are on by default. They suggest, and the player decides.
                kit::section(ui, &lang.upper(lang.t(Key::GroupSuggest)));
                kit::group(ui, |ui| {
                    changed |= kit::toggle(
                        ui,
                        &mut self.settings.suggest_items,
                        lang.t(Key::SetSuggestItems),
                    );
                    kit::hairline(ui, 0.0);
                    changed |= kit::toggle(
                        ui,
                        &mut self.settings.suggest_context,
                        lang.t(Key::SetSuggestContext),
                    );
                    ui.add(egui::Label::new(caption(lang.t(Key::SetSuggestNote))).wrap());
                });

                ui.add_space(8.0);
                kit::group(ui, |ui| {
                    self.tools_ui(ui, v);
                });

                kit::section(ui, &lang.upper(lang.t(Key::GroupAbout)));
                kit::group(ui, |ui| {
                    ui.add(egui::Label::new(caption(lang.t(Key::AboutText))).wrap());
                    // Where the games are kept: the My games page used to say it under the list.
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(caption(lang.t(Key::GamesSavedNote))).wrap());
                    if let Some(key) = portable_line(crate::cache::place()) {
                        ui.add_space(4.0);
                        ui.add(egui::Label::new(caption(lang.t(key))).wrap());
                    }
                    ui.add_space(4.0);
                    ui.add(egui::Label::new(caption(lang.t(Key::AboutRiot)).size(10.5)).wrap());
                    if let Some(n) = v.note {
                        ui.add(
                            egui::Label::new(
                                egui::RichText::new(lang.t(n)).size(11.5).color(ORANGE),
                            )
                            .wrap(),
                        );
                    }
                    ui.add_space(6.0);
                    kit::hairline(ui, 0.0);
                    if kit::toggle(
                        ui,
                        &mut self.settings.check_updates,
                        lang.t(Key::SetUpdateCheck),
                    ) {
                        changed = true;
                    }
                    ui.add(egui::Label::new(caption(lang.t(Key::SetUpdateCheckNote))).wrap());
                    // A newer version: a link to the download page.
                    if let Some(tag) = &v.update {
                        ui.add_space(4.0);
                        let text = egui::RichText::new(lang.fmt(Key::UpdateAvailable, &[tag]))
                            .size(12.5)
                            .family(semibold())
                            .color(BLUE);
                        let resp = ui.add(egui::Label::new(text).sense(Sense::click()));
                        if resp.hovered() {
                            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
                        }
                        if resp.on_hover_text(lang.t(Key::UpdateTip)).clicked() {
                            open_url(&update::releases_url(update::REPO));
                        }
                    }
                    ui.add_space(6.0);
                    ui.horizontal_wrapped(|ui| {
                        if kit::accent_button(ui, lang.t(Key::SendFeedback), SURFACE_3)
                            .on_hover_text(lang.t(Key::SendFeedbackNote))
                            .clicked()
                        {
                            let feedback = feedback_of(
                                &self.settings,
                                crate::cache::place(),
                                crate::win::windows_build(),
                            );
                            open_url(&update::issue_url(update::REPO, &feedback));
                        }
                        if kit::accent_button(ui, lang.t(Key::OpenDataFolder), SURFACE_3).clicked()
                        {
                            let _ = std::process::Command::new("explorer")
                                .arg(crate::cache::app_dir())
                                .spawn();
                        }
                        if kit::accent_button(ui, lang.t(Key::ReloadData), SURFACE_3).clicked() {
                            let _ = self.h.cmd.send(Cmd::Reload);
                        }
                        // The X of the window only hides it, so this is the way to end Hexsight.
                        quit = kit::accent_button(ui, lang.t(Key::QuitApp), BLUE).clicked();
                    });
                });
                ui.add_space(8.0);
            });
        self.debug_scroll_end = (page.content_size.y - page.inner_rect.height()).max(0.0);
        if changed {
            self.mark_dirty();
        }
        if quit {
            self.settings.save();
            ui.ctx().send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    /// A warning where the player looks: a black or frozen capture, or League in
    /// Fullscreen (the card reader cannot see the cards). A group with an orange mark and
    /// the text in orange, and the button "Fix" under it while League runs in Fullscreen.
    /// It draws nothing when the capture is fine and the window mode is not Fullscreen. The
    /// Settings page and the waiting screen of a game show it first; the overlay has its own
    /// hint. Only a click on "Fix" does anything.
    fn warning_banner(&mut self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let fullscreen = v.window_mode == Some(0);
        let Some(text) = capture_hint(lang, v.capture)
            .or_else(|| fullscreen.then(|| lang.t(Key::FullscreenNote)))
        else {
            return;
        };
        let mut fix = false;
        ui.add_space(4.0);
        kit::group(ui, |ui| {
            Self::check_row(ui, Check::Warn, text);
            if fullscreen {
                ui.horizontal(|ui| {
                    ui.add_space(24.0);
                    fix = self.fix_button(ui, v);
                });
            }
        });
        if fix {
            let ctx = ui.ctx().clone();
            self.fix_borderless(&ctx);
        }
        ui.add_space(4.0);
    }

    /// The card reading tools, folded under their title: what the reader did, the fix for
    /// Fullscreen, the hint for a bad capture, and the button that saves the last scan. The
    /// rows about the overlay (the unlock key, its place, the badges) are in the Overlay group.
    fn tools_ui(&mut self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let (r, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 28.0), Sense::click());
        let p = ui.painter();
        p.text(
            r.left_center(),
            Align2::LEFT_CENTER,
            lang.t(Key::GroupCardReading),
            FontId::proportional(13.5),
            LABEL,
        );
        let arrow = if self.tools_open {
            glyph::CHEVRON_UP
        } else {
            glyph::CHEVRON_DOWN
        };
        p.text(
            r.right_center(),
            Align2::RIGHT_CENTER,
            arrow,
            FontId::new(11.0, icons()),
            LABEL_3,
        );
        if resp.hovered() {
            ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
        }
        if resp.clicked() {
            self.tools_open = !self.tools_open;
        }
        if !self.tools_open {
            return;
        }
        ui.add_space(4.0);
        for (label, value, warn) in read_rows(&v.read, lang) {
            ui.horizontal(|ui| {
                ui.label(body(label).size(12.5));
                ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                    ui.label(egui::RichText::new(value).size(12.5).color(if warn {
                        ORANGE
                    } else {
                        LABEL_2
                    }));
                });
            });
            kit::hairline(ui, 0.0);
        }
        // League in Fullscreen: the fix sits under the rows, next to the warning value.
        if v.window_mode == Some(0) {
            ui.add_space(2.0);
            if self.fix_button(ui, v) {
                let ctx = ui.ctx().clone();
                self.fix_borderless(&ctx);
            }
        }
        // The age of the scan counts up. Wake the page each second.
        if v.read.scan_age.is_some() {
            ui.ctx()
                .request_repaint_after(std::time::Duration::from_secs(1));
        }
        // A black or frozen capture: say it, and say the fix.
        if let Some(hint) = capture_hint(lang, v.read.capture) {
            ui.add(egui::Label::new(egui::RichText::new(hint).size(11.5).color(ORANGE)).wrap());
        }
        if v.read.in_game {
            ui.add(egui::Label::new(caption(lang.t(Key::ReadInGameNote))).wrap());
        }
        ui.add_space(4.0);
        if kit::accent_button(ui, lang.t(Key::SaveScan), SURFACE_3).clicked() {
            let msg = match self.save_scan(v) {
                Ok(()) => lang.t(Key::ScanSaved).to_string(),
                Err(e) => lang.fmt(Key::ScanSaveFailed, &[&e]),
            };
            self.h.shared.lock().unwrap().say(msg);
        }
        ui.add(egui::Label::new(caption(lang.t(Key::SaveScanNote))).wrap());
    }
}

// ---------------------------------------------------------------------------
// Overlay

/// Height of a cell of the overlay strip, in points.
const CELL_H: f32 = 80.0;

/// The room at the left and right of a cell, in points (before: 10).
const CELL_PAD: f32 = 8.0;

/// The tier badge in the top right corner of a cell, in points.
const CELL_BADGE: f32 = 30.0;

/// The distance from the top of a cell to the top of its game count, in points.
const CELL_GAMES_TOP: f32 = 44.0;

/// The distance from the top of a cell to the middle of its "top" label, in points. The label
/// sits under the position, left of the badge.
const CELL_TOP_Y: f32 = 31.0;

/// The fonts of the overlay strip. The texts are bold or semibold and light. The tier
/// badge is the biggest mark of a cell, so the game count is a calm 12.5 points,
/// semibold (before: 14 points, bold) and the labels 11.5 (before: 10.5 and 11).
fn strip_name_font() -> FontId {
    FontId::new(13.0, bold())
}
fn strip_level_font() -> FontId {
    FontId::new(11.5, semibold())
}
fn cell_label_font() -> FontId {
    FontId::new(11.5, bold())
}
fn cell_top_font() -> FontId {
    FontId::new(11.0, bold())
}
fn cell_games_font() -> FontId {
    FontId::new(12.5, semibold())
}
fn cell_pair_font() -> FontId {
    FontId::new(11.5, bold())
}
fn cell_word_font() -> FontId {
    FontId::new(11.5, semibold())
}

/// The font of the orange hint lines of the overlay: a bad capture, and Fullscreen (before: 10.5).
fn hint_font() -> FontId {
    FontId::new(11.5, bold())
}

impl App {
    /// The overlay. In a game between pick screens it is a small pill. When the
    /// reader finds cards it opens the strip, and the strip closes 1.5 s after the
    /// cards go away. The window shrinks to its content, so the overlay covers as
    /// little of the game as possible. The strip is wider than the pill. It shows the
    /// tier letter, the games and the data bars. It has no augment name and no icon: the
    /// game prints them.
    fn overlay_ui(&mut self, ui: &mut Ui, v: &View) {
        let full = ui.max_rect();
        let mode = overlay_mode(v.offer.is_some(), self.shows_advice(v));
        // The advice card goes away `ADVICE_SHOW` after the last change. Draw the overlay again then.
        if let Some(age) = self.advice_age().filter(|age| *age < ADVICE_SHOW) {
            ui.ctx()
                .request_repaint_after(ADVICE_SHOW - age + std::time::Duration::from_millis(50));
        }
        let (card, w) = match mode {
            OverlayMode::Pill => {
                let card = self.overlay_pill(ui, v, full);
                (card, card.width() + 2.0 * OVERLAY_SIDE)
            }
            OverlayMode::Strip | OverlayMode::Advice => {
                // Reserve paint slots now. The card size is known after the content.
                let shadow = ui.painter().add(egui::Shape::Noop);
                let bg = ui.painter().add(egui::Shape::Noop);
                // Lay out for the wanted width. The window follows a frame later. The content
                // has 10 points of room at the sides and 8 at the top.
                let top_left = full.min + vec2(OVERLAY_SIDE, OVERLAY_TOP);
                let card_w = OVERLAY_STRIP_W - 8.0;
                let inner = Rect::from_min_max(
                    top_left + vec2(10.0, 8.0),
                    pos2(top_left.x + card_w - 10.0, full.bottom() + 2000.0),
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(inner)
                        .layout(egui::Layout::top_down(egui::Align::Min)),
                );
                child.spacing_mut().item_spacing.y = 4.0;
                if mode == OverlayMode::Strip {
                    self.overlay_strip(&mut child, v);
                } else {
                    self.overlay_advice(&mut child, v);
                }
                let card = Rect::from_min_max(
                    top_left,
                    pos2(top_left.x + card_w, child.min_rect().bottom() + 12.0),
                );
                let fill = overlay_fill(self.overlay_unlocked);
                let p = ui.painter();
                p.set(
                    shadow,
                    overlay_shadow().as_shape(card, CornerRadius::same(16)),
                );
                p.set(
                    bg,
                    egui::epaint::RectShape::new(
                        card,
                        CornerRadius::same(16),
                        fill,
                        overlay_edge(),
                        egui::StrokeKind::Inside,
                    ),
                );
                (card, OVERLAY_WINDOW_W)
            }
        };
        if self.overlay_unlocked {
            let r = ui.interact(card, ui.scope_id().with("drag"), Sense::drag());
            if r.drag_started() {
                ui.ctx().send_viewport_cmd(egui::ViewportCommand::StartDrag);
            }
        }
        // Fit the window to the card. The threshold stops resize loops. Not
        // while the zoom changes: the command would use the old zoom.
        let want = card.bottom() - full.top() + OVERLAY_BOTTOM;
        let size = vec2(w, want);
        if !self.place_window && self.overlay_size.is_none_or(|s| (s - size).length() > 3.0) {
            self.overlay_size = Some(size);
            ui.ctx()
                .send_viewport_cmd(egui::ViewportCommand::InnerSize(size));
        }
    }

    /// The pill of a game between pick screens: "Hexsight watching", 162 by 40 points. A
    /// hint shows under it when the reader cannot work: a black or frozen capture, or
    /// League in Fullscreen. While the overlay is unlocked, the pill carries the drag
    /// hint. Outside a game the overlay takes clicks, and a click on the pill closes it.
    /// Returns the rect that holds all of it.
    fn overlay_pill(&mut self, ui: &mut Ui, v: &View, full: Rect) -> Rect {
        let lang = self.lang;
        let drag_hint = self.overlay_unlocked.then(|| {
            lang.fmt(
                Key::OverlayDrag,
                &[&key_text(lang, self.settings.overlay_key)],
            )
        });
        let name = ui
            .painter()
            .layout_no_wrap("Hexsight".to_string(), pill_name_font(), LABEL);
        let word = ui.painter().layout_no_wrap(
            pill_word(lang, v.phase, v.level, v.picks_done),
            pill_word_font(),
            LABEL,
        );
        let hint_text = drag_hint.as_ref().map(|t| {
            ui.painter()
                .layout_no_wrap(t.clone(), FontId::new(12.0, bold()), ORANGE)
        });
        // 16 points of room, the dot (6) and 7 more, the text, 18 points of room.
        let text_w = hint_text
            .as_ref()
            .map_or(name.size().x + 6.0 + word.size().x, |g| g.size().x);
        let pill_w = (16.0 + 6.0 + 7.0 + text_w + 18.0).ceil().max(PILL_W);
        let hint = capture_hint(lang, v.capture)
            .or_else(|| (v.window_mode == Some(0)).then(|| lang.t(Key::FullscreenNote)));
        let hint_g = hint.map(|t| {
            ui.painter().layout(
                t.to_string(),
                hint_font(),
                ORANGE,
                OVERLAY_HINT_W - 8.0 - 20.0,
            )
        });
        // The button "Fix" under the Fullscreen note. A locked overlay in a game lets the mouse pass, so it shows only when the overlay takes clicks.
        let fix_here = capture_hint(lang, v.capture).is_none()
            && v.window_mode == Some(0)
            && !passes_mouse(true, v.phase, self.overlay_unlocked);
        let fix_h = if fix_here { 30.0 } else { 0.0 };
        let width = if hint_g.is_some() {
            (OVERLAY_HINT_W - 8.0).max(pill_w)
        } else {
            pill_w
        };
        let pill = Rect::from_min_size(
            full.min + vec2(OVERLAY_SIDE, OVERLAY_TOP),
            vec2(pill_w, PILL_H),
        );
        let fill = overlay_fill(self.overlay_unlocked);
        let line = overlay_edge();
        let p = ui.painter();
        p.add(overlay_shadow().as_shape(pill, CornerRadius::same(255)));
        p.rect_filled(pill, CornerRadius::same(255), fill);
        p.rect_stroke(
            pill,
            CornerRadius::same(255),
            line,
            egui::StrokeKind::Inside,
        );
        // The green dot says that Hexsight is on and waits for a pick screen.
        p.circle_filled(
            pos2(pill.left() + 16.0 + 3.0, pill.center().y),
            3.5,
            if self.overlay_unlocked { ORANGE } else { GREEN },
        );
        let x = pill.left() + 16.0 + 6.0 + 7.0;
        match hint_text {
            Some(g) => {
                p.galley(pos2(x, pill.center().y - g.size().y / 2.0), g, ORANGE);
            }
            None => {
                let after = x + name.size().x + 6.0;
                p.galley(pos2(x, pill.center().y - name.size().y / 2.0), name, LABEL);
                p.galley(
                    pos2(after, pill.center().y - word.size().y / 2.0),
                    word,
                    LABEL_2,
                );
            }
        }
        let mut bottom = pill.bottom();
        if let Some(hg) = hint_g {
            let r = Rect::from_min_size(
                pill.left_bottom() + vec2(0.0, 4.0),
                vec2(width, hg.size().y + 14.0 + fix_h),
            );
            p.add(overlay_shadow().as_shape(r, CornerRadius::same(12)));
            p.rect_filled(r, CornerRadius::same(12), fill);
            p.rect_stroke(
                r,
                CornerRadius::same(12),
                egui::Stroke::new(1.25, ORANGE.gamma_multiply(0.9)),
                egui::StrokeKind::Inside,
            );
            let note_h = hg.size().y;
            p.galley(r.min + vec2(10.0, 7.0), hg, ORANGE);
            bottom = r.bottom();
            if fix_here {
                let area = Rect::from_min_size(
                    r.min + vec2(10.0, note_h + 10.0),
                    vec2(width - 20.0, 24.0),
                );
                let mut child = ui.new_child(
                    egui::UiBuilder::new()
                        .max_rect(area)
                        .layout(egui::Layout::left_to_right(egui::Align::Center)),
                );
                if self.fix_button(&mut child, v) {
                    let ctx = ui.ctx().clone();
                    self.fix_borderless(&ctx);
                }
            }
        }
        // Outside a game the window takes the mouse, so a click on the pill is the way out.
        if v.phase != Phase::InGame && !self.overlay_unlocked {
            let click = ui.interact(pill, ui.scope_id().with("pill"), Sense::click());
            if click.hovered() {
                ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
            }
            let tip = lang.fmt(
                Key::OverlayClose,
                &[&key_text(lang, self.settings.overlay_key)],
            );
            if click.on_hover_text(tip).clicked() {
                self.overlay_auto = false;
                let ctx = ui.ctx().clone();
                self.set_overlay(&ctx, false);
            }
        }
        Rect::from_min_max(pill.min, pos2(pill.min.x + width, bottom))
    }

    /// The orange hint lines of the strip and of the advice card: a black or frozen
    /// capture, and League in Fullscreen. The advice card replaces the pill (and its hint)
    /// for a while, so it must show them too.
    fn hint_lines(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        if let Some(hint) = capture_hint(lang, v.capture) {
            ui.add(
                egui::Label::new(egui::RichText::new(hint).font(hint_font()).color(ORANGE)).wrap(),
            );
        }
        if v.window_mode == Some(0) {
            ui.add(
                egui::Label::new(
                    egui::RichText::new(lang.t(Key::FullscreenNote))
                        .font(hint_font())
                        .color(ORANGE),
                )
                .wrap(),
            );
        }
    }

    /// The strip of a pick screen, 340 points wide: "Hexsight  Level 7", then one cell
    /// for each card (Left, Middle, Right). A cell shows the tier badge of its card, the
    /// games behind the stats, and the confidence in three bars (and a word when the
    /// sample is small). Two marks are off by default: with "Highlight
    /// top-tier cards" on, the header gets the top-tier line and a cell gets "top" and
    /// the green frame when the card is in the top tier (`top_tier`). With "Show pair
    /// hints" on, a cell gets a small mark when the card pairs well with an earlier one.
    /// It shows the tier letter, the games and the data bars. It has no augment name and
    /// no icon: the game prints them.
    fn overlay_strip(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let Some(offer) = &v.offer else { return };
        if self.overlay_unlocked {
            let lock = lang.fmt(
                Key::OverlayDrag,
                &[&key_text(lang, self.settings.overlay_key)],
            );
            ui.label(
                egui::RichText::new(lock)
                    .size(11.5)
                    .family(semibold())
                    .color(ORANGE),
            );
        }
        let earlier = earlier_offers(&v.offered, offer.screen);
        let (cells, top) = pick_cells(
            v.details.as_deref(),
            offer,
            &earlier,
            self.settings.min_games,
            self.settings.marks(),
        );
        // The header: the mark and the name, your level, and the top-tier line at the right.
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        let mark =
            Rect::from_center_size(pos2(head.left() + 9.0, head.center().y), Vec2::splat(18.0));
        kit::app_mark(ui, mark);
        let name = text_at(
            ui,
            pos2(mark.right() + 8.0, head.center().y - 9.0),
            "Hexsight",
            strip_name_font(),
            LABEL,
            80.0,
        );
        if v.level > 0 {
            text_at(
                ui,
                pos2(name.right() + 8.0, head.center().y - 7.0),
                &lang.fmt(Key::Level, &[&v.level]),
                strip_level_font(),
                LABEL,
                80.0,
            );
        }
        // The top-tier line shows only when the player turned the highlight on.
        if let Some(line) = top {
            let (line, ink) = top_line_text(lang, line);
            let g = ui
                .painter()
                .layout_no_wrap(line, FontId::new(11.5, semibold()), ink);
            let left = head.right() - g.size().x;
            ui.painter()
                .circle_filled(pos2(left - 9.0, head.center().y), 3.0, ink);
            ui.painter()
                .galley(pos2(left, head.center().y - g.size().y / 2.0), g, ink);
        }
        self.hint_lines(ui, v);
        if !cells.is_empty() {
            let n = cells.len().min(4);
            let gap = 6.0;
            let width = ui.available_width();
            let cell_w = (width - gap * (n as f32 - 1.0)) / n as f32;
            let (row, _) = ui.allocate_exact_size(vec2(width, CELL_H), Sense::hover());
            for (i, c) in cells.iter().take(n).enumerate() {
                let rect = Rect::from_min_size(
                    pos2(row.left() + i as f32 * (cell_w + gap), row.top()),
                    vec2(cell_w, CELL_H),
                );
                self.overlay_cell(ui, rect, c);
            }
        }
        if let Some(n) = v.note {
            ui.add(
                egui::Label::new(egui::RichText::new(lang.t(n)).size(11.5).color(ORANGE)).wrap(),
            );
        }
    }

    /// One cell of the strip, 80 points high. Top left: where the card is (and, under it, "top").
    /// Top right: its tier badge, 30 by 30 (a "?" without stats). Under them: its games.
    /// At the bottom: the three confidence bars, the word for a small sample (`ConfLow`) only
    /// then, and the pairs mark at the right. A card without stats shows "No data".
    fn overlay_cell(&self, ui: &Ui, rect: Rect, c: &PickCell) {
        let lang = self.lang;
        let p = ui.painter();
        if c.top {
            kit::paint_top(p, rect, 11);
        } else {
            p.rect_filled(rect, CornerRadius::same(11), Color32::from_white_alpha(24));
            p.rect_stroke(
                rect,
                CornerRadius::same(11),
                egui::Stroke::new(1.0, Color32::from_white_alpha(36)),
                egui::StrokeKind::Inside,
            );
        }
        let (left, right) = (rect.left() + CELL_PAD, rect.right() - CELL_PAD);
        // The tier is the first thing a player reads, so it is the biggest mark of the cell.
        let badge = cell_badge_rect(rect);
        kit::paint_tier(p, badge, c.tier);
        let label = lang.upper(position_label(lang, c.x));
        kit::shadow_text(
            p,
            pos2(left, rect.top() + 14.0),
            Align2::LEFT_CENTER,
            &label,
            cell_label_font(),
            if c.top { GREEN } else { LABEL_2 },
            1.0,
        );
        if c.top {
            // "top" goes under the position, left of the badge. On the same line after the position it
            // would reach the badge in the middle cell ("MIDDLE" and "top" are 67 points, the room is 50).
            kit::shadow_text(
                p,
                pos2(left, rect.top() + CELL_TOP_Y),
                Align2::LEFT_CENTER,
                lang.t(Key::TopLabel),
                cell_top_font(),
                GREEN,
                1.0,
            );
        }
        // The game count: semibold, with a dark shadow one point down and right. Its top is 44
        // points down, 6 under the badge.
        let games = c
            .games
            .map_or_else(|| "–".to_string(), |g| lang.games(g, &fmt_n(lang, g)));
        kit::shadow_text(
            p,
            pos2(left, rect.top() + CELL_GAMES_TOP),
            Align2::LEFT_TOP,
            &games,
            cell_games_font(),
            LABEL,
            1.0,
        );
        let y = rect.bottom() - 14.0;
        // The bottom line: the bars and, for a small sample only, its word. `used` is how far
        // they reach from the left edge of the content.
        let used = match c.confidence() {
            Some(conf) => {
                kit::paint_bars(p, pos2(left, y - 2.0), conf.bars());
                if conf == Confidence::Low {
                    let g =
                        p.layout_no_wrap(lang.t(conf.key()).to_string(), cell_word_font(), LABEL_2);
                    let used = kit::BARS_W + 6.0 + g.size().x;
                    kit::shadow_galley(
                        p,
                        pos2(left + kit::BARS_W + 6.0, y - g.size().y / 2.0),
                        g,
                        LABEL_2,
                        1.0,
                    );
                    used
                } else {
                    kit::BARS_W
                }
            }
            None => {
                let g =
                    p.layout_no_wrap(lang.t(Key::NoData).to_string(), cell_word_font(), LABEL_2);
                let used = g.size().x;
                kit::shadow_galley(p, pos2(left, y - g.size().y / 2.0), g, LABEL_2, 1.0);
                used
            }
        };
        // The pairs mark (only with the setting on): bottom right, right-aligned on the line of the
        // bars. If the bars and the word of a small sample leave no room for it, it goes under the
        // position at the left instead, unless "top" is there. Then it does not show.
        if c.pair.is_some() {
            let word = p.layout_no_wrap(lang.t(Key::PairMark).to_string(), cell_pair_font(), TEAL);
            let mark_w = PAIR_GLYPH_W + word.size().x;
            let at = if right - (left + used) - MARK_GAP >= mark_w {
                Some(pos2(right - mark_w, y))
            } else if !c.top {
                Some(pos2(left, rect.top() + CELL_TOP_Y))
            } else {
                None
            };
            if let Some(at) = at {
                p.text(
                    at,
                    Align2::LEFT_CENTER,
                    glyph::LINK,
                    FontId::new(10.5, icons()),
                    TEAL,
                );
                kit::shadow_galley(
                    p,
                    pos2(at.x + PAIR_GLYPH_W, at.y - word.size().y / 2.0),
                    word,
                    TEAL,
                    1.0,
                );
            }
        }
    }
}

/// The width of the link glyph of the pairs mark with its gap, in points.
const PAIR_GLYPH_W: f32 = 14.0;

/// The least room between the bars (or their word) and the pairs mark, in points.
const MARK_GAP: f32 = 2.0;

/// The tier badge of a cell: 30 by 30 points in the top right corner.
fn cell_badge_rect(cell: Rect) -> Rect {
    Rect::from_min_size(
        pos2(cell.right() - CELL_PAD - CELL_BADGE, cell.top() + CELL_PAD),
        Vec2::splat(CELL_BADGE),
    )
}

// ---------------------------------------------------------------------------
// Suggestions in the windows

/// Height of a cell of the advice card of the overlay, in points (before: 46). The name is
/// 11.5 points and the reason 11, so the cell needs 10 points more.
const ADVICE_CELL_H: f32 = 56.0;

/// The gap between the cells of the advice card, in points.
const ADVICE_GAP: f32 = 6.0;

/// The width of a cell of the advice card for a card of this inner width: three cells and two gaps.
fn advice_cell_w(width: f32) -> f32 {
    (width - 2.0 * ADVICE_GAP) / 3.0
}

impl App {
    /// Remember when the advice last changed. The overlay shows its advice card for `ADVICE_SHOW` after a change.
    fn note_advice(&mut self, v: &View) {
        let sig = v.advice.as_ref().map(|a| a.signature());
        self.advice_seen = match (sig, self.advice_seen) {
            (None, _) => None,
            (Some(s), Some((old, at))) if s == old => Some((old, at)),
            (Some(s), _) => Some((s, std::time::Instant::now())),
        };
    }

    /// The time since the advice last changed. `None` without advice.
    fn advice_age(&self) -> Option<std::time::Duration> {
        self.advice_seen.map(|(_, at)| at.elapsed())
    }

    /// Does the overlay show the advice card in this frame? See `advice_visible`.
    fn shows_advice(&self, v: &View) -> bool {
        let (Some(sd), Some(a)) = (&v.sd, v.advice.as_deref()) else {
            return false;
        };
        advice_visible(
            overlay_has_content(self.lang, sd, &self.settings, a),
            a.me_dead,
            self.overlay_unlocked,
            self.advice_age(),
        )
    }

    /// The card "Next items": up to three items from the build of your champion, with the reason of each
    /// and a chip that says if you have the gold. It suggests and decides nothing: Hexsight buys nothing.
    /// A reason that comes from a rule of Hexsight is in the brighter ink and says so in its tooltip.
    /// The build style is the note at the right of the section header (`section_note`), and
    /// the note "From build data and Hexsight's own rules" is the tooltip of the card.
    fn items_card(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        ui.scope(|ui| {
            kit::group(ui, |ui| {
                let (Some(sd), Some(a)) = (&v.sd, v.advice.as_deref()) else {
                    ui.add(egui::Label::new(caption(lang.t(Key::SuggestWait))).wrap());
                    return;
                };
                if a.no_build {
                    ui.add(egui::Label::new(caption(lang.t(Key::SuggestNoBuild))).wrap());
                    return;
                }
                if a.items.is_empty() {
                    let key = if a.slots_full {
                        Key::SuggestFull
                    } else {
                        Key::SuggestNone
                    };
                    ui.add(egui::Label::new(caption(lang.t(key))).wrap());
                    return;
                }
                for (i, p) in a.items.iter().enumerate() {
                    if i > 0 {
                        kit::hairline(ui, 0.0);
                    }
                    self.item_row(ui, v, sd, a, p, i);
                }
            });
        })
        .response
        .on_hover_text(lang.t(Key::SuggestNote));
    }

    /// One row of the card "Next items": the icon, the name, the reason, and the chip.
    fn item_row(&self, ui: &mut Ui, v: &View, sd: &StaticData, a: &Advice, p: &ItemPick, i: usize) {
        let lang = self.lang;
        let (r, _) = ui.allocate_exact_size(vec2(ui.available_width(), ITEM_ROW_H), Sense::hover());
        let icon = Rect::from_min_size(
            pos2(r.left(), r.center().y - ITEM_ICON / 2.0),
            Vec2::splat(ITEM_ICON),
        );
        let item = sd.items.get(&p.id);
        match item {
            Some(it) => self.paint_icon(ui, &it.icon, icon, 8),
            None => {
                ui.painter()
                    .rect_filled(icon, CornerRadius::same(8), SURFACE_2);
            }
        }
        let name = item.map_or_else(|| format!("#{}", p.id), |it| it.name.clone());
        let x = icon.right() + 8.0;
        let room = r.right() - x - ITEM_CHIP_W;
        text_at(
            ui,
            pos2(x, r.top() + 4.0),
            &name,
            FontId::new(13.0, semibold()),
            LABEL,
            room,
        );
        let reason = reason_text(lang, sd, p.reason);
        text_at(
            ui,
            pos2(x, r.top() + 22.0),
            &reason,
            FontId::proportional(11.5),
            if p.reason.is_rule() { LABEL } else { LABEL_2 },
            room,
        );
        // The chip: a check mark when you have the gold, else how much is missing. Without the gold, no chip.
        let mut tip = format!("{name}\n{}", reason_tip(lang, p.reason));
        if a.has_gold {
            let short = p.need.saturating_sub(v.gold.unwrap_or(0));
            let at = pos2(r.right(), r.center().y);
            if p.ready {
                ui.painter().text(
                    at,
                    Align2::RIGHT_CENTER,
                    glyph::CHECK,
                    FontId::new(13.0, icons()),
                    GREEN,
                );
                tip = format!("{tip}\n{}", lang.t(Key::TipGoldReady));
            } else {
                ui.painter().text(
                    at,
                    Align2::RIGHT_CENTER,
                    format!("-{short}"),
                    FontId::proportional(11.5),
                    LABEL_2,
                );
                tip = format!("{tip}\n{}", lang.fmt(Key::TipGoldShort, &[&short]));
            }
        }
        ui.interact(r, ui.scope_id().with(("next_item", i)), Sense::hover())
            .on_hover_text(tip);
    }

    /// The card "This game": the damage mix of the enemy team as a bar and in words, the fed enemy, and
    /// the healers and tanks as small champion icons. Everything is Hexsight's own estimate, and the
    /// tooltip says so. It shows only the rows that have data.
    fn summary_card(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let (Some(sd), Some(c)) = (&v.sd, v.advice.as_deref().and_then(|a| a.ctx.as_ref())) else {
            return;
        };
        if !summary_has_rows(c) {
            return;
        }
        kit::section(ui, &lang.upper(lang.t(Key::SecThisGame)));
        ui.scope(|ui| {
            kit::group(ui, |ui| {
                if let Some(ap) = c.ap_pct {
                    let (row, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 22.0), Sense::hover());
                    let p = ui.painter();
                    p.text(
                        row.left_center(),
                        Align2::LEFT_CENTER,
                        lang.t(Key::CtxEnemyDamage),
                        FontId::new(13.5, semibold()),
                        LABEL,
                    );
                    p.text(
                        row.right_center(),
                        Align2::RIGHT_CENTER,
                        mix_text(lang, ap),
                        FontId::proportional(11.5),
                        LABEL_2,
                    );
                    // The bar: ability power in violet at the left, attack damage in orange. The words say the same.
                    let (bar, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 6.0), Sense::hover());
                    let p = ui.painter();
                    p.rect_filled(bar, CornerRadius::same(3), ORANGE);
                    if ap > 0 {
                        let split = bar.left() + bar.width() * f32::from(ap.min(100)) / 100.0;
                        let radius = if ap >= 100 {
                            CornerRadius::same(3)
                        } else {
                            CornerRadius {
                                nw: 3,
                                sw: 3,
                                ne: 0,
                                se: 0,
                            }
                        };
                        p.rect_filled(
                            Rect::from_min_max(bar.min, pos2(split, bar.bottom())),
                            radius,
                            PRISM,
                        );
                    }
                }
                if let Some(f) = &c.fed {
                    ui.add_space(4.0);
                    let (row, _) =
                        ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
                    let icon = Rect::from_min_size(
                        pos2(row.left(), row.center().y - SUMMARY_ICON / 2.0),
                        Vec2::splat(SUMMARY_ICON),
                    );
                    match sd.champions.get(&f.champion) {
                        Some(ch) => self.paint_icon(ui, &ch.icon, icon, 5),
                        None => {
                            ui.painter()
                                .rect_filled(icon, CornerRadius::same(5), SURFACE_2);
                        }
                    }
                    let level = (f.level > 0).then(|| lang.fmt(Key::Level, &[&f.level]));
                    let level_w = level.as_ref().map_or(0.0, |t| {
                        ui.painter()
                            .layout_no_wrap(t.clone(), FontId::proportional(11.5), LABEL_2)
                            .size()
                            .x
                            + 8.0
                    });
                    if let Some(t) = level {
                        ui.painter().text(
                            row.right_center(),
                            Align2::RIGHT_CENTER,
                            t,
                            FontId::proportional(11.5),
                            LABEL_2,
                        );
                    }
                    let text = lang.fmt(
                        Key::CtxFed,
                        &[
                            &champion_name(sd, f.champion),
                            &score_line(f.kills, f.deaths, f.assists),
                        ],
                    );
                    let x = icon.right() + 8.0;
                    text_at(
                        ui,
                        pos2(x, row.center().y - 8.0),
                        &text,
                        FontId::new(12.5, semibold()),
                        LABEL,
                        row.right() - x - level_w,
                    );
                }
                self.icon_rows(ui, sd, c);
            });
        })
        .response
        .on_hover_text(lang.t(Key::CtxTip));
    }

    /// The healers and the tanks of the enemy team as small champion icons (three at most for each),
    /// each group after its label. Both groups share a row when they fit in it, else each has a row.
    fn icon_rows(&self, ui: &mut Ui, sd: &StaticData, c: &GameContext) {
        let lang = self.lang;
        let groups: Vec<(&str, &[i64])> = [
            (lang.t(Key::CtxHealers), c.healers.as_slice()),
            (lang.t(Key::CtxTanks), c.tanks.as_slice()),
        ]
        .into_iter()
        .filter(|g| !g.1.is_empty())
        .collect();
        if groups.is_empty() {
            return;
        }
        let font = FontId::proportional(11.5);
        let step = SUMMARY_ICON + 4.0;
        let width_of = |g: &(&str, &[i64])| {
            ui.painter()
                .layout_no_wrap(g.0.to_string(), font.clone(), LABEL_2)
                .size()
                .x
                + 6.0
                + g.1.len().min(SUMMARY_ICONS) as f32 * step
        };
        let total: f32 =
            groups.iter().map(width_of).sum::<f32>() + 12.0 * (groups.len() - 1) as f32;
        let rows: Vec<&[(&str, &[i64])]> = if total <= ui.available_width() {
            vec![&groups[..]]
        } else {
            groups.chunks(1).collect()
        };
        let mut gi = 0;
        for row_groups in rows {
            ui.add_space(4.0);
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), step), Sense::hover());
            let mut x = row.left();
            for g in row_groups {
                let label = text_at(
                    ui,
                    pos2(x, row.center().y - 7.0),
                    g.0,
                    font.clone(),
                    LABEL_2,
                    row.right() - x,
                );
                x = label.right() + 6.0;
                for id in g.1.iter().take(SUMMARY_ICONS) {
                    let icon = Rect::from_min_size(
                        pos2(x, row.center().y - SUMMARY_ICON / 2.0),
                        Vec2::splat(SUMMARY_ICON),
                    );
                    match sd.champions.get(id) {
                        Some(ch) => {
                            self.paint_icon(ui, &ch.icon, icon, 5);
                            ui.interact(
                                icon,
                                ui.scope_id().with(("summary_icon", gi, *id)),
                                Sense::hover(),
                            )
                            .on_hover_text(&ch.name);
                        }
                        None => {
                            ui.painter()
                                .rect_filled(icon, CornerRadius::same(5), SURFACE_2);
                        }
                    }
                    x += step;
                }
                x += 8.0;
                gi += 1;
            }
        }
    }

    /// The advice card of the overlay, 340 points wide: the header with the build style, the hint
    /// lines of a bad capture or Fullscreen (this card replaces the pill and its hint for a while),
    /// the context line, and up to three item cells. It shows only when `advice_visible` says so, and
    /// never while the pick strip shows. The overlay lets the mouse pass in a game, so there is no
    /// tooltip: the name and the reason are in the cell. A reason from a rule of Hexsight has the
    /// brighter ink (the card has no footnote any more; the tooltip of the card in the main window
    /// says it).
    fn overlay_advice(&self, ui: &mut Ui, v: &View) {
        let lang = self.lang;
        let (Some(sd), Some(a)) = (&v.sd, v.advice.as_deref()) else {
            return;
        };
        if self.overlay_unlocked {
            let lock = lang.fmt(
                Key::OverlayDrag,
                &[&key_text(lang, self.settings.overlay_key)],
            );
            ui.label(
                egui::RichText::new(lock)
                    .size(11.5)
                    .family(semibold())
                    .color(ORANGE),
            );
        }
        let items = self.settings.suggest_items && !a.items.is_empty();
        let (head, _) = ui.allocate_exact_size(vec2(ui.available_width(), 24.0), Sense::hover());
        let mark =
            Rect::from_center_size(pos2(head.left() + 9.0, head.center().y), Vec2::splat(18.0));
        kit::app_mark(ui, mark);
        let name = text_at(
            ui,
            pos2(mark.right() + 8.0, head.center().y - 9.0),
            "Hexsight",
            strip_name_font(),
            LABEL,
            80.0,
        );
        if items && !a.style.is_empty() {
            let style = lang.fmt(Key::SuggestStyle, &[&lang.style(&a.style)]);
            let g = ui
                .painter()
                .layout_no_wrap(style, strip_level_font(), LABEL_2);
            let left = (head.right() - g.size().x).max(name.right() + 8.0);
            ui.painter()
                .galley(pos2(left, head.center().y - g.size().y / 2.0), g, LABEL_2);
        }
        // A black capture or Fullscreen must not hide behind this card.
        self.hint_lines(ui, v);
        let line = self
            .settings
            .suggest_context
            .then(|| a.ctx.as_ref().and_then(|c| context_line(lang, sd, c)))
            .flatten();
        if let Some(line) = &line {
            let (row, _) = ui.allocate_exact_size(vec2(ui.available_width(), 17.0), Sense::hover());
            text_at(
                ui,
                pos2(row.left(), row.center().y - 8.0),
                line,
                FontId::new(11.5, semibold()),
                LABEL,
                row.width(),
            );
        }
        if items {
            let width = ui.available_width();
            let cell_w = advice_cell_w(width);
            let (row, _) = ui.allocate_exact_size(vec2(width, ADVICE_CELL_H), Sense::hover());
            for (i, p) in a.items.iter().take(3).enumerate() {
                let rect = Rect::from_min_size(
                    pos2(row.left() + i as f32 * (cell_w + ADVICE_GAP), row.top()),
                    vec2(cell_w, ADVICE_CELL_H),
                );
                self.advice_cell(ui, sd, a, rect, p);
            }
        }
    }

    /// One cell of the advice card: the icon (with a green dot when you have the gold), the name in up to
    /// two lines, and the reason under them.
    fn advice_cell(&self, ui: &Ui, sd: &StaticData, a: &Advice, rect: Rect, p: &ItemPick) {
        let lang = self.lang;
        let plate = ui.painter();
        plate.rect_filled(rect, CornerRadius::same(11), Color32::from_white_alpha(24));
        plate.rect_stroke(
            rect,
            CornerRadius::same(11),
            egui::Stroke::new(1.0, Color32::from_white_alpha(36)),
            egui::StrokeKind::Inside,
        );
        let item = sd.items.get(&p.id);
        let icon = Rect::from_min_size(rect.min + vec2(5.0, 4.0), Vec2::splat(28.0));
        match item {
            Some(it) => self.paint_icon(ui, &it.icon, icon, 7),
            None => {
                ui.painter()
                    .rect_filled(icon, CornerRadius::same(7), SURFACE_2);
            }
        }
        if a.has_gold && p.ready {
            ui.painter()
                .circle_filled(icon.right_top() + vec2(-1.0, 1.0), 4.5, GREEN);
            ui.painter().circle_stroke(
                icon.right_top() + vec2(-1.0, 1.0),
                4.5,
                egui::Stroke::new(1.0, PLATE),
            );
        }
        let x = icon.right() + 5.0;
        let name = item.map_or_else(|| format!("#{}", p.id), |it| it.name.clone());
        let mut job = egui::text::LayoutJob::single_section(
            name,
            egui::TextFormat::simple(advice_name_font(), LABEL),
        );
        job.wrap.max_width = (rect.right() - 5.0 - x).max(10.0);
        job.wrap.max_rows = 2;
        let g = ui.painter().layout_job(job);
        ui.painter().galley(pos2(x, rect.top() + 3.0), g, LABEL);
        // The reason sits at the bottom of the cell: its last line ends 6 points above the edge.
        let reason = reason_text(lang, sd, p.reason);
        let ink = if p.reason.is_rule() { LABEL } else { LABEL_2 };
        let mut job = egui::text::LayoutJob::single_section(
            reason,
            egui::TextFormat::simple(advice_reason_font(), ink),
        );
        job.wrap.max_width = (rect.width() - 10.0).max(10.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let g = ui.painter().layout_job(job);
        ui.painter().galley(
            pos2(
                rect.left() + 5.0,
                rect.bottom() - ADVICE_REASON_BOTTOM - g.size().y,
            ),
            g,
            ink,
        );
    }
}

/// The fonts of a cell of the advice card (before: 10.5 and 10).
fn advice_name_font() -> FontId {
    FontId::new(11.5, semibold())
}
fn advice_reason_font() -> FontId {
    FontId::new(11.0, semibold())
}

/// The space between the reason and the bottom edge of a cell of the advice card, in points.
const ADVICE_REASON_BOTTOM: f32 = 6.0;

#[cfg(test)]
mod tests {
    use super::*;

    fn st(id: i64, tier: u8, wr: f32, games: u32) -> AugStat {
        AugStat {
            augment_id: id,
            win_rate: wr,
            games,
            tier,
        }
    }

    fn card(id: i64, x: f32) -> Card {
        Card {
            id,
            x,
            y: 0.48,
            h: 0.03,
        }
    }

    /// Three cards at the usual x centers. The order of ids is the screen order.
    fn cards(ids: [i64; 3]) -> Vec<Card> {
        ids.into_iter()
            .zip([0.2, 0.5, 0.8])
            .map(|(id, x)| card(id, x))
            .collect()
    }

    fn ids(found: &[OfferStat]) -> Vec<i64> {
        found.iter().map(|r| r.id).collect()
    }

    #[test]
    fn offer_uses_the_stats_of_the_pick_screen() {
        let cs = ChampStats {
            augments: vec![
                st(1, 1, 0.60, 5000),
                st(2, 2, 0.58, 5000),
                st(3, 3, 0.55, 5000),
            ],
            // On screen 2 augment 3 is the best.
            stages: vec![
                vec![],
                vec![st(3, 0, 0.70, 900), st(1, 2, 0.57, 900)],
                vec![],
                vec![],
            ],
            ..Default::default()
        };
        let r1 = offer_stats(&cs, &cards([2, 3, 1]), 1, 300);
        // The cards stay in screen order, whatever the tiers are.
        assert_eq!(ids(&r1), vec![2, 3, 1]);
        // Screen 1 has no stage list: every stat comes from all picks.
        let rates = |r: &[OfferStat]| {
            r.iter()
                .map(|c| c.stat.as_ref().map(|a| a.win_rate))
                .collect::<Vec<_>>()
        };
        assert_eq!(rates(&r1), vec![Some(0.58), Some(0.55), Some(0.60)]);
        let r2 = offer_stats(&cs, &cards([2, 3, 1]), 2, 300);
        assert_eq!(ids(&r2), vec![2, 3, 1]);
        // Augment 3 and 1 use the stage list, augment 2 has no stage row and keeps the stat of all picks.
        assert_eq!(rates(&r2), vec![Some(0.58), Some(0.70), Some(0.57)]);
    }

    #[test]
    fn offered_cards_come_in_screen_order() {
        let cs = ChampStats {
            augments: vec![
                st(1, 1, 0.60, 5000),
                st(2, 2, 0.58, 5000),
                st(3, 3, 0.55, 5000),
            ],
            ..Default::default()
        };
        // The scan lists the cards in any order. Left 3, middle 1, right 2.
        let r = offer_stats(&cs, &[card(2, 0.78), card(3, 0.22), card(1, 0.51)], 1, 300);
        let at: Vec<(i64, &str)> = r
            .iter()
            .map(|c| (c.id, position_label(Lang::En, c.x)))
            .collect();
        assert_eq!(at, vec![(3, "Left"), (1, "Middle"), (2, "Right")]);
        let at: Vec<(i64, &str)> = r
            .iter()
            .map(|c| (c.id, position_label(Lang::Tr, c.x)))
            .collect();
        assert_eq!(at, vec![(3, "Sol"), (1, "Orta"), (2, "Sağ")]);
    }

    #[test]
    fn offered_cards_keep_the_title_line_for_the_badges() {
        let cs = ChampStats {
            augments: vec![st(1, 1, 0.60, 5000), st(2, 2, 0.58, 5000)],
            ..Default::default()
        };
        let offer = [
            Card {
                id: 2,
                x: 0.25,
                y: 0.47,
                h: 0.031,
            },
            Card {
                id: 1,
                x: 0.75,
                y: 0.49,
                h: 0.029,
            },
        ];
        let r = offer_stats(&cs, &offer, 1, 300);
        let at: Vec<(i64, f32)> = r.iter().map(|c| (c.id, c.y)).collect();
        assert_eq!(at, vec![(2, 0.47), (1, 0.49)]);
    }

    /// The cards of a pick screen 1 (the stats of all picks), for the tests of `top_tier`.
    fn marked(augments: Vec<AugStat>, offer: [i64; 3]) -> TopTier {
        let cs = ChampStats {
            augments,
            ..Default::default()
        };
        top_tier(&cs, &cards(offer), 1, 300)
    }

    #[test]
    fn one_clear_top_card_is_marked() {
        let t = marked(
            vec![
                st(1, 3, 0.52, 5000),
                st(2, 1, 0.60, 5000),
                st(3, 2, 0.55, 5000),
            ],
            [1, 2, 3],
        );
        assert_eq!(
            t,
            TopTier {
                ids: vec![2],
                equal: false
            }
        );
    }

    #[test]
    fn two_tied_top_cards_are_both_marked() {
        let t = marked(
            vec![
                st(1, 1, 0.60, 5000),
                st(2, 3, 0.52, 5000),
                st(3, 1, 0.58, 5000),
            ],
            [1, 2, 3],
        );
        // In screen order.
        assert_eq!(
            t,
            TopTier {
                ids: vec![1, 3],
                equal: false
            }
        );
    }

    #[test]
    fn cards_with_the_same_tier_mark_nothing_and_say_so() {
        let t = marked(
            vec![
                st(1, 2, 0.55, 5000),
                st(2, 2, 0.54, 5000),
                st(3, 2, 0.53, 5000),
            ],
            [1, 2, 3],
        );
        assert_eq!(
            t,
            TopTier {
                ids: vec![],
                equal: true
            }
        );
        // Two eligible cards with the same tier, and a third one without data: equal as well.
        let t = marked(vec![st(1, 2, 0.55, 5000), st(2, 2, 0.54, 5000)], [1, 2, 9]);
        assert_eq!(
            t,
            TopTier {
                ids: vec![],
                equal: true
            }
        );
    }

    #[test]
    fn one_card_with_data_is_not_the_top() {
        let t = marked(vec![st(1, 1, 0.60, 5000)], [1, 8, 9]);
        assert_eq!(
            t,
            TopTier {
                ids: vec![],
                equal: false
            }
        );
        // Two cards with data and different tiers: the better one is marked. The card without data does not matter.
        let t = marked(vec![st(1, 1, 0.60, 5000), st(2, 5, 0.40, 5000)], [1, 2, 9]);
        assert_eq!(t.ids, vec![1]);
    }

    #[test]
    fn a_card_with_few_games_is_not_marked() {
        // Card 1 has the best tier, but only 100 games. Card 2 is the top of the others.
        let t = marked(
            vec![
                st(1, 0, 0.70, 100),
                st(2, 2, 0.55, 5000),
                st(3, 3, 0.52, 5000),
            ],
            [1, 2, 3],
        );
        assert_eq!(
            t,
            TopTier {
                ids: vec![2],
                equal: false
            }
        );
        // It does not count for "equal" either.
        let t = marked(
            vec![
                st(1, 0, 0.70, 100),
                st(2, 2, 0.55, 5000),
                st(3, 2, 0.52, 5000),
            ],
            [1, 2, 3],
        );
        assert_eq!(
            t,
            TopTier {
                ids: vec![],
                equal: true
            }
        );
        // Only low-sample cards: nothing to compare.
        let t = marked(
            vec![
                st(1, 0, 0.70, 100),
                st(2, 4, 0.55, 100),
                st(3, 3, 0.52, 100),
            ],
            [1, 2, 3],
        );
        assert_eq!(t, TopTier::default());
    }

    #[test]
    fn no_stats_at_all_mark_nothing() {
        let t = top_tier(&ChampStats::default(), &cards([7, 5, 6]), 2, 300);
        assert_eq!(t, TopTier::default());
        assert!(top_tier(&ChampStats::default(), &[], 1, 300).ids.is_empty());
    }

    #[test]
    fn the_highlight_uses_the_stats_of_the_pick_screen() {
        // All picks: card 1 is the best. On screen 2 (900 games), card 3 is the best.
        let cs = ChampStats {
            augments: vec![
                st(1, 1, 0.60, 5000),
                st(2, 2, 0.58, 5000),
                st(3, 3, 0.55, 5000),
            ],
            stages: vec![
                vec![],
                vec![st(3, 0, 0.70, 900), st(1, 2, 0.57, 900)],
                vec![],
                vec![],
            ],
            ..Default::default()
        };
        assert_eq!(top_tier(&cs, &cards([1, 2, 3]), 1, 300).ids, vec![1]);
        assert_eq!(top_tier(&cs, &cards([1, 2, 3]), 2, 300).ids, vec![3]);
        // A stage row with few games does not count: the stats of all picks stand in.
        assert_eq!(top_tier(&cs, &cards([1, 2, 3]), 2, 1000).ids, vec![1]);
    }

    #[test]
    fn your_win_rate_gets_a_color() {
        assert_eq!(rate_color(0.75, 50), GREEN);
        assert_eq!(rate_color(0.55, 50), GREEN);
        assert_eq!(rate_color(0.50, 50), LABEL);
        assert_eq!(rate_color(0.45, 50), RED);
        assert_eq!(rate_color(0.0, 50), RED);
        // The limit is 10 games: with fewer, any rate stays white (one win in one game is 100 %).
        assert_eq!(RATE_COLOR_GAMES, 10);
        assert_eq!(rate_color(0.75, 10), GREEN);
        assert_eq!(rate_color(0.30, 10), RED);
        for rate in [1.0, 0.75, 0.60, 0.50, 0.40, 0.0] {
            assert_eq!(rate_color(rate, 9), LABEL, "{rate}");
            assert_eq!(rate_color(rate, 1), LABEL, "{rate}");
        }
    }

    #[test]
    fn stage_stats_with_few_games_fall_back_to_all_picks() {
        let cs = ChampStats {
            augments: vec![st(1, 1, 0.60, 5000)],
            stages: vec![vec![st(1, 0, 0.90, 20)], vec![], vec![], vec![]],
            ..Default::default()
        };
        let r = offer_stats(&cs, &cards([1, 8, 9]), 1, 300);
        assert_eq!(r[0].stat.as_ref().map(|a| a.win_rate), Some(0.60));
    }

    #[test]
    fn unknown_augment_keeps_its_place() {
        let cs = ChampStats {
            augments: vec![st(1, 4, 0.50, 5000)],
            ..Default::default()
        };
        let r = offer_stats(&cs, &[card(99, 0.2), card(1, 0.5)], 1, 300);
        assert_eq!(ids(&r), vec![99, 1]);
        assert!(r[0].stat.is_none());
    }

    #[test]
    fn offer_without_stats_keeps_the_screen_order() {
        // No stats for the champion: the cards show left to right.
        let r = offer_stats(&ChampStats::default(), &cards([7, 5, 6]), 2, 300);
        assert_eq!(ids(&r), vec![7, 5, 6]);
        assert!(r.iter().all(|c| c.stat.is_none()));
    }

    #[test]
    fn position_label_follows_the_x_center() {
        for (lang, words) in [
            (Lang::En, ["Left", "Middle", "Right"]),
            (Lang::Tr, ["Sol", "Orta", "Sağ"]),
        ] {
            let [left, middle, right] = words;
            assert_eq!(position_label(lang, 0.0), left);
            assert_eq!(position_label(lang, 0.22), left);
            assert_eq!(position_label(lang, 0.399), left);
            assert_eq!(position_label(lang, 0.4), middle);
            assert_eq!(position_label(lang, 0.5), middle);
            assert_eq!(position_label(lang, 0.6), middle);
            assert_eq!(position_label(lang, 0.601), right);
            assert_eq!(position_label(lang, 0.78), right);
            assert_eq!(position_label(lang, 1.0), right);
        }
    }

    /// Run one frame of a bare egui context at `time` seconds.
    /// Returns the card fade for the card `key`.
    fn frame(ctx: &egui::Context, key: egui::Id, time: f64) -> f32 {
        let mut out = -1.0;
        let input = egui::RawInput {
            time: Some(time),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            out = appear(ui.ctx(), key, 0.15);
        })
        .drop_without_applying_deltas();
        out
    }

    #[test]
    fn a_new_card_fades_in_over_150_ms() {
        let ctx = egui::Context::default();
        let key = egui::Id::unique("test card A");
        // Frames at 60 Hz. The first frame is invisible, then the card grows to 1.
        let fade: Vec<f32> = (0..24).map(|i| frame(&ctx, key, i as f64 / 60.0)).collect();
        assert_eq!(fade[0], 0.0);
        assert!(fade[1] > 0.0 && fade[1] < 1.0, "{fade:?}");
        assert!(
            fade.windows(2).all(|w| w[0] <= w[1]),
            "the fade never goes back: {fade:?}"
        );
        // 150 ms are 9 frames. Allow a few more for the first frames.
        assert_eq!(fade[13], 1.0, "{fade:?}");
        // The same card later does not start again.
        assert_eq!(frame(&ctx, key, 5.0), 1.0);
        // Another card starts at 0.
        assert_eq!(frame(&ctx, egui::Id::unique("test card B"), 5.1), 0.0);
    }

    fn combo(augments: &[i64], win_rate: f32, games: u32) -> Combo {
        Combo {
            augments: augments.to_vec(),
            win_rate,
            games,
        }
    }

    #[test]
    fn a_top_pair_with_an_earlier_pick_gives_a_hint() {
        let cs = ChampStats {
            combos: vec![combo(&[1, 2], 0.60, 900), combo(&[3, 4], 0.58, 900)],
            ..Default::default()
        };
        // Offered 2, earlier card 1: the pair (1, 2) links them.
        assert_eq!(pairs_hint(&cs, 2, &[1], 300), Some(PairHint { with: 1 }));
        // The order inside the pair does not matter.
        assert_eq!(pairs_hint(&cs, 1, &[2], 300), Some(PairHint { with: 2 }));
        // No earlier card in a pair with the offered augment: no hint.
        assert_eq!(pairs_hint(&cs, 2, &[3], 300), None);
        assert_eq!(pairs_hint(&cs, 2, &[], 300), None);
        // An augment does not pair with itself.
        assert_eq!(pairs_hint(&cs, 1, &[1], 300), None);
    }

    #[test]
    fn only_top_pairs_with_enough_games_count() {
        // The ninth pair is outside the top 8.
        let mut combos: Vec<Combo> = (0..8)
            .map(|i| combo(&[100 + i, 200 + i], 0.6, 900))
            .collect();
        combos.push(combo(&[1, 2], 0.7, 900));
        let cs = ChampStats {
            combos,
            ..Default::default()
        };
        assert_eq!(pairs_hint(&cs, 2, &[1], 300), None);
        // A pair with few games does not count, and it takes no place of the top 8.
        let mut combos = vec![combo(&[1, 2], 0.7, 100)];
        combos.extend((0..8).map(|i| combo(&[100 + i, 200 + i], 0.6, 900)));
        combos.push(combo(&[5, 6], 0.65, 900));
        let cs = ChampStats {
            combos,
            ..Default::default()
        };
        assert_eq!(pairs_hint(&cs, 2, &[1], 300), None);
        assert_eq!(pairs_hint(&cs, 6, &[5], 300), None);
        // A lower limit lets the first pair in.
        assert_eq!(pairs_hint(&cs, 2, &[1], 50).map(|h| h.with), Some(1));
    }

    #[test]
    fn the_best_pair_wins_among_several_earlier_cards() {
        let cs = ChampStats {
            combos: vec![combo(&[1, 9], 0.55, 900), combo(&[2, 9], 0.65, 900)],
            ..Default::default()
        };
        assert_eq!(pairs_hint(&cs, 9, &[1, 2], 300).map(|h| h.with), Some(2));
    }

    #[test]
    fn earlier_cards_come_from_the_earlier_screens_only() {
        let offered = [
            (3, vec![30, 31, 32]),
            (1, vec![10, 11, 12]),
            (2, vec![20, 21, 22]),
        ];
        assert!(earlier_offers(&offered, 1).is_empty());
        assert_eq!(earlier_offers(&offered, 2), vec![10, 11, 12]);
        // The cards of the same screen do not count (the cards came back).
        assert_eq!(earlier_offers(&offered, 3), vec![10, 11, 12, 20, 21, 22]);
        assert_eq!(earlier_offers(&offered, 4).len(), 9);
    }

    #[test]
    fn the_hint_never_changes_the_highlight() {
        // Augment 3 pairs with the card 7 of an earlier screen, but it is the worst card. It stays unmarked.
        let cs = ChampStats {
            augments: vec![
                st(1, 1, 0.60, 5000),
                st(2, 2, 0.58, 5000),
                st(3, 5, 0.45, 5000),
            ],
            combos: vec![combo(&[3, 7], 0.7, 900)],
            ..Default::default()
        };
        let offer = cards([3, 1, 2]);
        assert_eq!(top_tier(&cs, &offer, 2, 300).ids, vec![1]);
        // Any card of an earlier screen can pair, not only the best one of that screen.
        let earlier = earlier_offers(&[(1, vec![5, 6, 7])], 2);
        assert_eq!(pairs_hint(&cs, 3, &earlier, 300).map(|h| h.with), Some(7));
        assert_eq!(pairs_hint(&cs, 1, &earlier, 300), None);
    }

    #[test]
    fn the_pairs_line_leaves_room_for_a_name_in_the_pick_card() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        // The row of a pick card is 272 points wide. The line starts 10 points from the left (22 for
        // a top card, which has a dot) and keeps 8 points at the right.
        let room = MAIN_W - 2.0 * MARGIN - 22.0 - 8.0;
        let font = FontId::proportional(11.0);
        for lang in [Lang::En, Lang::Tr] {
            // A name of 12 letters still shows whole.
            let line = pairs_line(lang.t(Key::PairsLine), "Kral Benim 12", |t| {
                text_width(&ctx, t, font.clone()) <= room
            });
            assert!(!line[1].ends_with('…'), "{lang:?}: {line:?}");
        }
    }

    #[test]
    fn the_pairs_line_keeps_the_caveat() {
        let fits = |limit: usize| move |s: &str| s.chars().count() <= limit;
        // Room enough: the whole line, with no number.
        let line = pairs_line(Lang::En.t(Key::PairsLine), "Kral Benim", fits(80));
        assert_eq!(
            line.concat(),
            "Pairs well with Kral Benim if picked earlier"
        );
        assert!(
            !line
                .concat()
                .contains(|c: char| c.is_ascii_digit() || c == '%' || c == '+')
        );
        // A narrow row: the name gets shorter, the rest stays.
        let [pre, name, post] = pairs_line(
            Lang::En.t(Key::PairsLine),
            "Kral Benim Cok Uzun Bir Ad",
            fits(40),
        );
        let all = format!("{pre}{name}{post}");
        assert!(all.chars().count() <= 40, "{all}");
        assert!(name.starts_with("Kral") && name.ends_with('…'), "{name}");
        assert_eq!(post, " if picked earlier");
        // A line that never fits still keeps one letter and the caveat.
        let [_, name, post] = pairs_line(Lang::En.t(Key::PairsLine), "Kral Benim", fits(5));
        assert_eq!((name.as_str(), post.as_str()), ("K…", " if picked earlier"));
    }

    #[test]
    fn the_pairs_line_reads_in_turkish() {
        let fits = |limit: usize| move |s: &str| s.chars().count() <= limit;
        // The words stand on both sides of the name.
        let line = pairs_line(Lang::Tr.t(Key::PairsLine), "Kral Benim", fits(80));
        assert_eq!(line.concat(), "Önceden seçtiysen Kral Benim ile iyi gider");
        assert_eq!(line[0], "Önceden seçtiysen ");
        assert_eq!(line[1], "Kral Benim");
        // A narrow row: the name gets shorter, and the caveat stays.
        let [pre, name, post] = pairs_line(
            Lang::Tr.t(Key::PairsLine),
            "Kral Benim Çok Uzun Bir Ad",
            fits(40),
        );
        let all = format!("{pre}{name}{post}");
        assert!(all.chars().count() <= 40, "{all}");
        assert!(name.starts_with("Kral") && name.ends_with('…'), "{name}");
        assert_eq!(pre, "Önceden seçtiysen ");
        assert_eq!(post, " ile iyi gider");
        // Without a partner name, the line of its own words comes back whole.
        let line = pairs_line(Lang::Tr.t(Key::PairsLineEarlier), "", fits(80));
        assert_eq!(line.concat(), "Önceki seçiminle iyi gider");
        assert_eq!(line[0], "Önceki seçiminle iyi gider");
    }

    #[test]
    fn the_pairs_template_has_one_place_for_the_name_and_none_for_a_number() {
        // `pairs_line` splits the template at {0}. A second place would be a number.
        for lang in [Lang::En, Lang::Tr] {
            let t = lang.t(Key::PairsLine);
            assert_eq!(t.matches('{').count(), 1, "{lang:?}: {t}");
            assert!(t.contains("{0}"), "{lang:?}: {t}");
        }
    }

    #[test]
    fn overlay_sizes_map_to_zoom_factors() {
        assert_eq!(overlay_zoom(0), 0.9);
        assert_eq!(overlay_zoom(1), 1.0);
        assert_eq!(overlay_zoom(2), 1.2);
        // A bad value from a hand-edited file gives the large size, not a panic.
        assert_eq!(overlay_zoom(200), 1.2);
    }

    #[test]
    fn the_overlay_stays_on_the_same_spot_when_the_zoom_changes() {
        // A command moves the window to points * zoom * monitor scale pixels.
        let monitor = 1.25;
        let saved = [100.0, 200.0];
        for zoom in OVERLAY_ZOOMS {
            let p = place_from_saved(saved, zoom);
            let (x, y) = (p.x * zoom * monitor, p.y * zoom * monitor);
            assert!(
                (x - 125.0).abs() < 0.01 && (y - 250.0).abs() < 0.01,
                "zoom {zoom}: {x} {y}"
            );
            // The window reports the same place back in the points of this zoom.
            let back = place_to_saved(p, zoom);
            assert!(
                (back[0] - 100.0).abs() < 0.01 && (back[1] - 200.0).abs() < 0.01,
                "zoom {zoom}: {back:?}"
            );
        }
    }

    #[test]
    fn a_hotkey_cannot_take_the_key_of_the_other_one() {
        let now = Keys {
            overlay: 0x77,
            scan: 0x78,
        };
        // A free key changes only its own hotkey.
        assert_eq!(
            rebound(now, Hotkey::Overlay, 0x74),
            Some(Keys {
                overlay: 0x74,
                scan: 0x78
            })
        );
        assert_eq!(
            rebound(now, Hotkey::Scan, 0x41),
            Some(Keys {
                overlay: 0x77,
                scan: 0x41
            })
        );
        // The same key again is fine (nothing changes).
        assert_eq!(rebound(now, Hotkey::Overlay, 0x77), Some(now));
        // The key of the other hotkey is refused.
        assert_eq!(rebound(now, Hotkey::Overlay, 0x78), None);
        assert_eq!(rebound(now, Hotkey::Scan, 0x77), None);
    }

    fn key_event(key: egui::Key, pressed: bool, repeat: bool) -> egui::Event {
        egui::Event::Key {
            key,
            physical_key: None,
            pressed,
            repeat,
            modifiers: egui::Modifiers::NONE,
        }
    }

    #[test]
    fn the_next_key_press_becomes_the_hotkey() {
        use egui::Key;
        assert_eq!(rebind_from(&[]), Rebind::Wait);
        assert_eq!(
            rebind_from(&[key_event(Key::F5, true, false)]),
            Rebind::Key(0x74)
        );
        assert_eq!(
            rebind_from(&[key_event(Key::Q, true, false)]),
            Rebind::Key(0x51)
        );
        assert_eq!(
            rebind_from(&[key_event(Key::Num7, true, false)]),
            Rebind::Key(0x37)
        );
        // Esc cancels.
        assert_eq!(
            rebind_from(&[key_event(Key::Escape, true, false)]),
            Rebind::Cancel
        );
        // A release, a repeat, and a key that cannot be a hotkey do not count.
        assert_eq!(
            rebind_from(&[key_event(Key::F5, false, false)]),
            Rebind::Wait
        );
        assert_eq!(rebind_from(&[key_event(Key::F5, true, true)]), Rebind::Wait);
        assert_eq!(
            rebind_from(&[
                key_event(Key::Space, true, false),
                key_event(Key::Tab, true, false)
            ]),
            Rebind::Wait
        );
        // The first press that counts decides.
        let events = [
            key_event(Key::Space, true, false),
            key_event(Key::F2, true, false),
            key_event(Key::F3, true, false),
        ];
        assert_eq!(rebind_from(&events), Rebind::Key(0x71));
    }

    fn row(rows: &[(&'static str, String, bool)], label: &str) -> (String, bool) {
        rows.iter()
            .find(|r| r.0 == label)
            .map(|r| (r.1.clone(), r.2))
            .unwrap()
    }

    #[test]
    fn the_card_reading_rows_say_what_the_reader_did() {
        // Outside a game, before a scan: nothing started.
        let rows = read_rows(&ReadView::default(), Lang::En);
        assert_eq!(
            row(&rows, "Reading language"),
            ("Shown in a game".to_string(), false)
        );
        assert_eq!(
            row(&rows, "Game window"),
            ("Not in a game".to_string(), false)
        );
        assert_eq!(
            row(&rows, "League window mode"),
            ("Unknown".to_string(), false)
        );
        assert_eq!(
            row(&rows, "Champion source"),
            ("Not available yet".to_string(), false)
        );
        assert_eq!(row(&rows, "Last scan"), ("No scan yet".to_string(), false));
        assert_eq!(row(&rows, "Lines read"), ("No scan yet".to_string(), false));
        assert_eq!(
            row(&rows, "Pick screens"),
            ("Not in a game".to_string(), false)
        );
        // In a game, after a scan.
        let r = ReadView {
            in_game: true,
            ocr_language: Some("en-US".into()),
            game_window: Some(true),
            window_mode: Some(2),
            champ_source: Some(ChampSource::LiveClient),
            scan_age: Some(12),
            lines: 34,
            dropped: 3,
            matched: 3,
            due: 2,
            done: 1,
            ..Default::default()
        };
        let rows = read_rows(&r, Lang::En);
        assert_eq!(row(&rows, "Reading language").0, "English (United States)");
        assert_eq!(row(&rows, "Game window"), ("Found".to_string(), false));
        assert_eq!(row(&rows, "League window mode").0, "Borderless");
        assert_eq!(row(&rows, "Champion source").0, "In game");
        assert_eq!(row(&rows, "Last scan").0, "12 s ago");
        assert_eq!(row(&rows, "Lines read").0, "37 (3 under the overlay)");
        assert_eq!(row(&rows, "Names matched").0, "3");
        assert_eq!(row(&rows, "Pick screens").0, "Expected: 2 · completed: 1");
        // Warnings: no game window in a game, Fullscreen, and an engine error.
        let bad = ReadView {
            in_game: true,
            game_window: Some(false),
            window_mode: Some(0),
            scan_age: Some(5),
            engine_failed: true,
            ..Default::default()
        };
        let rows = read_rows(&bad, Lang::En);
        assert_eq!(row(&rows, "Game window"), ("Not found".to_string(), true));
        assert_eq!(
            row(&rows, "League window mode"),
            ("Fullscreen".to_string(), true)
        );
        assert_eq!(
            row(&rows, "Lines read"),
            ("Reading failed".to_string(), true)
        );
    }

    #[test]
    fn the_card_reading_rows_read_in_turkish() {
        let rows = read_rows(&ReadView::default(), Lang::Tr);
        assert_eq!(
            row(&rows, "Okuma dili"),
            ("Oyunda belli olur".to_string(), false)
        );
        assert_eq!(
            row(&rows, "Oyun penceresi"),
            ("Oyunda değil".to_string(), false)
        );
        assert_eq!(
            row(&rows, "League’in pencere modu"),
            ("Bilinmiyor".to_string(), false)
        );
        assert_eq!(
            row(&rows, "Şampiyon kaynağı"),
            ("Henüz bilinmiyor".to_string(), false)
        );
        assert_eq!(
            row(&rows, "Son tarama"),
            ("Henüz tarama yok".to_string(), false)
        );
        let r = ReadView {
            in_game: true,
            ocr_language: Some("tr-TR".into()),
            game_window: Some(true),
            window_mode: Some(2),
            champ_source: Some(ChampSource::ChampSelect),
            scan_age: Some(75),
            lines: 34,
            dropped: 3,
            matched: 3,
            due: 2,
            done: 1,
            ..Default::default()
        };
        let rows = read_rows(&r, Lang::Tr);
        assert_eq!(row(&rows, "Okuma dili").0, "Türkçe");
        assert_eq!(row(&rows, "Oyun penceresi").0, "Bulundu");
        assert_eq!(row(&rows, "League’in pencere modu").0, "Çerçevesiz");
        assert_eq!(row(&rows, "Şampiyon kaynağı").0, "Şampiyon seçimi");
        assert_eq!(row(&rows, "Son tarama").0, "1 dk önce");
        assert_eq!(
            row(&rows, "Okunan satırlar").0,
            "37 (3 satır panelin altında)"
        );
        assert_eq!(
            row(&rows, "Seçim ekranları").0,
            "Beklenen: 2 · tamamlanan: 1"
        );
        let bad = ReadView {
            in_game: true,
            game_window: Some(false),
            window_mode: Some(0),
            scan_age: Some(5),
            engine_failed: true,
            ..Default::default()
        };
        let rows = read_rows(&bad, Lang::Tr);
        assert_eq!(
            row(&rows, "Oyun penceresi"),
            ("Bulunamadı".to_string(), true)
        );
        assert_eq!(
            row(&rows, "League’in pencere modu"),
            ("Tam ekran".to_string(), true)
        );
        assert_eq!(
            row(&rows, "Okunan satırlar"),
            ("Okunamadı".to_string(), true)
        );
    }

    #[test]
    fn ages_read_in_short_words() {
        assert_eq!(age_text(Lang::En, 0), "0 s ago");
        assert_eq!(age_text(Lang::En, 59), "59 s ago");
        assert_eq!(age_text(Lang::En, 60), "1 min ago");
        assert_eq!(age_text(Lang::En, 3599), "59 min ago");
        assert_eq!(age_text(Lang::En, 3600), "1 h ago");
        assert_eq!(age_text(Lang::Tr, 12), "12 sn önce");
        assert_eq!(age_text(Lang::Tr, 180), "3 dk önce");
        assert_eq!(age_text(Lang::Tr, 7200), "2 sa önce");
    }

    #[test]
    fn a_key_code_without_a_name_shows_in_the_language() {
        assert_eq!(key_text(Lang::En, 0x77), "F8");
        assert_eq!(key_text(Lang::Tr, 0x77), "F8");
        assert_eq!(key_text(Lang::En, 0x41), "A");
        assert_eq!(key_text(Lang::En, 0x2D), "Key 45");
        assert_eq!(key_text(Lang::Tr, 0x2D), "Tuş 45");
    }

    #[test]
    fn the_language_setting_defaults_to_auto_and_survives_a_bad_value() {
        // No key in the file (an old settings.json): Auto.
        let s: Settings = serde_json::from_str(r#"{"min_games": 600}"#).unwrap();
        assert_eq!(s.language, LangPref::Auto);
        assert_eq!(Settings::default().language, LangPref::Auto);
        // A saved choice comes back.
        let s: Settings =
            serde_json::from_str(r#"{"language": "tr", "last_locale": "en_US"}"#).unwrap();
        assert_eq!(s.language, LangPref::Turkish);
        assert_eq!(s.language.resolve(&s.last_locale), Lang::Tr);
        let saved = serde_json::to_string(&Settings {
            language: LangPref::English,
            ..Default::default()
        })
        .unwrap();
        assert!(saved.contains(r#""language":"en""#), "{saved}");
        // A word that Hexsight does not know must not reset the other settings.
        let s: Settings =
            serde_json::from_str(r#"{"language": "klingon", "min_games": 600}"#).unwrap();
        assert_eq!((s.language, s.min_games), (LangPref::Auto, 600));
    }

    #[test]
    fn auto_uses_the_saved_locale_until_the_client_answers() {
        // The client has not answered: the settings hold the locale of the last run.
        let s = Settings {
            last_locale: "tr_TR".into(),
            ..Default::default()
        };
        assert_eq!(s.language.resolve(&s.last_locale), Lang::Tr);
        let s = Settings {
            last_locale: "en_US".into(),
            ..Default::default()
        };
        assert_eq!(s.language.resolve(&s.last_locale), Lang::En);
        // First start, no client yet: English.
        assert_eq!(
            Settings::default()
                .language
                .resolve(&Settings::default().last_locale),
            Lang::En
        );
    }

    /// Width of a text in points, in the font and the size that the app draws it with.
    fn text_width(ctx: &egui::Context, text: &str, font: FontId) -> f32 {
        let mut width = 0.0;
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            width = ui
                .painter()
                .layout_no_wrap(text.to_string(), font.clone(), LABEL)
                .size()
                .x
        })
        .drop_without_applying_deltas();
        width
    }

    #[test]
    fn the_trade_hook_reads_the_call_the_id_and_the_time() {
        assert_eq!(
            parse_debug_trade("accept:101:8"),
            Some((TradeCall::Accept, 101, 8.0))
        );
        assert_eq!(
            parse_debug_trade("request:100:2.5"),
            Some((TradeCall::Request, 100, 2.5))
        );
        // No time: 5 seconds.
        assert_eq!(
            parse_debug_trade("decline:7"),
            Some((TradeCall::Decline, 7, 5.0))
        );
        assert_eq!(
            parse_debug_trade("cancel: 104 : 3"),
            Some((TradeCall::Cancel, 104, 3.0))
        );
        assert_eq!(parse_debug_trade("swap:1:1"), None);
        assert_eq!(parse_debug_trade("accept"), None);
        assert_eq!(parse_debug_trade("accept:x"), None);
        assert_eq!(parse_debug_trade(""), None);
    }

    // ---- The overlay: a pill in a game, a card at a pick screen, clicks outside a game.

    #[test]
    fn the_mouse_passes_only_in_a_game_and_only_while_locked() {
        for phase in [
            Phase::Offline,
            Phase::Idle,
            Phase::Lobby,
            Phase::ChampSelect,
            Phase::PostGame,
        ] {
            for unlocked in [false, true] {
                assert!(
                    !passes_mouse(true, phase, unlocked),
                    "{phase:?} unlocked {unlocked}"
                );
            }
        }
        assert!(passes_mouse(true, Phase::InGame, false));
        // Unlocked to move it: the overlay takes the mouse, even in a game.
        assert!(!passes_mouse(true, Phase::InGame, true));
        // No overlay, no state to send.
        assert!(!passes_mouse(false, Phase::InGame, false));
    }

    fn offer_of(ids: [i64; 3]) -> Offer {
        Offer {
            cards: cards(ids),
            screen: 1,
            geometry: None,
        }
    }

    #[test]
    fn the_card_stays_one_and_a_half_seconds_after_the_cards_go_away() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let ids = |o: Option<Offer>| o.map(|o| o.augment_ids());
        let mut held = None;
        // The cards show: the card shows them, and there is no hold.
        let o = offer_of([1, 2, 3]);
        assert_eq!(held_card(&mut held, Some(&o), at(0)).1, None);
        // The cards are gone at 100 ms: the last offer stays for 1.5 s.
        let (shown, left) = held_card(&mut held, None, at(100));
        assert_eq!(
            (ids(shown), left),
            (Some(vec![1, 2, 3]), Some(Duration::from_millis(1500)))
        );
        let (shown, left) = held_card(&mut held, None, at(1100));
        assert_eq!(
            (ids(shown), left),
            (Some(vec![1, 2, 3]), Some(Duration::from_millis(500)))
        );
        // At 1.5 s after they went away, the card closes. It stays closed.
        assert_eq!(held_card(&mut held, None, at(1600)), (None, None));
        assert!(held.is_none());
        assert_eq!(held_card(&mut held, None, at(1700)), (None, None));
    }

    #[test]
    fn cards_that_come_back_start_the_hold_again() {
        use std::time::{Duration, Instant};
        let t0 = Instant::now();
        let at = |ms: u64| t0 + Duration::from_millis(ms);
        let mut held = None;
        held_card(&mut held, Some(&offer_of([1, 2, 3])), at(0));
        // A missed scan at 100 ms, and the cards are back at 800 ms with another set.
        assert!(held_card(&mut held, None, at(100)).0.is_some());
        let (shown, left) = held_card(&mut held, Some(&offer_of([4, 5, 6])), at(800));
        assert_eq!(
            (shown.map(|o| o.augment_ids()), left),
            (Some(vec![4, 5, 6]), None)
        );
        // The hold counts from the new moment: at 2000 ms, 100 ms after they went again.
        assert!(held_card(&mut held, None, at(1900)).1 == Some(Duration::from_millis(1500)));
        assert!(held_card(&mut held, None, at(3300)).0.is_some());
        assert!(held_card(&mut held, None, at(3500)).0.is_none());
    }

    #[test]
    fn nothing_is_held_without_an_offer() {
        let mut held = None;
        assert_eq!(
            held_card(&mut held, None, std::time::Instant::now()),
            (None, None)
        );
        assert!(held.is_none());
    }

    fn facts() -> FirstRun {
        FirstRun {
            league_found: true,
            window_mode: Some(2),
            ocr_probe: Some(true),
            data_loaded: true,
            data_failed: false,
            hotkeys: [(0x77, true), (0x78, true)],
        }
    }

    fn states(rows: &[(Check, String)]) -> Vec<Check> {
        rows.iter().map(|r| r.0).collect()
    }

    #[test]
    fn the_checklist_has_five_live_checks() {
        let rows = first_run_checks(Lang::En, &facts());
        assert_eq!(states(&rows), vec![Check::Ok; 5]);
        let text: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
        assert_eq!(
            text,
            vec![
                "League detected",
                "League is in Borderless mode",
                "Card reading is ready",
                "Data loaded",
                "Hotkeys F8 and F9 are available"
            ]
        );
        let rows = first_run_checks(Lang::Tr, &facts());
        let text: Vec<&str> = rows.iter().map(|r| r.1.as_str()).collect();
        assert_eq!(
            text,
            vec![
                "League bulundu",
                "League çerçevesiz modda",
                "Kart okuma hazır",
                "Veriler yüklendi",
                "F8 ve F9 tuşları kullanılabilir"
            ]
        );
    }

    #[test]
    fn an_unknown_window_mode_shows_no_row_and_fullscreen_tells_the_way() {
        // No game.cfg yet (League never started on this PC): no row, so the card can say that all is well.
        let unknown = FirstRun {
            window_mode: None,
            ..facts()
        };
        assert_eq!(first_run_checks(Lang::Tr, &unknown).len(), 4);
        let rows = checks_to_show(
            first_run_checks(Lang::Tr, &unknown),
            Lang::Tr.t(Key::CheckAllOk),
        );
        assert_eq!(rows, vec![(Check::Ok, "Her şey yolunda".to_string())]);
        // Fullscreen: the row says where the setting is, in the words of the game menu. No file name.
        let full = FirstRun {
            window_mode: Some(0),
            ..facts()
        };
        let rows = checks_to_show(
            first_run_checks(Lang::Tr, &full),
            Lang::Tr.t(Key::CheckAllOk),
        );
        assert_eq!(
            rows,
            vec![(
                Check::Warn,
                "League’i çerçevesiz moda al: Ayarlar > Video > Pencere Modu".to_string()
            )]
        );
        for lang in [Lang::En, Lang::Tr] {
            for mode in [0, 1, 2] {
                for (_, text) in first_run_checks(
                    lang,
                    &FirstRun {
                        window_mode: Some(mode),
                        ..facts()
                    },
                ) {
                    assert!(!text.contains("game.cfg"), "{lang:?} {mode}: {text}");
                }
            }
        }
    }

    #[test]
    fn the_card_shows_one_line_when_all_is_well_and_only_the_problems_else() {
        // Every check passes: one green line.
        let rows = checks_to_show(
            first_run_checks(Lang::Tr, &facts()),
            Lang::Tr.t(Key::CheckAllOk),
        );
        assert_eq!(rows, vec![(Check::Ok, "Her şey yolunda".to_string())]);
        // A check that waits and one that needs the player: only those two, in order.
        let some = FirstRun {
            ocr_probe: None,
            hotkeys: [(0x77, false), (0x78, true)],
            ..facts()
        };
        let rows = checks_to_show(
            first_run_checks(Lang::En, &some),
            Lang::En.t(Key::CheckAllOk),
        );
        assert_eq!(states(&rows), vec![Check::Wait, Check::Warn]);
        assert_eq!(
            rows[1].1,
            "F8 is taken by another app. Change it in Settings."
        );
    }

    #[test]
    fn a_check_that_has_no_answer_waits_and_a_problem_warns() {
        // The first seconds: no League, no game.cfg, no OCR answer, no data, and F9 taken by another app.
        let start = FirstRun {
            league_found: false,
            window_mode: None,
            ocr_probe: None,
            data_loaded: false,
            data_failed: false,
            hotkeys: [(0x77, true), (0x78, false)],
        };
        let rows = first_run_checks(Lang::En, &start);
        // The unknown window mode has no row.
        assert_eq!(
            states(&rows),
            vec![Check::Warn, Check::Wait, Check::Wait, Check::Warn]
        );
        assert_eq!(rows[0].1, "Open League so Hexsight can find it");
        assert_eq!(
            rows[3].1,
            "F9 is taken by another app. Change it in Settings."
        );
        // Fullscreen needs the player. Windowed works. The OCR engine can be missing. The data can fail.
        let full = FirstRun {
            window_mode: Some(0),
            ocr_probe: Some(false),
            data_failed: true,
            ..start
        };
        let rows = first_run_checks(Lang::En, &full);
        assert_eq!(states(&rows)[1..4], [Check::Warn, Check::Warn, Check::Warn]);
        assert_eq!(
            rows[1].1,
            "Set League to Borderless: Settings > Video > Window Mode"
        );
        let windowed = FirstRun {
            window_mode: Some(1),
            ..facts()
        };
        assert_eq!(states(&first_run_checks(Lang::En, &windowed))[1], Check::Ok);
        // Loaded data beats an old error.
        let late = FirstRun {
            data_loaded: true,
            data_failed: true,
            ..facts()
        };
        assert_eq!(states(&first_run_checks(Lang::En, &late))[3], Check::Ok);
        // The first taken key is the one that the text names.
        let both = FirstRun {
            hotkeys: [(0x77, false), (0x78, false)],
            ..facts()
        };
        assert_eq!(
            first_run_checks(Lang::Tr, &both)[4].1,
            "F8 başka bir uygulamada kullanılıyor, Ayarlar’dan değiştir"
        );
    }

    // ---- The capture: a black or frozen image shows in Settings and in the overlay.

    #[test]
    fn a_bad_capture_shows_in_settings_with_a_warning_and_the_fix_in_a_hint() {
        let rows = read_rows(
            &ReadView {
                capture: CaptureHealth::Black,
                ..Default::default()
            },
            Lang::En,
        );
        assert_eq!(row(&rows, "Screen capture"), ("Black".to_string(), true));
        let rows = read_rows(
            &ReadView {
                capture: CaptureHealth::Frozen,
                ..Default::default()
            },
            Lang::Tr,
        );
        assert_eq!(row(&rows, "Ekran görüntüsü"), ("Dondu".to_string(), true));
        // A fine capture needs a scan to say so, and it is no warning.
        assert_eq!(
            row(&read_rows(&ReadView::default(), Lang::En), "Screen capture"),
            ("No scan yet".to_string(), false)
        );
        assert_eq!(
            row(
                &read_rows(
                    &ReadView {
                        scan_age: Some(3),
                        ..Default::default()
                    },
                    Lang::En
                ),
                "Screen capture"
            ),
            ("OK".to_string(), false)
        );
        // The hint is one line with the fix, in both languages.
        assert_eq!(
            capture_hint(Lang::En, CaptureHealth::Black),
            Some("The game image is black. Switch League to Borderless.")
        );
        assert_eq!(
            capture_hint(Lang::Tr, CaptureHealth::Black),
            Some("Oyun görüntüsü siyah geliyor. League’i çerçevesiz moda al.")
        );
        assert!(
            capture_hint(Lang::En, CaptureHealth::Frozen).is_some_and(|t| t.contains("Borderless"))
        );
        assert_eq!(capture_hint(Lang::En, CaptureHealth::Fine), None);
    }

    #[test]
    fn a_locked_screen_and_a_remote_desktop_have_their_own_state_and_hint() {
        let rows = read_rows(
            &ReadView {
                capture: CaptureHealth::Locked,
                ..Default::default()
            },
            Lang::En,
        );
        assert_eq!(row(&rows, "Screen capture"), ("Locked".to_string(), true));
        let rows = read_rows(
            &ReadView {
                capture: CaptureHealth::Remote,
                ..Default::default()
            },
            Lang::Tr,
        );
        assert_eq!(
            row(&rows, "Ekran görüntüsü"),
            ("Uzak masaüstü".to_string(), true)
        );
        // The locked hint does not send the player to Borderless: that would not help.
        for lang in [Lang::En, Lang::Tr] {
            for health in [CaptureHealth::Locked, CaptureHealth::Remote] {
                let hint = capture_hint(lang, health).unwrap();
                assert!(
                    !hint.contains("Borderless") && !hint.contains("çerçevesiz"),
                    "{hint}"
                );
            }
        }
        assert!(
            capture_hint(Lang::En, CaptureHealth::Locked).is_some_and(|t| t.contains("locked"))
        );
        assert!(
            capture_hint(Lang::Tr, CaptureHealth::Locked).is_some_and(|t| t.contains("kilitli"))
        );
    }

    #[test]
    fn the_capture_hint_fits_the_overlay_in_two_lines() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        // The overlay hint box is `OVERLAY_HINT_W - 8` points wide with 10 points of room on each side.
        // The hint is 11.5 points bold (before 10.5), and the strip and the advice card use the same font.
        let room = OVERLAY_HINT_W - 8.0 - 20.0;
        assert_eq!(hint_font(), FontId::new(11.5, bold()));
        let mut rows = Vec::new();
        ctx.run_ui(egui::RawInput::default(), |ui| {
            for lang in [Lang::En, Lang::Tr] {
                for health in [
                    CaptureHealth::Black,
                    CaptureHealth::Frozen,
                    CaptureHealth::Locked,
                    CaptureHealth::Remote,
                ] {
                    let text = capture_hint(lang, health).unwrap();
                    rows.push((
                        lang,
                        text,
                        ui.painter()
                            .layout(text.to_string(), hint_font(), ORANGE, room)
                            .rows
                            .len(),
                    ));
                }
            }
        })
        .drop_without_applying_deltas();
        assert_eq!(rows.len(), 8);
        for (lang, text, n) in rows {
            assert!(n <= 2, "{lang:?} {text:?} needs {n} lines in {room} points");
        }
    }

    // ---- Confidence: three bars from the games behind a stat.

    #[test]
    fn confidence_follows_the_game_count() {
        // The limits are placeholders. The tests name them, so a change is a decision.
        assert_eq!((CONF_HIGH, CONF_MID), (8000, 3000));
        assert_eq!(Confidence::of(0), Confidence::Low);
        assert_eq!(Confidence::of(CONF_MID - 1), Confidence::Low);
        assert_eq!(Confidence::of(CONF_MID), Confidence::Mid);
        assert_eq!(Confidence::of(CONF_HIGH - 1), Confidence::Mid);
        assert_eq!(Confidence::of(CONF_HIGH), Confidence::High);
        assert_eq!(Confidence::of(u32::MAX), Confidence::High);
    }

    #[test]
    fn confidence_lights_one_to_three_bars_and_has_a_word_in_both_languages() {
        assert_eq!(
            [Confidence::Low, Confidence::Mid, Confidence::High].map(Confidence::bars),
            [1, 2, 3]
        );
        // "low" read as "this card is weak", so the word for a small sample says what is missing: "limited"
        // (it was "few games" until the strip cell got a tier badge; that did not fit beside the bars).
        for (conf, en, tr) in [
            (Confidence::High, "high", "yüksek"),
            (Confidence::Mid, "medium", "orta"),
            (Confidence::Low, "limited", "az veri"),
        ] {
            assert_eq!(Lang::En.t(conf.key()), en);
            assert_eq!(Lang::Tr.t(conf.key()), tr);
        }
    }

    // ---- The pick cells and the header line.

    fn offer_on(screen: usize, ids: [i64; 3]) -> Offer {
        Offer {
            cards: cards(ids),
            screen,
            geometry: None,
        }
    }

    /// Both marks on, as a player who turned both settings on sees the views.
    const BOTH: Marks = Marks {
        top: true,
        pairs: true,
        fit: false,
    };

    #[test]
    fn pick_cells_keep_the_screen_order_and_mark_every_top_card() {
        let cs = ChampStats {
            augments: vec![
                st(1, 3, 0.52, 2_000),
                st(2, 1, 0.60, 12_000),
                st(3, 1, 0.58, 9_800),
            ],
            ..Default::default()
        };
        // The scan lists the cards in any order: left 3, middle 1, right 2.
        let offer = Offer {
            cards: vec![card(2, 0.78), card(3, 0.22), card(1, 0.51)],
            screen: 1,
            geometry: None,
        };
        let (cells, line) = pick_cells(Some(&cs), &offer, &[], 300, BOTH);
        assert_eq!(
            cells.iter().map(|c| c.id).collect::<Vec<_>>(),
            vec![3, 1, 2]
        );
        assert_eq!(
            cells.iter().map(|c| c.top).collect::<Vec<_>>(),
            vec![true, false, true]
        );
        assert_eq!(
            cells.iter().map(|c| c.games).collect::<Vec<_>>(),
            vec![Some(9_800), Some(2_000), Some(12_000)]
        );
        assert_eq!(
            cells.iter().map(PickCell::confidence).collect::<Vec<_>>(),
            vec![
                Some(Confidence::High),
                Some(Confidence::Low),
                Some(Confidence::High)
            ]
        );
        assert_eq!(line, Some(TopLine::Count(2)));
    }

    #[test]
    fn the_confidence_follows_the_games_that_the_cell_shows() {
        // Screen 2 has its own rows: fewer games than all picks, so fewer bars.
        let cs = ChampStats {
            augments: vec![
                st(1, 1, 0.60, 12_000),
                st(2, 1, 0.58, 12_000),
                st(3, 2, 0.55, 5_000),
            ],
            stages: vec![vec![], vec![st(1, 1, 0.61, 900), st(2, 1, 0.57, 3_200)]],
            ..Default::default()
        };
        let bars = |screen| {
            let (cells, _) = pick_cells(
                Some(&cs),
                &offer_on(screen, [1, 2, 3]),
                &[],
                300,
                Marks::default(),
            );
            cells
                .iter()
                .map(|c| c.confidence().map(Confidence::bars))
                .collect::<Vec<_>>()
        };
        // Screen 1 has no stage rows: all picks decide (high, high, medium).
        assert_eq!(bars(1), vec![Some(3), Some(3), Some(2)]);
        // Screen 2: low, medium, and the card without a stage row falls back to all picks (medium).
        assert_eq!(bars(2), vec![Some(1), Some(2), Some(2)]);
    }

    #[test]
    fn pick_cells_without_stats_are_empty() {
        let (cells, line) = pick_cells(None, &offer_on(2, [7, 5, 6]), &[], 300, BOTH);
        assert_eq!(cells.len(), 3);
        assert!(cells.iter().all(|c| c.tier.is_none()
            && c.games.is_none()
            && !c.top
            && c.pair.is_none()
            && c.confidence().is_none()));
        assert_eq!(line, Some(TopLine::NoData));
    }

    #[test]
    fn a_pair_marks_its_cell_and_never_the_top_tier() {
        let cs = ChampStats {
            augments: vec![
                st(1, 1, 0.60, 5_000),
                st(2, 2, 0.58, 5_000),
                st(3, 5, 0.45, 5_000),
            ],
            combos: vec![combo(&[3, 7], 0.7, 900)],
            ..Default::default()
        };
        let (cells, _) = pick_cells(Some(&cs), &offer_on(2, [3, 1, 2]), &[5, 6, 7], 300, BOTH);
        assert_eq!(
            cells.iter().map(|c| c.pair.is_some()).collect::<Vec<_>>(),
            vec![true, false, false]
        );
        assert_eq!(cells[0].pair.as_ref().map(|h| h.with), Some(7));
        // The worst card pairs well, and it stays unmarked: only card 1 is on top.
        assert_eq!(
            cells.iter().map(|c| c.top).collect::<Vec<_>>(),
            vec![false, true, false]
        );
        // No earlier card, no pair.
        let (cells, _) = pick_cells(Some(&cs), &offer_on(1, [3, 1, 2]), &[], 300, BOTH);
        assert!(cells.iter().all(|c| c.pair.is_none()));
    }

    #[test]
    fn both_marks_are_off_by_default() {
        let s = Settings::default();
        assert!(!s.top_highlight && !s.pair_hints);
        assert_eq!(
            s.marks(),
            Marks {
                top: false,
                pairs: false,
                fit: false
            }
        );
        assert_eq!(Marks::default(), s.marks());
        // An old settings.json has neither key: both stay off.
        let s: Settings =
            serde_json::from_str(r#"{"tiers_only": true, "card_badges": true}"#).unwrap();
        assert_eq!(s.marks(), Marks::default());
        // Each setting turns on its own mark and nothing else.
        let s: Settings = serde_json::from_str(r#"{"top_highlight": true}"#).unwrap();
        assert_eq!(
            s.marks(),
            Marks {
                top: true,
                pairs: false,
                fit: false
            }
        );
        let s: Settings = serde_json::from_str(r#"{"pair_hints": true}"#).unwrap();
        assert_eq!(
            s.marks(),
            Marks {
                top: false,
                pairs: true,
                fit: false
            }
        );
    }

    #[test]
    fn with_the_marks_off_no_card_is_on_top_and_no_card_pairs() {
        // The same offer as the tests above, with a clear top card and a pair.
        let cs = ChampStats {
            augments: vec![
                st(1, 1, 0.60, 5_000),
                st(2, 2, 0.58, 5_000),
                st(3, 5, 0.45, 5_000),
            ],
            combos: vec![combo(&[3, 7], 0.7, 900)],
            ..Default::default()
        };
        let (cells, line) = pick_cells(
            Some(&cs),
            &offer_on(2, [3, 1, 2]),
            &[5, 6, 7],
            300,
            Marks::default(),
        );
        assert!(cells.iter().all(|c| !c.top && c.pair.is_none()));
        // The tiers, the games and the confidence stay.
        assert_eq!(
            cells.iter().map(|c| c.tier).collect::<Vec<_>>(),
            vec![Some(5), Some(1), Some(2)]
        );
        assert!(
            cells
                .iter()
                .all(|c| c.games == Some(5_000) && c.confidence().is_some())
        );
        // No header line about the top tier: the header shows the level only.
        assert_eq!(line, None);
        // Each mark works alone.
        let (cells, line) = pick_cells(
            Some(&cs),
            &offer_on(2, [3, 1, 2]),
            &[5, 6, 7],
            300,
            Marks {
                top: true,
                pairs: false,
                fit: false,
            },
        );
        assert_eq!(
            cells.iter().map(|c| c.top).collect::<Vec<_>>(),
            vec![false, true, false]
        );
        assert!(cells.iter().all(|c| c.pair.is_none()));
        assert_eq!(line, Some(TopLine::Count(1)));
        let (cells, line) = pick_cells(
            Some(&cs),
            &offer_on(2, [3, 1, 2]),
            &[5, 6, 7],
            300,
            Marks {
                top: false,
                pairs: true,
                fit: false,
            },
        );
        assert!(cells.iter().all(|c| !c.top));
        assert_eq!(
            cells.iter().map(|c| c.pair.is_some()).collect::<Vec<_>>(),
            vec![true, false, false]
        );
        assert_eq!(line, None);
        // Equal tiers and missing stats say nothing in the header either, with the highlight off.
        let (_, line) = pick_cells(None, &offer_on(2, [7, 5, 6]), &[], 300, Marks::default());
        assert_eq!(line, None);
    }

    #[test]
    fn the_top_line_says_how_many_or_that_the_tiers_are_equal() {
        assert_eq!(
            top_line(&TopTier {
                ids: vec![4],
                equal: false
            }),
            TopLine::Count(1)
        );
        assert_eq!(
            top_line(&TopTier {
                ids: vec![4, 5],
                equal: false
            }),
            TopLine::Count(2)
        );
        assert_eq!(
            top_line(&TopTier {
                ids: vec![],
                equal: true
            }),
            TopLine::Equal
        );
        assert_eq!(top_line(&TopTier::default()), TopLine::NoData);
        assert_eq!(
            top_line_text(Lang::Tr, TopLine::Count(2)),
            ("2 kart en üst kademede".to_string(), GREEN)
        );
        assert_eq!(
            top_line_text(Lang::Tr, TopLine::Count(1)).0,
            "1 kart en üst kademede"
        );
        assert_eq!(
            top_line_text(Lang::En, TopLine::Count(1)).0,
            "1 card in the top tier"
        );
        assert_eq!(
            top_line_text(Lang::En, TopLine::Count(3)).0,
            "3 cards in the top tier"
        );
        // Equal tiers have no green: nothing is on top.
        assert_eq!(
            top_line_text(Lang::Tr, TopLine::Equal),
            ("Kademeler eşit".to_string(), LABEL_3)
        );
    }

    #[test]
    fn the_pick_views_never_tell_which_card_to_take() {
        // The words that tell a player what to do stay out of every text of the pick views.
        let keys = [
            Key::TopOne,
            Key::TopMany,
            Key::TierEqual,
            Key::TopLabel,
            Key::PairMark,
            Key::PairsLine,
            Key::ConfHigh,
            Key::ConfMid,
            Key::ConfLow,
            Key::PicksDone,
            Key::PickTitle,
            Key::Level,
        ];
        for lang in [Lang::En, Lang::Tr] {
            for key in keys {
                let text = lang.t(key).to_lowercase();
                for word in [
                    "pick this",
                    "take this",
                    "choose",
                    "best card",
                    "seç!",
                    "al!",
                    "önerilen",
                    "recommended",
                ] {
                    assert!(!text.contains(word), "{lang:?} {key:?}: {text}");
                }
            }
        }
        // The pairs line keeps its caveat: it says "if picked earlier", and never that the player did.
        assert!(Lang::En.t(Key::PairsLine).ends_with("if picked earlier"));
        assert!(Lang::Tr.t(Key::PairsLine).starts_with("Önceden seçtiysen"));
        // The mark on the strip cell is one word that says the augment pairs with another one. It
        // names no pick as a fact: the caveat stays in the pairs line and its tooltip.
        assert_eq!(Lang::En.t(Key::PairMark), "pairs");
        assert_eq!(Lang::Tr.t(Key::PairMark), "eşleşir");
        for lang in [Lang::En, Lang::Tr] {
            let mark = lang.t(Key::PairMark);
            assert!(
                !mark.contains("your") && !mark.contains("taken") && !mark.contains("seçtiğin"),
                "{lang:?}: {mark}"
            );
        }
        // The lines for a partner without a name never say that the player took the card.
        for key in [Key::PairsLineEarlier, Key::ReasonPairsEarlier] {
            assert!(
                !Lang::En.t(key).contains("your") && !Lang::Tr.t(key).contains("seçtiğin"),
                "{key:?}"
            );
        }
        // No text of the waiting card names an earlier pick.
        for lang in [Lang::En, Lang::Tr] {
            assert!(!lang.t(Key::PicksDone).contains("{0}"));
        }
    }

    // ---- The phase decides what the main window shows.

    #[test]
    fn the_phase_picks_the_screen() {
        for phase in [Phase::Offline, Phase::Idle, Phase::Lobby] {
            assert_eq!(
                screen_for(phase, false, false, false),
                Screen::Ready,
                "{phase:?}"
            );
        }
        assert_eq!(
            screen_for(Phase::ChampSelect, false, false, false),
            Screen::Select
        );
        assert_eq!(
            screen_for(Phase::InGame, false, false, false),
            Screen::Waiting
        );
        assert_eq!(
            screen_for(Phase::PostGame, false, false, false),
            Screen::After
        );
        // Cards on the screen are a pick screen. The reader finds them in a game only, and a capture shows them anywhere.
        assert_eq!(screen_for(Phase::InGame, true, false, false), Screen::Pick);
        assert_eq!(screen_for(Phase::Lobby, true, false, false), Screen::Pick);
        // A game of another mode has no pick screens and no saved result. Champ select of an ARAM queue keeps its bench.
        assert_eq!(screen_for(Phase::InGame, false, true, false), Screen::Other);
        assert_eq!(
            screen_for(Phase::PostGame, false, true, false),
            Screen::Other
        );
        assert_eq!(
            screen_for(Phase::ChampSelect, false, true, false),
            Screen::Select
        );
        assert_eq!(screen_for(Phase::Lobby, false, true, false), Screen::Ready);
        assert_eq!(screen_for(Phase::InGame, true, true, false), Screen::Pick);
    }

    #[test]
    fn a_champ_select_without_a_bench_has_no_mayhem_page() {
        // The queue decides (`non_aram_queue` feeds `screen_for`). Ranked solo (420) and Normal draft (400): no page.
        for queue in [420, 400, 440] {
            let non_aram = non_aram_queue(Some(queue));
            assert!(non_aram, "{queue}");
            assert_eq!(
                screen_for(Phase::ChampSelect, false, true, non_aram),
                Screen::Other,
                "{queue}"
            );
        }
        // ARAM (450) and Mayhem (2400) keep the Select page.
        for queue in [450, 2400] {
            let non_aram = non_aram_queue(Some(queue));
            assert!(!non_aram, "{queue}");
            assert_eq!(
                screen_for(Phase::ChampSelect, false, !is_mayhem(queue), non_aram),
                Screen::Select,
                "{queue}"
            );
        }
        // An unknown queue, and a custom game (queue 0 or less), keep the Select page too.
        for queue in [None, Some(0), Some(-1)] {
            assert!(!non_aram_queue(queue), "{queue:?}");
            assert_eq!(
                screen_for(
                    Phase::ChampSelect,
                    false,
                    queue.is_some(),
                    non_aram_queue(queue)
                ),
                Screen::Select,
                "{queue:?}"
            );
        }
        // Cards on the screen still win, whatever the queue.
        assert_eq!(
            screen_for(Phase::ChampSelect, true, true, true),
            Screen::Pick
        );
        // The window comes up in champ select only for a queue with a bench or an unknown queue,
        // with the setting on, and while the overlay is closed.
        assert!(raises_on_champ_select(true, false, Some(450)));
        assert!(raises_on_champ_select(true, false, Some(2400)));
        assert!(raises_on_champ_select(true, false, None));
        assert!(raises_on_champ_select(true, false, Some(0)));
        assert!(!raises_on_champ_select(true, false, Some(420)));
        assert!(!raises_on_champ_select(false, false, Some(450)));
        assert!(!raises_on_champ_select(true, true, Some(450)));
    }

    #[test]
    fn a_ranked_champ_select_does_not_pull_the_window_to_the_main_page() {
        // A queue with a bench, an unknown queue, and a custom game (queue 0 or less): back to the main page.
        for queue in [Some(450), Some(2400), Some(3270), None, Some(0), Some(-1)] {
            assert!(champ_select_resets_page(queue), "{queue:?}");
        }
        // Ranked, Normal and the other queues without a bench: the page that is open stays (Settings, My games).
        for queue in [420, 400, 440, 430, 1700] {
            assert!(!champ_select_resets_page(Some(queue)), "{queue}");
        }
        // The window is raised for the same queues that it is sent to the main page for. The raise rule is the same as before.
        for queue in [None, Some(0), Some(420), Some(450), Some(2400)] {
            assert_eq!(
                champ_select_resets_page(queue),
                raises_on_champ_select(true, false, queue),
                "{queue:?}"
            );
        }
    }

    #[test]
    fn a_game_that_starts_or_ends_sends_the_window_back_to_the_main_page() {
        // From the lobby or champ select to a game, and from the game to its end: back to the main page.
        for from in [Phase::Idle, Phase::Lobby, Phase::ChampSelect] {
            assert!(phase_resets_page(from, Phase::InGame), "{from:?}");
        }
        assert!(phase_resets_page(Phase::InGame, Phase::PostGame));
        // Other phases keep the page (champ select resets it in its own branch), and so does
        // the first phase after the start of Hexsight: the page that is open stays open.
        for to in [
            Phase::Offline,
            Phase::Idle,
            Phase::Lobby,
            Phase::ChampSelect,
        ] {
            assert!(!phase_resets_page(Phase::InGame, to), "{to:?}");
        }
        assert!(!phase_resets_page(Phase::Offline, Phase::InGame));
        assert!(!phase_resets_page(Phase::Offline, Phase::PostGame));
    }

    #[test]
    fn esc_is_the_way_back_from_settings_and_my_games_but_not_while_a_hotkey_waits() {
        for page in [Page::Settings, Page::Games] {
            assert!(esc_goes_back(page, false, true), "{page:?}");
            // No Esc, no way back. A hotkey that waits for a key takes the Esc to cancel the wait.
            assert!(!esc_goes_back(page, false, false), "{page:?}");
            assert!(!esc_goes_back(page, true, true), "{page:?}");
        }
        // On the main page, Esc does nothing.
        assert!(!esc_goes_back(Page::Main, false, true));
        // The window follows it: a frame with Esc on the Settings page goes back, a frame without Esc stays.
        let v = View {
            phase: Phase::Idle,
            offer: None,
            ..pick_view()
        };
        let mut app = test_app(Settings::default(), Lang::En);
        app.page = Page::Settings;
        window_texts(&mut app, &v, None);
        assert_eq!(app.page, Page::Settings);
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let esc = key_event(egui::Key::Escape, true, false);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(MAIN_W, 680.0))),
            events: vec![esc],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            app.window_ui(ui, &v);
        })
        .drop_without_applying_deltas();
        assert_eq!(app.page, Page::Main);
        // While a hotkey waits, the same frame leaves the page open and cancels the wait.
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let mut app = test_app(Settings::default(), Lang::En);
        app.page = Page::Settings;
        app.set_rebinding(Some(Hotkey::Overlay));
        let esc = key_event(egui::Key::Escape, true, false);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(MAIN_W, 680.0))),
            events: vec![esc],
            ..Default::default()
        };
        ctx.run_ui(input, |ui| {
            app.window_ui(ui, &v);
        })
        .drop_without_applying_deltas();
        assert_eq!((app.page, app.rebinding), (Page::Settings, None));
    }

    #[test]
    fn the_overlay_is_a_pill_between_pick_screens() {
        assert_eq!(overlay_mode(false, false), OverlayMode::Pill);
        assert_eq!(overlay_mode(true, false), OverlayMode::Strip);
    }

    #[test]
    fn the_next_pick_follows_the_level_and_the_closed_pick_screens() {
        assert_eq!(PICK_LEVELS, [3, 7, 11, 15]);
        // A new game: the first pick is next, at any level before 3.
        assert_eq!(next_pick(0, 0), Some(0));
        assert_eq!(next_pick(1, 0), Some(0));
        assert_eq!(next_pick(3, 0), Some(0));
        // A pick screen that showed before level 3 counts as the first one.
        assert_eq!(next_pick(2, 1), Some(1));
        // After the first pick screen closed.
        assert_eq!(next_pick(3, 1), Some(1));
        assert_eq!(next_pick(6, 1), Some(1));
        // At level 7 the second screen is due until it closes.
        assert_eq!(next_pick(7, 1), Some(1));
        assert_eq!(next_pick(7, 2), Some(2));
        // Hexsight started late: the levels that are past count, whatever the screens say.
        assert_eq!(next_pick(9, 0), Some(2));
        assert_eq!(next_pick(12, 0), Some(3));
        // All four are past.
        assert_eq!(next_pick(15, 4), None);
        assert_eq!(next_pick(16, 0), None);
    }

    // ---- The size of the main window.

    #[test]
    fn the_main_window_fits_its_content() {
        // The content sets the height, between 120 and the maximum, rounded up.
        assert_eq!(fit_height(300.2, 680.0, None), Some(301.0));
        assert_eq!(fit_height(50.0, 680.0, None), Some(120.0));
        assert_eq!(fit_height(900.0, 680.0, None), Some(680.0));
        // A change of 2 points or less keeps the height. A larger change resizes.
        assert_eq!(fit_height(301.0, 680.0, Some(300.0)), None);
        assert_eq!(fit_height(302.0, 680.0, Some(300.0)), None);
        assert_eq!(fit_height(303.0, 680.0, Some(300.0)), Some(303.0));
        // Content that is taller than the maximum settles at the maximum.
        assert_eq!(fit_height(900.0, 680.0, Some(680.0)), None);
        // A small monitor lowers the maximum: 80 points stay free, and it never goes under 320.
        assert_eq!(max_height_on(None), MAIN_MAX_H);
        assert_eq!(max_height_on(Some(2160.0)), MAIN_MAX_H);
        assert_eq!(max_height_on(Some(600.0)), 520.0);
        assert_eq!(max_height_on(Some(300.0)), 320.0);
    }

    // ---- The bench.

    #[test]
    fn the_best_swap_needs_a_better_tier_and_a_real_sample() {
        let row = |tier: u8, win_rate: f32, games: u32| {
            Some(ChampRow {
                win_rate,
                games,
                tier,
            })
        };
        let mine = row(3, 0.50, 9_000);
        // The best tier among the bench champions that beat yours.
        let bench = [
            (10, row(2, 0.53, 9_000)),
            (11, row(1, 0.56, 9_000)),
            (12, row(4, 0.45, 9_000)),
        ];
        assert_eq!(best_swap(mine, &bench, 300), Some(11));
        // The same tier is no gain.
        assert_eq!(best_swap(mine, &[(10, row(3, 0.55, 9_000))], 300), None);
        // A small sample is no reason to swap. A lower minimum lets it in.
        assert_eq!(best_swap(mine, &[(10, row(0, 0.60, 299))], 300), None);
        assert_eq!(best_swap(mine, &[(10, row(0, 0.60, 299))], 0), Some(10));
        // A tie goes to the higher win rate.
        assert_eq!(
            best_swap(
                mine,
                &[(10, row(1, 0.55, 5_000)), (11, row(1, 0.57, 5_000))],
                300
            ),
            Some(11)
        );
        // A champion without stats never wins, and nothing compares without your own stats.
        assert_eq!(best_swap(mine, &[(10, None)], 300), None);
        assert_eq!(best_swap(None, &bench, 300), None);
        assert_eq!(best_swap(mine, &[], 300), None);
    }

    #[test]
    fn the_bench_shows_the_best_tier_first_and_the_unknown_last() {
        let row = |tier: u8, win_rate: f32| {
            Some(ChampRow {
                win_rate,
                games: 5_000,
                tier,
            })
        };
        let mut bench = [
            (1, None),
            (2, row(3, 0.50)),
            (3, row(1, 0.55)),
            (4, row(3, 0.52)),
            (5, row(0, 0.58)),
        ];
        bench.sort_by(|a, b| bench_order(a.1, b.1));
        assert_eq!(
            bench.iter().map(|b| b.0).collect::<Vec<_>>(),
            vec![5, 3, 4, 2, 1]
        );
    }

    // ---- After the game.

    fn game(id: i64, win: bool, secs: u32, augments: usize) -> Game {
        Game {
            id,
            date: String::new(),
            queue: 2400,
            champion: 157,
            augments: (1..=augments as i64).collect(),
            win,
            kills: 1,
            deaths: 2,
            assists: 3,
            secs,
        }
    }

    #[test]
    fn the_result_is_the_game_that_came_after_the_last_one_seen() {
        let history = [game(12, true, 1300, 4), game(11, false, 900, 4)];
        // Nothing new: the newest game is the one that was there before the game.
        assert!(fresh_game(&history, Some(12)).is_none());
        // A game came in while Hexsight ran: it is the result.
        assert_eq!(fresh_game(&history, Some(11)).map(|g| g.id), Some(12));
        // The first game ever.
        assert_eq!(fresh_game(&history, None).map(|g| g.id), Some(12));
        assert!(fresh_game(&[], None).is_none());
    }

    #[test]
    fn a_game_length_reads_as_minutes_and_seconds() {
        assert_eq!(clock(1300), Some("21:40".to_string()));
        assert_eq!(clock(65), Some("1:05".to_string()));
        assert_eq!(clock(3_725), Some("62:05".to_string()));
        // A saved game from an old version has no length.
        assert_eq!(clock(0), None);
    }

    #[test]
    fn the_result_line_names_the_champion_the_length_and_the_augments() {
        let g = game(1, true, 1300, 4);
        assert_eq!(
            result_line(Lang::Tr, "Jayce", &g),
            "Jayce · 21:40 · 4 eklenti"
        );
        assert_eq!(
            result_line(Lang::En, "Jayce", &g),
            "Jayce · 21:40 · 4 augments"
        );
        assert_eq!(
            result_line(Lang::En, "Jayce", &game(1, false, 0, 1)),
            "Jayce · 1 augment"
        );
        assert_eq!(Lang::Tr.t(Key::AfterWin), "Galibiyet");
        assert_eq!(Lang::Tr.t(Key::AfterLoss), "Mağlubiyet");
    }

    // ---- Texts in the 300 point window and the overlay.

    #[test]
    fn the_title_bar_fits_in_the_300_point_window() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let name_w = text_width(&ctx, "Hexsight", FontId::new(13.5, semibold()));
        let phases = [
            Key::PhaseOffline,
            Key::PhaseClient,
            Key::PhaseLobby,
            Key::PhaseChampSelect,
            Key::PhaseInGame,
            Key::PhasePostGame,
        ];
        for lang in [Lang::En, Lang::Tr] {
            for key in phases {
                let pill_w = text_width(&ctx, lang.t(key), FontId::new(11.5, semibold())) + 22.0;
                // The mark (20), two gaps of 8, 2 and 4 points, the name, and the pill. At the right: the gear, a gap, and the close button.
                let used = 20.0 + 8.0 + 2.0 + name_w + 8.0 + 4.0 + pill_w + 28.0 + 8.0 + 28.0;
                assert!(
                    used + 2.0 <= MAIN_W - 2.0 * MARGIN,
                    "{lang:?} {key:?} needs {used:.1} points of {}",
                    MAIN_W - 2.0 * MARGIN
                );
            }
            // The titles of the two other pages sit beside the way back, and fit as well.
            for key in [Key::SettingsTitle, Key::MyGames] {
                let w = text_width(&ctx, lang.t(key), FontId::new(17.0, bold()));
                assert!(
                    28.0 + 2.0 + w + 28.0 + 8.0 <= MAIN_W - 2.0 * MARGIN,
                    "{lang:?} {key:?}"
                );
            }
        }
    }

    #[test]
    fn the_window_and_the_pill_have_the_planned_sizes() {
        assert_eq!(MAIN_W, 300.0);
        assert_eq!((PILL_W, PILL_H), (162.0, 40.0));
        assert_eq!(OVERLAY_STRIP_W, 340.0);
        // The tallest window leaves room for the title bar and the footer.
        let ctx = egui::Context::default();
        assert!(App::page_height(&ctx) > 400.0 && App::page_height(&ctx) < MAIN_MAX_H);
    }

    #[test]
    fn the_overlay_plate_is_near_opaque_and_has_a_light_edge() {
        // Locked: 250 of 255 (before 230). Unlocked: fully opaque (before 250).
        assert_eq!(overlay_fill(false).a(), 250);
        assert_eq!(overlay_fill(true).a(), 255);
        // The outline is white at 80 (before 22). The shadow is soft: 6 points down, 18 points of
        // blur, black at 110 of 255 (before a solid black rect, 3 points down, at 130).
        assert_eq!(overlay_edge().color, Color32::from_white_alpha(80));
        assert_eq!(
            overlay_shadow(),
            egui::epaint::Shadow {
                offset: [0, 6],
                blur: 18,
                spread: 0,
                color: Color32::from_black_alpha(110)
            }
        );
        // The overlay uses them: the strip of a pick screen and the pill each paint one plate, and
        // one soft shadow under it. The shadow is a blurred shape that comes before the plate.
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        for strip in [true, false] {
            let mut app = test_app(Settings::default(), Lang::En);
            let mut v = pick_view();
            if !strip {
                v.offer = None;
            }
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    pos2(0.0, 0.0),
                    vec2(OVERLAY_WINDOW_W, 300.0),
                )),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| app.overlay_ui(ui, &v));
            // The strip paints plate and edge in one shape. The pill paints the fill and the edge one after the other.
            let plates = rects_where(&out.shapes, |r| r.fill == overlay_fill(false));
            let edges = rects_where(&out.shapes, |r| r.stroke == overlay_edge());
            let all = shapes_in_order(&out.shapes);
            out.drop_without_applying_deltas();
            assert_eq!(
                (plates.len(), edges.len()),
                (1, 1),
                "strip {strip}: {plates:?} {edges:?}"
            );
            let shadows: Vec<usize> = all.iter().enumerate().filter(|(_, s)| matches!(s, egui::Shape::Rect(r) if r.blur_width == 18.0 && r.fill == Color32::from_black_alpha(110))).map(|(i, _)| i).collect();
            let plate = all
                .iter()
                .position(|s| matches!(s, egui::Shape::Rect(r) if r.fill == overlay_fill(false)))
                .unwrap();
            assert_eq!(shadows.len(), 1, "strip {strip}: one soft shadow");
            assert!(
                shadows[0] < plate,
                "strip {strip}: the shadow comes before the plate"
            );
            // No solid black rect is left: that was the grey band under the plate.
            let bands = all.iter().filter(|s| matches!(s, egui::Shape::Rect(r) if r.blur_width == 0.0 && r.fill.a() > 0 && r.fill.r() == 0 && r.fill.g() == 0 && r.fill.b() == 0)).count();
            assert_eq!(bands, 0, "strip {strip}: a solid black rect is back");
        }
    }

    /// Every flat shape of a shape list, in paint order.
    fn shapes_in_order(shapes: &[egui::epaint::ClippedShape]) -> Vec<egui::Shape> {
        fn walk(shape: &egui::Shape, out: &mut Vec<egui::Shape>) {
            match shape {
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                other => out.push(other.clone()),
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|c| walk(&c.shape, &mut out));
        out
    }

    #[test]
    fn the_overlay_window_leaves_room_for_the_soft_shadow() {
        // The shadow reaches 9 points (half of the blur) past the plate, and 6 more down. The window
        // keeps that room, or the shadow would end in a hard line at the edge of the window.
        let s = overlay_shadow();
        let reach = f32::from(s.blur) / 2.0;
        assert!(OVERLAY_SIDE >= reach, "{OVERLAY_SIDE} at the sides");
        assert!(
            OVERLAY_TOP >= reach - f32::from(s.offset[1]),
            "{OVERLAY_TOP} at the top"
        );
        assert!(
            OVERLAY_BOTTOM >= reach + f32::from(s.offset[1]),
            "{OVERLAY_BOTTOM} at the bottom"
        );
        // The card of the strip is 332 points wide in a window of 352.
        assert_eq!(OVERLAY_WINDOW_W, OVERLAY_STRIP_W - 8.0 + 2.0 * OVERLAY_SIDE);
        // The window follows the card: the strip, the advice card and the pill all fit their shadow.
        let dead = Advice {
            me_dead: true,
            ..game_advice()
        };
        for (name, v) in [
            ("advice", waiting_view(Some(dead))),
            ("strip", pick_view()),
            (
                "pill",
                View {
                    offer: None,
                    ..pick_view()
                },
            ),
        ] {
            let (_, size) = overlay_texts(Lang::En, &v);
            let size = size.unwrap_or_else(|| panic!("{name}: the overlay sized itself"));
            if name != "pill" {
                assert_eq!(size.x, OVERLAY_WINDOW_W, "{name}");
            }
            assert!(size.x >= PILL_W + 2.0 * OVERLAY_SIDE, "{name}: {size:?}");
        }
    }

    #[test]
    fn the_pill_text_fits_in_its_162_points_and_the_pill_grows_for_the_next_pick() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        // The room of the pill: 16 before the dot, the dot (6) and 7 more, the name, 6, the word, 18 after.
        let pill_for = |word: &str| {
            16.0 + 6.0
                + 7.0
                + text_width(&ctx, "Hexsight", pill_name_font())
                + 6.0
                + text_width(&ctx, word, pill_word_font())
                + 18.0
        };
        for lang in [Lang::En, Lang::Tr] {
            // Outside a game, and before the level is known, the word is "watching" and the pill stays 162 wide.
            for (phase, level) in [
                (Phase::Lobby, 9),
                (Phase::ChampSelect, 0),
                (Phase::InGame, 0),
                (Phase::PostGame, 18),
            ] {
                let word = pill_word(lang, phase, level, 0);
                assert_eq!(
                    word,
                    lang.t(Key::OverlayWatching),
                    "{lang:?} {phase:?} {level}"
                );
                let used = pill_for(&word);
                assert!(
                    used <= PILL_W,
                    "{lang:?}: the pill text needs {used:.1} points"
                );
            }
            // In a game with a known level, the word tells the next pick. The pill grows with it, but
            // stays well under the width of the strip.
            for level in 1..=18 {
                for done in 0..=4 {
                    let word = pill_word(lang, Phase::InGame, level, done);
                    let used = pill_for(&word).ceil().max(PILL_W);
                    assert!(
                        used <= 230.0,
                        "{lang:?} level {level} done {done}: {word:?} needs {used:.1} points"
                    );
                }
            }
        }
    }

    #[test]
    fn the_pill_says_when_the_next_pick_comes() {
        for lang in [Lang::En, Lang::Tr] {
            let at = |level: i64, done: usize| pill_word(lang, Phase::InGame, level, done);
            // Level 1: the first pick screen is at level 3.
            assert_eq!(at(1, 0), lang.fmt(Key::OverlayNextPick, &[&3]));
            // Level 7 with one pick done: the second screen is at level 7, and it follows the level.
            assert_eq!(at(7, 1), lang.fmt(Key::OverlayNextPick, &[&7]));
            // A screen that Hexsight missed is past once the level is higher: level 9, no pick seen.
            assert_eq!(at(9, 0), lang.fmt(Key::OverlayNextPick, &[&11]));
            assert_eq!(at(14, 3), lang.fmt(Key::OverlayNextPick, &[&15]));
            // All four are done, or the level is past the last one.
            assert_eq!(at(16, 4), lang.t(Key::OverlayPicksDone));
            assert_eq!(at(18, 0), lang.t(Key::OverlayPicksDone));
            // The words are the keys of P3, in both languages.
            assert_eq!(
                at(1, 0),
                if lang == Lang::En {
                    "pick at level 3"
                } else {
                    "3. seviyede seçim"
                }
            );
            // Outside a game, or without a level, it stays "watching".
            assert_eq!(
                pill_word(lang, Phase::ChampSelect, 7, 1),
                lang.t(Key::OverlayWatching)
            );
            assert_eq!(
                pill_word(lang, Phase::InGame, 0, 0),
                lang.t(Key::OverlayWatching)
            );
        }
        // The pill paints the word: a game at level 7 with one pick done says "pick at level 7".
        let v = View {
            offer: None,
            phase: Phase::InGame,
            level: 7,
            picks_done: 1,
            ..pick_view()
        };
        let (texts, _) = overlay_texts(Lang::En, &v);
        assert!(
            texts.contains(&"pick at level 7".to_string())
                && !texts.contains(&"watching".to_string()),
            "{texts:?}"
        );
        let lobby = View {
            offer: None,
            phase: Phase::Lobby,
            level: 0,
            ..pick_view()
        };
        let (texts, _) = overlay_texts(Lang::En, &lobby);
        assert!(texts.contains(&"watching".to_string()), "{texts:?}");
    }

    #[test]
    fn the_strip_header_fits_in_the_340_points() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        // The inner width is 340 - 28. The left part: the mark (18), the name, the level. The right part: a dot and the top-tier line.
        let room = OVERLAY_STRIP_W - 28.0;
        for lang in [Lang::En, Lang::Tr] {
            let left = 18.0
                + 8.0
                + text_width(&ctx, "Hexsight", strip_name_font())
                + 8.0
                + text_width(&ctx, &lang.fmt(Key::Level, &[&15]), strip_level_font());
            for line in [
                TopLine::Count(1),
                TopLine::Count(3),
                TopLine::Equal,
                TopLine::NoData,
            ] {
                let right = 9.0
                    + text_width(
                        &ctx,
                        &top_line_text(lang, line).0,
                        FontId::new(11.5, semibold()),
                    );
                assert!(
                    left + right + 8.0 <= room,
                    "{lang:?} {line:?}: {:.1} of {room}",
                    left + right
                );
            }
        }
    }

    #[test]
    fn a_strip_cell_holds_its_texts_with_three_cards() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        // Three cells share 312 points with two gaps of 6, and a cell keeps CELL_PAD points at each side.
        let room = (OVERLAY_STRIP_W - 28.0 - 12.0) / 3.0 - 2.0 * CELL_PAD;
        for lang in [Lang::En, Lang::Tr] {
            // The widest game counts that a card shows: thousands, ten thousands, and a million.
            for games in [999, 9_999, 47_000, 468_000, 5_200_000] {
                let text = lang.games(games, &fmt_n(lang, games));
                let w = text_width(&ctx, &text, cell_games_font());
                assert!(
                    w <= room,
                    "{lang:?} {text:?} is {w:.1} points wide, the room is {room:.1}"
                );
            }
            // The label row: the position sits left of the tier badge (30 points, with 4 between), and so
            // does "top", under it. "top" after the position on one line would reach the badge.
            let label_room = room - CELL_BADGE - 4.0;
            for x in [0.2, 0.5, 0.8] {
                let label = text_width(
                    &ctx,
                    &lang.upper(position_label(lang, x)),
                    cell_label_font(),
                );
                let top = text_width(&ctx, lang.t(Key::TopLabel), cell_top_font());
                assert!(
                    label <= label_room,
                    "{lang:?} {x}: {label:.1} of {label_room:.1}"
                );
                assert!(
                    top <= label_room,
                    "{lang:?} {x}: {top:.1} of {label_room:.1}"
                );
            }
            // The bottom row: the bars, and the word only for a small sample. The other two words are gone.
            let low = kit::BARS_W
                + 6.0
                + text_width(&ctx, lang.t(Confidence::Low.key()), cell_word_font());
            assert!(
                low <= room,
                "{lang:?} {:?}: {low:.1} of {room:.1}",
                lang.t(Confidence::Low.key())
            );
            // The pairs mark: the link glyph and its word. Beside the bars it fits. Beside the bars and the
            // word of a small sample it does not, so it goes under the position, where it fits as well.
            let pair = PAIR_GLYPH_W + text_width(&ctx, lang.t(Key::PairMark), cell_pair_font());
            assert!(
                kit::BARS_W + MARK_GAP + pair <= room,
                "{lang:?}: {pair:.1} beside the bars"
            );
            assert!(pair <= label_room, "{lang:?}: {pair:.1} under the position");
            assert!(text_width(&ctx, lang.t(Key::NoData), cell_word_font()) <= room);
        }
    }

    /// The boxes of the texts that one cell paints, once each (the dark copy under a text is dropped).
    fn cell_boxes(lang: Lang, c: &PickCell) -> (Rect, Vec<(String, Rect, Color32)>) {
        let app = test_app(Settings::default(), lang);
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let cell = Rect::from_min_size(
            pos2(0.0, 0.0),
            vec2((OVERLAY_STRIP_W - 28.0 - 12.0) / 3.0, CELL_H),
        );
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(cell.expand(10.0)),
                ..Default::default()
            },
            |ui| app.overlay_cell(ui, cell, c),
        );
        let mut boxes: Vec<(String, Rect, Color32)> = Vec::new();
        for b in painted_boxes(&out.shapes) {
            // A text is painted twice: the dark copy first, 1 point down and right. Keep the real one.
            match boxes
                .iter_mut()
                .find(|x| x.0 == b.0 && (x.1.min - b.1.min).abs().max_elem() <= 1.01)
            {
                Some(x) => *x = b,
                None => boxes.push(b),
            }
        }
        out.drop_without_applying_deltas();
        (cell, boxes)
    }

    #[test]
    fn a_cell_has_its_tier_badge_top_right_and_keeps_its_texts_apart() {
        for lang in [Lang::En, Lang::Tr] {
            for top in [false, true] {
                for (tier, games) in [
                    (Some(0u8), Some(120u32)),
                    (Some(2), Some(5_000)),
                    (Some(5), Some(468_000)),
                    (None, None),
                ] {
                    let c = PickCell {
                        id: 1,
                        x: 0.5,
                        tier,
                        games,
                        top,
                        pair: None,
                        fit: None,
                    };
                    let (cell, boxes) = cell_boxes(lang, &c);
                    let badge = cell_badge_rect(cell);
                    // 30 by 30 points, in the corner, 8 points from the top and from the right.
                    assert_eq!((badge.width(), badge.height()), (30.0, 30.0));
                    assert_eq!(
                        (badge.top() - cell.top(), cell.right() - badge.right()),
                        (CELL_PAD, CELL_PAD)
                    );
                    let find = |t: &str| boxes.iter().find(|b| b.0 == t).map(|b| b.1);
                    // The badge letter: the tier, or the question mark without stats.
                    let letter = crate::model::TIER_NAMES[usize::from(tier.unwrap_or(0))];
                    let letter_box = find(if tier.is_some() { letter } else { "?" })
                        .unwrap_or_else(|| panic!("{lang:?}: no tier letter in {boxes:?}"));
                    assert!(
                        badge.contains_rect(letter_box.shrink(1.0))
                            || badge.expand(2.0).contains_rect(letter_box),
                        "{lang:?}: {letter_box:?} {badge:?}"
                    );
                    // The position label sits top left, clear of the badge.
                    let label = boxes
                        .iter()
                        .find(|b| b.0 == lang.upper(position_label(lang, 0.5)))
                        .unwrap()
                        .1;
                    assert!(
                        label.left() == CELL_PAD
                            && label.top() < badge.center().y
                            && label.right() <= badge.left() - 3.0,
                        "{lang:?}: {label:?} {badge:?}"
                    );
                    // "top" shows only on a top cell. It does not touch the label or the badge.
                    let top_box = find(lang.t(Key::TopLabel));
                    assert_eq!(top_box.is_some(), top, "{lang:?}");
                    if let Some(t) = top_box {
                        assert!(
                            !t.expand(1.0).intersects(label) && !t.expand(1.0).intersects(badge),
                            "{lang:?}: {t:?} against {label:?} and {badge:?}"
                        );
                        // Under the position at the left edge, left of the badge, in every cell.
                        assert!(
                            t.left() == CELL_PAD
                                && t.top() >= label.bottom()
                                && t.right() <= badge.left() - 3.0,
                            "{lang:?}: {t:?} {label:?}"
                        );
                    }
                    // The games sit under the badge, at the left, and end above the bars.
                    let text =
                        games.map_or_else(|| "–".to_string(), |g| lang.games(g, &fmt_n(lang, g)));
                    let g = find(&text)
                        .unwrap_or_else(|| panic!("{lang:?}: no games {text:?} in {boxes:?}"));
                    assert!(
                        g.left() == CELL_PAD
                            && g.top() >= badge.bottom() + 5.0
                            && g.bottom() <= CELL_H - 14.0 - 3.0,
                        "{lang:?}: {g:?} {badge:?}"
                    );
                    assert!(g.right() <= cell.right() - CELL_PAD, "{lang:?}: {g:?}");
                }
            }
        }
    }

    #[test]
    fn a_cell_shows_the_word_for_a_small_sample_and_only_then() {
        for lang in [Lang::En, Lang::Tr] {
            let words = [Confidence::Low, Confidence::Mid, Confidence::High]
                .map(|c| lang.t(c.key()).to_string());
            for (games, shows) in [
                (Some(120u32), Some(0usize)),
                (Some(CONF_MID - 1), Some(0)),
                (Some(CONF_MID), None),
                (Some(CONF_HIGH), None),
                (None, None),
            ] {
                let c = PickCell {
                    id: 1,
                    x: 0.2,
                    tier: games.map(|_| 1),
                    games,
                    top: false,
                    pair: None,
                    fit: None,
                };
                let (cell, boxes) = cell_boxes(lang, &c);
                let texts: Vec<&str> = boxes.iter().map(|b| b.0.as_str()).collect();
                for (i, w) in words.iter().enumerate() {
                    assert_eq!(
                        texts.contains(&w.as_str()),
                        shows == Some(i),
                        "{lang:?} {games:?}: {w:?} in {texts:?}"
                    );
                }
                // The word is grey (`LABEL_2`), at the right of the bars, and inside the cell.
                if let Some(b) = boxes.iter().find(|b| b.0 == words[0]) {
                    assert_eq!(b.2, LABEL_2);
                    assert!(
                        b.1.left() >= CELL_PAD + kit::BARS_W + 5.0
                            && b.1.right() <= cell.right() - CELL_PAD,
                        "{lang:?}: {:?}",
                        b.1
                    );
                }
                // A card without stats says so, and has the question mark.
                assert_eq!(
                    texts.contains(&lang.t(Key::NoData)),
                    games.is_none(),
                    "{lang:?}: {texts:?}"
                );
                assert_eq!(texts.contains(&"?"), games.is_none(), "{lang:?}: {texts:?}");
            }
        }
    }

    #[test]
    fn the_pair_mark_sits_bottom_right_and_gives_way_to_the_word_of_a_small_sample() {
        for lang in [Lang::En, Lang::Tr] {
            let pair = Some(PairHint { with: 7 });
            // Enough games: no word, and the mark has its word at the right of the bars, on their line.
            let c = PickCell {
                id: 1,
                x: 0.5,
                tier: Some(1),
                games: Some(5_000),
                top: false,
                pair,
                fit: None,
            };
            let (cell, boxes) = cell_boxes(lang, &c);
            let mark = boxes
                .iter()
                .find(|b| b.0 == lang.t(Key::PairMark))
                .map(|b| b.1)
                .unwrap_or_else(|| panic!("{lang:?}: no pairs mark in {boxes:?}"));
            assert!(
                mark.right() <= cell.right() - CELL_PAD + 0.5
                    && mark.left() >= CELL_PAD + kit::BARS_W,
                "{lang:?}: {mark:?}"
            );
            assert!(
                mark.center().y > cell.bottom() - 20.0,
                "{lang:?}: the mark is on the line of the bars: {mark:?}"
            );
            // A small sample shows its word. The mark has no room beside it, so it goes under the position
            // (left of the badge), and it never overlaps the word.
            let low = PickCell {
                games: Some(120),
                ..c
            };
            let (cell, boxes) = cell_boxes(lang, &low);
            let word = boxes
                .iter()
                .find(|b| b.0 == lang.t(Confidence::Low.key()))
                .map(|b| b.1)
                .expect("the word of a small sample");
            let mark = boxes
                .iter()
                .find(|b| b.0 == lang.t(Key::PairMark))
                .map(|b| b.1)
                .unwrap_or_else(|| panic!("{lang:?}: no pairs mark in {boxes:?}"));
            assert!(word.right() <= cell.right() - CELL_PAD);
            assert!(
                !mark.intersects(word),
                "{lang:?}: {mark:?} against {word:?}"
            );
            assert!(
                mark.right() <= cell_badge_rect(cell).left() - 3.0
                    && mark.center().y < cell_badge_rect(cell).bottom(),
                "{lang:?}: {mark:?}"
            );
            // With "top" on the same cell, the second line is taken: the mark gives way, and nothing overlaps.
            let both = PickCell { top: true, ..low };
            let (_, boxes) = cell_boxes(lang, &both);
            assert!(
                boxes.iter().any(|b| b.0 == lang.t(Key::TopLabel)),
                "{lang:?}"
            );
            assert!(
                !boxes.iter().any(|b| b.0 == lang.t(Key::PairMark)),
                "{lang:?}: {boxes:?}"
            );
        }
    }

    #[test]
    fn the_trade_buttons_fit_beside_each_other() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        // The card of a trade request is 249 points wide inside (272, less 10 points of margin and 1.5 of
        // edge at each side). The two buttons share the row in equal parts, with 8 points between them,
        // and each text keeps 8 points at both sides.
        let part = (249.0 - 8.0) / 2.0;
        for lang in [Lang::En, Lang::Tr] {
            for key in [Key::TradeAccept, Key::TradeDecline] {
                let w = text_width(&ctx, lang.t(key), FontId::new(12.5, semibold())) + 16.0;
                assert!(w <= part, "{lang:?} {key:?}: {w:.1} of {part}");
            }
        }
    }

    #[test]
    fn the_trade_card_is_a_calm_card_with_badges_and_two_equal_buttons() {
        let sd = window_data(24);
        // Champion 1 is yours (tier C). Teammate 1 (champion 20, no stats) asks for it.
        let v = View {
            phase: Phase::ChampSelect,
            cs: Some(champ_select(0, 1)),
            sd: Some(sd),
            stats: Some(select_stats()),
            offer: None,
            ..pick_view()
        };
        for lang in [Lang::En, Lang::Tr] {
            let mut app = test_app(Settings::default(), lang);
            let ctx = egui::Context::default();
            theme::setup(&ctx);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    pos2(0.0, 0.0),
                    vec2(MAIN_W - 2.0 * MARGIN, 600.0),
                )),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| app.trade_requests(ui, &v));
            let texts = painted_text(&out.shapes);
            // The tier badge of each icon: yours is C, theirs has no stats (a dash).
            assert!(
                texts.iter().any(|t| t == "C") && texts.iter().any(|t| t == "—"),
                "{lang:?}: {texts:?}"
            );
            assert!(
                texts.iter().any(|t| *t == lang.t(Key::TradeWants)),
                "{lang:?}: {texts:?}"
            );
            // The card: the surface color and a blue edge of 1.5 points. No orange, and no glow.
            let frames = rects_where(&out.shapes, |r| {
                r.fill == SURFACE && r.stroke.color == BLUE && r.stroke.width == 1.5
            });
            assert_eq!(frames.len(), 1, "{lang:?}");
            let old = rects_where(&out.shapes, |r| {
                r.fill == Color32::from_rgba_unmultiplied(52, 40, 20, 235)
                    || [0.45, 0.5, 0.14, 0.06]
                        .iter()
                        .any(|f| r.stroke.color == ORANGE.gamma_multiply(*f))
            });
            assert!(
                old.is_empty(),
                "{lang:?}: the brown fill, the orange edge or the orange glow is back: {old:?}"
            );
            // Decline at the left and Accept at the right, with the same width, across the bottom of the card.
            let decline = rects_where(&out.shapes, |r| r.fill == SURFACE_3.gamma_multiply(0.85));
            let accept = rects_where(&out.shapes, |r| r.fill == GREEN.gamma_multiply(0.85));
            assert_eq!((decline.len(), accept.len()), (1, 1), "{lang:?}");
            let (d, a) = (decline[0], accept[0]);
            assert!(
                (d.width() - a.width()).abs() < 0.01
                    && d.right() < a.left()
                    && (d.top() - a.top()).abs() < 0.01,
                "{lang:?}: {d:?} {a:?}"
            );
            let inner = frames[0].width() - 2.0 * (10.0 + 1.5);
            assert!(
                (a.right() - d.left() - inner).abs() < 0.5,
                "{lang:?}: the buttons fill the row ({inner}): {d:?} {a:?}"
            );
            assert!(
                frames[0].contains_rect(d) && frames[0].contains_rect(a),
                "{lang:?}"
            );
            out.drop_without_applying_deltas();
        }
    }

    #[test]
    fn the_build_row_holds_as_many_items_as_it_can() {
        // The style is a column of plain text, and a gap (12) follows it. The row has no check mark.
        assert_eq!(items_that_fit(CARD_INNER - 68.0 - 12.0, 26.0), 5);
        // A long style name leaves room for fewer.
        assert_eq!(items_that_fit(CARD_INNER - 120.0 - 12.0, 26.0), 3);
        // No room, no item, and never more than five.
        assert_eq!(items_that_fit(10.0, 26.0), 0);
        assert_eq!(items_that_fit(-5.0, 26.0), 0);
        assert_eq!(items_that_fit(900.0, 26.0), 5);
    }

    #[test]
    fn the_first_run_texts_fit_the_card_in_two_lines() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let rows = [
            Key::CheckLeagueOk,
            Key::CheckLeagueNo,
            Key::CheckBorderlessOk,
            Key::CheckWindowedOk,
            Key::CheckBorderlessNo,
            Key::CheckOcrOk,
            Key::CheckOcrNo,
            Key::CheckOcrWait,
            Key::CheckDataOk,
            Key::CheckDataNo,
            Key::CheckDataWait,
        ];
        // The card is 272 points wide with 12 points of room at each side. A text starts 24 points in.
        let room = MAIN_W - 2.0 * MARGIN - 24.0 - 24.0;
        let lines = |text: &str| {
            let mut count = 0;
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                count = ui
                    .painter()
                    .layout(text.to_string(), FontId::proportional(12.5), LABEL, room)
                    .rows
                    .len()
            })
            .drop_without_applying_deltas();
            count
        };
        for lang in [Lang::En, Lang::Tr] {
            for key in rows {
                assert!(
                    lines(lang.t(key)) <= 2,
                    "{lang:?} {key:?} needs more than two lines"
                );
            }
            // The widest key names: F12 and F11, and "Key 45" for a hand-edited file.
            for (a, b) in [(0x7B, 0x7A), (0x2D, 0x2D)] {
                let texts = [
                    lang.fmt(
                        Key::CheckHotkeyOk,
                        &[&key_text(lang, a), &key_text(lang, b)],
                    ),
                    lang.fmt(Key::CheckHotkeyNo, &[&key_text(lang, a)]),
                ];
                for t in texts {
                    assert!(lines(&t) <= 2, "{lang:?} {t:?}");
                }
            }
        }
    }

    #[test]
    fn a_settings_row_keeps_its_label_beside_the_switch() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        // The hotkey rows: the label, the key and the button share the 246 points of a card (`CARD_INNER`).
        // The label wraps to a second line when it needs to (see `hotkey_row`), never a third.
        for lang in [Lang::En, Lang::Tr] {
            for key in [Key::HotOverlay, Key::HotScan] {
                let right = text_width(&ctx, "F12", FontId::new(12.0, semibold()))
                    + 16.0
                    + [Key::HotChange, Key::HotCancel]
                        .iter()
                        .map(|k| text_width(&ctx, lang.t(*k), FontId::new(12.5, semibold())) + 24.0)
                        .fold(0.0, f32::max)
                    + 16.0;
                let room = CARD_INNER - right;
                let mut rows = 0;
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                    ..Default::default()
                };
                ctx.run_ui(input, |ui| {
                    rows = ui
                        .painter()
                        .layout(
                            lang.t(key).to_string(),
                            FontId::proportional(13.5),
                            LABEL,
                            room,
                        )
                        .rows
                        .len()
                })
                .drop_without_applying_deltas();
                assert!(
                    rows <= 2,
                    "{lang:?} {key:?}: {rows} lines in {room:.1} points"
                );
            }
        }
    }

    #[test]
    fn the_portable_lines_fit_the_about_card_in_two_lines() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        // The About card has the 246 points of a card (`CARD_INNER`). The line is a caption (11.5 points) that wraps.
        let room = CARD_INNER;
        for lang in [Lang::En, Lang::Tr] {
            for key in [Key::AboutPortable, Key::AboutPortableFallback] {
                let mut rows = 0;
                let mut widest = 0.0;
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                    ..Default::default()
                };
                ctx.run_ui(input, |ui| {
                    let g = ui.painter().layout(
                        lang.t(key).to_string(),
                        FontId::proportional(11.5),
                        LABEL_2,
                        room,
                    );
                    rows = g.rows.len();
                    widest = g.size().x;
                })
                .drop_without_applying_deltas();
                assert!(rows <= 2, "{lang:?} {key:?}: {rows} lines in {room} points");
                // The long path is one word. It must not stick out of the card.
                assert!(widest <= room, "{lang:?} {key:?}: {widest:.1} points wide");
            }
        }
    }

    // ---- Settings.

    #[test]
    fn the_first_run_question_is_asked_once() {
        assert!(!Settings::default().autostart_asked);
        // An old settings.json has keys that the window does not keep any more. They are ignored.
        let s: Settings = serde_json::from_str(r#"{"tiers_only": true, "width": 420, "height": 720, "on_top": true, "always_list": true, "overlay_rows": 6}"#).unwrap();
        assert!(s.tiers_only && !s.autostart_asked);
        let s: Settings = serde_json::from_str(r#"{"autostart_asked": true}"#).unwrap();
        assert!(s.autostart_asked);
    }

    #[test]
    fn a_portable_copy_never_asks_to_start_with_windows() {
        use crate::cache::Place;
        // The normal copy asks once, and not when Windows starts it already. The debug hook asks again.
        assert!(asks_autostart(Place::Local, false, false, false));
        assert!(!asks_autostart(Place::Local, false, true, false));
        assert!(!asks_autostart(Place::Local, false, false, true));
        assert!(asks_autostart(Place::Local, true, true, true));
        // A portable copy never asks, also after the fallback to %LOCALAPPDATA%.
        for place in [Place::Portable, Place::PortableFallback] {
            for (debug, asked, on) in [
                (false, false, false),
                (true, false, false),
                (true, true, true),
            ] {
                assert!(
                    !asks_autostart(place, debug, asked, on),
                    "{place:?} {debug} {asked} {on}"
                );
            }
        }
    }

    #[test]
    fn the_about_group_says_where_the_portable_data_is() {
        use crate::cache::Place;
        assert_eq!(portable_line(Place::Local), None);
        assert_eq!(portable_line(Place::Portable), Some(Key::AboutPortable));
        assert_eq!(
            portable_line(Place::PortableFallback),
            Some(Key::AboutPortableFallback)
        );
        assert!(
            Lang::En.t(Key::AboutPortable).contains("Hexsight-data")
                && Lang::Tr.t(Key::AboutPortable).contains("Hexsight-data")
        );
        for lang in [Lang::En, Lang::Tr] {
            assert!(
                lang.t(Key::AboutPortableFallback)
                    .contains(r"%LOCALAPPDATA%\Hexsight"),
                "{lang:?}"
            );
        }
    }

    #[test]
    fn the_settings_that_have_no_switch_keep_their_values() {
        // The settings file still sets them, and an old file keeps what it had.
        let s = Settings::default();
        assert_eq!(
            (
                s.min_games,
                s.overlay_scale,
                s.glass,
                s.show_on_champ_select
            ),
            (300, 1, true, true)
        );
        assert!(
            !s.scan_always
                && !s.overlay_in_captures
                && !s.card_badges
                && s.auto_overlay
                && s.item_sets
        );
        let s: Settings =
            serde_json::from_str(r#"{"min_games": 800, "overlay_scale": 2, "glass": false}"#)
                .unwrap();
        assert_eq!((s.min_games, s.overlay_scale, s.glass), (800, 2, false));
    }

    #[test]
    fn an_old_settings_file_with_the_removed_augment_key_still_loads() {
        // Older versions had the setting "Show augment win rates". The key is gone. serde ignores
        // it, whatever its value, and the other settings keep their values.
        for value in ["true", "false"] {
            let json = format!(
                r#"{{"augment_rates": {value}, "tiers_only": true, "min_games": 800, "language": "tr"}}"#
            );
            let s: Settings = serde_json::from_str(&json).unwrap();
            assert!(s.tiers_only && !s.top_highlight && !s.pair_hints);
            assert_eq!((s.min_games, s.language), (800, LangPref::Turkish));
        }
        // Saving writes no such key back.
        let text = serde_json::to_string(&Settings::default()).unwrap();
        assert!(!text.contains("augment_rates"), "{text}");
    }

    // ---- No view shows an augment win rate.

    /// An app with no thread and no window, for the tests that paint a view.
    fn test_app(settings: Settings, lang: Lang) -> App {
        let (cmd, _) = std::sync::mpsc::channel();
        let (icon, _) = std::sync::mpsc::channel();
        let (want, _) = std::sync::mpsc::channel();
        let h = Handles {
            shared: Arc::new(std::sync::Mutex::new(worker::Shared::default())),
            cmd,
            icon,
            want,
        };
        App::assemble(h, settings, None, lang, None)
    }

    /// A pick screen with strong numbers: three cards, a top card, a pair with an earlier pick, and
    /// win rates that would show as numbers if a view drew them.
    fn pick_view() -> View {
        let aug = |name: &str| Augment {
            name: name.into(),
            name_en: name.into(),
            icon: String::new(),
        };
        let mut sd = StaticData::default();
        for (id, name) in [(1, "Alpha"), (2, "Bravo"), (3, "Charlie"), (7, "Echo")] {
            sd.augments.insert(id, aug(name));
        }
        let cs = ChampStats {
            augments: vec![
                st(1, 0, 0.671, 5_000),
                st(2, 2, 0.583, 5_000),
                st(3, 5, 0.412, 5_000),
            ],
            combos: vec![combo(&[1, 7], 0.699, 900)],
            ..Default::default()
        };
        View {
            phase: Phase::InGame,
            queue: Some(2400),
            cs: None,
            mine: Some(1),
            sd: Some(Arc::new(sd)),
            stats: None,
            details: Some(Arc::new(cs)),
            stale: false,
            no_data: None,
            offer: Some(offer_on(2, [1, 2, 3])),
            error: None,
            note: None,
            window_mode: Some(2),
            action: None,
            action_detail: None,
            offered: vec![(1, vec![7, 8, 9])],
            scan: None,
            read: ReadView::default(),
            history: Arc::new(Vec::new()),
            importing: false,
            league_found: true,
            ocr_probe: Some(true),
            capture: CaptureHealth::Fine,
            level: 7,
            picks_done: 1,
            item_sets_for: None,
            update: None,
            advice: None,
            gold: None,
        }
    }

    /// Every text that a shape list paints.
    fn painted_text(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        fn walk(shape: &egui::Shape, out: &mut Vec<String>) {
            match shape {
                egui::Shape::Text(t) => out.push(t.galley.text().to_string()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|c| walk(&c.shape, &mut out));
        out
    }

    /// The texts of the pick card in the main window and of the overlay strip, for one setting and one language.
    fn pick_view_texts(settings: Settings, lang: Lang) -> (Vec<String>, Vec<String>) {
        let app = test_app(settings, lang);
        let v = pick_view();
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let (mut card, mut strip) = (Vec::new(), Vec::new());
        // Several frames, so the fade of the pick card has run.
        for i in 0..30 {
            let input = egui::RawInput {
                time: Some(i as f64 / 30.0),
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 900.0))),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| {
                app.pick_card(ui, &v);
            });
            card = painted_text(&out.shapes);
            out.drop_without_applying_deltas();
            let out = ctx.run_ui(
                egui::RawInput {
                    time: Some(i as f64 / 30.0),
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 900.0))),
                    ..Default::default()
                },
                |ui| app.overlay_strip(ui, &v),
            );
            strip = painted_text(&out.shapes);
            out.drop_without_applying_deltas();
        }
        (card, strip)
    }

    /// Does the text hold a percent sign, a unit of points, a signed number, or one of the numbers
    /// that `pick_view` uses as win rates? Those are the forms of a win rate or a delta. The tier
    /// "S+" is not a number.
    fn looks_like_a_rate(text: &str) -> bool {
        let chars: Vec<char> = text.chars().collect();
        let signed = chars
            .windows(2)
            .any(|w| matches!(w[0], '+' | '-') && w[1].is_ascii_digit());
        text.contains('%')
            || text.contains("pts")
            || text.contains("puan")
            || signed
            || ["67", "58", "41", "69", "70"]
                .iter()
                .any(|n| text.contains(n))
    }

    #[test]
    fn the_pick_views_show_no_win_rate_with_any_settings() {
        for lang in [Lang::En, Lang::Tr] {
            for (top, pairs) in [(false, false), (true, false), (false, true), (true, true)] {
                for tiers_only in [false, true] {
                    let settings = Settings {
                        top_highlight: top,
                        pair_hints: pairs,
                        tiers_only,
                        ..Settings::default()
                    };
                    let (card, strip) = pick_view_texts(settings, lang);
                    for (view, texts) in [("pick card", &card), ("strip", &strip)] {
                        assert!(!texts.is_empty(), "{view} painted no text");
                        for t in texts {
                            assert!(
                                !looks_like_a_rate(t),
                                "{lang:?} top {top} pairs {pairs} tiers_only {tiers_only}: {view} paints {t:?}"
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn the_pick_header_shows_only_the_level_with_the_highlight_off() {
        for lang in [Lang::En, Lang::Tr] {
            let level = lang.fmt(Key::Level, &[&7]);
            let top_words = [
                lang.fmt(Key::TopOne, &[]),
                lang.fmt(Key::TopMany, &[&2]),
                lang.t(Key::TierEqual).to_string(),
                lang.t(Key::TopLabel).to_string(),
            ];
            let (card, strip) = pick_view_texts(Settings::default(), lang);
            // The level shows in the pick card and in the strip.
            assert!(
                card.contains(&level) && strip.contains(&level),
                "{lang:?}: {card:?} {strip:?}"
            );
            // No top-tier line, no "top" label, no pairs line and no pairs mark.
            for texts in [&card, &strip] {
                for t in texts {
                    assert!(top_words.iter().all(|w| w != t), "{lang:?}: {t:?}");
                    assert!(
                        !t.contains(lang.t(Key::PairMark)) && !t.contains("Echo"),
                        "{lang:?}: {t:?}"
                    );
                }
            }
            // The tiers, the game counts and the confidence stay. The strip now prints the tier letter of
            // each card (S+, A and D here), the games, and the bars. A medium sample has no word.
            let games = lang.games(5_000, &fmt_n(lang, 5_000));
            assert!(card.iter().any(|t| t.contains(&games)), "{card:?}");
            assert!(strip.iter().any(|t| t == &games), "{strip:?}");
            for tier in ["S+", "A", "D"] {
                assert!(
                    strip.iter().any(|t| t == tier),
                    "{lang:?}: tier {tier} is missing in {strip:?}"
                );
            }
            assert!(
                !strip.iter().any(|t| t == lang.t(Key::ConfMid)
                    || t == lang.t(Key::ConfHigh)
                    || t == lang.t(Key::ConfLow)),
                "{lang:?}: {strip:?}"
            );
            // With the highlight on, the line and the label show.
            let on = Settings {
                top_highlight: true,
                pair_hints: true,
                ..Settings::default()
            };
            let (card, strip) = pick_view_texts(on, lang);
            assert!(
                card.contains(&lang.fmt(Key::TopOne, &[])),
                "{lang:?}: {card:?}"
            );
            assert!(
                strip.contains(&lang.fmt(Key::TopOne, &[]))
                    && strip.contains(&lang.t(Key::TopLabel).to_string()),
                "{lang:?}: {strip:?}"
            );
            assert!(
                strip.contains(&lang.t(Key::PairMark).to_string()),
                "{lang:?}: {strip:?}"
            );
            assert!(
                card.iter().any(|t| t.contains("Echo")),
                "{lang:?}: {card:?}"
            );
        }
    }

    // ---- The main window fits on the screen.

    /// A monitor height that makes `max_height` 680 (the cap), and one that makes it 520.
    const MONITORS: [(f32, f32); 2] = [(760.0, 680.0), (600.0, 520.0)];

    /// Lay out the full main window for some frames on a monitor of height `monitor_h`.
    /// Returns the content height that `window_ui` measures, and the tallest window.
    fn window_height(app: &mut App, v: &View, monitor_h: f32) -> (f32, f32) {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let monitor = egui::ViewportInfo {
            monitor_size: Some(vec2(1920.0, monitor_h)),
            ..Default::default()
        };
        let (mut used, mut max) = (0.0, 0.0);
        // Several frames: the scroll area and the fades settle after the first one.
        for i in 0..10 {
            let input = egui::RawInput {
                time: Some(i as f64 / 10.0),
                screen_rect: Some(Rect::from_min_size(
                    pos2(0.0, 0.0),
                    vec2(MAIN_W, max_height_on(Some(monitor_h))),
                )),
                viewports: std::iter::once((egui::ViewportId::ROOT, monitor.clone())).collect(),
                ..Default::default()
            };
            ctx.run_ui(input, |ui| {
                used = app.window_ui(ui, v);
                max = App::max_height(ui.ctx());
            })
            .drop_without_applying_deltas();
        }
        (used, max)
    }

    /// Static data with `n` champions (ids 1 to n), three augments with long names, and some items.
    fn window_data(n: i64) -> Arc<StaticData> {
        let mut sd = StaticData::default();
        for id in 1..=n {
            sd.champions.insert(
                id,
                Champion {
                    id,
                    name: format!("Nunu ve Willump {id}"),
                    alias: format!("C{id}"),
                    icon: String::new(),
                },
            );
        }
        for id in [1, 2, 3, 7] {
            sd.augments.insert(
                id,
                Augment {
                    name: format!("Sonsuz Güç ve Zamanlama {id}"),
                    name_en: String::new(),
                    icon: String::new(),
                },
            );
        }
        for id in 1..=6 {
            sd.items.insert(
                3000 + id,
                Item {
                    name: format!("Item {id}"),
                    icon: String::new(),
                },
            );
        }
        // Two spells with long Turkish names: the champ select screens show the spell card.
        sd.spells.insert(
            4,
            Item {
                name: "İyileştirme".into(),
                icon: String::new(),
            },
        );
        sd.spells.insert(
            6,
            Item {
                name: "Tutuşturma".into(),
                icon: String::new(),
            },
        );
        Arc::new(sd)
    }

    /// `n` saved games over `champions` champions, newest first.
    fn many_games(n: i64, champions: i64) -> Arc<Vec<Game>> {
        let game = |i: i64| Game {
            id: n - i,
            date: String::new(),
            queue: 2400,
            champion: 1 + i % champions,
            augments: vec![1, 2],
            win: i % 3 != 0,
            kills: 5,
            deaths: 4,
            assists: 9,
            secs: 1200,
        };
        Arc::new((0..n).map(game).collect())
    }

    #[test]
    fn the_settings_and_games_pages_never_make_the_window_cut_the_footer() {
        // 200 saved games over 40 champions: My games lists its most played champions.
        let v = View {
            phase: Phase::Idle,
            offer: None,
            sd: Some(window_data(40)),
            history: many_games(200, 40),
            ..pick_view()
        };
        for lang in [Lang::En, Lang::Tr] {
            for page in [Page::Settings, Page::Games] {
                for (monitor_h, max) in MONITORS {
                    let mut app = test_app(Settings::default(), lang);
                    app.page = page;
                    let (used, measured) = window_height(&mut app, &v, monitor_h);
                    assert_eq!(measured, max, "the test input sets the monitor size");
                    assert!(
                        used <= max,
                        "{lang:?} {page:?}, max height {max}: the content needs {used:.1} points, so the window cuts the footer"
                    );
                    // The Settings page is longer than the tallest window: its list scrolls, and the window is full.
                    if page == Page::Settings {
                        assert!(
                            used >= max - 2.0,
                            "{lang:?} max height {max}: the Settings page uses only {used:.1} points"
                        );
                    }
                }
            }
        }
    }

    /// Static data for the champ select screens: 24 champions, your champion 1 (tier C), and
    /// a bench from champion 2 on. The bench has every tier, and the last two have no stats.
    fn select_stats() -> Arc<crate::data::Stats> {
        let tiers = [
            (1, 4),
            (2, 1),
            (3, 2),
            (4, 2),
            (5, 3),
            (6, 3),
            (7, 4),
            (8, 4),
            (9, 5),
        ];
        let rows = tiers.into_iter().map(|(id, tier)| {
            (
                id,
                ChampRow {
                    win_rate: 0.6 - f32::from(tier) * 0.02 - id as f32 * 0.001,
                    games: 5_000,
                    tier,
                },
            )
        });
        Arc::new(crate::data::Stats::of_champions(rows.collect()))
    }

    /// A champ select with a bench of `bench` champions and `trades` trade requests from
    /// teammates (champions 20 and 21).
    fn champ_select(bench: i64, trades: i64) -> crate::lcu::ChampSelect {
        use crate::lcu::{Mate, Trade};
        let team = (0..3)
            .map(|cell| Mate {
                cell,
                champion: if cell == 0 { 1 } else { 19 + cell },
            })
            .collect();
        let trades = (1..=trades)
            .map(|cell| Trade {
                id: 100 + cell,
                cell,
                state: TradeState::Received,
            })
            .collect();
        crate::lcu::ChampSelect {
            my_champion: 1,
            team,
            bench: (2..2 + bench).collect(),
            bench_enabled: true,
            trades,
            choices: Vec::new(),
        }
    }

    /// Every main page screen fits in the window at the tallest size (680) and on a small
    /// screen (520). The champ select has a bench of 1 to 10 and up to two trade requests.
    /// A page that is taller than the room scrolls, so the content that `window_ui`
    /// measures is never taller than the window, and the footer stays.
    #[test]
    fn the_main_page_screens_fit_in_the_window() {
        let sd = window_data(24);
        let builds = (0..3)
            .map(|i| Build {
                name: ["Tank", "AP", "AD"][i].into(),
                items: (3001..=3006).collect(),
                starters: vec![],
                win_rate: 0.5,
                ..Default::default()
            })
            .collect();
        let spells = vec![SpellPair {
            spells: [4, 6],
            rank: 1,
            games: 9_000,
        }];
        let details = Arc::new(ChampStats {
            builds,
            spells,
            ..(*pick_view().details.unwrap()).clone()
        });
        let history = many_games(30, 12);
        let base = || View {
            sd: Some(sd.clone()),
            details: Some(details.clone()),
            history: history.clone(),
            ..pick_view()
        };
        // Champ select: a bench of `n`, `trades` trade requests, the build, and the line about the item sets.
        let select = |n: i64, trades: i64| View {
            phase: Phase::ChampSelect,
            cs: Some(champ_select(n, trades)),
            stats: Some(select_stats()),
            offer: None,
            item_sets_for: Some(1),
            ..base()
        };
        // The worst first-run card: every check needs the player, and the question about Windows.
        let error = LoadError::Both(
            Box::new(LoadError::Download(Key::LoadingData)),
            Box::new(LoadError::Names("tr_TR".into())),
        );
        let mut views = vec![
            (
                "ready".to_string(),
                View {
                    phase: Phase::Offline,
                    queue: None,
                    mine: None,
                    offer: None,
                    league_found: false,
                    window_mode: Some(0),
                    ocr_probe: Some(false),
                    error: Some(error),
                    ..base()
                },
            ),
            (
                "waiting".to_string(),
                View {
                    offer: None,
                    ..base()
                },
            ),
            ("pick".to_string(), base()),
            (
                "after".to_string(),
                View {
                    phase: Phase::PostGame,
                    offer: None,
                    ..base()
                },
            ),
            (
                "other mode".to_string(),
                View {
                    queue: Some(450),
                    offer: None,
                    ..base()
                },
            ),
        ];
        for n in [1, 4, 5, 8, 10] {
            for trades in 0..=2 {
                views.push((
                    format!("champ select, bench of {n}, {trades} trade requests"),
                    select(n, trades),
                ));
            }
        }
        // The first step: no champion yet, and 0, 1, 3 or 6 offered champions.
        for n in [0, 1, 3, 6] {
            let cs = crate::lcu::ChampSelect {
                choices: (2..2 + n).collect(),
                ..Default::default()
            };
            let first_step = View {
                phase: Phase::ChampSelect,
                cs: Some(cs),
                mine: None,
                details: None,
                stats: Some(select_stats()),
                offer: None,
                item_sets_for: Some(1),
                ..base()
            };
            views.push((format!("champ select, {n} offered champions"), first_step));
        }
        let mut cut = Vec::new();
        for lang in [Lang::En, Lang::Tr] {
            for (name, v) in &views {
                for (monitor_h, max) in MONITORS {
                    // Both marks on: a pair hint makes a row of the pick card taller.
                    let mut app = test_app(
                        Settings {
                            top_highlight: true,
                            pair_hints: true,
                            ..Settings::default()
                        },
                        lang,
                    );
                    app.debug_first_run = true;
                    let (used, measured) = window_height(&mut app, v, monitor_h);
                    assert_eq!(measured, max, "the test input sets the monitor size");
                    if used > max {
                        cut.push(format!(
                            "{lang:?} {name}, max height {max}: the content needs {used:.1} points"
                        ));
                    }
                    // A page that scrolls fills the window, so the footer sits at the bottom.
                    if app.main_h.is_some_and(|h| h > room_of(max)) {
                        assert!(
                            used >= max - 2.0,
                            "{lang:?} {name}, max height {max}: the page scrolls in {used:.1} points"
                        );
                    }
                }
            }
        }
        assert!(cut.is_empty(), "the window cuts the footer: {cut:#?}");
        // The fullest champ select needs the scroll area on the small screen, and the shortest one does not.
        let mut app = test_app(Settings::default(), Lang::Tr);
        let (used, max) = window_height(&mut app, &select(10, 2), 600.0);
        assert!(
            app.main_h.is_some_and(|h| h > room_of(max)) && used <= max && used >= max - 2.0,
            "{used:.1} of {max}, page {:?}",
            app.main_h
        );
        let mut app = test_app(Settings::default(), Lang::Tr);
        let (used, max) = window_height(&mut app, &select(1, 0), 760.0);
        assert!(
            app.main_h.is_some_and(|h| h <= room_of(max)) && used < max - 20.0,
            "{used:.1} of {max}, page {:?}",
            app.main_h
        );
    }

    /// The room for the page in a window of height `max`: what the title bar, the footer
    /// and the spacing (6 points, from `theme::setup`) leave. See `App::page_height`.
    fn room_of(max: f32) -> f32 {
        max - TITLE_H - FOOTER_GAP - 1.0 - FOOTER_H - 2.0 * 6.0
    }

    /// The inside of a card (`kit::group`): the 272 points of the content, less the 12
    /// points of margin and the 1 point edge at each side. The bench card test below
    /// checks it against the painted grid.
    const CARD_INNER: f32 = MAIN_W - 2.0 * MARGIN - 24.0 - 2.0;

    #[test]
    fn a_bench_of_ten_fits_in_two_rows_of_five_and_the_best_swap_is_the_marked_cell() {
        // The card is 272 points of content, less the margins and the edge of the group: 246.
        assert_eq!(CARD_INNER, 246.0);
        let width = CARD_INNER;
        let grid = bench_grid(width, 10);
        let mut rows: Vec<i32> = grid.iter().map(|r| r.top().round() as i32).collect();
        rows.dedup();
        assert_eq!(rows.len(), 2, "{grid:?}");
        assert_eq!(BENCH_COLS, 5);
        assert!((36.0..=44.0).contains(&BENCH_ICON));
        // Every icon stays inside the card. The gaps are even, across and down, and leave room for the glow.
        for r in &grid {
            assert!(r.left() >= 0.0 && r.right() <= width + 0.01, "{r:?}");
        }
        let across = grid[1].left() - grid[0].right();
        let down = grid[BENCH_COLS].top() - grid[0].bottom();
        assert!(
            (across - down).abs() < 0.01 && across >= 8.0,
            "{across} {down}"
        );
        assert!((grid[BENCH_COLS - 1].right() - width).abs() < 0.01);
        // Five in the first row, five under them, and the grid is two icons and one gap tall.
        assert_eq!(grid.iter().filter(|r| r.top() == 0.0).count(), 5);
        for i in 0..BENCH_COLS {
            assert_eq!(grid[i].left(), grid[i + BENCH_COLS].left(), "{i}");
            assert_eq!(grid[i].size(), Vec2::splat(BENCH_ICON));
        }
        assert!((grid[9].bottom() - (2.0 * BENCH_ICON + across)).abs() < 0.01);
        // Five to a row: six takes two rows, five takes one.
        assert_eq!(bench_grid(width, 5).last().map(|r| r.top()), Some(0.0));
        assert!(bench_grid(width, 6)[5].top() > 0.0);

        // The cells keep the order of `bench_order`, and only the champion of `best_swap` is marked.
        let row = |tier: u8, win_rate: f32, games: u32| {
            Some(ChampRow {
                win_rate,
                games,
                tier,
            })
        };
        let bench = vec![
            (10, None),
            (11, row(3, 0.50, 5_000)),
            (12, row(1, 0.55, 5_000)),
            (13, row(0, 0.61, 120)),
            (14, row(1, 0.57, 5_000)),
            (15, row(2, 0.53, 5_000)),
            (16, row(4, 0.49, 5_000)),
            (17, row(5, 0.44, 5_000)),
            (18, None),
            (19, row(2, 0.52, 5_000)),
        ];
        let mine = row(4, 0.50, 5_000);
        let cells = bench_cells(bench.clone(), mine, 300);
        let mut sorted = bench.clone();
        sorted.sort_by(|a, b| bench_order(a.1, b.1));
        assert_eq!(
            cells.iter().map(|c| c.id).collect::<Vec<_>>(),
            sorted.iter().map(|b| b.0).collect::<Vec<_>>()
        );
        let marked: Vec<i64> = cells.iter().filter(|c| c.best).map(|c| c.id).collect();
        assert_eq!(marked, vec![14]);
        assert_eq!(best_swap(mine, &sorted, 300), Some(14));
        // The S+ champion with 120 games comes first, but a small sample is no swap.
        assert_eq!(cells[0].id, 13);
        // Without your stats, or without a better tier, nothing is marked.
        assert!(
            bench_cells(bench.clone(), None, 300)
                .iter()
                .all(|c| !c.best)
        );
        assert!(
            bench_cells(bench, row(0, 0.70, 9_000), 300)
                .iter()
                .all(|c| !c.best)
        );
    }

    /// The rects of a shape list that `pick` takes, in paint order.
    fn rects_where(
        shapes: &[egui::epaint::ClippedShape],
        pick: impl Fn(&egui::epaint::RectShape) -> bool,
    ) -> Vec<Rect> {
        fn walk(
            shape: &egui::Shape,
            pick: &dyn Fn(&egui::epaint::RectShape) -> bool,
            out: &mut Vec<Rect>,
        ) {
            match shape {
                egui::Shape::Rect(r) if pick(r) => out.push(r.rect),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, pick, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|c| walk(&c.shape, &pick, &mut out));
        out
    }

    /// An app with a receiver for the calls it sends to the worker.
    fn test_app_rx(settings: Settings, lang: Lang) -> (App, std::sync::mpsc::Receiver<Cmd>) {
        let (cmd, sent) = std::sync::mpsc::channel();
        let (icon, _) = std::sync::mpsc::channel();
        let (want, _) = std::sync::mpsc::channel();
        let h = Handles {
            shared: Arc::new(std::sync::Mutex::new(worker::Shared::default())),
            cmd,
            icon,
            want,
        };
        (App::assemble(h, settings, None, lang, None), sent)
    }

    /// A champ select where Garen (C) is yours, Lux (S) is the best swap, and Teemo has no stats.
    fn bench_view() -> View {
        let mut sd = StaticData::default();
        for (id, name) in [
            (1, "Garen"),
            (2, "Lux"),
            (3, "Jinx"),
            (4, "Sion"),
            (5, "Galio"),
            (6, "Yasuo"),
            (7, "Teemo"),
        ] {
            sd.champions.insert(
                id,
                Champion {
                    id,
                    name: name.into(),
                    alias: name.into(),
                    icon: String::new(),
                },
            );
        }
        let row = |tier: u8, win_rate: f32| ChampRow {
            win_rate,
            games: 5_000,
            tier,
        };
        let stats = [
            (1, row(4, 0.50)),
            (2, row(1, 0.56)),
            (3, row(2, 0.54)),
            (4, row(2, 0.53)),
            (5, row(3, 0.51)),
            (6, row(3, 0.50)),
        ];
        let cs = crate::lcu::ChampSelect {
            my_champion: 1,
            bench: vec![7, 6, 5, 4, 3, 2],
            bench_enabled: true,
            ..Default::default()
        };
        View {
            phase: Phase::ChampSelect,
            cs: Some(cs),
            sd: Some(Arc::new(sd)),
            stats: Some(Arc::new(crate::data::Stats::of_champions(
                stats.into_iter().collect(),
            ))),
            offer: None,
            ..pick_view()
        }
    }

    #[test]
    fn the_bench_card_glows_around_the_best_swap_and_names_it_on_a_button_under_the_grid() {
        let v = bench_view();
        // The button says what a click does. The whole "Best swap" line is its tooltip, not a painted text.
        for (lang, button, line) in [
            (
                Lang::Tr,
                "Lux (S) ile değiştir",
                "En iyi değişim: Lux (S) · seninki Garen (C)",
            ),
            (
                Lang::En,
                "Swap to Lux (S)",
                "Best swap: Lux (S) · yours is Garen (C)",
            ),
        ] {
            let app = test_app(Settings::default(), lang);
            let ctx = egui::Context::default();
            theme::setup(&ctx);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    pos2(0.0, 0.0),
                    vec2(MAIN_W - 2.0 * MARGIN, 600.0),
                )),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| app.bench_card(ui, &v));
            let texts = painted_text(&out.shapes);
            assert!(texts.iter().any(|t| t == button), "{lang:?}: {texts:?}");
            assert!(
                !texts
                    .iter()
                    .any(|t| t == line || t.starts_with("Best swap") || t.starts_with("En iyi")),
                "{lang:?}: the line is the tooltip now: {texts:?}"
            );
            assert_eq!(
                lang.fmt(Key::BenchBestSwap, &[&"Lux (S)", &"Garen (C)"]),
                line
            );
            // The badges: the tiers of the bench, and a muted dash for Teemo.
            for tier in ["S", "A", "B", "—"] {
                assert!(
                    texts.iter().any(|t| t == tier),
                    "{lang:?} {tier}: {texts:?}"
                );
            }
            // The icons (grey tiles: the test has no image) in the order of the bench: Lux (S) first.
            let icons = rects_where(&out.shapes, |r| {
                r.fill == SURFACE_2 && (r.rect.width() - BENCH_ICON).abs() < 0.01
            });
            assert_eq!(icons.len(), 6, "{lang:?}: {icons:?}");
            // The glow: six rings around one icon. The first has the color at 0.6 and sits 1 point out;
            // each next ring is 1 point further out and fainter. It is Lux.
            let factors = [0.6, 0.22, 0.14, 0.09, 0.05, 0.025];
            let first = rects_where(&out.shapes, |r| r.stroke.color == GREEN.gamma_multiply(0.6));
            assert_eq!(first.len(), 1, "{lang:?}: {first:?}");
            assert!(
                (first[0].shrink(1.0).min - icons[0].min).length() < 0.01,
                "{lang:?}: {first:?} {icons:?}"
            );
            for (i, f) in factors.into_iter().enumerate() {
                let ring = rects_where(&out.shapes, |r| r.stroke.color == GREEN.gamma_multiply(f));
                assert_eq!(ring.len(), 1, "{lang:?} ring {i}: {ring:?}");
                assert!(
                    (ring[0].shrink(i as f32 + 1.0).min - icons[0].min).length() < 0.01,
                    "{lang:?} ring {i}: {ring:?} {:?}",
                    icons[0]
                );
            }
            // The button is as wide as the card and 32 points high, with the green fill at 0.18.
            let buttons = rects_where(&out.shapes, |r| {
                r.fill == GREEN.gamma_multiply(SWAP_BUTTON_REST)
            });
            assert_eq!(buttons.len(), 1, "{lang:?}: {buttons:?}");
            assert!(
                (buttons[0].width() - CARD_INNER).abs() < 0.01
                    && (buttons[0].height() - 32.0).abs() < 0.01,
                "{lang:?}: {:?}",
                buttons[0]
            );
            assert!(
                buttons[0].top() > icons[5].bottom(),
                "{lang:?}: the button sits under the grid"
            );
            // Five icons in the first row, the rest in the second.
            assert!(
                icons[..5].iter().all(|r| r.top() == icons[0].top())
                    && icons[5].top() > icons[0].bottom()
            );
            // The first row fills the inside of the card, so the other tests use its real width.
            assert!(
                (icons[4].right() - icons[0].left() - CARD_INNER).abs() < 0.01,
                "{lang:?}: {icons:?}"
            );
            out.drop_without_applying_deltas();
        }
    }

    #[test]
    fn a_click_on_the_swap_button_asks_for_the_best_swap_and_nothing_else_does() {
        let v = bench_view();
        let (app, sent) = test_app_rx(Settings::default(), Lang::En);
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(MAIN_W - 2.0 * MARGIN, 600.0));
        let frame = |i: usize, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                time: Some(i as f64 / 10.0),
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| app.bench_card(ui, &v));
            let button = rects_where(&out.shapes, |r| {
                r.fill.a() > 0
                    && r.fill.g() > r.fill.r()
                    && (r.rect.height() - SWAP_BUTTON_H).abs() < 0.01
                    && r.rect.width() > 200.0
            });
            out.drop_without_applying_deltas();
            button
        };
        // Painting alone, and a mouse that only moves over the button, send nothing.
        let found = frame(0, vec![]);
        assert_eq!(found.len(), 1, "{found:?}");
        let at = found[0].center();
        frame(1, vec![egui::Event::PointerMoved(at)]);
        frame(2, vec![]);
        assert!(
            sent.try_recv().is_err(),
            "a frame sent a call without a click"
        );
        // A press and a release on the button ask for the swap to Lux (champion 2), once.
        let button = egui::PointerButton::Primary;
        frame(
            3,
            vec![egui::Event::PointerButton {
                pos: at,
                button,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(
            4,
            vec![egui::Event::PointerButton {
                pos: at,
                button,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(matches!(sent.try_recv(), Ok(Cmd::BenchSwap(2))));
        assert!(sent.try_recv().is_err());
        // Without a better champion on the bench there is no button.
        let flat = View {
            stats: Some(select_stats()),
            ..bench_view()
        };
        let none = View {
            cs: Some(crate::lcu::ChampSelect {
                bench: vec![9],
                ..flat.cs.clone().unwrap()
            }),
            ..flat
        };
        let app = test_app(Settings::default(), Lang::En);
        let out = ctx.run_ui(
            egui::RawInput {
                screen_rect: Some(screen),
                ..Default::default()
            },
            |ui| app.bench_card(ui, &none),
        );
        let texts = painted_text(&out.shapes);
        out.drop_without_applying_deltas();
        assert!(!texts.iter().any(|t| t.starts_with("Swap to")), "{texts:?}");
    }

    #[test]
    fn the_best_swap_line_reads_as_the_template_and_fits_with_common_names() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let width = CARD_INNER;
        for lang in [Lang::En, Lang::Tr] {
            for (theirs, yours) in [
                ("Lux (S)", "Garen (C)"),
                ("Ezreal (A)", "Malphite (B)"),
                ("Jinx (S+)", "Kassadin (D)"),
            ] {
                let parts = best_swap_parts(lang, theirs, yours);
                assert_eq!(
                    parts.concat(),
                    lang.fmt(Key::BenchBestSwap, &[&theirs, &yours])
                );
                assert_eq!((parts[1].as_str(), parts[3].as_str()), (theirs, yours));
                let w = text_width(&ctx, &parts.concat(), FontId::proportional(11.5));
                assert!(w <= width, "{lang:?} {theirs} {yours}: {w:.1} of {width}");
            }
        }
        assert_eq!(tier_text(None), "—");
        assert_eq!(
            tier_text(Some(ChampRow {
                win_rate: 0.5,
                games: 1,
                tier: 0
            })),
            "S+"
        );
        // The button under the grid says the same in fewer words: 13 point semibold, with 8 points of room at each side.
        for lang in [Lang::En, Lang::Tr] {
            for theirs in ["Lux (S)", "Ezreal (A)", "Jinx (S+)", "Kassadin (D)"] {
                let w = text_width(
                    &ctx,
                    &lang.fmt(Key::BenchSwapButton, &[&theirs]),
                    FontId::new(13.0, semibold()),
                );
                assert!(w <= width - 16.0, "{lang:?} {theirs}: {w:.1} of {width}");
            }
        }
    }

    #[test]
    fn the_item_sets_wait_for_the_data() {
        let due = |ready: bool, asked: Option<i64>| {
            item_set_due(true, Phase::ChampSelect, Some(2400), Some(7), ready, asked)
        };
        // While the data loads, nothing is asked, so no "League not found" shows.
        assert_eq!(due(false, None), None);
        // Once the data is there: once per champion.
        assert_eq!(due(true, None), Some(7));
        assert_eq!(due(true, Some(7)), None);
        assert_eq!(due(true, Some(3)), Some(7));
        // Off, another phase, another mode, or no champion: nothing.
        assert_eq!(
            item_set_due(false, Phase::ChampSelect, Some(2400), Some(7), true, None),
            None
        );
        assert_eq!(
            item_set_due(true, Phase::InGame, Some(2400), Some(7), true, None),
            None
        );
        assert_eq!(
            item_set_due(true, Phase::ChampSelect, Some(450), Some(7), true, None),
            None
        );
        assert_eq!(
            item_set_due(true, Phase::ChampSelect, None, Some(7), true, None),
            None
        );
        assert_eq!(
            item_set_due(true, Phase::ChampSelect, Some(2400), None, true, None),
            None
        );
    }

    #[test]
    fn no_item_sets_are_due_while_the_session_names_no_champion() {
        // The last game was Swain (50). A new champ select starts, and the session says champion 0.
        let picked = worker::keep_picked(Phase::ChampSelect, Some(0), None);
        let mine = worker::next_champion(Phase::ChampSelect, true, true, picked, Some(50));
        assert_eq!(mine, None);
        // The data is there (Swain's stats and the static data), and the setting is on: still nothing is asked.
        assert_eq!(
            item_set_due(true, Phase::ChampSelect, Some(2400), mine, true, None),
            None
        );
        // Also when the first pass could not read the session.
        assert_eq!(
            worker::next_champion(Phase::ChampSelect, true, false, None, Some(50)),
            None
        );
        // Even with the old Swain in the memory of the window, a session with champion 0 gives none.
        let again = worker::keep_picked(Phase::ChampSelect, Some(0), Some(50));
        assert_eq!(again, None);
        // The player takes Camille (164): the sets of this champ select are due, once.
        let picked = worker::keep_picked(Phase::ChampSelect, Some(164), picked);
        let mine = worker::next_champion(Phase::ChampSelect, false, true, picked, mine);
        assert_eq!(
            item_set_due(true, Phase::ChampSelect, Some(2400), mine, true, None),
            Some(164)
        );
        assert_eq!(
            item_set_due(true, Phase::ChampSelect, Some(2400), mine, true, Some(164)),
            None
        );
    }

    // ---- The first step of a champ select: the champions that the client offers.

    /// A champ select where the player has no champion: Camille (A, 5,200 games), Senna (no stats), and
    /// Alistar (B, 48,000 games) are offered. The win rates are strong numbers that must not show.
    fn first_step_view(choices: Vec<i64>) -> View {
        let mut sd = StaticData::default();
        for (id, name) in [
            (164, "Camille"),
            (235, "Senna"),
            (12, "Alistar"),
            (50, "Swain"),
        ] {
            sd.champions.insert(
                id,
                Champion {
                    id,
                    name: name.into(),
                    alias: name.into(),
                    icon: String::new(),
                },
            );
        }
        let row = |tier: u8, games: u32| ChampRow {
            win_rate: 0.631,
            games,
            tier,
        };
        let stats = crate::data::Stats::of_champions(
            [
                (164, row(2, 5_200)),
                (12, row(3, 48_000)),
                (50, row(1, 90_000)),
            ]
            .into_iter()
            .collect(),
        );
        let cs = crate::lcu::ChampSelect {
            my_champion: 0,
            choices,
            ..Default::default()
        };
        // `item_sets_for` and the details still name Swain of the game before.
        View {
            phase: Phase::ChampSelect,
            cs: Some(cs),
            mine: None,
            sd: Some(Arc::new(sd)),
            stats: Some(Arc::new(stats)),
            details: None,
            offer: None,
            item_sets_for: Some(50),
            ..pick_view()
        }
    }

    #[test]
    fn the_first_step_shows_the_offered_champions_with_tier_and_games_and_no_rate() {
        for lang in [Lang::En, Lang::Tr] {
            let mut app = test_app(Settings::default(), lang);
            let texts = window_texts(&mut app, &first_step_view(vec![164, 235, 12]), None);
            let has = |t: &str| texts.iter().any(|x| x == t);
            // The section. The page has no empty champion row, and the note that Hexsight picks nothing is the tooltip of the card.
            assert!(
                has(&lang.upper(lang.t(Key::SecChoose))),
                "{lang:?}: {texts:?}"
            );
            assert!(
                !has(lang.t(Key::NoChampion)) && !has(lang.t(Key::ChooseNote)),
                "{lang:?}: {texts:?}"
            );
            // The tier badges (A, a dash for no stats, B) and the games of each champion.
            for badge in ["A", "—", "B"] {
                assert!(has(badge), "{lang:?} {badge}: {texts:?}");
            }
            for games in [
                lang.games(5_200, &fmt_n(lang, 5_200)),
                lang.t(Key::NoData).to_string(),
                lang.games(48_000, &fmt_n(lang, 48_000)),
            ] {
                assert!(has(&games), "{lang:?} {games}: {texts:?}");
            }
            // No win rate and no percent sign anywhere on the page. The names show under the icons.
            for t in &texts {
                assert!(!looks_like_a_rate(t) && !t.contains('%'), "{lang:?}: {t:?}");
            }
            for name in ["Camille", "Senna", "Alistar"] {
                assert!(has(name), "{lang:?} {name}: {texts:?}");
            }
            // Each name comes before the games of its champion.
            let at = |t: &str| texts.iter().position(|x| x == t);
            assert!(
                at("Camille") < at(&lang.games(5_200, &fmt_n(lang, 5_200)))
                    && at("Alistar") < at(&lang.games(48_000, &fmt_n(lang, 48_000))),
                "{lang:?}: {texts:?}"
            );
            // Swain of the game before is gone: no name, no tier of his, no bench, no build, no "added to shop" line.
            assert!(
                !texts.iter().any(|t| t.contains("Swain")),
                "{lang:?}: {texts:?}"
            );
            for gone in [Key::SecBuild, Key::SecBench] {
                assert!(
                    !has(&lang.upper(lang.t(gone))),
                    "{lang:?} {gone:?}: {texts:?}"
                );
            }
            assert!(
                !has(lang.t(Key::ShopReady)) && !has(lang.t(Key::SpellsLabel)),
                "{lang:?}: {texts:?}"
            );
        }
    }

    #[test]
    fn the_first_step_without_an_offer_says_where_to_choose() {
        for lang in [Lang::En, Lang::Tr] {
            let mut app = test_app(Settings::default(), lang);
            // The client gave no list (an empty list, a missing route, or no session yet).
            for v in [
                first_step_view(Vec::new()),
                View {
                    cs: None,
                    ..first_step_view(Vec::new())
                },
            ] {
                let texts = window_texts(&mut app, &v, None);
                assert!(
                    texts.iter().any(|t| t == lang.t(Key::ChooseEmpty)),
                    "{lang:?}: {texts:?}"
                );
                assert!(
                    !texts.iter().any(|t| t == lang.t(Key::ChooseNote)),
                    "{lang:?}: {texts:?}"
                );
            }
        }
    }

    #[test]
    fn the_screen_goes_back_to_the_normal_champ_select_when_the_player_has_picked() {
        let sd = window_data(24);
        let details = Arc::new(ChampStats {
            builds: vec![Build {
                name: "Tank".into(),
                items: vec![],
                ..Default::default()
            }],
            ..Default::default()
        });
        let cs = crate::lcu::ChampSelect {
            choices: vec![2, 3, 4],
            ..Default::default()
        };
        let first = View {
            phase: Phase::ChampSelect,
            cs: Some(cs),
            mine: None,
            details: None,
            sd: Some(sd.clone()),
            stats: Some(select_stats()),
            offer: None,
            ..pick_view()
        };
        // The player picks champion 1: the session has it, the offer is gone, and the bench and the build show.
        let picked = View {
            cs: Some(champ_select(3, 0)),
            mine: Some(1),
            details: Some(details),
            item_sets_for: Some(1),
            ..View {
                sd: Some(sd),
                stats: Some(select_stats()),
                offer: None,
                ..pick_view()
            }
        };
        let picked = View {
            phase: Phase::ChampSelect,
            ..picked
        };
        let mut app = test_app(Settings::default(), Lang::En);
        let before = window_texts(&mut app, &first, None);
        let after = window_texts(&mut app, &picked, None);
        assert!(
            before.iter().any(|t| t == "PICK YOUR CHAMPION")
                && !before.iter().any(|t| t == "BENCH" || t == "BUILD"),
            "{before:?}"
        );
        assert!(
            !after
                .iter()
                .any(|t| t == "PICK YOUR CHAMPION" || t == lang_text(Key::ChooseNote)),
            "{after:?}"
        );
        assert!(
            after.iter().any(|t| t == "BENCH") && after.iter().any(|t| t == "BUILD"),
            "{after:?}"
        );
        // The sets are in the shop: the note sits at the right of the header "BUILD".
        assert!(
            after.iter().any(|t| t == lang_text(Key::ShopReady))
                && after.iter().any(|t| *t == glyph::CHECK.to_string()),
            "{after:?}"
        );
    }

    /// The English text of a key.
    fn lang_text(key: Key) -> &'static str {
        Lang::En.t(key)
    }

    #[test]
    fn the_first_step_icons_sit_in_even_columns_and_the_badge_stays_in_its_column() {
        let width = CARD_INNER;
        let col = width / CHOICE_COLS as f32;
        assert_eq!(CHOICE_COLS, 3);
        // Three offered champions: one row, one icon in the middle of each column.
        let grid = choice_grid(width, 3);
        assert_eq!(grid.len(), 3);
        for (i, r) in grid.iter().enumerate() {
            assert_eq!(r.size(), Vec2::splat(CHOICE_ICON));
            assert_eq!(r.top(), 0.0);
            assert!(
                (r.center().x - (i as f32 + 0.5) * col).abs() < 0.01,
                "{i}: {r:?}"
            );
            // The badge and its ring stay inside the column, so the badges never touch.
            let ring = choice_badge(*r).expand(CHOICE_BADGE_RING);
            assert!(
                ring.right() <= (i as f32 + 1.0) * col
                    && ring.bottom() > r.bottom()
                    && ring.right() > r.right(),
                "{i}: {ring:?}"
            );
        }
        // Fewer than three keep their columns (the first ones). More than three start a second row.
        assert_eq!(choice_grid(width, 1)[0], grid[0]);
        let six = choice_grid(width, 6);
        assert_eq!(six[3].left(), six[0].left());
        assert_eq!(six[3].top(), CHOICE_ROW_H);
        assert!(
            six[3].top() >= six[0].bottom() + 4.0 + CHOICE_TEXT_H,
            "the name and the games of the first row have room"
        );
        assert!(choice_grid(width, 0).is_empty());
    }

    #[test]
    fn the_games_under_an_offered_champion_fit_their_column() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let col = CARD_INNER / CHOICE_COLS as f32;
        for lang in [Lang::En, Lang::Tr] {
            // The count with its noun fits for the counts of a champion (up to 99,999 games). A larger count
            // drops the noun (`choice_cell`), and the count alone fits too.
            for n in [0u32, 1, 9, 999, 1_000, 5_200, 48_000, 99_999] {
                let w = text_width(
                    &ctx,
                    &lang.games(n, &fmt_n(lang, n)),
                    FontId::proportional(11.0),
                );
                assert!(w <= col, "{lang:?} {n}: {w:.1} of {col:.1}");
            }
            for n in [100_000u32, 999_999, 5_200_000, 99_000_000] {
                let w = text_width(&ctx, &fmt_n(lang, n), FontId::proportional(11.0));
                assert!(w <= col, "{lang:?} {n}: {w:.1} of {col:.1}");
            }
            let w = text_width(&ctx, lang.t(Key::NoData), FontId::proportional(11.0));
            assert!(w <= col, "{lang:?}: {w:.1} of {col:.1}");
            // The note and the empty line wrap to a few lines in the card.
            let rows = |text: &str| {
                let mut n = 0;
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                    ..Default::default()
                };
                ctx.run_ui(input, |ui| {
                    n = ui
                        .painter()
                        .layout(
                            text.to_string(),
                            FontId::proportional(12.0),
                            LABEL_2,
                            CARD_INNER,
                        )
                        .rows
                        .len()
                })
                .drop_without_applying_deltas();
                n
            };
            assert!(
                rows(lang.t(Key::ChooseNote)) <= 2 && rows(lang.t(Key::ChooseEmpty)) <= 2,
                "{lang:?}"
            );
            // The section title fits one line of the page, in capitals.
            let w = text_width(
                &ctx,
                &lang.upper(lang.t(Key::SecChoose)),
                FontId::new(11.0, semibold()),
            );
            assert!(w <= CARD_INNER, "{lang:?}: {w:.1}");
        }
    }

    #[test]
    fn a_click_on_the_badge_asks_for_the_swap_and_the_badge_stays_clear_of_the_next_glow() {
        let grid = bench_grid(CARD_INNER, 10);
        for (i, icon) in grid.iter().enumerate() {
            let ring = bench_badge(*icon).expand(BENCH_BADGE_RING);
            let hit = bench_hit(*icon);
            // The badge and its ring take the click of their icon.
            assert!(
                hit.contains_rect(ring) && hit.contains_rect(*icon),
                "{i}: {hit:?} {ring:?}"
            );
            assert!(
                ring.right() > icon.right() && ring.bottom() > icon.bottom(),
                "{i}: the badge sits on the corner"
            );
            for (j, other) in grid.iter().enumerate().filter(|(j, _)| *j != i) {
                // No point takes the click of two icons.
                assert!(!hit.intersects(bench_hit(*other)), "{i} {j}");
                // The ring never reaches into the glow of another icon (the best swap).
                assert!(
                    !ring.intersects(other.expand(kit::GLOW_REACH)),
                    "{i} {j}: {ring:?} {other:?}"
                );
            }
        }
    }

    #[test]
    fn a_declined_second_pass_asks_for_the_next_frame_at_once() {
        let input = || egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(300.0, 300.0))),
            ..Default::default()
        };
        // Two passes: egui lays the frame out again.
        let ctx = egui::Context::default();
        let mut passes = 0;
        ctx.run_ui(input(), |ui| {
            passes += 1;
            if passes == 1 {
                lay_out_again(ui.ctx(), "test");
            }
        })
        .drop_without_applying_deltas();
        assert_eq!(passes, 2);
        // One pass only: egui declines the second pass, so the next frame is due at once.
        let ctx = egui::Context::default();
        ctx.options_mut(|o| o.max_passes = 1.try_into().unwrap());
        // The first frames of a new context repaint anyway. After them, nothing is due.
        for _ in 0..10 {
            ctx.run_ui(input(), |_| {}).drop_without_applying_deltas();
            if !ctx.has_requested_repaint() {
                break;
            }
        }
        assert!(!ctx.has_requested_repaint(), "{:?}", ctx.repaint_causes());
        let mut passes = 0;
        ctx.run_ui(input(), |ui| {
            passes += 1;
            lay_out_again(ui.ctx(), "test");
        })
        .drop_without_applying_deltas();
        assert_eq!(passes, 1);
        assert!(ctx.has_requested_repaint());
    }

    #[test]
    fn the_longest_reading_language_fits_beside_its_label() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        for lang in [Lang::En, Lang::Tr] {
            for tag in ["en-US", "en-GB", "en-AU", "tr-TR"] {
                let label = text_width(
                    &ctx,
                    lang.t(Key::ReadOcrLanguage),
                    FontId::proportional(12.5),
                );
                let value = text_width(&ctx, &lang.language_name(tag), FontId::proportional(12.5));
                // A row has the label at the left, the value at the right, and a gap of 8 between.
                assert!(
                    label + 8.0 + value <= CARD_INNER,
                    "{lang:?} {tag}: {:.1} of {CARD_INNER}",
                    label + 8.0 + value
                );
            }
        }
    }

    #[test]
    fn only_a_champion_without_stats_at_the_source_says_no_data() {
        assert_eq!(stats_wait_text(Some(NoStats::Missing)), Key::NoDataChampion);
        // A failed download is no final answer: the card says that Hexsight tries again.
        assert_eq!(stats_wait_text(Some(NoStats::Transient)), Key::HeroRetrying);
        assert_eq!(stats_wait_text(None), Key::HeroLoading);
        // The line of a new try fits the build card on one line in both languages.
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        for lang in [Lang::En, Lang::Tr] {
            let w = text_width(&ctx, lang.t(Key::HeroRetrying), FontId::proportional(11.5));
            assert!(w <= CARD_INNER, "{lang:?}: {w:.1} of {CARD_INNER}");
        }
    }

    // ---- The button "Fix", the update note, the feedback link, the panel switch, and the spell card.

    /// Every text that a window page paints, after some frames. `scroll` is the offset of a long page.
    fn window_texts(app: &mut App, v: &View, scroll: Option<f32>) -> Vec<String> {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        app.debug_scroll = scroll;
        let mut texts = Vec::new();
        for i in 0..6 {
            let input = egui::RawInput {
                time: Some(i as f64 / 10.0),
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(MAIN_W, 680.0))),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| {
                app.window_ui(ui, v);
            });
            texts = painted_text(&out.shapes);
            out.drop_without_applying_deltas();
        }
        texts
    }

    #[test]
    fn the_fix_button_works_only_with_a_client_and_only_once_in_a_while() {
        use std::time::Duration;
        // A client runs and nothing was clicked: the button works.
        for phase in [
            Phase::Idle,
            Phase::Lobby,
            Phase::ChampSelect,
            Phase::InGame,
            Phase::PostGame,
        ] {
            assert_eq!(
                fix_label(phase, None),
                (Key::FixBorderless, true),
                "{phase:?}"
            );
        }
        // No client: the button says to open League, and does nothing.
        assert_eq!(fix_label(Phase::Offline, None), (Key::FixOpenLeague, false));
        assert_eq!(
            fix_label(Phase::Offline, Some(Duration::from_secs(60))),
            (Key::FixOpenLeague, false)
        );
        // After a click it is off for FIX_PAUSE.
        assert_eq!(
            fix_label(Phase::Lobby, Some(Duration::from_millis(100))),
            (Key::FixBorderless, false)
        );
        assert_eq!(
            fix_label(Phase::Lobby, Some(FIX_PAUSE - Duration::from_millis(1))),
            (Key::FixBorderless, false)
        );
        assert_eq!(
            fix_label(Phase::Lobby, Some(FIX_PAUSE)),
            (Key::FixBorderless, true)
        );
    }

    #[test]
    fn the_fix_is_sent_only_by_a_click_and_once() {
        let (cmd, sent) = std::sync::mpsc::channel();
        let (icon, _) = std::sync::mpsc::channel();
        let (want, _) = std::sync::mpsc::channel();
        let h = Handles {
            shared: Arc::new(std::sync::Mutex::new(worker::Shared::default())),
            cmd,
            icon,
            want,
        };
        let mut app = App::assemble(h, Settings::default(), None, Lang::En, None);
        let v = View {
            phase: Phase::Lobby,
            offer: None,
            window_mode: Some(0),
            ..pick_view()
        };
        // Painting the first-run card, the Settings page, and the footer sends nothing.
        window_texts(&mut app, &v, None);
        app.page = Page::Settings;
        app.tools_open = true;
        window_texts(&mut app, &v, None);
        assert!(
            sent.try_recv().is_err(),
            "a frame sent a call without a click"
        );
        // The click sends one call to the worker, and the button stays off.
        app.fix_borderless(&egui::Context::default());
        assert!(matches!(sent.try_recv(), Ok(Cmd::FixBorderless)));
        assert!(sent.try_recv().is_err());
        assert_eq!(
            fix_label(Phase::Lobby, app.fix_clicked.map(|t| t.elapsed())),
            (Key::FixBorderless, false)
        );
    }

    #[test]
    fn the_fix_shows_next_to_the_fullscreen_warning_in_the_first_run_card_and_the_settings() {
        for lang in [Lang::En, Lang::Tr] {
            let fix = lang.t(Key::FixBorderless);
            let open = lang.t(Key::FixOpenLeague);
            let card = |phase: Phase, mode: Option<i32>| {
                let v = View {
                    phase,
                    queue: None,
                    mine: None,
                    offer: None,
                    window_mode: mode,
                    ..pick_view()
                };
                let mut app = test_app(Settings::default(), lang);
                window_texts(&mut app, &v, None)
            };
            // Fullscreen: the warning and the button. Borderless, Windowed, or unknown: no button.
            let texts = card(Phase::Idle, Some(0));
            assert!(texts.iter().any(|t| t == fix), "{lang:?}: {texts:?}");
            assert!(
                texts.iter().any(|t| t == lang.t(Key::CheckBorderlessNo)),
                "{lang:?}: {texts:?}"
            );
            for mode in [Some(2), Some(1), None] {
                let texts = card(Phase::Idle, mode);
                assert!(
                    !texts.iter().any(|t| t == fix || t == open),
                    "{lang:?} {mode:?}: {texts:?}"
                );
            }
            // League closed: the button says to open League.
            let texts = card(Phase::Offline, Some(0));
            assert!(
                texts.iter().any(|t| t == open) && !texts.iter().any(|t| t == fix),
                "{lang:?}: {texts:?}"
            );
            // The Settings page shows it with the card reading rows.
            let v = View {
                phase: Phase::Idle,
                offer: None,
                window_mode: Some(0),
                read: ReadView {
                    window_mode: Some(0),
                    ..ReadView::default()
                },
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Settings;
            app.tools_open = true;
            assert!(
                window_texts(&mut app, &v, None).iter().any(|t| t == fix),
                "{lang:?}"
            );
            // The warning banner at the top of the page has the fix too, so it shows with the tools folded.
            app.tools_open = false;
            let folded = window_texts(&mut app, &v, None);
            assert!(
                folded.iter().any(|t| t == fix)
                    && folded.iter().any(|t| t == lang.t(Key::FullscreenNote)),
                "{lang:?}: folded {folded:?}"
            );
            // Without Fullscreen (and with a fine capture) there is no banner and no fix.
            let calm = View {
                window_mode: Some(2),
                ..View {
                    phase: Phase::Idle,
                    offer: None,
                    ..pick_view()
                }
            };
            let texts = window_texts(&mut app, &calm, None);
            assert!(
                !texts
                    .iter()
                    .any(|t| t == fix || t == lang.t(Key::FullscreenNote)),
                "{lang:?}: {texts:?}"
            );
        }
    }

    #[test]
    fn a_bad_capture_or_fullscreen_shows_a_warning_first_on_the_settings_page_and_the_waiting_screen()
     {
        for lang in [Lang::En, Lang::Tr] {
            let hint = |h| capture_hint(lang, h).unwrap().to_string();
            let fix = lang.t(Key::FixBorderless);
            let has = |texts: &[String], t: &str| texts.iter().any(|x| x == t);
            // A black capture: the hint shows on both pages, and there is no Fix button (League is not in Fullscreen).
            let black = View {
                phase: Phase::Idle,
                offer: None,
                capture: CaptureHealth::Black,
                window_mode: Some(2),
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Settings;
            let texts = window_texts(&mut app, &black, None);
            assert!(
                has(&texts, &hint(CaptureHealth::Black)) && !has(&texts, fix),
                "{lang:?}: {texts:?}"
            );
            // The banner comes before the first group, GENERAL.
            let at = |texts: &[String], t: &str| texts.iter().position(|x| x == t);
            assert!(
                at(&texts, &hint(CaptureHealth::Black))
                    < at(&texts, &lang.upper(lang.t(Key::GroupGeneral))),
                "{lang:?}: {texts:?}"
            );
            let waiting = View {
                phase: Phase::InGame,
                capture: CaptureHealth::Frozen,
                ..black
            };
            let mut app = test_app(Settings::default(), lang);
            let texts = window_texts(&mut app, &waiting, None);
            assert!(
                has(&texts, &hint(CaptureHealth::Frozen)),
                "{lang:?}: {texts:?}"
            );
            assert!(
                at(&texts, &hint(CaptureHealth::Frozen))
                    < at(&texts, &lang.upper(lang.t(Key::SecNextItems)))
                        .or(at(&texts, &lang.upper(lang.t(Key::SecAugments)))),
                "{lang:?}: {texts:?}"
            );
            // Fullscreen on the waiting screen: the note and the Fix button.
            let fullscreen = View {
                capture: CaptureHealth::Fine,
                window_mode: Some(0),
                ..waiting
            };
            let texts = window_texts(&mut app, &fullscreen, None);
            assert!(
                has(&texts, lang.t(Key::FullscreenNote)) && has(&texts, fix),
                "{lang:?}: {texts:?}"
            );
            // A fine capture and another window mode: nothing. The two shots with and without the fault differ.
            let fine = View {
                window_mode: Some(2),
                ..fullscreen
            };
            let calm = window_texts(&mut app, &fine, None);
            assert!(
                !has(&calm, lang.t(Key::FullscreenNote))
                    && !has(&calm, fix)
                    && !has(&calm, &hint(CaptureHealth::Frozen)),
                "{lang:?}: {calm:?}"
            );
            assert_ne!(calm, texts, "{lang:?}");
            // The banner on the waiting screen is the capture hint of the overlay, in the main window.
            let mut app = test_app(Settings::default(), lang);
            let both = View {
                capture: CaptureHealth::Black,
                window_mode: Some(0),
                ..fine
            };
            let texts = window_texts(&mut app, &both, None);
            assert!(
                has(&texts, &hint(CaptureHealth::Black))
                    && !has(&texts, lang.t(Key::FullscreenNote))
                    && has(&texts, fix),
                "{lang:?}: the capture hint comes first: {texts:?}"
            );
        }
    }

    #[test]
    fn the_fix_texts_fit_their_places() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let button = |lang: Lang, key: Key| {
            text_width(&ctx, lang.t(key), FontId::new(12.5, semibold())) + 24.0
        };
        for lang in [Lang::En, Lang::Tr] {
            for key in [Key::FixBorderless, Key::FixOpenLeague] {
                // The first-run card indents the button by 24 points under the check rows.
                assert!(
                    24.0 + button(lang, key) <= CARD_INNER,
                    "{lang:?} {key:?}: {:.1}",
                    button(lang, key)
                );
                // The overlay note card has OVERLAY_HINT_W - 8 points, 10 points of room at each side.
                assert!(
                    button(lang, key) <= OVERLAY_HINT_W - 8.0 - 20.0,
                    "{lang:?} {key:?}"
                );
            }
            // The results are toasts. They name no file and no engine, and they wrap inside the window (toast width 244).
            for key in [Key::FixDone, Key::FixStill, Key::FixSent] {
                let text = lang.t(key);
                assert!(
                    !text.contains("game.cfg") && !text.contains("LCU") && !text.contains("{0}"),
                    "{lang:?} {key:?}"
                );
                let mut rows = 0;
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                    ..Default::default()
                };
                ctx.run_ui(input, |ui| {
                    rows = ui
                        .painter()
                        .layout(
                            text.to_string(),
                            FontId::new(12.5, semibold()),
                            LABEL,
                            MAIN_W - 56.0,
                        )
                        .rows
                        .len()
                })
                .drop_without_applying_deltas();
                assert!(rows <= 4, "{lang:?} {key:?}: {rows} lines");
            }
        }
    }

    #[test]
    fn the_update_link_shows_in_the_footer_and_in_about_only_when_a_newer_version_exists() {
        for lang in [Lang::En, Lang::Tr] {
            let link = lang.t(Key::UpdateShort);
            let about = lang.fmt(Key::UpdateAvailable, &[&"v0.2.0"]);
            // The main page: the footer carries the link.
            let mut app = test_app(Settings::default(), lang);
            let with = View {
                phase: Phase::Idle,
                offer: None,
                update: Some("v0.2.0".into()),
                ..pick_view()
            };
            let texts = window_texts(&mut app, &with, None);
            assert!(texts.iter().any(|t| t == link), "{lang:?}: {texts:?}");
            let without = View {
                update: None,
                ..View {
                    phase: Phase::Idle,
                    offer: None,
                    ..pick_view()
                }
            };
            assert!(
                !window_texts(&mut app, &without, None)
                    .iter()
                    .any(|t| t == link)
            );
            // Settings: the footer link, and the line in About at the end of the page.
            app.page = Page::Settings;
            let texts = window_texts(&mut app, &with, Some(10_000.0));
            assert!(
                texts.iter().any(|t| t == link),
                "{lang:?}: footer {texts:?}"
            );
            assert!(texts.contains(&about), "{lang:?}: about {texts:?}");
            let texts = window_texts(&mut app, &without, Some(10_000.0));
            assert!(
                !texts.iter().any(|t| t == link || *t == about),
                "{lang:?}: {texts:?}"
            );
        }
        // The Turkish link is the words of the owner.
        assert_eq!(Lang::Tr.t(Key::UpdateShort), "Yeni sürüm var");
    }

    #[test]
    fn the_footer_with_the_update_link_keeps_its_texts_apart() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        for lang in [Lang::En, Lang::Tr] {
            // The source text of the footer, the longest patch name, and the link must share the 272 points.
            let left = lang.fmt(Key::FooterSource, &[&"aramkit", &"16.21"]);
            let w = text_width(&ctx, &left, FontId::proportional(11.0))
                + 12.0
                + text_width(&ctx, lang.t(Key::UpdateShort), FontId::proportional(11.0));
            assert!(w <= MAIN_W - 2.0 * MARGIN, "{lang:?}: {w:.1}");
            // The Settings footer: "Hexsight v0.1.0" and the link.
            let w = text_width(
                &ctx,
                &format!("Hexsight v{}", update::VERSION),
                FontId::proportional(11.0),
            ) + 12.0
                + text_width(&ctx, lang.t(Key::UpdateShort), FontId::proportional(11.0));
            assert!(w <= MAIN_W - 2.0 * MARGIN, "{lang:?}: {w:.1}");
        }
    }

    #[test]
    fn the_update_setting_is_on_by_default_and_old_files_keep_it_on() {
        assert!(Settings::default().check_updates);
        let old: Settings = serde_json::from_str(r#"{"language": "tr"}"#).unwrap();
        assert!(old.check_updates);
        let off: Settings = serde_json::from_str(r#"{"check_updates": false}"#).unwrap();
        assert!(!off.check_updates);
    }

    #[test]
    fn the_feedback_link_holds_the_version_windows_and_settings_and_no_personal_data() {
        use crate::cache::Place;
        // A settings file with everything a player has: a window place, a saved locale, other hotkeys.
        let settings = Settings {
            pos: Some([123.0, 456.0]),
            overlay_pos: Some([77.0, 88.0]),
            overlay_key: 0x70,
            last_locale: "tr_TR".into(),
            top_highlight: true,
            ..Settings::default()
        };
        let f = feedback_of(&settings, Place::Local, 26200);
        let url = update::issue_url("someone/hexsight", &f);
        let body = update::issue_body(&f);
        assert!(
            url.starts_with("https://github.com/someone/hexsight/issues/new?"),
            "{url}"
        );
        assert!(
            body.contains(&format!("**Hexsight version:** {}", update::VERSION)),
            "{body}"
        );
        assert!(body.contains("Windows 11 (build 26200)"), "{body}");
        // The main settings.
        for line in [
            "- Language: auto",
            "- Highlight top-tier cards: on",
            "- Show pair hints: off",
            "- Hide the panel from recordings and screenshots: on",
            "- Check for new versions: on",
            "- Portable mode: off",
        ] {
            assert!(body.contains(line), "{line}: {body}");
        }
        // No place of a window, no locale of the client, no hotkey, no path, no Riot ID.
        let lower = body.to_lowercase();
        for private in [
            "123", "456", "tr_tr", "77", "88", "f1", "c:\\", "appdata", "users", "puuid",
            "riot id", "lockfile",
        ] {
            assert!(
                !lower.contains(private),
                "the text holds {private:?}: {body}"
            );
        }
        // The link follows a switch: the hidden panel is on when the file setting is false.
        let shown = Settings {
            overlay_in_captures: true,
            ..Settings::default()
        };
        assert!(
            update::issue_body(&feedback_of(&shown, Place::Portable, 19045))
                .contains("- Hide the panel from recordings and screenshots: off")
        );
        assert!(
            update::issue_body(&feedback_of(&shown, Place::Portable, 19045))
                .contains("- Portable mode: on")
        );
    }

    #[test]
    fn the_settings_page_has_the_panel_switch_the_update_switch_and_the_feedback_button() {
        for lang in [Lang::En, Lang::Tr] {
            let v = View {
                phase: Phase::Idle,
                offer: None,
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Settings;
            let top = window_texts(&mut app, &v, None);
            let has = |texts: &[String], key: Key| texts.iter().any(|t| t == lang.t(key));
            assert!(
                has(&top, Key::SetHideInCaptures) && has(&top, Key::SetHideInCapturesNote),
                "{lang:?}: {top:?}"
            );
            // The switch sits in the Panel group, above the hotkeys.
            let place = |key: Key| top.iter().position(|t| t == lang.t(key));
            assert!(
                place(Key::SetHideInCaptures) > place(Key::SetAutoOverlay)
                    && place(Key::SetHideInCaptures) < place(Key::HotOverlay),
                "{lang:?}"
            );
            let end = window_texts(&mut app, &v, Some(10_000.0));
            for key in [
                Key::SetUpdateCheck,
                Key::SetUpdateCheckNote,
                Key::SendFeedback,
                Key::OpenDataFolder,
            ] {
                assert!(has(&end, key), "{lang:?} {key:?}: {end:?}");
            }
        }
        assert_eq!(Lang::Tr.t(Key::GroupOverlay), "Panel");
    }

    #[test]
    fn the_settings_page_has_no_switch_that_does_nothing() {
        // "Hide win rates" hid nothing since the item sets lost their rates. No switch, no note, no feedback line.
        for lang in [Lang::En, Lang::Tr] {
            let v = View {
                phase: Phase::Idle,
                offer: None,
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Settings;
            let all = [
                window_texts(&mut app, &v, None),
                window_texts(&mut app, &v, Some(10_000.0)),
            ]
            .concat()
            .join(
                "
",
            )
            .to_lowercase();
            for gone in [
                "hide win rates",
                "kazanma oranlarını gizle",
                "win rates are also removed",
            ] {
                assert!(!all.contains(gone), "{lang:?}: {gone}");
            }
        }
        let f = feedback_of(&Settings::default(), crate::cache::Place::Local, 26200);
        assert!(
            f.settings
                .iter()
                .all(|(name, _)| !name.to_lowercase().contains("win rate"))
        );
    }

    #[test]
    fn the_update_and_feedback_texts_fit_the_about_card() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        for lang in [Lang::En, Lang::Tr] {
            let rows = |text: &str, size: f32| {
                let mut rows = 0;
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                    ..Default::default()
                };
                ctx.run_ui(input, |ui| {
                    rows = ui
                        .painter()
                        .layout(
                            text.to_string(),
                            FontId::proportional(size),
                            LABEL_2,
                            CARD_INNER,
                        )
                        .rows
                        .len()
                })
                .drop_without_applying_deltas();
                rows
            };
            // Captions wrap inside the card. The notes stay short.
            for key in [Key::SetUpdateCheckNote, Key::SetHideInCapturesNote] {
                assert!(rows(lang.t(key), 11.5) <= 3, "{lang:?} {key:?}");
            }
            assert!(rows(lang.t(Key::SendFeedbackNote), 11.5) <= 4, "{lang:?}");
            // The switch labels use the room beside the switch (44 points and the gap), in two lines at most.
            let room = CARD_INNER - 44.0 - 6.0;
            for key in [Key::SetHideInCaptures, Key::SetUpdateCheck] {
                let mut lines = 0;
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
                    ..Default::default()
                };
                ctx.run_ui(input, |ui| {
                    lines = ui
                        .painter()
                        .layout(
                            lang.t(key).to_string(),
                            FontId::proportional(13.5),
                            LABEL,
                            room,
                        )
                        .rows
                        .len()
                })
                .drop_without_applying_deltas();
                assert!(lines <= 2, "{lang:?} {key:?}: {lines} lines");
            }
            // The link line in About fits on one line with a long tag.
            let w = text_width(
                &ctx,
                &lang.fmt(Key::UpdateAvailable, &[&"v10.20.30-beta.4"]),
                FontId::new(12.5, semibold()),
            );
            assert!(w <= CARD_INNER, "{lang:?}: {w:.1}");
        }
    }

    /// Static data with the spells Flash (4), Ghost (6) and Clarity (13), as the spell list names them.
    fn spell_data() -> Arc<StaticData> {
        let mut sd = StaticData::default();
        for (id, name) in [(4, "Flash"), (6, "Ghost"), (13, "Clarity")] {
            sd.spells.insert(
                id,
                Item {
                    name: name.into(),
                    icon: String::new(),
                },
            );
        }
        Arc::new(sd)
    }

    fn spell_stats(pairs: &[([i64; 2], u32, u32)]) -> Arc<ChampStats> {
        let spells = pairs
            .iter()
            .map(|&(spells, rank, games)| SpellPair {
                spells,
                rank,
                games,
            })
            .collect();
        Arc::new(ChampStats {
            spells,
            ..Default::default()
        })
    }

    #[test]
    fn the_spell_suggestion_is_the_most_played_pair_and_needs_both_names() {
        let sd = spell_data();
        // Rank 1 is a rare pair. The pair with the most games is Flash and Ghost.
        let cs = spell_stats(&[([4, 13], 1, 8_194), ([4, 6], 2, 184_278), ([4, 1], 3, 787)]);
        assert_eq!(spell_suggestion(&cs, &sd, 300), Some([4, 6]));
        // A pair whose spell has no name or icon is not shown: the card needs both. The next pair does not take its place.
        let unknown = spell_stats(&[([4, 99], 1, 50_000), ([4, 6], 2, 9_000)]);
        assert_eq!(spell_suggestion(&unknown, &sd, 300), None);
        // No spell list (the file did not download), no pairs, or too few games: no card.
        assert_eq!(spell_suggestion(&cs, &StaticData::default(), 300), None);
        assert_eq!(spell_suggestion(&ChampStats::default(), &sd, 300), None);
        assert_eq!(spell_suggestion(&cs, &sd, 1_000_000), None);
    }

    #[test]
    fn the_spell_card_shows_a_label_two_names_and_no_rate() {
        let sd = spell_data();
        // The pair data keeps no win rate, so the card has none to show. Even so, the text holds no number.
        let cs = spell_stats(&[([4, 13], 1, 8_194), ([4, 6], 2, 184_278)]);
        for lang in [Lang::En, Lang::Tr] {
            let app = test_app(Settings::default(), lang);
            let v = View {
                phase: Phase::ChampSelect,
                sd: Some(sd.clone()),
                details: Some(cs.clone()),
                offer: None,
                ..pick_view()
            };
            let paint = |v: &View| {
                let ctx = egui::Context::default();
                theme::setup(&ctx);
                let input = egui::RawInput {
                    screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 900.0))),
                    ..Default::default()
                };
                let out = ctx.run_ui(input, |ui| app.spells_card(ui, v));
                let texts = painted_text(&out.shapes);
                out.drop_without_applying_deltas();
                texts
            };
            let texts = paint(&v);
            assert!(
                texts.iter().any(|t| t == lang.t(Key::SpellsLabel)),
                "{lang:?}: {texts:?}"
            );
            assert!(
                texts.iter().any(|t| t == "Flash + Ghost"),
                "{lang:?}: {texts:?}"
            );
            assert!(
                !texts.iter().any(|t| t.contains("Clarity")),
                "{lang:?}: only the most played pair shows: {texts:?}"
            );
            for t in &texts {
                assert!(
                    !looks_like_a_rate(t) && !t.chars().any(|c| c.is_ascii_digit()),
                    "{lang:?}: {t:?}"
                );
            }
            // Without a pair, without the spell names, or without stats: no card at all.
            for none in [
                View {
                    details: Some(Arc::new(ChampStats::default())),
                    ..View {
                        sd: Some(sd.clone()),
                        ..pick_view()
                    }
                },
                View {
                    sd: Some(Arc::new(StaticData::default())),
                    details: Some(cs.clone()),
                    ..pick_view()
                },
                View {
                    details: None,
                    sd: Some(sd.clone()),
                    ..pick_view()
                },
            ] {
                assert!(paint(&none).is_empty(), "{lang:?}");
            }
        }
    }

    #[test]
    fn a_long_spell_pair_hides_its_names_and_keeps_label_and_icons() {
        // A pair of very long names does not fit beside the icons. The card then drops the names
        // (the tooltip has them), so no text runs into the icons.
        let mut sd = StaticData::default();
        for (id, name) in [
            (4, "İyileştirme Büyüsü Uzun Ad"),
            (6, "Tutuşturma Büyüsü Uzun Ad"),
        ] {
            sd.spells.insert(
                id,
                Item {
                    name: name.into(),
                    icon: String::new(),
                },
            );
        }
        let app = test_app(Settings::default(), Lang::Tr);
        let v = View {
            phase: Phase::ChampSelect,
            sd: Some(Arc::new(sd)),
            details: Some(spell_stats(&[([4, 6], 1, 9_000)])),
            ..pick_view()
        };
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 900.0))),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| app.spells_card(ui, &v));
        let texts = painted_text(&out.shapes);
        out.drop_without_applying_deltas();
        assert!(
            texts.contains(&Lang::Tr.t(Key::SpellsLabel).to_string()),
            "{texts:?}"
        );
        assert!(!texts.iter().any(|t| t.contains("Tutuşturma")), "{texts:?}");
    }

    #[test]
    fn the_champ_select_screen_shows_the_spell_card_between_the_bench_and_the_build() {
        let mut sd = StaticData::default();
        sd.champions.insert(
            1,
            Champion {
                id: 1,
                name: "Yasuo".into(),
                alias: "Yasuo".into(),
                icon: String::new(),
            },
        );
        sd.spells = spell_data().spells.clone();
        let details = Arc::new(ChampStats {
            builds: vec![Build {
                name: "Tank".into(),
                items: vec![],
                ..Default::default()
            }],
            ..(*spell_stats(&[([4, 6], 1, 9_000)])).clone()
        });
        let v = View {
            phase: Phase::ChampSelect,
            cs: Some(champ_select(0, 0)),
            sd: Some(Arc::new(sd)),
            details: Some(details),
            stats: Some(select_stats()),
            offer: None,
            ..pick_view()
        };
        let mut app = test_app(Settings::default(), Lang::En);
        let texts = window_texts(&mut app, &v, None);
        let place = |text: &str| texts.iter().position(|t| t == text);
        assert!(place("Flash + Ghost").is_some(), "{texts:?}");
        // The spell card comes before the section "Build".
        assert!(place("Spells") < place("BUILD"), "{texts:?}");
        // In a game (not champ select) the card is not there.
        let in_game = View {
            phase: Phase::InGame,
            offer: None,
            ..View {
                sd: v.sd.clone(),
                details: v.details.clone(),
                ..pick_view()
            }
        };
        assert!(
            !window_texts(&mut app, &in_game, None)
                .iter()
                .any(|t| t == "Flash + Ghost")
        );
    }

    // ---- The suggestions: the item card, the summary, the overlay card and the fit mark.

    /// Static data for the suggestion tests: some items, the champions of an enemy team, and an augment.
    fn suggest_data() -> Arc<StaticData> {
        let mut sd = StaticData::default();
        for (id, name) in [
            (2503, "Blackfire Torch"),
            (6653, "Liandry's Torment"),
            (3165, "Morellonomicon"),
            (3089, "Rabadon's Deathcap"),
            (4646, "Stormsurge"),
        ] {
            sd.items.insert(
                id,
                Item {
                    name: name.into(),
                    icon: String::new(),
                },
            );
        }
        for (id, name) in [
            (4, "Zed"),
            (6, "Soraka"),
            (8, "Yuumi"),
            (5, "Malphite"),
            (3, "Garen"),
            (10, "Blitzcrank"),
        ] {
            sd.champions.insert(
                id,
                Champion {
                    id,
                    name: name.into(),
                    alias: name.into(),
                    icon: String::new(),
                },
            );
        }
        for (id, name) in [
            (1, "Alpha"),
            (2, "Bravo"),
            (3, "Charlie"),
            (7, "Echo"),
            (3150, "Upgrade Stormsurge"),
        ] {
            sd.augments.insert(
                id,
                Augment {
                    name: name.into(),
                    name_en: name.into(),
                    icon: String::new(),
                },
            );
        }
        Arc::new(sd)
    }

    /// Three suggested items: a core item, an item that a rule raised, and an anti-heal item.
    fn picks() -> Vec<ItemPick> {
        vec![
            ItemPick {
                id: 2503,
                reason: Reason::Core(1),
                need: 2800,
                ready: false,
            },
            ItemPick {
                id: 6653,
                reason: Reason::VsAp(70),
                need: 600,
                ready: true,
            },
            ItemPick {
                id: 3165,
                reason: Reason::AntiHeal(2),
                need: 2850,
                ready: false,
            },
        ]
    }

    /// The advice of a game: three items and a full summary (a fed Zed, two healers, three tanks).
    fn game_advice() -> Advice {
        let fed = suggest::Fed {
            champion: 4,
            kills: 9,
            deaths: 2,
            assists: 4,
            level: 11,
        };
        Advice {
            items: picks(),
            style: "ap".into(),
            ctx: Some(GameContext {
                ap_pct: Some(70),
                fed: Some(fed),
                healers: vec![6, 8],
                tanks: vec![5, 3, 10],
                enemies: 5,
            }),
            has_gold: true,
            wanted: vec![2503, 4646],
            ..Default::default()
        }
    }

    /// A game between pick screens, with this advice and 1000 gold.
    fn waiting_view(advice: Option<Advice>) -> View {
        View {
            phase: Phase::InGame,
            offer: None,
            sd: Some(suggest_data()),
            advice: advice.map(Arc::new),
            gold: Some(1000),
            ..pick_view()
        }
    }

    /// The texts that a part of the window paints, after some frames.
    fn part_texts(app: &App, paint: impl Fn(&App, &mut Ui)) -> Vec<String> {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let mut texts = Vec::new();
        for i in 0..6 {
            let input = egui::RawInput {
                time: Some(i as f64 / 10.0),
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(MAIN_W, 900.0))),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| paint(app, ui));
            texts = painted_text(&out.shapes);
            out.drop_without_applying_deltas();
        }
        texts
    }

    /// The texts of the whole main page (no scroll), for one setting and one language.
    fn page_texts(settings: Settings, lang: Lang, v: &View) -> Vec<String> {
        let mut app = test_app(settings, lang);
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let mut texts = Vec::new();
        for i in 0..4 {
            let input = egui::RawInput {
                time: Some(i as f64 / 10.0),
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(MAIN_W, 1600.0))),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| app.main_page(ui, v));
            texts = painted_text(&out.shapes);
            out.drop_without_applying_deltas();
        }
        texts
    }

    #[test]
    fn the_suggestion_settings_default_to_items_and_summary_on_and_the_fit_mark_off() {
        let s = Settings::default();
        assert!(s.suggest_items && s.suggest_context && !s.fit_marks);
        assert!(!s.marks().fit);
        // Any one of the three keeps the player list on. With all off, the poll thread reads nothing.
        assert!(s.suggests());
        let off = Settings {
            suggest_items: false,
            suggest_context: false,
            fit_marks: false,
            ..Settings::default()
        };
        assert!(!off.suggests());
        assert!(
            Settings {
                fit_marks: true,
                ..off.clone()
            }
            .suggests()
        );
        assert!(
            Settings {
                suggest_context: true,
                ..off.clone()
            }
            .suggests()
        );
        assert!(
            Settings {
                suggest_items: true,
                ..off
            }
            .suggests()
        );
        assert!(
            Settings {
                fit_marks: true,
                ..Settings::default()
            }
            .marks()
            .fit
        );
    }

    #[test]
    fn an_old_settings_file_loads_with_the_new_defaults() {
        let old = r#"{"overlay_key":119,"scan_key":120,"top_highlight":true,"pair_hints":false,"min_games":300,"language":"tr"}"#;
        let s: Settings = serde_json::from_str(old).unwrap();
        assert!(s.suggest_items && s.suggest_context && !s.fit_marks);
        assert!(s.top_highlight && s.language == LangPref::Turkish);
        // Saved again, the file holds the three keys, and they load as they were.
        let text = serde_json::to_string(&s).unwrap();
        for key in ["suggest_items", "suggest_context", "fit_marks"] {
            assert!(text.contains(key), "{key}");
        }
        let back: Settings = serde_json::from_str(&text).unwrap();
        assert_eq!(
            (back.suggest_items, back.suggest_context, back.fit_marks),
            (true, true, false)
        );
        // A file that turned them off or on keeps its choice, and a key that is missing keeps the default.
        let own: Settings =
            serde_json::from_str(r#"{"suggest_items":false,"fit_marks":true}"#).unwrap();
        assert!(!own.suggest_items && own.suggest_context && own.fit_marks);
    }

    #[test]
    fn the_waiting_screen_shows_the_item_card_and_the_summary_in_both_languages() {
        let v = waiting_view(Some(game_advice()));
        for lang in [Lang::En, Lang::Tr] {
            // The card, the summary and their texts.
            let texts = page_texts(Settings::default(), lang, &v);
            for t in [
                lang.upper(lang.t(Key::SecNextItems)),
                lang.fmt(Key::SuggestStyle, &[&lang.style("ap")]),
                "Blackfire Torch".to_string(),
                lang.fmt(Key::ReasonCore, &[&1]),
                lang.fmt(Key::ReasonVsAp, &[&lang.pct(70.0, 0)]),
                lang.fmt(Key::ReasonAntiHeal, &[&2]),
                lang.upper(lang.t(Key::SecThisGame)),
                lang.t(Key::CtxEnemyDamage).to_string(),
                mix_text(lang, 70),
                lang.fmt(Key::CtxFed, &[&"Zed", &"9/2/4"]),
                lang.fmt(Key::Level, &[&11]),
                lang.t(Key::CtxHealers).to_string(),
                lang.t(Key::CtxTanks).to_string(),
            ] {
                assert!(
                    texts.contains(&t),
                    "{lang:?}: {t:?} is missing in {texts:?}"
                );
            }
            // The note "From build data and Hexsight's own rules" is the tooltip of the card now. The style is
            // the note at the right of the header, and the card has no title row of its own.
            let style = lang.fmt(Key::SuggestStyle, &[&lang.style("ap")]);
            assert!(
                !texts.contains(&lang.t(Key::SuggestNote).to_string()),
                "{lang:?}: {texts:?}"
            );
            assert_eq!(
                texts.iter().filter(|t| **t == style).count(),
                1,
                "{lang:?}: {texts:?}"
            );
            let place = |text: String| texts.iter().position(|t| *t == text);
            assert_eq!(
                place(style).map(|i| i - 1),
                place(lang.upper(lang.t(Key::SecNextItems))),
                "{lang:?}: the style follows its header"
            );
            // The next-pick card is one line: the level of the next pick, and no level of yours.
            assert!(
                texts.contains(&lang.fmt(Key::NextPickAt, &[&7])),
                "{lang:?}: {texts:?}"
            );
            assert!(
                !texts.contains(&lang.fmt(Key::Level, &[&7])),
                "{lang:?}: {texts:?}"
            );
            // The item cards come before the section of the augments, which come before the build.
            assert!(
                place(lang.upper(lang.t(Key::SecNextItems)))
                    < place(lang.upper(lang.t(Key::SecThisGame)))
            );
            assert!(
                place(lang.upper(lang.t(Key::SecThisGame)))
                    < place(lang.upper(lang.t(Key::SecAugments)))
            );
            assert!(
                place(lang.upper(lang.t(Key::SecAugments)))
                    < place(lang.upper(lang.t(Key::SecBuild)))
            );
            // The page fits both heights: it scrolls above the room, and the footer stays.
            for (monitor_h, max) in MONITORS {
                let mut app = test_app(Settings::default(), lang);
                let (used, measured) = window_height(&mut app, &v, monitor_h);
                assert_eq!(measured, max);
                assert!(
                    used <= max,
                    "{lang:?}, max height {max}: the content needs {used:.1} points"
                );
                if app.main_h.is_some_and(|h| h > room_of(max)) {
                    assert!(
                        used >= max - 2.0,
                        "{lang:?} max height {max}: the page scrolls in {used:.1} points"
                    );
                }
            }
        }
    }

    #[test]
    fn the_item_card_says_what_is_missing_instead_of_an_item() {
        for lang in [Lang::En, Lang::Tr] {
            let app = test_app(Settings::default(), lang);
            let say = |a: Option<Advice>| {
                part_texts(&app, |app, ui| app.items_card(ui, &waiting_view(a.clone())))
            };
            // The game data has not arrived.
            assert!(say(None).contains(&lang.t(Key::SuggestWait).to_string()));
            // The champion has no build, the slots are full, or nothing is worth a tip.
            assert!(
                say(Some(Advice {
                    no_build: true,
                    ..Default::default()
                }))
                .contains(&lang.t(Key::SuggestNoBuild).to_string())
            );
            assert!(
                say(Some(Advice {
                    slots_full: true,
                    style: "ap".into(),
                    ..Default::default()
                }))
                .contains(&lang.t(Key::SuggestFull).to_string())
            );
            let none = say(Some(Advice {
                style: "ap".into(),
                ..Default::default()
            }));
            assert!(
                none.contains(&lang.t(Key::SuggestNone).to_string())
                    && !none.contains(&lang.t(Key::SuggestNote).to_string()),
                "{none:?}"
            );
        }
    }

    #[test]
    fn the_item_card_shows_no_number_but_the_gold_chip() {
        for lang in [Lang::En, Lang::Tr] {
            let app = test_app(Settings::default(), lang);
            let v = waiting_view(Some(game_advice()));
            let texts = part_texts(&app, |app, ui| app.items_card(ui, &v));
            assert!(!texts.is_empty());
            let vs_ap = lang.fmt(Key::ReasonVsAp, &[&lang.pct(70.0, 0)]);
            for t in &texts {
                // A percent sign only in the reason of a rule, which names the enemy damage and no win rate.
                assert!(!t.contains('%') || *t == vs_ap, "{lang:?}: {t:?}");
                assert!(
                    !t.contains("pts")
                        && !t.contains("puan")
                        && !t.to_lowercase().contains("win")
                        && !t.to_lowercase().contains("kazanma"),
                    "{lang:?}: {t:?}"
                );
            }
            // The chip: gold that is missing, with a minus (1000 gold against 2800 and 2850), and a check mark for the item you can buy.
            assert!(
                texts.contains(&"-1800".to_string()) && texts.contains(&"-1850".to_string()),
                "{texts:?}"
            );
            assert!(texts.contains(&glyph::CHECK.to_string()), "{texts:?}");
            // Without gold there is no chip.
            let no_gold = Advice {
                has_gold: false,
                ..game_advice()
            };
            let texts = part_texts(&app, |app, ui| {
                app.items_card(ui, &waiting_view(Some(no_gold.clone())))
            });
            assert!(
                !texts
                    .iter()
                    .any(|t| t.starts_with('-') || *t == glyph::CHECK.to_string()),
                "{texts:?}"
            );
        }
    }

    #[test]
    fn each_setting_hides_its_card() {
        let v = waiting_view(Some(game_advice()));
        for lang in [Lang::En, Lang::Tr] {
            let texts_of = |items: bool, context: bool| {
                page_texts(
                    Settings {
                        suggest_items: items,
                        suggest_context: context,
                        ..Settings::default()
                    },
                    lang,
                    &v,
                )
            };
            let (items, summary) = (
                lang.upper(lang.t(Key::SecNextItems)),
                lang.upper(lang.t(Key::SecThisGame)),
            );
            let both = texts_of(true, true);
            assert!(both.contains(&items) && both.contains(&summary));
            let only_items = texts_of(true, false);
            assert!(
                only_items.contains(&items)
                    && !only_items.contains(&summary)
                    && !only_items.contains(&lang.t(Key::CtxEnemyDamage).to_string())
            );
            let only_summary = texts_of(false, true);
            assert!(
                !only_summary.contains(&items)
                    && only_summary.contains(&summary)
                    && !only_summary.iter().any(|t| t == "Blackfire Torch")
            );
            let neither = texts_of(false, false);
            assert!(
                !neither.contains(&items) && !neither.contains(&summary),
                "{neither:?}"
            );
            // The augments and the build stay.
            assert!(
                neither.contains(&lang.upper(lang.t(Key::SecAugments)))
                    && neither.contains(&lang.upper(lang.t(Key::SecBuild)))
            );
        }
        // A summary with nothing in it shows no card.
        let empty = waiting_view(Some(Advice {
            ctx: Some(GameContext::default()),
            ..game_advice()
        }));
        assert!(
            !page_texts(Settings::default(), Lang::En, &empty).contains(&"THIS GAME".to_string())
        );
    }

    #[test]
    fn the_pick_screen_has_no_item_card() {
        let v = View {
            advice: Some(Arc::new(game_advice())),
            ..View {
                sd: Some(suggest_data()),
                ..pick_view()
            }
        };
        assert!(v.offer.is_some());
        for lang in [Lang::En, Lang::Tr] {
            let texts = page_texts(Settings::default(), lang, &v);
            assert!(
                !texts.contains(&lang.upper(lang.t(Key::SecNextItems)))
                    && !texts.contains(&lang.upper(lang.t(Key::SecThisGame))),
                "{texts:?}"
            );
            assert!(!texts.iter().any(|t| t == "Blackfire Torch"), "{texts:?}");
        }
    }

    #[test]
    fn the_reasons_read_in_both_languages() {
        let sd = suggest_data();
        let all = [
            Reason::Core(2),
            Reason::Boots,
            Reason::Later,
            Reason::Slot(3),
            Reason::OtherBuild,
            Reason::VsAp(65),
            Reason::VsAd(80),
            Reason::AntiHeal(2),
            Reason::VsTanks(3),
            Reason::Pairs {
                augment: 3150,
                known: false,
            },
        ];
        let en: Vec<String> = all.iter().map(|r| reason_text(Lang::En, &sd, *r)).collect();
        assert_eq!(
            en,
            [
                "core item 2",
                "boots",
                "after the core",
                "common item 3",
                "in other builds",
                "vs 65% AP",
                "vs 80% AD",
                "vs 2 healers",
                "vs 3 tanks",
                "pairs with Upgrade Stormsurge"
            ]
        );
        let tr: Vec<String> = all.iter().map(|r| reason_text(Lang::Tr, &sd, *r)).collect();
        assert_eq!(
            tr,
            [
                "2. çekirdek eşya",
                "botlar",
                "çekirdekten sonra",
                "sık alınan 3. eşya",
                "diğer dizilimlerde",
                "%65 AP’ye karşı",
                "%80 AD’ye karşı",
                "2 iyileştirene karşı",
                "3 tanka karşı",
                "Upgrade Stormsurge ile uyumlu"
            ]
        );
        // An augment that Hexsight cannot name reads as "an earlier pick".
        assert_eq!(
            reason_text(
                Lang::En,
                &sd,
                Reason::Pairs {
                    augment: 99,
                    known: true
                }
            ),
            "pairs with an earlier pick"
        );
        // The tooltip says where a reason comes from: a rule, an offered augment, or build data.
        for lang in [Lang::En, Lang::Tr] {
            assert_eq!(
                reason_tip(lang, Reason::VsAp(70)),
                lang.t(Key::TipReasonRule)
            );
            assert_eq!(
                reason_tip(lang, Reason::AntiHeal(2)),
                lang.t(Key::TipReasonRule)
            );
            assert_eq!(
                reason_tip(
                    lang,
                    Reason::Pairs {
                        augment: 1,
                        known: false
                    }
                ),
                lang.t(Key::TipPairsOffered)
            );
            assert_eq!(
                reason_tip(
                    lang,
                    Reason::Pairs {
                        augment: 1,
                        known: true
                    }
                ),
                lang.t(Key::TipReasonData)
            );
            assert_eq!(
                reason_tip(lang, Reason::Core(1)),
                lang.t(Key::TipReasonData)
            );
        }
    }

    #[test]
    fn the_context_line_leaves_out_what_does_not_exist() {
        let sd = suggest_data();
        let full = game_advice().ctx.unwrap();
        assert_eq!(
            context_line(Lang::En, &sd, &full).as_deref(),
            Some("70% AP · 30% AD · Fed: Zed 9/2/4 · 2 healers")
        );
        assert_eq!(
            context_line(Lang::Tr, &sd, &full).as_deref(),
            Some("%70 AP · %30 AD · Fed: Zed 9/2/4 · 2 iyileştiren")
        );
        // One healer is no part, and a context without a mix and a fed enemy has no line.
        let few = GameContext {
            healers: vec![6],
            tanks: vec![5],
            enemies: 5,
            ..Default::default()
        };
        assert_eq!(context_line(Lang::En, &sd, &few), None);
        assert_eq!(
            context_line(
                Lang::En,
                &sd,
                &GameContext {
                    ap_pct: Some(45),
                    ..few
                }
            )
            .as_deref(),
            Some("45% AP · 55% AD")
        );
        // The summary card still has a row for one healer, and none for an empty context.
        assert!(summary_has_rows(&GameContext {
            healers: vec![6],
            ..Default::default()
        }));
        assert!(!summary_has_rows(&GameContext::default()));
    }

    #[test]
    fn the_reason_texts_fit_the_item_row_in_both_languages() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let sd = suggest_data();
        // The room of a reason: the card, less the icon, the gap, and the chip. 162 points.
        let room = CARD_INNER - ITEM_ICON - 8.0 - ITEM_CHIP_W;
        assert_eq!(room, 162.0);
        // The fixed texts with the widest numbers (a two digit share, 5 enemies).
        let worst = [
            Reason::Core(6),
            Reason::Boots,
            Reason::Later,
            Reason::Slot(6),
            Reason::OtherBuild,
            Reason::VsAp(100),
            Reason::VsAd(100),
            Reason::AntiHeal(5),
            Reason::VsTanks(5),
        ];
        for lang in [Lang::En, Lang::Tr] {
            for r in worst {
                let text = reason_text(lang, &sd, r);
                let w = text_width(&ctx, &text, FontId::proportional(11.5));
                assert!(w <= room, "{lang:?} {text:?} needs {w:.1} points of {room}");
            }
            // The item name has the same room, and the tooltip has the long form.
            assert!(text_width(&ctx, "Rabadon's Deathcap", FontId::new(13.0, semibold())) <= room);
        }
    }

    #[test]
    fn the_reason_texts_fit_an_overlay_cell() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let sd = suggest_data();
        // Three cells share the inner width with two gaps. The reason has the cell less 5 points at each side.
        let room = advice_cell_w(OVERLAY_STRIP_W - 28.0) - 10.0;
        let worst = [
            Reason::Core(6),
            Reason::Boots,
            Reason::Later,
            Reason::Slot(6),
            Reason::OtherBuild,
            Reason::VsAp(100),
            Reason::VsAd(100),
            Reason::AntiHeal(5),
            Reason::VsTanks(5),
        ];
        for lang in [Lang::En, Lang::Tr] {
            for r in worst {
                let text = reason_text(lang, &sd, r);
                let w = text_width(&ctx, &text, advice_reason_font());
                assert!(
                    w <= room,
                    "{lang:?} {text:?} needs {w:.1} points of {room:.1}"
                );
            }
        }
    }

    #[test]
    fn the_summary_rows_fit_246_points() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        for lang in [Lang::En, Lang::Tr] {
            // The damage row: the label at the left and the mix at the right, with the widest mix.
            let label = text_width(
                &ctx,
                lang.t(Key::CtxEnemyDamage),
                FontId::new(13.5, semibold()),
            );
            let mix = text_width(&ctx, &mix_text(lang, 55), FontId::proportional(11.5));
            let widest = text_width(&ctx, &mix_text(lang, 100), FontId::proportional(11.5)).max(
                text_width(&ctx, &mix_text(lang, 0), FontId::proportional(11.5)),
            );
            assert!(
                label + 12.0 + mix.max(widest) <= CARD_INNER,
                "{lang:?}: {:.1}",
                label + mix.max(widest)
            );
            // The fed row: the icon, a long champion name with a long score line, and the level.
            let fed = text_width(
                &ctx,
                &lang.fmt(Key::CtxFed, &[&"Aurelion Sol", &"15/4/12"]),
                FontId::new(12.5, semibold()),
            );
            let level = text_width(
                &ctx,
                &lang.fmt(Key::Level, &[&18]),
                FontId::proportional(11.5),
            );
            assert!(
                SUMMARY_ICON + 8.0 + fed + 8.0 + level <= CARD_INNER,
                "{lang:?}: {:.1}",
                SUMMARY_ICON + 8.0 + fed + level
            );
            // A group of icons in a row of its own: the label, the gap and three icons.
            for key in [Key::CtxHealers, Key::CtxTanks] {
                let w = text_width(&ctx, lang.t(key), FontId::proportional(11.5))
                    + 6.0
                    + SUMMARY_ICONS as f32 * (SUMMARY_ICON + 4.0);
                assert!(w <= CARD_INNER, "{lang:?} {key:?}: {w:.1}");
            }
            // The note of the item card (its tooltip now) wraps in two lines at most.
            let note = ctx.run_ui(egui::RawInput::default(), |ui| {
                let g = ui.painter().layout(
                    lang.t(Key::SuggestNote).to_string(),
                    FontId::proportional(11.0),
                    LABEL_2,
                    CARD_INNER,
                );
                assert!(g.rows.len() <= 2, "{lang:?}: {} rows", g.rows.len());
            });
            note.drop_without_applying_deltas();
        }
    }

    /// An app that paints the overlay, with the advice for these items and a player who is dead (the card shows).
    fn overlay_texts(lang: Lang, v: &View) -> (Vec<String>, Option<Vec2>) {
        let (boxes, size) = overlay_boxes(lang, v);
        (boxes.into_iter().map(|b| b.0).collect(), size)
    }

    /// Like `overlay_texts`, with the rect and the color of each text.
    fn overlay_boxes(lang: Lang, v: &View) -> (Vec<(String, Rect, Color32)>, Option<Vec2>) {
        let mut app = test_app(Settings::default(), lang);
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let mut boxes = Vec::new();
        for i in 0..4 {
            let input = egui::RawInput {
                time: Some(i as f64 / 10.0),
                screen_rect: Some(Rect::from_min_size(
                    pos2(0.0, 0.0),
                    vec2(OVERLAY_WINDOW_W, 400.0),
                )),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| app.overlay_ui(ui, v));
            boxes = painted_boxes(&out.shapes);
            out.drop_without_applying_deltas();
        }
        (boxes, app.overlay_size)
    }

    #[test]
    fn a_rule_reason_has_the_brighter_ink_in_the_advice_card_and_the_cells_are_big_enough() {
        let dead = Advice {
            me_dead: true,
            ..game_advice()
        };
        let v = waiting_view(Some(dead));
        for lang in [Lang::En, Lang::Tr] {
            let (boxes, _) = overlay_boxes(lang, &v);
            let sd = suggest_data();
            let find = |r: Reason| {
                let t = reason_text(lang, &sd, r);
                // The text is painted twice in the strip; here once. Take the last box of that text.
                boxes
                    .iter()
                    .rev()
                    .find(|b| b.0 == t)
                    .unwrap_or_else(|| panic!("{lang:?}: {t:?} is missing in {boxes:?}"))
                    .clone()
            };
            // A core item comes from the build data: grey. The two rules: bright white.
            assert_eq!(find(Reason::Core(1)).2, LABEL_2, "{lang:?}");
            assert_eq!(find(Reason::VsAp(70)).2, LABEL, "{lang:?}");
            assert_eq!(find(Reason::AntiHeal(2)).2, LABEL, "{lang:?}");
            // The reason ends 6 points above the bottom of its cell, and it is 11 points; the name is 11.5.
            let (_, reason, _) = find(Reason::VsAp(70));
            let cells: Vec<f32> = boxes
                .iter()
                .filter(|b| {
                    b.0.starts_with("Blackfire")
                        || b.0.starts_with("Liandry")
                        || b.0.starts_with("Morello")
                })
                .map(|b| b.1.top())
                .collect();
            assert_eq!(cells.len(), 3, "{lang:?}: {boxes:?}");
            let cell_top = cells[0] - 3.0;
            assert!(
                (reason.bottom() - (cell_top + ADVICE_CELL_H - ADVICE_REASON_BOTTOM)).abs() < 0.6,
                "{lang:?}: {reason:?} in a cell from {cell_top}"
            );
            // The context line is 11.5 points (before 10.5) and the header is above it.
            let line = context_line(lang, &sd, game_advice().ctx.as_ref().unwrap()).unwrap();
            let line_box = boxes
                .iter()
                .find(|b| b.0 == line)
                .expect("the context line")
                .1;
            assert!(line_box.height() > 14.0, "{lang:?}: {line_box:?}");
        }
        assert_eq!(
            (ADVICE_CELL_H, advice_name_font(), advice_reason_font()),
            (
                56.0,
                FontId::new(11.5, semibold()),
                FontId::new(11.0, semibold())
            )
        );
    }

    #[test]
    fn a_black_capture_or_fullscreen_shows_in_the_advice_card_too() {
        let dead = Advice {
            me_dead: true,
            ..game_advice()
        };
        for lang in [Lang::En, Lang::Tr] {
            let calm = waiting_view(Some(dead.clone()));
            let (texts, calm_size) = overlay_texts(lang, &calm);
            let black = View {
                capture: CaptureHealth::Black,
                ..waiting_view(Some(dead.clone()))
            };
            let (with_black, black_size) = overlay_texts(lang, &black);
            // The card is the advice card (it shows the items), and it carries the hint of the bad capture.
            assert!(
                with_black.iter().any(|t| t.contains("Blackfire")),
                "{lang:?}: {with_black:?}"
            );
            assert!(
                with_black.contains(
                    &capture_hint(lang, CaptureHealth::Black)
                        .unwrap()
                        .to_string()
                ),
                "{lang:?}: {with_black:?}"
            );
            assert!(
                !texts.contains(
                    &capture_hint(lang, CaptureHealth::Black)
                        .unwrap()
                        .to_string()
                ),
                "{lang:?}"
            );
            // The card grows to make room for it.
            assert!(
                black_size.unwrap().y > calm_size.unwrap().y + 10.0,
                "{lang:?}: {black_size:?} against {calm_size:?}"
            );
            // Fullscreen shows its note the same way.
            let full = View {
                window_mode: Some(0),
                ..waiting_view(Some(dead.clone()))
            };
            let (with_full, _) = overlay_texts(lang, &full);
            assert!(
                with_full.contains(&lang.t(Key::FullscreenNote).to_string()),
                "{lang:?}: {with_full:?}"
            );
            assert!(
                with_full.iter().any(|t| t.contains("Blackfire")),
                "{lang:?}"
            );
        }
    }

    #[test]
    fn the_overlay_advice_card_is_340_wide_and_at_most_132_high() {
        let dead = Advice {
            me_dead: true,
            ..game_advice()
        };
        let v = waiting_view(Some(dead));
        for lang in [Lang::En, Lang::Tr] {
            let (texts, size) = overlay_texts(lang, &v);
            let size = size.expect("the overlay sized itself");
            // The card is 332 wide (340 less 8) in a window with room for the shadow at each side.
            assert_eq!(size.x, OVERLAY_WINDOW_W, "{lang:?}");
            // The cells are 56 high (before 46), and the footnote is gone: the card is lower than before (132).
            let card_h = size.y - OVERLAY_TOP - OVERLAY_BOTTOM;
            assert!(card_h <= 132.0, "{lang:?}: {card_h} points high");
            // The header, the context line, and the three cells with their names and reasons.
            for t in [
                "Hexsight".to_string(),
                lang.fmt(Key::SuggestStyle, &[&lang.style("ap")]),
                context_line(lang, &suggest_data(), game_advice().ctx.as_ref().unwrap()).unwrap(),
                lang.fmt(Key::ReasonCore, &[&1]),
                lang.fmt(Key::ReasonVsAp, &[&lang.pct(70.0, 0)]),
                lang.fmt(Key::ReasonAntiHeal, &[&2]),
            ] {
                assert!(
                    texts.contains(&t),
                    "{lang:?}: {t:?} is missing in {texts:?}"
                );
            }
            for name in ["Blackfire Torch", "Liandry's Torment", "Morellonomicon"] {
                assert!(
                    texts.iter().any(|t| t.contains(name)),
                    "{lang:?}: {name} is missing in {texts:?}"
                );
            }
            // No chip text and no tooltip in the overlay: the cell has no minus sign and no percent sign but the reason.
            assert!(!texts.iter().any(|t| t.starts_with('-')), "{texts:?}");
            // The footnote "Hexsight's own rule" is gone (it was 10 points of grey under the cells). A
            // reason from a rule has the brighter ink instead (see the test of the inks).
            assert!(
                !texts.contains(&lang.t(Key::TipReasonRule).to_string()),
                "{lang:?}: the footnote is back in {texts:?}"
            );
        }
        // Each setting takes its part away. With both off the card has nothing to show.
        let mut only_context = test_app(
            Settings {
                suggest_items: false,
                ..Settings::default()
            },
            Lang::En,
        );
        assert!(only_context.shows_advice(&v));
        only_context.settings.suggest_context = false;
        assert!(!only_context.shows_advice(&v));
    }

    #[test]
    fn the_overlay_card_shows_when_dead_or_for_twelve_seconds_after_a_change_or_while_unlocked() {
        use std::time::Duration;
        let secs = |s: u64| Some(Duration::from_secs(s));
        assert_eq!(ADVICE_SHOW, Duration::from_secs(12));
        // A change a moment ago shows the card. Twelve seconds later it is gone.
        assert!(advice_visible(true, false, false, secs(0)));
        assert!(advice_visible(
            true,
            false,
            false,
            Some(Duration::from_millis(11_999))
        ));
        assert!(!advice_visible(true, false, false, secs(12)));
        assert!(!advice_visible(true, false, false, None));
        // Dead: you can shop, so it stays. Unlocked: you drag it, so it stays.
        assert!(advice_visible(true, true, false, secs(300)));
        assert!(advice_visible(true, false, true, secs(300)));
        // Nothing to show: never.
        assert!(!advice_visible(false, true, true, secs(0)));
        // The clock of the app: a new list of items starts it again. The same advice keeps it.
        let mut app = test_app(Settings::default(), Lang::En);
        let v = waiting_view(Some(game_advice()));
        app.note_advice(&v);
        let (first_sig, first_at) = app.advice_seen.unwrap();
        app.note_advice(&v);
        assert_eq!(app.advice_seen, Some((first_sig, first_at)));
        // More gold with no flag change leaves the advice equal.
        app.note_advice(&View {
            gold: Some(2000),
            ..waiting_view(Some(game_advice()))
        });
        assert_eq!(app.advice_seen, Some((first_sig, first_at)));
        // An item gets ready: the gold reaches its price, which often happens after a kill in a fight.
        // The card must not pop up then, so the clock keeps running (the signature ignores `ready`).
        let mut ready = game_advice();
        ready.items[0].ready = true;
        let mut unready = game_advice();
        unready.items[1].ready = false;
        assert_ne!(
            ready.items,
            game_advice().items,
            "the test must change a flag"
        );
        assert_eq!(ready.signature(), game_advice().signature());
        assert_eq!(unready.signature(), game_advice().signature());
        app.note_advice(&waiting_view(Some(ready)));
        assert_eq!(app.advice_seen, Some((first_sig, first_at)));
        app.note_advice(&waiting_view(Some(unready)));
        assert_eq!(app.advice_seen, Some((first_sig, first_at)));
        // The list of items changes: the clock starts again.
        let mut changed = game_advice();
        changed.items[0].id = 3089;
        app.note_advice(&waiting_view(Some(changed)));
        let (second_sig, second_at) = app.advice_seen.unwrap();
        assert_ne!(second_sig, first_sig);
        assert!(second_at >= first_at);
        // Without advice (the game is over) the clock is gone.
        app.note_advice(&waiting_view(None));
        assert_eq!(app.advice_seen, None);
        assert!(app.advice_age().is_none());
        // The card shows for a new game right away, and the settings decide what it holds.
        app.note_advice(&v);
        assert!(app.shows_advice(&v));
        assert!(!app.shows_advice(&waiting_view(None)));
        let off = test_app(
            Settings {
                suggest_items: false,
                suggest_context: false,
                ..Settings::default()
            },
            Lang::En,
        );
        assert!(!off.shows_advice(&v));
    }

    #[test]
    fn the_pick_strip_wins_over_the_advice_card() {
        assert_eq!(overlay_mode(true, true), OverlayMode::Strip);
        assert_eq!(overlay_mode(true, false), OverlayMode::Strip);
        assert_eq!(overlay_mode(false, true), OverlayMode::Advice);
        assert_eq!(overlay_mode(false, false), OverlayMode::Pill);
        // A frame with cards and a dead player paints the strip and none of the advice.
        let dead = Advice {
            me_dead: true,
            ..game_advice()
        };
        let v = View {
            offer: Some(offer_on(2, [1, 2, 3])),
            ..waiting_view(Some(dead))
        };
        let (texts, _) = overlay_texts(Lang::En, &v);
        assert!(texts.contains(&"Level 7".to_string()), "{texts:?}");
        assert!(
            !texts
                .iter()
                .any(|t| t.contains("Blackfire Torch") || t.contains("core item 1")),
            "{texts:?}"
        );
    }

    /// A pick view with the sd of the suggestion tests, and the data tie from augment 1 to Stormsurge.
    fn fit_view(advice: Option<Advice>) -> View {
        let base = pick_view();
        let cs = ChampStats {
            aug_links: vec![
                AugLink {
                    item: 4646,
                    augment: 1,
                    with_pick: 0.8,
                },
                AugLink {
                    item: 4646,
                    augment: 2,
                    with_pick: 0.1,
                },
            ],
            ..(*base.details.clone().unwrap()).clone()
        };
        View {
            sd: Some(suggest_data()),
            details: Some(Arc::new(cs)),
            advice: advice.map(Arc::new),
            ..base
        }
    }

    #[test]
    fn an_offered_augment_fits_when_the_data_ties_it_to_a_wanted_item() {
        let v = fit_view(Some(game_advice()));
        let offer = offer_on(2, [1, 2, 3]);
        let build = |marks: Marks, advice: Option<&Advice>| {
            let (mut cells, _) = pick_cells(v.details.as_deref(), &offer, &[], 300, marks);
            fit_cells(&mut cells, v.details.as_deref(), advice, marks);
            cells
        };
        let on = Marks {
            fit: true,
            ..Marks::default()
        };
        // Augment 1 is tied to Stormsurge, which the build still wants. Augment 2 has a weak link, augment 3 none.
        let cells = build(on, v.advice.as_deref());
        assert_eq!(
            cells.iter().map(|c| (c.id, c.fit)).collect::<Vec<_>>(),
            vec![(1, Some(4646)), (2, None), (3, None)]
        );
        // Off: no cell has a fit. No advice or no stats: none either.
        assert!(
            build(Marks::default(), v.advice.as_deref())
                .iter()
                .all(|c| c.fit.is_none())
        );
        assert!(build(on, None).iter().all(|c| c.fit.is_none()));
        let mut none = vec![];
        fit_cells(&mut none, None, v.advice.as_deref(), on);
        // The item is no longer wanted once you own it.
        let owned = Advice {
            wanted: vec![2503],
            ..game_advice()
        };
        assert!(build(on, Some(&owned)).iter().all(|c| c.fit.is_none()));
        // The mark changes neither the order nor the highlight nor the tier.
        let plain = build(Marks::default(), v.advice.as_deref());
        let marked = build(on, v.advice.as_deref());
        assert_eq!(
            plain
                .iter()
                .map(|c| (c.id, c.tier, c.top))
                .collect::<Vec<_>>(),
            marked
                .iter()
                .map(|c| (c.id, c.tier, c.top))
                .collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_fit_mark_is_off_by_default_and_shows_no_frame_and_no_number() {
        let v = fit_view(Some(game_advice()));
        for lang in [Lang::En, Lang::Tr] {
            let card = |settings: Settings| {
                let app = test_app(settings, lang);
                part_texts(&app, |app, ui| app.pick_card(ui, &v))
            };
            // Off by default: the card shows no chip and no line.
            let off = card(Settings::default());
            let (mark, line) = (
                lang.t(Key::FitMark).to_string(),
                lang.fmt(Key::FitLine, &[&"Stormsurge"]),
            );
            assert!(
                !off.contains(&mark) && !off.contains(&line),
                "{lang:?}: {off:?}"
            );
            // On: the chip and the line with the name of the item.
            let on = card(Settings {
                fit_marks: true,
                ..Settings::default()
            });
            assert!(on.contains(&mark) && on.contains(&line), "{lang:?}: {on:?}");
            // No number comes with it, and the other marks stay off: no "top" line, no green frame.
            for t in &on {
                assert!(!looks_like_a_rate(t), "{lang:?}: {t:?}");
            }
            assert!(
                !on.contains(&lang.t(Key::TopLabel).to_string())
                    && !on.contains(&lang.fmt(Key::TopOne, &[]))
            );
            // The overlay strip never shows it.
            let app = test_app(
                Settings {
                    fit_marks: true,
                    ..Settings::default()
                },
                lang,
            );
            let strip = part_texts(&app, |app, ui| app.overlay_strip(ui, &v));
            assert!(
                !strip.contains(&mark) && !strip.contains(&line),
                "{lang:?}: {strip:?}"
            );
        }
        // The text of the mark says whose rule it is.
        assert!(
            Lang::En.t(Key::FitTip).contains("not statistics")
                && Lang::Tr.t(Key::FitTip).contains("istatistik değil")
        );
    }

    #[test]
    fn the_suggestion_settings_show_in_the_settings_page_in_both_languages() {
        let v = View {
            phase: Phase::Idle,
            offer: None,
            ..pick_view()
        };
        for lang in [Lang::En, Lang::Tr] {
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Settings;
            let texts: Vec<String> = [None, Some(250.0), Some(500.0), Some(2000.0)]
                .into_iter()
                .flat_map(|scroll| window_texts(&mut app, &v, scroll))
                .collect();
            for t in [
                lang.upper(lang.t(Key::GroupSuggest)),
                lang.t(Key::SetSuggestItems).into(),
                lang.t(Key::SetSuggestContext).into(),
                lang.t(Key::SetSuggestNote).into(),
                lang.t(Key::SetFitMarks).into(),
            ] {
                assert!(
                    texts.contains(&t),
                    "{lang:?}: {t:?} is missing in {texts:?}"
                );
            }
        }
    }

    // ---- The main window after the review of 2026-10-29 (P2).

    /// The shapes that the window paints in its last frame, after some frames.
    fn window_frame(
        app: &mut App,
        v: &View,
        scroll: Option<f32>,
    ) -> Vec<egui::epaint::ClippedShape> {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        app.debug_scroll = scroll;
        let mut shapes = Vec::new();
        for i in 0..6 {
            let input = egui::RawInput {
                time: Some(i as f64 / 10.0),
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(MAIN_W, 680.0))),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.window_ui(ui, v);
            });
            shapes = std::mem::take(&mut out.shapes);
            out.drop_without_applying_deltas();
        }
        shapes
    }

    /// Every text of a shape list with the rect it covers and its color.
    fn painted_boxes(shapes: &[egui::epaint::ClippedShape]) -> Vec<(String, Rect, Color32)> {
        fn walk(shape: &egui::Shape, out: &mut Vec<(String, Rect, Color32)>) {
            match shape {
                egui::Shape::Text(t) => {
                    let color = t
                        .galley
                        .job
                        .sections
                        .first()
                        .map_or(t.fallback_color, |s| s.format.color);
                    out.push((
                        t.galley.text().to_string(),
                        Rect::from_min_size(t.pos, t.galley.size()),
                        color,
                    ));
                }
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|c| walk(&c.shape, &mut out));
        out
    }

    /// The circles of a shape list that `pick` takes: the center, the radius, the fill and the stroke.
    fn circles_where(
        shapes: &[egui::epaint::ClippedShape],
        pick: impl Fn(&egui::epaint::CircleShape) -> bool,
    ) -> Vec<egui::epaint::CircleShape> {
        fn walk(
            shape: &egui::Shape,
            pick: &dyn Fn(&egui::epaint::CircleShape) -> bool,
            out: &mut Vec<egui::epaint::CircleShape>,
        ) {
            match shape {
                egui::Shape::Circle(c) if pick(c) => out.push(*c),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, pick, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|c| walk(&c.shape, &pick, &mut out));
        out
    }

    /// The meshes of a shape list.
    fn meshes(shapes: &[egui::epaint::ClippedShape]) -> Vec<std::sync::Arc<egui::epaint::Mesh>> {
        fn walk(shape: &egui::Shape, out: &mut Vec<std::sync::Arc<egui::epaint::Mesh>>) {
            match shape {
                egui::Shape::Mesh(m) => out.push(m.clone()),
                egui::Shape::Vec(v) => v.iter().for_each(|s| walk(s, out)),
                _ => {}
            }
        }
        let mut out = Vec::new();
        shapes.iter().for_each(|c| walk(&c.shape, &mut out));
        out
    }

    #[test]
    fn the_x_hides_the_window_on_the_main_page_and_the_other_pages_have_no_x() {
        let close = glyph::CLOSE.to_string();
        let v = View {
            phase: Phase::ChampSelect,
            offer: None,
            ..pick_view()
        };
        for lang in [Lang::En, Lang::Tr] {
            let mut app = test_app(Settings::default(), lang);
            let main = window_texts(&mut app, &v, None);
            assert!(
                main.contains(&close) && main.contains(&glyph::SETTINGS.to_string()),
                "{lang:?}: {main:?}"
            );
            // Settings and My games: the way back is the only control, and Esc goes back too.
            for page in [Page::Settings, Page::Games] {
                app.page = page;
                let texts = window_texts(&mut app, &v, None);
                assert!(
                    texts.contains(&glyph::BACK.to_string()) && !texts.contains(&close),
                    "{lang:?} {page:?}: {texts:?}"
                );
            }
        }
        // The X minimizes the window. It says nothing yet: nobody sees a toast in a minimized window.
        // The first return in a run says that Hexsight keeps running, and later returns do not.
        let mut app = test_app(Settings::default(), Lang::Tr);
        let ctx = egui::Context::default();
        let hide = |app: &mut App| {
            let mut out = ctx.run_ui(egui::RawInput::default(), |ui| app.hide_window(ui.ctx()));
            let cmds: Vec<_> = out
                .viewport_output
                .values()
                .flat_map(|v| v.commands.clone())
                .collect();
            out.shapes.clear();
            out.drop_without_applying_deltas();
            cmds
        };
        assert!(!app.hidden_told);
        let cmds = hide(&mut app);
        assert!(
            cmds.iter()
                .any(|c| matches!(c, egui::ViewportCommand::Minimized(true))),
            "{cmds:?}"
        );
        assert!(
            !cmds
                .iter()
                .any(|c| matches!(c, egui::ViewportCommand::Close)),
            "{cmds:?}"
        );
        let said = |app: &App| app.h.shared.lock().unwrap().last_action.clone();
        assert!(!app.hidden_told && app.hide == Hide::Asked);
        assert_eq!(said(&app), None, "no toast at the minimize");
        // The frames after the X: the window is not minimized yet, then it is. Still no toast.
        app.tell_on_return(false);
        app.tell_on_return(true);
        app.tell_on_return(true);
        assert!(!app.hidden_told && app.hide == Hide::Minimized);
        assert_eq!(said(&app), None, "no toast in the minimized window");
        // The window comes back: the toast shows now.
        app.tell_on_return(false);
        assert!(app.hidden_told && app.hide == Hide::Shown);
        assert_eq!(said(&app).as_deref(), Some(Lang::Tr.t(Key::HiddenToast)));
        {
            let mut s = app.h.shared.lock().unwrap();
            s.last_action = None;
            s.action_at = None;
        }
        // A second hide and return: no toast, once per run.
        let cmds = hide(&mut app);
        assert!(
            cmds.iter()
                .any(|c| matches!(c, egui::ViewportCommand::Minimized(true)))
        );
        app.tell_on_return(true);
        app.tell_on_return(false);
        assert_eq!(app.hide, Hide::Shown);
        assert_eq!(said(&app), None, "the toast shows once");
    }

    #[test]
    fn the_hide_toast_waits_for_the_window_to_come_back() {
        // A window that was never hidden by the X says nothing, whatever it does (the minimize
        // button, `--background`).
        let mut state = Hide::Shown;
        for minimized in [false, true, true, false, true, false] {
            let back;
            (state, back) = hide_step(state, minimized);
            assert!(!back && state == Hide::Shown, "{minimized}");
        }
        // The frame right after the X: the window is not minimized yet. That is not a return.
        assert_eq!(hide_step(Hide::Asked, false), (Hide::Asked, false));
        assert_eq!(hide_step(Hide::Asked, true), (Hide::Minimized, false));
        assert_eq!(hide_step(Hide::Minimized, true), (Hide::Minimized, false));
        // The first frame that is no longer minimized is the return, once.
        assert_eq!(hide_step(Hide::Minimized, false), (Hide::Shown, true));
        assert_eq!(hide_step(Hide::Shown, false), (Hide::Shown, false));
    }

    #[test]
    fn the_status_pill_is_blue_in_champ_select_and_orange_stays_for_warnings() {
        let fill = |phase: Phase, color: Color32| {
            let mut app = test_app(Settings::default(), Lang::En);
            let v = View {
                phase,
                offer: None,
                queue: None,
                mine: None,
                ..pick_view()
            };
            let shapes = window_frame(&mut app, &v, None);
            !rects_where(&shapes, |r| r.fill == color.gamma_multiply(0.16)).is_empty()
        };
        assert!(fill(Phase::ChampSelect, BLUE));
        assert!(!fill(Phase::ChampSelect, ORANGE));
        // The other phases keep their colors.
        assert!(fill(Phase::InGame, GREEN) && fill(Phase::Lobby, TEAL));
    }

    #[test]
    fn the_footer_names_the_patch_and_keeps_the_source_for_the_tooltip() {
        let mut stats = crate::data::Stats::of_champions(Default::default());
        stats.patch = "16.21".into();
        let v = View {
            phase: Phase::Idle,
            offer: None,
            stats: Some(Arc::new(stats)),
            ..pick_view()
        };
        for (lang, left) in [(Lang::En, "Patch 16.21"), (Lang::Tr, "16.21 yaması")] {
            let mut app = test_app(Settings::default(), lang);
            let texts = window_texts(&mut app, &v, None);
            assert!(texts.iter().any(|t| t == left), "{lang:?}: {texts:?}");
            // The name of the source is not on the footer. The match count stays at the right.
            assert!(
                !texts.iter().any(|t| t.contains("aramkit")),
                "{lang:?}: {texts:?}"
            );
            assert!(
                texts
                    .iter()
                    .any(|t| *t == lang.games(120_000, &fmt_n(lang, 120_000))),
                "{lang:?}: {texts:?}"
            );
            // The tooltip carries the source and the patch.
            assert_eq!(
                lang.fmt(Key::FooterSource, &[&"aramkit", &"16.21"]),
                if lang == Lang::En {
                    "aramkit · patch 16.21"
                } else {
                    "aramkit · 16.21 yaması"
                }
            );
            // The footer text fits beside the match count.
            let ctx = egui::Context::default();
            theme::setup(&ctx);
            let w = text_width(&ctx, left, FontId::proportional(11.0))
                + 12.0
                + text_width(
                    &ctx,
                    &lang.games(999_999, &fmt_n(lang, 999_999)),
                    FontId::proportional(11.0),
                );
            assert!(w <= MAIN_W - 2.0 * MARGIN, "{lang:?}: {w:.1}");
        }
    }

    #[test]
    fn the_next_pick_dots_show_the_next_pick_the_past_ones_and_the_ones_to_come() {
        let later = Color32::from_white_alpha(46);
        assert_eq!(next_pick_dots(Some(0)), [GREEN, later, later, later]);
        assert_eq!(next_pick_dots(Some(1)), [LABEL_3, GREEN, later, later]);
        assert_eq!(next_pick_dots(Some(3)), [LABEL_3, LABEL_3, LABEL_3, GREEN]);
        assert_eq!(next_pick_dots(None), [LABEL_3; 4]);
        // The card paints four discs of 6 points, 10 points apart, at the right of the row.
        let app = test_app(Settings::default(), Lang::En);
        let v = waiting_view(Some(game_advice()));
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(
                pos2(0.0, 0.0),
                vec2(MAIN_W - 2.0 * MARGIN, 600.0),
            )),
            ..Default::default()
        };
        let mut out = ctx.run_ui(input, |ui| app.next_pick_card(ui, &v));
        let discs = circles_where(&out.shapes, |c| c.radius == 3.0);
        let texts = painted_boxes(&out.shapes);
        let card = rects_where(&out.shapes, |r| r.fill == SURFACE);
        out.shapes.clear();
        out.drop_without_applying_deltas();
        // The level is 7 and one pick screen is done: the next pick is at level 7 (the second dot).
        assert_eq!(
            discs.iter().map(|c| c.fill).collect::<Vec<_>>(),
            next_pick_dots(Some(1)).to_vec()
        );
        assert!(
            discs
                .windows(2)
                .all(|w| (w[1].center.x - w[0].center.x - 10.0).abs() < 0.01
                    && w[0].center.y == w[1].center.y)
        );
        assert!(
            texts.iter().any(|(t, ..)| t == "Next pick: level 7"),
            "{texts:?}"
        );
        // The row is 32 points high: the whole card is the row and the margins of the group.
        assert_eq!(card.len(), 1);
        assert!(
            (card[0].height() - (NEXT_PICK_ROW_H + 20.0 + 2.0)).abs() < 0.01,
            "{card:?}"
        );
    }

    #[test]
    fn a_header_note_sits_at_the_right_end_of_the_header_line() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(272.0, 200.0))),
            ..Default::default()
        };
        let shop = format!("{} In shop", glyph::CHECK);
        let mut out = ctx.run_ui(input, |ui| {
            section_note(ui, "BUILD", Some((shop.as_str(), GREEN)));
            section_note(ui, "NEXT ITEMS", Some(("AP build", LABEL_3)));
            section_note(ui, "AUGMENTS", None);
        });
        let boxes = painted_boxes(&out.shapes);
        out.shapes.clear();
        out.drop_without_applying_deltas();
        let find = |t: &str| {
            boxes
                .iter()
                .find(|(x, ..)| x == t)
                .unwrap_or_else(|| panic!("{t:?} is missing in {boxes:?}"))
        };
        // The header is 11 points, semibold, in LABEL_3, at the left edge.
        let (_, header, ink) = find("BUILD");
        assert_eq!(*ink, LABEL_3);
        assert!(header.left() < 1.0);
        // The note is green, ends at the right edge, and sits on the line of the header. The check mark comes first.
        let (_, note, ink) = find("In shop");
        let (_, mark, mark_ink) = find(&glyph::CHECK.to_string());
        assert_eq!((*ink, *mark_ink), (GREEN, GREEN));
        assert!((note.right() - 272.0).abs() < 0.6, "{note:?}");
        assert!(
            mark.right() < note.left() && note.left() - mark.right() <= 5.0,
            "{mark:?} {note:?}"
        );
        assert!(
            (note.center().y - header.center().y).abs() < 2.0,
            "{note:?} {header:?}"
        );
        // A note in LABEL_3, and a header without a note.
        let (_, style, ink) = find("AP build");
        assert_eq!(*ink, LABEL_3);
        assert!((style.right() - 272.0).abs() < 0.6);
        assert!(find("AUGMENTS").1.left() < 1.0);
        // The second header is lower than the first by about one line.
        assert!(find("NEXT ITEMS").1.top() > header.bottom());
    }

    #[test]
    fn the_build_card_lines_the_items_up_and_has_no_check_mark() {
        let sd = window_data(24);
        let builds = ["Tank", "AP", "Utility"]
            .map(|name| Build {
                name: name.into(),
                items: (3001..=3006).collect(),
                ..Default::default()
            })
            .to_vec();
        let details = Arc::new(ChampStats {
            builds,
            ..(*pick_view().details.unwrap()).clone()
        });
        // The sets are in the shop (item_sets_for == mine): the old check mark at the end of the first row is gone.
        let v = View {
            phase: Phase::ChampSelect,
            sd: Some(sd),
            details: Some(details),
            item_sets_for: Some(1),
            offer: None,
            ..pick_view()
        };
        for lang in [Lang::En, Lang::Tr] {
            let app = test_app(Settings::default(), lang);
            let ctx = egui::Context::default();
            theme::setup(&ctx);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(
                    pos2(0.0, 0.0),
                    vec2(MAIN_W - 2.0 * MARGIN, 600.0),
                )),
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| app.build_card(ui, &v));
            let boxes = painted_boxes(&out.shapes);
            let icons = rects_where(&out.shapes, |r| {
                r.fill == SURFACE_2 && (r.rect.width() - 26.0).abs() < 0.01
            });
            let pills = rects_where(&out.shapes, |r| r.fill == LABEL_2.gamma_multiply(0.16));
            out.shapes.clear();
            out.drop_without_applying_deltas();
            // The styles are plain text, no pill, and no check mark.
            assert!(pills.is_empty(), "{lang:?}: {pills:?}");
            assert!(
                !boxes.iter().any(|(t, ..)| *t == glyph::CHECK.to_string()),
                "{lang:?}: {boxes:?}"
            );
            for style in ["tank", "ap", "utility"] {
                let name = lang.style(style);
                let (_, at, ink) = boxes
                    .iter()
                    .find(|(t, ..)| *t == name)
                    .unwrap_or_else(|| panic!("{name:?} in {boxes:?}"));
                assert_eq!(*ink, LABEL_2, "{lang:?} {name}");
                assert!(
                    (at.left() - 13.0).abs() < 0.6,
                    "{lang:?} {name}: the style column starts at the inside of the card: {at:?}"
                );
            }
            // Three rows of five items, and the first item of each row sits at the same x.
            assert_eq!(icons.len(), 15, "{lang:?}: {icons:?}");
            let mut rows: Vec<(i32, f32)> = Vec::new();
            for r in &icons {
                let top = r.top().round() as i32;
                match rows.iter_mut().find(|(t, _)| *t == top) {
                    Some((_, left)) => *left = left.min(r.left()),
                    None => rows.push((top, r.left())),
                }
            }
            assert_eq!(rows.len(), 3, "{lang:?}: {rows:?}");
            assert!(
                rows.iter().all(|(_, left)| (left - rows[0].1).abs() < 0.01),
                "{lang:?}: the items of the rows start at different places: {rows:?}"
            );
            // The column is as wide as the widest style, and the items start after it.
            let widest = ["tank", "ap", "utility"]
                .map(|s| {
                    boxes
                        .iter()
                        .find(|(t, ..)| *t == lang.style(s))
                        .unwrap()
                        .1
                        .width()
                })
                .into_iter()
                .fold(0.0, f32::max);
            assert!(
                rows[0].1 >= 13.0 + widest,
                "{lang:?}: {} {widest}",
                rows[0].1
            );
        }
    }

    #[test]
    fn the_first_run_card_says_what_to_do_and_has_a_button_for_what_can_be_fixed() {
        let failed = LoadError::Download(Key::LoadingData);
        let base = View {
            phase: Phase::Idle,
            queue: None,
            mine: None,
            offer: None,
            sd: None,
            stats: None,
            details: None,
            window_mode: Some(2),
            ocr_probe: Some(true),
            ..pick_view()
        };
        for lang in [Lang::En, Lang::Tr] {
            let has = |texts: &[String], t: &str| texts.iter().any(|x| x == t);
            let texts_of =
                |v: &View| window_texts(&mut test_app(Settings::default(), lang), v, None);
            // The title: League closed says to open it, else the card says that Hexsight is ready.
            let idle = texts_of(&base);
            assert!(
                has(&idle, lang.t(Key::HeroReady)) && !has(&idle, lang.t(Key::HeroOpenLeague)),
                "{lang:?}: {idle:?}"
            );
            let offline = texts_of(&View {
                phase: Phase::Offline,
                ..base_clone(&base)
            });
            assert!(
                has(&offline, lang.t(Key::HeroOpenLeague))
                    && !has(&offline, lang.t(Key::HeroReady)),
                "{lang:?}: {offline:?}"
            );
            // No failed check, no fix button.
            for key in [Key::RetryButton, Key::OpenLangSettings] {
                assert!(!has(&idle, lang.t(key)), "{lang:?} {key:?}");
            }
            // The data did not load: "Try again". The OCR engine cannot start: "Language settings".
            let data = texts_of(&View {
                error: Some(failed.clone()),
                ..base_clone(&base)
            });
            assert!(
                has(&data, lang.t(Key::RetryButton)) && !has(&data, lang.t(Key::OpenLangSettings)),
                "{lang:?}: {data:?}"
            );
            let ocr = texts_of(&View {
                ocr_probe: Some(false),
                ..base_clone(&base)
            });
            assert!(
                has(&ocr, lang.t(Key::OpenLangSettings)) && !has(&ocr, lang.t(Key::RetryButton)),
                "{lang:?}: {ocr:?}"
            );
            // The data came (sd and stats are there): no button, even if an old error is still on.
            let loaded = View {
                sd: Some(suggest_data()),
                stats: Some(select_stats()),
                error: Some(failed.clone()),
                ..base_clone(&base)
            };
            assert!(
                !has(&texts_of(&loaded), lang.t(Key::RetryButton)),
                "{lang:?}"
            );
            // The question about Windows is not on this card, even when the debug hook asks for it.
            let mut app = test_app(Settings::default(), lang);
            app.debug_first_run = true;
            let asked = window_texts(&mut app, &base, None);
            assert!(
                !has(&asked, lang.t(Key::AskAutostart)) && !has(&asked, lang.t(Key::AskYes)),
                "{lang:?}: {asked:?}"
            );
            // The hint shows the key twice: once alone, once after Ctrl+.
            let start = lang.fmt(Key::HeroStartGame, &[&"F8"]);
            assert!(
                has(&idle, &start) && start.matches("F8").count() == 2,
                "{lang:?}: {idle:?}"
            );
        }
        // Each failed check has its button.
        let f = |data_failed: bool, data_loaded: bool, ocr: Option<bool>, free: bool| FirstRun {
            league_found: true,
            window_mode: Some(2),
            ocr_probe: ocr,
            data_loaded,
            data_failed,
            hotkeys: [(0x77, true), (0x78, free)],
        };
        assert_eq!(
            first_run_fixes(&f(false, true, Some(true), true)),
            FirstRunFixes::default()
        );
        assert_eq!(
            first_run_fixes(&f(true, false, Some(true), true)),
            FirstRunFixes {
                retry: true,
                ..Default::default()
            }
        );
        // The data loaded after an error: no button. Still loading: no button.
        assert!(!first_run_fixes(&f(true, true, Some(true), true)).retry);
        assert!(!first_run_fixes(&f(false, false, Some(true), true)).retry);
        assert_eq!(
            first_run_fixes(&f(false, true, Some(false), true)),
            FirstRunFixes {
                language: true,
                ..Default::default()
            }
        );
        // The OCR probe still runs (None): no button yet.
        assert!(!first_run_fixes(&f(false, true, None, true)).language);
        assert_eq!(
            first_run_fixes(&f(false, true, Some(true), false)),
            FirstRunFixes {
                hotkey: true,
                ..Default::default()
            }
        );
    }

    /// A copy of a view (`View` has no `Clone`): the fields that the tests change come from `pick_view`.
    fn base_clone(v: &View) -> View {
        View {
            phase: v.phase,
            queue: v.queue,
            cs: v.cs.clone(),
            mine: v.mine,
            sd: v.sd.clone(),
            stats: v.stats.clone(),
            details: v.details.clone(),
            stale: v.stale,
            no_data: v.no_data,
            offer: v.offer.clone(),
            error: v.error.clone(),
            note: v.note,
            window_mode: v.window_mode,
            action: v.action.clone(),
            action_detail: v.action_detail.clone(),
            offered: v.offered.clone(),
            scan: v.scan.clone(),
            read: v.read.clone(),
            history: v.history.clone(),
            importing: v.importing,
            league_found: v.league_found,
            ocr_probe: v.ocr_probe,
            capture: v.capture,
            level: v.level,
            picks_done: v.picks_done,
            item_sets_for: v.item_sets_for,
            update: v.update.clone(),
            advice: v.advice.clone(),
            gold: v.gold,
        }
    }

    #[test]
    fn the_retry_button_asks_the_worker_for_the_data_again() {
        let v = View {
            phase: Phase::Idle,
            queue: None,
            mine: None,
            offer: None,
            sd: None,
            stats: None,
            details: None,
            error: Some(LoadError::Download(Key::LoadingData)),
            ..pick_view()
        };
        let (mut app, sent) = test_app_rx(Settings::default(), Lang::En);
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let mut frame = |i: usize, events: Vec<egui::Event>| {
            let input = egui::RawInput {
                time: Some(i as f64 / 10.0),
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(MAIN_W, 680.0))),
                events,
                ..Default::default()
            };
            let mut out = ctx.run_ui(input, |ui| {
                app.window_ui(ui, &v);
            });
            let boxes = painted_boxes(&out.shapes);
            out.shapes.clear();
            out.drop_without_applying_deltas();
            boxes
        };
        let boxes = frame(0, vec![]);
        let at = boxes
            .iter()
            .find(|(t, ..)| t == "Try again")
            .expect("the button")
            .1
            .center();
        frame(1, vec![egui::Event::PointerMoved(at)]);
        frame(2, vec![]);
        assert!(
            sent.try_recv().is_err(),
            "a frame sent a call without a click"
        );
        let button = egui::PointerButton::Primary;
        frame(
            3,
            vec![egui::Event::PointerButton {
                pos: at,
                button,
                pressed: true,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        frame(
            4,
            vec![egui::Event::PointerButton {
                pos: at,
                button,
                pressed: false,
                modifiers: egui::Modifiers::NONE,
            }],
        );
        assert!(matches!(sent.try_recv(), Ok(Cmd::Reload)));
        assert!(sent.try_recv().is_err());
    }

    #[test]
    fn after_a_game_the_card_shows_the_augments_the_score_and_asks_about_windows_once() {
        let sd = window_data(24);
        let v = View {
            phase: Phase::PostGame,
            offer: None,
            sd: Some(sd),
            history: many_games(3, 1),
            ..pick_view()
        };
        for lang in [Lang::En, Lang::Tr] {
            let mut app = test_app(Settings::default(), lang);
            app.debug_first_run = true;
            let shapes = window_frame(&mut app, &v, None);
            let boxes = painted_boxes(&shapes);
            let icons = rects_where(&shapes, |r| {
                r.fill == SURFACE_2 && (r.rect.width() - TOOK_ICON).abs() < 0.01
            });
            let texts: Vec<&str> = boxes.iter().map(|(t, ..)| t.as_str()).collect();
            // The newest game (i = 0) is a loss with 5/4/9 and two augments: two icons of 24 points, 4 points apart.
            assert!(
                texts.contains(&lang.t(Key::AfterLoss)),
                "{lang:?}: {texts:?}"
            );
            assert!(texts.contains(&"5/4/9"), "{lang:?}: {texts:?}");
            assert_eq!(icons.len(), 2, "{lang:?}: {icons:?}");
            assert!(
                (icons[1].left() - icons[0].right() - TOOK_GAP).abs() < 0.01
                    && icons[0].top() == icons[1].top(),
                "{lang:?}: {icons:?}"
            );
            // The score is at the right edge of the row, and in 12.5 point semibold ink LABEL.
            let (_, kda, ink) = boxes.iter().find(|(t, ..)| t == "5/4/9").unwrap();
            assert_eq!(*ink, LABEL);
            assert!(kda.left() > icons[1].right(), "{lang:?}: {kda:?} {icons:?}");
            // The question about Windows sits under the result, with its two buttons, and above the link to My games.
            let at = |t: &str| texts.iter().position(|x| *x == t);
            assert!(
                at(lang.t(Key::AskAutostart)).is_some()
                    && at(lang.t(Key::AskYes)).is_some()
                    && at(lang.t(Key::AskNo)).is_some(),
                "{lang:?}: {texts:?}"
            );
            assert!(
                at("5/4/9") < at(lang.t(Key::AskAutostart))
                    && at(lang.t(Key::AskAutostart)) < at(lang.t(Key::MyGames)),
                "{lang:?}: {texts:?}"
            );
            // Answered (or never asked): the card is the result and the link only.
            let mut app = test_app(
                Settings {
                    autostart_asked: true,
                    ..Settings::default()
                },
                lang,
            );
            let texts = window_texts(&mut app, &v, None);
            assert!(
                !texts.iter().any(|t| t == lang.t(Key::AskAutostart))
                    && texts.iter().any(|t| t == "5/4/9"),
                "{lang:?}: {texts:?}"
            );
        }
        // The question is the player's answer, once: yes starts with Windows (not tested here: it writes the registry), no only ends the question.
        let mut app = test_app(Settings::default(), Lang::En);
        app.debug_first_run = true;
        app.answer_autostart(false, true);
        assert!(app.settings.autostart_asked && !app.debug_first_run);
        // The augments that do not fit beside the score are left out, and a long row keeps the score.
        assert_eq!(took_icons_that_fit(0.0), 0);
        assert_eq!(took_icons_that_fit(24.0), 1);
        assert_eq!(took_icons_that_fit(52.0), 2);
        assert_eq!(took_icons_that_fit(51.9), 1);
        assert_eq!(took_icons_that_fit(-10.0), 0);
    }

    #[test]
    fn the_after_card_without_a_saved_game_does_not_ask_about_windows() {
        let ask = |v: &View, lang: Lang| {
            // The debug hook asks in every card that is allowed to ask.
            let mut app = test_app(Settings::default(), lang);
            app.debug_first_run = true;
            let texts = window_texts(&mut app, v, None);
            let has = |key: Key| texts.iter().any(|t| t == lang.t(key));
            (
                has(Key::AfterWaiting),
                has(Key::AskAutostart),
                has(Key::AskYes),
                has(Key::AfterOver),
            )
        };
        for lang in [Lang::En, Lang::Tr] {
            // A Mayhem game that the history thread has not saved yet: the card waits, and so does the question.
            let waiting = View {
                phase: Phase::PostGame,
                queue: Some(2400),
                offer: None,
                sd: Some(window_data(24)),
                history: Arc::new(Vec::new()),
                ..pick_view()
            };
            assert_eq!(ask(&waiting, lang), (true, false, false, true), "{lang:?}");
            // The game is saved: the card shows it, and the question comes with it.
            let saved = View {
                history: many_games(3, 1),
                ..base_clone(&waiting)
            };
            assert_eq!(ask(&saved, lang), (false, true, true, false), "{lang:?}");
            // A game of another queue (Ranked) has no After card: no waiting text and no question.
            let ranked = View {
                queue: Some(420),
                ..base_clone(&waiting)
            };
            assert_eq!(ask(&ranked, lang), (false, false, false, false), "{lang:?}");
        }
    }

    #[test]
    fn my_games_marks_a_win_with_a_disc_and_a_loss_with_a_ring_and_colors_only_a_real_sample() {
        let sd = window_data(40);
        for lang in [Lang::En, Lang::Tr] {
            // 30 games: the newest ten are L W W L W W L W W L (6 wins, 4 losses), so 20 wins in all (67 %).
            let v = View {
                phase: Phase::Idle,
                offer: None,
                sd: Some(sd.clone()),
                stats: Some(select_stats()),
                history: many_games(30, 12),
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Games;
            let shapes = window_frame(&mut app, &v, None);
            let boxes = painted_boxes(&shapes);
            let texts: Vec<&str> = boxes.iter().map(|(t, ..)| t.as_str()).collect();
            // The rows of "Recent games" use the same marks, under the "Last 10" row: only the marks above that title count here.
            let recent = lang.upper(lang.t(Key::GamesRecent));
            let (_, title, _) = boxes
                .iter()
                .find(|(t, ..)| *t == recent)
                .unwrap_or_else(|| panic!("{lang:?}: {texts:?}"));
            let wins = circles_where(&shapes, |c| {
                c.center.y < title.top() && c.radius == 5.0 && c.fill == GREEN
            });
            let losses = circles_where(&shapes, |c| {
                c.center.y < title.top()
                    && c.radius == 4.25
                    && c.fill == Color32::TRANSPARENT
                    && c.stroke.color == RED
                    && c.stroke.width == 1.5
            });
            assert_eq!((wins.len(), losses.len()), (6, 4), "{lang:?}");
            // The dots keep the order of time: the oldest at the left.
            let mut all: Vec<_> = wins
                .iter()
                .map(|c| (c.center.x, true))
                .chain(losses.iter().map(|c| (c.center.x, false)))
                .collect();
            all.sort_by(|a, b| a.0.total_cmp(&b.0));
            assert_eq!(
                all.iter().map(|a| a.1).collect::<Vec<_>>(),
                [
                    false, true, true, false, true, true, false, true, true, false
                ]
                .into_iter()
                .rev()
                .collect::<Vec<_>>(),
                "{lang:?}"
            );
            assert!(
                all.windows(2)
                    .all(|w| (w[1].0 - w[0].0 - RESULT_DOT_STEP).abs() < 0.01)
            );
            // The score follows the dots, and the totals line uses the same short form.
            let score = lang.fmt(Key::GamesLast10Score, &[&6, &4]);
            assert!(
                texts.contains(&score.as_str()) && texts.contains(&lang.t(Key::GamesLast10)),
                "{lang:?}: {texts:?}"
            );
            assert!(
                boxes.iter().any(|(t, ..)| *t == lang.totals(30, 20, 10)),
                "{lang:?}: {texts:?}"
            );
            let (_, last10, _) = boxes
                .iter()
                .find(|(t, ..)| t == lang.t(Key::GamesLast10))
                .unwrap();
            let (_, score_box, _) = boxes.iter().find(|(t, ..)| *t == score).unwrap();
            assert!(
                score_box.left()
                    > wins
                        .iter()
                        .chain(&losses)
                        .map(|c| c.center.x)
                        .fold(0.0, f32::max)
                    && last10.right()
                        < wins
                            .iter()
                            .chain(&losses)
                            .map(|c| c.center.x)
                            .fold(f32::MAX, f32::min),
                "{lang:?}"
            );
            // The big number is green from 55 % over a real sample. The tier badges and the note about the games are gone.
            let (_, _, ink) = boxes
                .iter()
                .find(|(t, ..)| *t == lang.pct(66.666, 0))
                .unwrap();
            assert_eq!(*ink, GREEN, "{lang:?}");
            assert!(
                !texts
                    .iter()
                    .any(|t| crate::model::TIER_NAMES.contains(t) || *t == "?"),
                "{lang:?}: {texts:?}"
            );
            assert!(
                !texts.contains(&lang.t(Key::GamesSavedNote)),
                "{lang:?}: {texts:?}"
            );
            // Few games: the same rate stays white.
            let few = View {
                history: many_games(5, 2),
                ..base_clone(&v)
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Games;
            let boxes = painted_boxes(&window_frame(&mut app, &few, None));
            let (_, _, ink) = boxes
                .iter()
                .find(|(t, ..)| *t == lang.pct(60.0, 0))
                .unwrap_or_else(|| panic!("{lang:?}: {boxes:?}"));
            assert_eq!(*ink, LABEL, "{lang:?}: five games give no color");
        }
    }

    // ---- My games: the list of recent games, the date and the length of a row, and "All champions".

    #[test]
    fn a_game_length_reads_as_minutes_and_seconds_or_a_dash_when_it_is_not_known() {
        assert_eq!(length_text(1186), "19:46");
        assert_eq!(length_text(300), "5:00");
        assert_eq!(length_text(65), "1:05");
        assert_eq!(length_text(3_725), "62:05");
        // A game that an older version saved has no length.
        assert_eq!(length_text(0), "–");
    }

    #[test]
    fn a_moment_is_the_clock_and_the_calendar_of_the_zone() {
        // 09:20:31 UTC on the 23rd of October 2026, in Turkey (UTC+3).
        assert_eq!(
            moment(1_792_747_231, 3 * 3600),
            Moment {
                day_no: 20_749,
                year: 2026,
                month: 10,
                day: 23,
                hour: 12,
                minute: 20
            }
        );
        assert_eq!(
            moment(1_792_747_231, 0),
            Moment {
                day_no: 20_749,
                year: 2026,
                month: 10,
                day: 23,
                hour: 9,
                minute: 20
            }
        );
        // A zone behind UTC goes back over midnight: 01:00 UTC on 1970-01-02 is 23:00 on 1970-01-01 at UTC-2.
        assert_eq!(
            moment(86_400 + 3600, -2 * 3600),
            Moment {
                day_no: 0,
                year: 1970,
                month: 1,
                day: 1,
                hour: 23,
                minute: 0
            }
        );
        // Before 1970 the day number is negative and the clock still runs forward.
        assert_eq!(
            moment(-1, 0),
            Moment {
                day_no: -1,
                year: 1969,
                month: 12,
                day: 31,
                hour: 23,
                minute: 59
            }
        );
    }

    #[test]
    fn a_game_date_reads_in_local_time_as_today_yesterday_or_a_short_date() {
        let at = |text: &str| history::parse_iso(text).unwrap();
        // Turkey is UTC+3 all year. Now is 18:40 on the 29th of October there.
        let turkey = |_: i64| 3 * 3600;
        let now = at("2026-10-29T15:40:00Z");
        let cases = [
            ("2026-10-29T12:31:00Z", "Today 15:31", "Bugün 15:31"),
            // The local day decides, not the UTC date: 01:30 on the 29th is today, though UTC still says the 28th.
            ("2026-10-28T22:30:00Z", "Today 01:30", "Bugün 01:30"),
            ("2026-10-28T21:00:00Z", "Today 00:00", "Bugün 00:00"),
            ("2026-10-28T20:59:00Z", "Yesterday 23:59", "Dün 23:59"),
            ("2026-10-28T18:04:00Z", "Yesterday 21:04", "Dün 21:04"),
            ("2026-10-27T21:00:00Z", "Yesterday 00:00", "Dün 00:00"),
            // Two days ago is a date. The day has no leading zero.
            ("2026-10-27T20:59:00Z", "Oct 27", "27 Eki"),
            ("2026-10-26T09:00:00Z", "Oct 26", "26 Eki"),
            ("2026-01-01T00:00:00Z", "Jan 1", "1 Oca"),
            // 01:00 on the 1st of January 2026 in Turkey is the year 2025 in UTC. The year that counts is the local one.
            ("2025-12-31T22:00:00Z", "Jan 1", "1 Oca"),
            // Another year shows the year, so "Oct 26" never means the wrong year.
            ("2025-10-26T09:00:00Z", "Oct 26, 2025", "26 Eki 2025"),
            ("2025-12-31T20:59:00Z", "Dec 31, 2025", "31 Ara 2025"),
            // A game after now (the clock of the PC went back) reads as today.
            ("2026-10-30T10:00:00Z", "Today 13:00", "Bugün 13:00"),
        ];
        for (date, en, tr) in cases {
            assert_eq!(game_date(Lang::En, date, now, &turkey), en, "{date}");
            assert_eq!(game_date(Lang::Tr, date, now, &turkey), tr, "{date}");
        }
        // No date, or a text that is no date: a dash.
        for date in ["", "2026-10-29", "yesterday"] {
            assert_eq!(game_date(Lang::En, date, now, &turkey), "–", "{date:?}");
            assert_eq!(game_date(Lang::Tr, date, now, &turkey), "–", "{date:?}");
        }
        // A zone behind UTC (UTC-5): now is 22:00 on the 28th, so a game 1 hour before is today and one at 23:59 on the 27th is yesterday.
        let behind = |_: i64| -5 * 3600;
        let now = at("2026-10-29T03:00:00Z");
        assert_eq!(
            game_date(Lang::En, "2026-10-29T02:00:00Z", now, &behind),
            "Today 21:00"
        );
        assert_eq!(
            game_date(Lang::Tr, "2026-10-28T04:59:00Z", now, &behind),
            "Dün 23:59"
        );
    }

    #[test]
    fn a_game_date_uses_the_offset_of_its_own_day_when_the_clocks_change() {
        let at = |text: &str| history::parse_iso(text).unwrap();
        // Central Europe: UTC+2 in summer time, and UTC+1 from 01:00 UTC on the 25th of October 2026.
        let change = at("2026-10-25T01:00:00Z");
        let europe = move |utc: i64| if utc < change { 2 * 3600 } else { 3600 };
        let now = at("2026-10-26T10:00:00Z");
        // 00:30 on the 25th (summer time) is yesterday. With the offset of now (UTC+1) it would be 23:30 on the 24th and read "Oct 24".
        assert_eq!(
            game_date(Lang::En, "2026-10-24T22:30:00Z", now, &europe),
            "Yesterday 00:30"
        );
        // 02:30 on the 25th, before and after the change: the same local time, the same day.
        assert_eq!(
            game_date(Lang::En, "2026-10-25T00:30:00Z", now, &europe),
            "Yesterday 02:30"
        );
        assert_eq!(
            game_date(Lang::En, "2026-10-25T01:30:00Z", now, &europe),
            "Yesterday 02:30"
        );
        assert_eq!(
            game_date(Lang::Tr, "2026-10-24T21:59:00Z", now, &europe),
            "24 Eki"
        );
    }

    #[test]
    fn recent_games_list_ten_rows_and_show_all_lists_a_hundred_at_most() {
        assert_eq!((games_listed(0, false), games_listed(0, true)), (0, 0));
        assert_eq!((games_listed(7, false), games_listed(7, true)), (7, 7));
        assert_eq!((games_listed(10, false), games_listed(10, true)), (10, 10));
        assert_eq!((games_listed(11, false), games_listed(11, true)), (10, 11));
        assert_eq!(
            (games_listed(100, false), games_listed(100, true)),
            (10, 100)
        );
        // The file keeps 500 games. The list stops at 100.
        assert_eq!(
            (games_listed(500, false), games_listed(500, true)),
            (10, 100)
        );
    }

    /// `n` games for the list of recent games, newest first. The K/D/A tells the row: game 0 has
    /// "0/4/9", game 1 has "1/4/9". With the zone UTC+3 and the time 15:40 on the 29th of October
    /// 2026 (`NOW`): game 0 is "Today 15:31" and game 1 is "Yesterday 21:04", and the games from 2
    /// on are one day apart, from the 26th back. Game 3 has no length, game 4 has six augments,
    /// and game 5 has an augment that the data does not know. Every third game is a loss (0, 3, ...).
    fn dated_games(n: i64) -> Arc<Vec<Game>> {
        let at = |text: &str| history::parse_iso(text).unwrap();
        let game = |i: i64| {
            let date = match i {
                0 => "2026-10-29T12:31:00Z".to_string(),
                1 => "2026-10-28T18:04:00Z".to_string(),
                _ => history::iso_date((at("2026-10-26T09:00:00Z") - (i - 2) * 86_400) * 1000),
            };
            let augments = match i {
                4 => vec![1, 2, 3, 7, 1, 2],
                5 => vec![1, 99],
                _ => vec![1, 2],
            };
            Game {
                id: n - i,
                date,
                queue: 2400,
                champion: 1 + i % 40,
                augments,
                win: i % 3 != 0,
                kills: i as u32,
                deaths: 4,
                assists: 9,
                secs: if i == 3 { 0 } else { 1200 },
            }
        };
        Arc::new((0..n).map(game).collect())
    }

    /// The time of `dated_games`: 15:40 UTC on the 29th of October 2026, and the zone UTC+3.
    fn games_clock() -> Option<(i64, i64)> {
        Some((
            history::parse_iso("2026-10-29T15:40:00Z").unwrap(),
            3 * 3600,
        ))
    }

    /// The My games page on one egui context, so that a click can run over several frames.
    struct GamesFrames {
        ctx: egui::Context,
        n: usize,
    }

    impl GamesFrames {
        fn new() -> Self {
            let ctx = egui::Context::default();
            theme::setup(&ctx);
            Self { ctx, n: 0 }
        }

        /// One frame of the window with these events. A frame is 0.1 seconds after the one before.
        fn frame(
            &mut self,
            app: &mut App,
            v: &View,
            events: Vec<egui::Event>,
        ) -> Vec<egui::epaint::ClippedShape> {
            self.n += 1;
            let screen = Rect::from_min_size(pos2(0.0, 0.0), vec2(MAIN_W, 680.0));
            let input = egui::RawInput {
                time: Some(self.n as f64 / 10.0),
                screen_rect: Some(screen),
                events,
                ..Default::default()
            };
            let mut out = self.ctx.run_ui(input, |ui| {
                app.window_ui(ui, v);
            });
            let shapes = std::mem::take(&mut out.shapes);
            out.drop_without_applying_deltas();
            shapes
        }

        /// Some frames without events, so the layout settles. The shapes of the last one.
        fn settle(&mut self, app: &mut App, v: &View) -> Vec<egui::epaint::ClippedShape> {
            let mut shapes = Vec::new();
            for _ in 0..4 {
                shapes = self.frame(app, v, vec![]);
            }
            shapes
        }

        /// A click: the mouse moves to the point, presses and lets go. The shapes after it.
        fn click(
            &mut self,
            app: &mut App,
            v: &View,
            at: egui::Pos2,
        ) -> Vec<egui::epaint::ClippedShape> {
            let button = |pressed| egui::Event::PointerButton {
                pos: at,
                button: egui::PointerButton::Primary,
                pressed,
                modifiers: egui::Modifiers::NONE,
            };
            self.frame(app, v, vec![egui::Event::PointerMoved(at)]);
            self.frame(app, v, vec![button(true)]);
            self.frame(app, v, vec![button(false)]);
            self.settle(app, v)
        }
    }

    /// The texts that look like a K/D/A ("12/4/9"): one for each row of "Recent games", in paint order.
    fn row_boxes(boxes: &[(String, Rect, Color32)]) -> Vec<(String, Rect, Color32)> {
        let is_kda = |t: &str| {
            t.split('/').count() == 3
                && t.split('/')
                    .all(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()))
        };
        boxes.iter().filter(|(t, ..)| is_kda(t)).cloned().collect()
    }

    /// The scroll offset that puts a text that sits at `y` in the window (it is `y` at offset 250) at `want`.
    fn scroll_to(y: f32, want: f32) -> f32 {
        250.0 + y - want
    }

    #[test]
    fn my_games_lists_the_newest_ten_games_with_the_result_the_score_the_length_the_date_and_the_augments()
     {
        let sd = window_data(40);
        for lang in [Lang::En, Lang::Tr] {
            let v = View {
                phase: Phase::Idle,
                offer: None,
                sd: Some(sd.clone()),
                history: dated_games(120),
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Games;
            app.debug_clock = games_clock();
            let shapes = window_frame(&mut app, &v, None);
            let boxes = painted_boxes(&shapes);
            let rows = row_boxes(&boxes);
            // Ten rows, the newest first, in a column. A row is 44 points high and a line of 1 point sits between two rows.
            assert_eq!(
                rows.iter().map(|(t, ..)| t.clone()).collect::<Vec<_>>(),
                (0..10).map(|i| format!("{i}/4/9")).collect::<Vec<_>>(),
                "{lang:?}"
            );
            assert!(
                rows.windows(2).all(|w| (w[1].1.center().y
                    - w[0].1.center().y
                    - (GAME_ROW_H + 1.0))
                    .abs()
                    < 0.01),
                "{lang:?}: {rows:?}"
            );
            // The section sits under the "Last 10" row and over the champions (the champion rows come after the last game).
            let y = |t: &str| {
                boxes
                    .iter()
                    .find(|(x, ..)| x == t)
                    .map(|b| b.1.center().y)
                    .unwrap_or_else(|| panic!("{lang:?}: {t:?} is missing in {boxes:?}"))
            };
            let title = y(&lang.upper(lang.t(Key::GamesRecent)));
            assert!(
                y(lang.t(Key::GamesLast10)) < title && title < rows[0].1.center().y,
                "{lang:?}"
            );
            let first_champion = boxes
                .iter()
                .find(|(t, ..)| t.starts_with("Nunu ve Willump"))
                .unwrap_or_else(|| panic!("{lang:?}: no champion row in {boxes:?}"));
            assert!(
                rows[9].1.center().y < first_champion.1.center().y,
                "{lang:?}: the champions follow the games"
            );
            // The date and the length of the rows: the first line has the date at the right, the second the length.
            let discs = rects_where(&shapes, |r| {
                r.fill == SURFACE_2
                    && (r.rect.width() - GAME_ICON).abs() < 0.01
                    && r.corner_radius == CornerRadius::same(16)
            });
            let tiles = rects_where(&shapes, |r| {
                r.fill == SURFACE_2
                    && (r.rect.width() - GAME_AUG).abs() < 0.01
                    && r.corner_radius == CornerRadius::same(GAME_AUG_RADIUS)
            });
            let mut date_right = Vec::new();
            for (i, (_, kda, _)) in rows.iter().enumerate() {
                let line = kda.center().y;
                let (top, bottom) = (line - 13.0, line - 13.0 + GAME_ROW_H);
                let text_on = |wanted: &str, dy: f32| {
                    boxes
                        .iter()
                        .filter(|(t, r, _)| t == wanted && (r.center().y - line - dy).abs() < 0.6)
                        .map(|b| b.1)
                        .collect::<Vec<_>>()
                };
                // The result: the letter (W or L, G or M) in the color of the result, left of the score, and the mark: a disc or a ring.
                let win = i % 3 != 0;
                let (letter, ink) = (
                    lang.t(if win {
                        Key::GameWinShort
                    } else {
                        Key::GameLossShort
                    }),
                    if win { GREEN } else { RED },
                );
                let letters: Vec<_> = boxes
                    .iter()
                    .filter(|(t, r, c)| {
                        t == letter && (r.center().y - line).abs() < 0.6 && *c == ink
                    })
                    .collect();
                assert_eq!(letters.len(), 1, "{lang:?} row {i}: the letter {letter:?}");
                assert!(letters[0].1.right() < kda.left(), "{lang:?} row {i}");
                let marks = circles_where(&shapes, |c| (c.center.y - line).abs() < 0.6);
                assert_eq!(marks.len(), 1, "{lang:?} row {i}: {marks:?}");
                if win {
                    assert!(
                        marks[0].radius == 5.0 && marks[0].fill == GREEN,
                        "{lang:?} row {i}: {:?}",
                        marks[0]
                    );
                } else {
                    assert!(
                        marks[0].radius == 4.25
                            && marks[0].fill == Color32::TRANSPARENT
                            && marks[0].stroke.color == RED
                            && marks[0].stroke.width == 1.5,
                        "{lang:?} row {i}: {:?}",
                        marks[0]
                    );
                }
                assert!(
                    marks[0].center.x < letters[0].1.left(),
                    "{lang:?} row {i}: the mark sits left of the letter"
                );
                // The date.
                let date = match i {
                    0 => lang.fmt(Key::GameToday, &[&"15:31"]),
                    1 => lang.fmt(Key::GameYesterday, &[&"21:04"]),
                    _ => lang.fmt(Key::GameDate, &[&(28 - i), &lang.month(10)]),
                };
                let dates = text_on(&date, 0.0);
                assert_eq!(dates.len(), 1, "{lang:?} row {i}: {date:?}");
                assert!(dates[0].left() > kda.right(), "{lang:?} row {i}");
                date_right.push(dates[0].right());
                // The length: on the second line, at the same right edge. A game with no length shows a dash.
                let length = if i == 3 { "–" } else { "20:00" };
                let lengths = text_on(length, 18.0);
                assert_eq!(lengths.len(), 1, "{lang:?} row {i}: {length:?}");
                assert!(
                    (lengths[0].right() - dates[0].right()).abs() < 0.01,
                    "{lang:?} row {i}"
                );
                // The icons paint for a row in view only. The first six rows are in view in a window of 680 points.
                if i >= 6 {
                    continue;
                }
                // The augments, in pick order, from the left: 18 point tiles 3 points apart, on the second line.
                let mut row_tiles: Vec<Rect> = tiles
                    .iter()
                    .filter(|t| t.center().y > top && t.center().y < bottom)
                    .copied()
                    .collect();
                row_tiles.sort_by(|a, b| a.left().total_cmp(&b.left()));
                let count = match i {
                    4 => 6,
                    _ => 2,
                };
                assert_eq!(row_tiles.len(), count, "{lang:?} row {i}: {row_tiles:?}");
                assert!(
                    row_tiles.iter().all(|t| t.top() == row_tiles[0].top()
                        && t.bottom() <= bottom
                        && t.top() > line),
                    "{lang:?} row {i}: {row_tiles:?}"
                );
                assert!(
                    row_tiles
                        .windows(2)
                        .all(|w| (w[1].left() - w[0].right() - GAME_AUG_GAP).abs() < 0.01),
                    "{lang:?} row {i}: {row_tiles:?}"
                );
                // The champion is a round icon of 32 points at the left edge of the row, in the middle of its height.
                let disc: Vec<_> = discs
                    .iter()
                    .filter(|d| d.center().y > top && d.center().y < bottom)
                    .collect();
                assert_eq!(disc.len(), 1, "{lang:?} row {i}: {discs:?}");
                assert!(
                    (disc[0].center().y - (top + GAME_ROW_H / 2.0)).abs() < 0.01,
                    "{lang:?} row {i}: {disc:?}"
                );
                assert!(
                    (row_tiles[0].left() - disc[0].right() - GAME_TEXT_GAP).abs() < 0.01,
                    "{lang:?} row {i}: the tiles start after the disc"
                );
            }
            // Every date ends at the same edge, the right edge of the card.
            assert!(
                date_right.windows(2).all(|w| (w[0] - w[1]).abs() < 0.01),
                "{lang:?}: {date_right:?}"
            );
            // 120 games: the button "Show all" ends the list, and the button for fewer rows is not there yet.
            let texts: Vec<&str> = boxes.iter().map(|(t, ..)| t.as_str()).collect();
            assert!(
                texts.contains(&lang.t(Key::GamesShowAll))
                    && !texts.contains(&lang.t(Key::GamesShowLess)),
                "{lang:?}"
            );
            // The augments are icons. Their names show in the hover only, and the rows show no number for an augment.
            assert!(
                !boxes.iter().any(|(t, ..)| t.contains("Sonsuz")),
                "{lang:?}: an augment name is a tooltip, not a row text"
            );
        }
    }

    #[test]
    fn show_all_lists_up_to_a_hundred_rows_and_show_fewer_takes_them_back() {
        let sd = window_data(40);
        for lang in [Lang::En, Lang::Tr] {
            let v = View {
                phase: Phase::Idle,
                offer: None,
                sd: Some(sd.clone()),
                history: dated_games(120),
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Games;
            app.debug_clock = games_clock();
            app.debug_scroll = Some(250.0);
            let mut page = GamesFrames::new();
            let shapes = page.settle(&mut app, &v);
            let boxes = painted_boxes(&shapes);
            assert_eq!(row_boxes(&boxes).len(), 10, "{lang:?}");
            // The button sits in the window, under the tenth row.
            let button = boxes
                .iter()
                .find(|(t, ..)| t == lang.t(Key::GamesShowAll))
                .unwrap_or_else(|| panic!("{lang:?}: {boxes:?}"))
                .1;
            assert!(
                button.center().y > row_boxes(&boxes)[9].1.center().y
                    && button.center().y < 680.0 - TITLE_H,
                "{lang:?}: {button:?}"
            );
            // A click on the left end of the button row, away from the text, lists 100 rows (of 120 games).
            let shapes = page.click(&mut app, &v, pos2(MARGIN + 30.0, button.center().y));
            let boxes = painted_boxes(&shapes);
            let rows = row_boxes(&boxes);
            assert_eq!(rows.len(), 100, "{lang:?}");
            assert_eq!(
                (rows[0].0.as_str(), rows[99].0.as_str()),
                ("0/4/9", "99/4/9"),
                "{lang:?}"
            );
            assert!(
                rows.windows(2)
                    .all(|w| w[1].1.center().y > w[0].1.center().y),
                "{lang:?}: newest first, top to bottom"
            );
            let texts: Vec<&str> = boxes.iter().map(|(t, ..)| t.as_str()).collect();
            assert!(
                texts.contains(&lang.t(Key::GamesShowLess))
                    && !texts.contains(&lang.t(Key::GamesShowAll)),
                "{lang:?}"
            );
            // The icons paint only for the rows in view: a few of the 100 rows, so the window does not ask for 600 icons at once.
            let discs = rects_where(&shapes, |r| {
                r.fill == SURFACE_2 && (r.rect.width() - GAME_ICON).abs() < 0.01
            });
            assert!(
                (1..=16).contains(&discs.len()),
                "{lang:?}: {} champion icons",
                discs.len()
            );
            // "Show fewer" is under the hundredth row. Scroll there, click it, and the list is short again.
            let button = boxes
                .iter()
                .find(|(t, ..)| t == lang.t(Key::GamesShowLess))
                .unwrap()
                .1;
            assert!(button.center().y > rows[99].1.center().y, "{lang:?}");
            app.debug_scroll = Some(scroll_to(button.center().y, 300.0));
            let shapes = page.settle(&mut app, &v);
            let button = painted_boxes(&shapes)
                .into_iter()
                .find(|(t, ..)| t == lang.t(Key::GamesShowLess))
                .unwrap()
                .1;
            assert!(
                button.center().y > TITLE_H && button.center().y < 680.0 - TITLE_H,
                "{lang:?}: {button:?}"
            );
            let shapes = page.click(&mut app, &v, pos2(MARGIN + 30.0, button.center().y));
            let boxes = painted_boxes(&shapes);
            assert_eq!(row_boxes(&boxes).len(), 10, "{lang:?}");
            assert!(
                boxes.iter().any(|(t, ..)| t == lang.t(Key::GamesShowAll))
                    && !boxes.iter().any(|(t, ..)| t == lang.t(Key::GamesShowLess)),
                "{lang:?}"
            );
            // Leaving the page puts the short list back: open "Show all", go to the main page, come back.
            app.games_all = true;
            app.page = Page::Main;
            window_frame(&mut app, &v, None);
            assert!(!app.games_all && !app.champions_all);
        }
    }

    #[test]
    fn ten_games_or_fewer_have_no_show_all_button_and_no_games_still_lists_nothing() {
        let sd = window_data(40);
        for lang in [Lang::En, Lang::Tr] {
            for n in [1, 7, 10] {
                let v = View {
                    phase: Phase::Idle,
                    offer: None,
                    sd: Some(sd.clone()),
                    history: dated_games(n),
                    ..pick_view()
                };
                let mut app = test_app(Settings::default(), lang);
                app.page = Page::Games;
                app.debug_clock = games_clock();
                let boxes = painted_boxes(&window_frame(&mut app, &v, None));
                assert_eq!(row_boxes(&boxes).len(), n as usize, "{lang:?} {n} games");
                assert!(
                    !boxes
                        .iter()
                        .any(|(t, ..)| t == lang.t(Key::GamesShowAll)
                            || t == lang.t(Key::GamesShowLess)),
                    "{lang:?} {n} games"
                );
            }
            // The first read of the history shows the empty page, not an empty list.
            let v = View {
                phase: Phase::Idle,
                offer: None,
                sd: Some(sd.clone()),
                history: Arc::new(Vec::new()),
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Games;
            let boxes = painted_boxes(&window_frame(&mut app, &v, None));
            assert!(
                row_boxes(&boxes).is_empty()
                    && !boxes
                        .iter()
                        .any(|(t, ..)| *t == lang.upper(lang.t(Key::GamesRecent))),
                "{lang:?}"
            );
        }
    }

    /// 15 games over 6 champions. The newest game is the only one of champion 6, which is the
    /// least played (the single game that you cannot find in a list of the five most played).
    fn games_of_six_champions() -> Arc<Vec<Game>> {
        let champions = [6, 1, 1, 1, 1, 2, 2, 2, 3, 3, 3, 4, 4, 5, 5];
        let game = |i: usize| Game {
            id: (100 - i) as i64,
            date: String::new(),
            queue: 2400,
            champion: champions[i],
            augments: vec![1, 2],
            // All wins, so the order of the champions is the number of games, then the id.
            win: true,
            kills: i as u32,
            deaths: 4,
            assists: 9,
            secs: 1200,
        };
        Arc::new((0..champions.len()).map(game).collect())
    }

    /// The champion names that the page paints, in order.
    fn champion_names(shapes: &[egui::epaint::ClippedShape]) -> Vec<String> {
        painted_boxes(shapes)
            .into_iter()
            .map(|b| b.0)
            .filter(|t| t.starts_with("Nunu ve Willump"))
            .collect()
    }

    #[test]
    fn the_champion_list_keeps_five_and_the_switch_all_champions_lists_every_one() {
        let sd = window_data(40);
        for lang in [Lang::En, Lang::Tr] {
            let v = View {
                phase: Phase::Idle,
                offer: None,
                sd: Some(sd.clone()),
                history: games_of_six_champions(),
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Games;
            app.debug_clock = games_clock();
            app.debug_scroll = Some(250.0);
            let mut page = GamesFrames::new();
            let shapes = page.settle(&mut app, &v);
            // The five most played, the most games first. Champion 6 played once and is not among them.
            let five: Vec<String> = [1, 2, 3, 4, 5]
                .iter()
                .map(|i| format!("Nunu ve Willump {i}"))
                .collect();
            assert_eq!(champion_names(&shapes), five, "{lang:?}");
            // The switch comes first in the card, over the first champion.
            let boxes = painted_boxes(&shapes);
            let switch = boxes
                .iter()
                .find(|(t, ..)| t == lang.t(Key::GamesAllChampions))
                .unwrap_or_else(|| panic!("{lang:?}: {boxes:?}"))
                .1;
            let first = boxes
                .iter()
                .find(|(t, ..)| t == "Nunu ve Willump 1")
                .unwrap()
                .1;
            assert!(
                switch.center().y < first.top()
                    && switch.center().y > TITLE_H
                    && switch.center().y < 680.0 - TITLE_H,
                "{lang:?}: {switch:?} {first:?}"
            );
            // A click on the row, away from the label and the switch, lists every champion with a game: six, with the single game's champion last.
            let shapes = page.click(&mut app, &v, pos2(MARGIN + 190.0, switch.center().y));
            let all: Vec<String> = (1..=6).map(|i| format!("Nunu ve Willump {i}")).collect();
            assert_eq!(champion_names(&shapes), all, "{lang:?}");
            assert!(app.champions_all, "{lang:?}");
            // The switch stays where it was: the list grows under it.
            let again = painted_boxes(&shapes)
                .into_iter()
                .find(|(t, ..)| t == lang.t(Key::GamesAllChampions))
                .unwrap()
                .1;
            assert!(
                (again.center().y - switch.center().y).abs() < 0.01,
                "{lang:?}: {again:?} {switch:?}"
            );
            // A second click takes the five back.
            let shapes = page.click(&mut app, &v, pos2(MARGIN + 190.0, switch.center().y));
            assert_eq!(champion_names(&shapes), five, "{lang:?}");
            assert!(!app.champions_all);
        }
        // Forty champions, three games each: the switch lists all forty.
        let v = View {
            phase: Phase::Idle,
            offer: None,
            sd: Some(sd.clone()),
            history: dated_games(120),
            ..pick_view()
        };
        let mut app = test_app(Settings::default(), Lang::En);
        app.page = Page::Games;
        app.debug_clock = games_clock();
        assert_eq!(
            champion_names(&window_frame(&mut app, &v, None)).len(),
            MY_CHAMPIONS
        );
        app.champions_all = true;
        assert_eq!(champion_names(&window_frame(&mut app, &v, None)).len(), 40);
        // Five champions or fewer: there is nothing more to list, so there is no switch.
        for lang in [Lang::En, Lang::Tr] {
            let v = View {
                phase: Phase::Idle,
                offer: None,
                sd: Some(sd.clone()),
                history: many_games(30, MY_CHAMPIONS as i64),
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Games;
            let boxes = painted_boxes(&window_frame(&mut app, &v, None));
            assert!(
                !boxes
                    .iter()
                    .any(|(t, ..)| t == lang.t(Key::GamesAllChampions)),
                "{lang:?}"
            );
            assert_eq!(
                boxes
                    .iter()
                    .filter(|(t, ..)| t.starts_with("Nunu ve Willump"))
                    .count(),
                MY_CHAMPIONS,
                "{lang:?}"
            );
        }
    }

    #[test]
    fn a_hover_on_a_row_names_the_champion_and_the_augments_in_pick_order() {
        let sd = window_data(40);
        for lang in [Lang::En, Lang::Tr] {
            let v = View {
                phase: Phase::Idle,
                offer: None,
                sd: Some(sd.clone()),
                history: dated_games(30),
                ..pick_view()
            };
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Games;
            app.debug_clock = games_clock();
            let mut page = GamesFrames::new();
            let boxes = painted_boxes(&page.settle(&mut app, &v));
            // Row 4 is a win with champion 5 and six augments: 1, 2, 3, 7, 1, 2 (the data names 1, 2, 3 and 7).
            let row = row_boxes(&boxes)[4].1;
            assert!(
                !boxes.iter().any(|(t, ..)| t.contains(". Sonsuz")),
                "{lang:?}: no tooltip before the hover"
            );
            let at = pos2(MARGIN + 100.0, row.center().y);
            page.frame(&mut app, &v, vec![egui::Event::PointerMoved(at)]);
            let mut texts = Vec::new();
            for _ in 0..12 {
                texts = painted_boxes(&page.frame(&mut app, &v, vec![]))
                    .into_iter()
                    .map(|b| b.0)
                    .collect();
            }
            let result = lang.t(Key::AfterWin);
            assert!(
                texts.contains(&format!("Nunu ve Willump 5 · {result}")),
                "{lang:?}: {texts:?}"
            );
            for (n, id) in [(1, 1), (2, 2), (3, 3), (4, 7), (5, 1), (6, 2)] {
                assert!(
                    texts.contains(&format!("{n}. Sonsuz Güç ve Zamanlama {id}")),
                    "{lang:?} {n}: {texts:?}"
                );
            }
        }
    }

    #[test]
    fn the_settings_page_holds_the_overlay_rows_and_the_way_to_quit() {
        let v = View {
            phase: Phase::Idle,
            offer: None,
            ..pick_view()
        };
        for lang in [Lang::En, Lang::Tr] {
            let mut app = test_app(Settings::default(), lang);
            app.page = Page::Settings;
            // The card reading tools stay folded: the three rows moved to the Overlay group show anyway.
            assert!(!app.tools_open);
            let mut texts: Vec<String> = Vec::new();
            for scroll in [None, Some(250.0), Some(500.0), Some(750.0), Some(10_000.0)] {
                texts.extend(window_texts(&mut app, &v, scroll));
            }
            let has = |t: &str| texts.iter().any(|x| x == t);
            for key in [
                Key::HotUnlock,
                Key::OverlaySize,
                Key::SizeSmall,
                Key::SizeMedium,
                Key::SizeLarge,
                Key::SetResetOverlay,
                Key::SetBadges,
                Key::SetBadgesNote,
                Key::QuitApp,
                Key::GamesSavedNote,
                Key::AboutText,
            ] {
                assert!(has(lang.t(key)), "{lang:?} {key:?}: {texts:?}");
            }
            // The hotkeys are keycaps: the keys F8 and F9, and Ctrl + F8 for the unlock.
            for key in ["F8", "F9", "Ctrl + F8"] {
                assert!(has(key), "{lang:?} {key}: {texts:?}");
            }
            // The card reading tools hold only the reader's own rows now.
            assert!(!has(lang.t(Key::ReadOcrLanguage)));
            app.tools_open = true;
            let tools = window_texts(&mut app, &v, Some(2000.0));
            assert!(
                tools.iter().any(|t| t == lang.t(Key::ReadOcrLanguage))
                    || tools.iter().any(|t| t == lang.t(Key::SaveScan)),
                "{lang:?}: {tools:?}"
            );
        }
        // The keys are keycaps: a thin LABEL_3 edge, no fill, radius 6.
        let mut app = test_app(Settings::default(), Lang::En);
        app.page = Page::Settings;
        let shapes = window_frame(&mut app, &v, None);
        let caps = rects_where(&shapes, |r| {
            r.stroke.color == LABEL_3
                && r.stroke.width == 1.0
                && r.fill == Color32::TRANSPARENT
                && r.corner_radius == CornerRadius::same(6)
        });
        assert!(
            caps.len() >= 2,
            "the hotkey rows have two keycaps: {caps:?}"
        );
        // The size sits in the settings file as 0, 1 or 2 and the row sets the same values.
        assert_eq!(Settings::default().overlay_scale, 1);
    }

    #[test]
    fn a_pick_card_names_only_a_small_sample() {
        // Three cards with 500, 5,000 and 12,000 games: low, medium and high confidence.
        let aug = |name: &str| Augment {
            name: name.into(),
            name_en: name.into(),
            icon: String::new(),
        };
        let mut sd = StaticData::default();
        for (id, name) in [(1, "Alpha"), (2, "Bravo"), (3, "Charlie")] {
            sd.augments.insert(id, aug(name));
        }
        let cs = ChampStats {
            augments: vec![
                st(1, 1, 0.55, 500),
                st(2, 2, 0.55, 5_000),
                st(3, 3, 0.55, 12_000),
            ],
            ..Default::default()
        };
        let v = View {
            sd: Some(Arc::new(sd)),
            details: Some(Arc::new(cs)),
            ..pick_view()
        };
        for lang in [Lang::En, Lang::Tr] {
            let app = test_app(Settings::default(), lang);
            let texts = part_texts(&app, |app, ui| app.pick_card(ui, &v));
            assert_eq!(
                texts.iter().filter(|t| *t == lang.t(Key::ConfLow)).count(),
                1,
                "{lang:?}: {texts:?}"
            );
            assert!(
                !texts
                    .iter()
                    .any(|t| t == lang.t(Key::ConfMid) || t == lang.t(Key::ConfHigh)),
                "{lang:?}: {texts:?}"
            );
        }
    }

    #[test]
    fn a_page_that_scrolls_fades_out_at_the_bottom_and_a_short_page_does_not() {
        let sd = window_data(24);
        let builds = (0..3)
            .map(|i| Build {
                name: ["Tank", "AP", "AD"][i].into(),
                items: (3001..=3006).collect(),
                ..Default::default()
            })
            .collect();
        let details = Arc::new(ChampStats {
            builds,
            ..(*pick_view().details.unwrap()).clone()
        });
        let select = |n: i64, trades: i64| View {
            phase: Phase::ChampSelect,
            cs: Some(champ_select(n, trades)),
            sd: Some(sd.clone()),
            details: Some(details.clone()),
            stats: Some(select_stats()),
            offer: None,
            item_sets_for: Some(1),
            ..pick_view()
        };
        let frame_meshes = |v: &View, monitor_h: f32| {
            let mut app = test_app(Settings::default(), Lang::En);
            let ctx = egui::Context::default();
            theme::setup(&ctx);
            let monitor = egui::ViewportInfo {
                monitor_size: Some(vec2(1920.0, monitor_h)),
                ..Default::default()
            };
            let mut found = Vec::new();
            for i in 0..10 {
                let input = egui::RawInput {
                    time: Some(i as f64 / 10.0),
                    screen_rect: Some(Rect::from_min_size(
                        pos2(0.0, 0.0),
                        vec2(MAIN_W, max_height_on(Some(monitor_h))),
                    )),
                    viewports: std::iter::once((egui::ViewportId::ROOT, monitor.clone())).collect(),
                    ..Default::default()
                };
                let mut out = ctx.run_ui(input, |ui| {
                    app.window_ui(ui, v);
                });
                found = meshes(&out.shapes);
                found.retain(|m| m.vertices.len() == 4 && m.vertices.iter().any(|v| v.color == BG));
                out.shapes.clear();
                out.drop_without_applying_deltas();
            }
            (found, app.main_h)
        };
        // The fullest champ select on a small screen scrolls: one fade, four vertices, clear at the top and BG at the bottom.
        let (found, page) = frame_meshes(&select(10, 2), 600.0);
        assert!(page.is_some_and(|h| h > room_of(520.0)), "{page:?}");
        assert_eq!(found.len(), 1);
        let colors: Vec<Color32> = found[0].vertices.iter().map(|v| v.color).collect();
        assert_eq!(colors, [Color32::TRANSPARENT, Color32::TRANSPARENT, BG, BG]);
        let ys: Vec<f32> = found[0].vertices.iter().map(|v| v.pos.y).collect();
        assert!(
            (ys[2] - ys[0] - SCROLL_FADE_H).abs() < 0.01 && ys[0] == ys[1] && ys[2] == ys[3],
            "{ys:?}"
        );
        // The shortest one fits, and has no fade.
        let (found, page) = frame_meshes(&select(1, 0), 760.0);
        assert!(page.is_some_and(|h| h <= room_of(680.0)), "{page:?}");
        assert!(found.is_empty());
    }
}
