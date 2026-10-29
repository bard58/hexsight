//! Badges on the game cards (beta). While an offer shows in a Mayhem game, a
//! second window that covers the game draws a tier badge and the game count above
//! each offered card. The badge shows no win rate. When the player turns on
//! "Highlight top-tier cards" (off by default), every top-tier card also gets a
//! green edge (`top_tier`).
//!
//! The window is transparent and lets every mouse click pass. It never takes the
//! focus. It is hidden from screen captures, so the card reader never reads it.
//! It exists only while the cards show. Everything that places a badge is a pure
//! function with a unit test. The window itself needs a screen.

use crate::app::{offer_stats, top_tier};
use crate::i18n::Lang;
use crate::kit;
use crate::model::ChampStats;
use crate::ocr::Geometry;
use crate::theme::*;
use crate::worker::Offer;
use egui::epaint::Shadow;
use egui::{Color32, CornerRadius, FontId, Pos2, Rect, Stroke, StrokeKind, Vec2, pos2, vec2};
use std::sync::atomic::{AtomicBool, AtomicIsize, AtomicU32, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Title of the badge window. The app finds the native window by this title.
const TITLE: &str = "Hexsight badges";

/// The window needs a few frames to get ready: Hexsight finds it, moves it over the
/// game, and hides it from captures. It draws nothing meanwhile. After this many
/// frames it draws anyway.
const SETUP_FRAMES: u32 = 6;

/// How many times Hexsight looks for the native window before it gives up.
const FIND_TRIES: u32 = 100;

/// One badge: where the title line of the card sits, and what to show.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Badge {
    /// Center of the title line (fractions of the capture). The badge sits at
    /// the top edge of the card, which is `CARD_TOP` above the title line.
    pub x: f32,
    pub y: f32,
    /// Tier of the augment. `None` without stats: the badge shows a question mark.
    pub tier: Option<u8>,
    /// Games in the sample. `None` without stats.
    pub games: Option<u32>,
    /// The card has the best tier of the offer, and the player turned the
    /// highlight on: it gets the green edge. Always `false` with the highlight off.
    pub top: bool,
    /// The sample has few games: the badge gets a dashed outline. The colors stay
    /// at full strength.
    pub dim: bool,
}

/// Everything that the badge window draws in one frame.
#[derive(Clone, Debug, PartialEq)]
pub struct Layer {
    pub geometry: Geometry,
    pub badges: Vec<Badge>,
    /// The language of the interface: the numbers follow it.
    pub lang: Lang,
}

/// The badges for an offer, in screen order. `None` when the scan did not read
/// the game window, because then the position of a card on the screen is not
/// known. `top_tier` decides which cards get the green edge, as on the pick
/// card, but only with `highlight` on (the setting "Highlight top-tier cards").
/// A badge never carries a win rate.
pub fn layer(
    lang: Lang,
    cs: Option<&ChampStats>,
    offer: &Offer,
    min_games: u32,
    highlight: bool,
) -> Option<Layer> {
    let geometry = offer.geometry?;
    let none = ChampStats::default();
    let cs = cs.unwrap_or(&none);
    let top = top_tier(cs, &offer.cards, offer.screen, min_games);
    let badges = offer_stats(cs, &offer.cards, offer.screen, min_games)
        .iter()
        .map(|r| Badge {
            x: r.x,
            y: r.y,
            tier: r.stat.as_ref().map(|a| a.tier),
            games: r.stat.as_ref().map(|a| a.games),
            top: highlight && top.ids.contains(&r.id),
            dim: r.stat.as_ref().is_some_and(|a| a.games < min_games),
        })
        .collect();
    Some(Layer {
        geometry,
        badges,
        lang,
    })
}

// ---------------------------------------------------------------------------
// Where a badge goes. Pure functions, unit tested below.

/// How far the top edge of a card sits above its title line, as a share of the
/// height of the capture. The augment icon sits between the two, so a badge that
/// sits on the title line covers the icon. The badge goes to the top edge of the
/// card instead. This is an estimate from the layout of the pick screen (the
/// preview in the main window draws its fake cards with the same number).
pub const CARD_TOP: f32 = 0.30;

/// The point in the badge window at the middle of the top edge of a card.
/// `x` and `y` are the center of the title line, as fractions of the capture.
/// `origin` is the screen pixel of the top left corner of the window. `ppp` is
/// the number of pixels in one point (the scale of the monitor).
pub fn card_top(g: &Geometry, x: f32, y: f32, origin: (i32, i32), ppp: f32) -> Pos2 {
    let (px, py) = g.crop.at(x, y - CARD_TOP);
    pos2((px - origin.0 as f32) / ppp, (py - origin.1 as f32) / ppp)
}

/// The size factor of the badges. They follow the size of the game: 1.0 for a
/// game that is 1080 pixels high. With this factor, the badge has the same share
/// of the card on each screen.
pub fn badge_scale(game_height: i32, ppp: f32) -> f32 {
    (game_height as f32 / 1080.0 / ppp).clamp(0.4, 4.0)
}

/// The size of a badge at scale 1, in points: the pill height, the room at its
/// ends, and the tier square. The tier is what the player reads first, so the square
/// is big (36, before 31, and 24 before the 1.3 times growth) and its letter is 18
/// points. The pill is 5 points higher than before (44, before 39), so the square keeps
/// its 4 points of room above and below.
const PILL_H: f32 = 44.0;
const PILL_PAD: f32 = 6.5;
const PILL_SQUARE: f32 = 36.0;
const TEXT_GAP: f32 = 10.0;
const TEXT_TAIL: f32 = 5.0;

/// The rects of one badge at scale `k`, in points: the pill, and the square for
/// the tier inside it. The pill sits on the top edge of the card: it is centered
/// on `top.x`, and the edge runs through the middle of its height. So half of
/// the pill is outside the card, and the augment icon stays free.
/// `text_w` is the width of the text next to the tier (0 without one).
fn pill_rects(top: Pos2, k: f32, text_w: f32) -> (Rect, Rect) {
    let (h, pad, square) = (PILL_H * k, PILL_PAD * k, PILL_SQUARE * k);
    let text = if text_w > 0.0 {
        TEXT_GAP * k + text_w + TEXT_TAIL * k
    } else {
        0.0
    };
    let w = pad + square + text + pad;
    let pill = Rect::from_min_size(pos2(top.x - w / 2.0, top.y - h / 2.0), vec2(w, h));
    let tier = Rect::from_min_size(
        pos2(pill.left() + pad, pill.center().y - square / 2.0),
        Vec2::splat(square),
    );
    (pill, tier)
}

/// The shift that brings `rect` inside `area`, with a small margin. Zero when it
/// sits inside already. A rect that is larger than the area aligns to the top left.
fn fit_in(rect: Rect, area: Rect) -> Vec2 {
    let area = area.shrink(4.0);
    let dx = if rect.left() < area.left() {
        area.left() - rect.left()
    } else {
        (area.right() - rect.right()).min(0.0)
    };
    let dy = if rect.top() < area.top() {
        area.top() - rect.top()
    } else {
        (area.bottom() - rect.bottom()).min(0.0)
    };
    vec2(dx, dy)
}

// ---------------------------------------------------------------------------
// Painting

/// The font of the game count: semibold, 14 points at scale 1 (before: bold 18.5). The
/// tier letter (18 points) is the bigger mark: the player reads the tier first.
fn text_font(k: f32) -> FontId {
    FontId::new(14.0 * k, semibold())
}

/// The text next to the tier square: the game count. `None` for a card without stats.
/// It has the grey of the second labels (`LABEL_2`), also for a sample with few games.
fn badge_text(p: &egui::Painter, lang: Lang, b: &Badge, k: f32) -> Option<Arc<egui::Galley>> {
    let games = lang.games(b.games?, &kit::fmt_n(lang, b.games?));
    let mut job = egui::text::LayoutJob::default();
    job.append(&games, 0.0, egui::TextFormat::simple(text_font(k), LABEL_2));
    Some(p.layout_job(job))
}

/// The outline of a plate (before: 1 point, white at 50 of 255).
fn plate_edge(k: f32) -> Stroke {
    Stroke::new((1.25 * k).max(1.0), Color32::from_white_alpha(PLATE_EDGE_A))
}

fn paint_badge(p: &egui::Painter, area: Rect, top: Pos2, k: f32, b: &Badge, lang: Lang) {
    let text = badge_text(p, lang, b, k);
    let (pill, tier) = pill_rects(top, k, text.as_ref().map_or(0.0, |g| g.size().x));
    let shift = fit_in(pill, area);
    let (pill, tier) = (pill.translate(shift), tier.translate(shift));
    if b.top {
        // A green glow around the pill (before: 55 % of the green).
        let glow = Shadow {
            offset: [0, 0],
            blur: (26.0 * k).min(255.0) as u8,
            spread: (2.0 * k).min(255.0) as u8,
            color: GREEN.gamma_multiply(0.8),
        };
        p.add(glow.as_shape(pill, CornerRadius::same(255)));
    }
    p.rect_filled(
        pill.translate(vec2(0.0, 3.0 * k)),
        CornerRadius::same(255),
        Color32::from_black_alpha(150),
    );
    p.rect_filled(pill, CornerRadius::same(255), PLATE);
    if b.top {
        p.rect_stroke(
            pill,
            CornerRadius::same(255),
            Stroke::new((2.5 * k).max(1.0), GREEN),
            StrokeKind::Inside,
        );
    } else if b.dim {
        // Few games: a dashed outline in place of fading the colors.
        kit::paint_dashed(p, pill, plate_edge(k), 5.0 * k, 3.5 * k);
    } else {
        p.rect_stroke(
            pill,
            CornerRadius::same(255),
            plate_edge(k),
            StrokeKind::Inside,
        );
    }
    kit::paint_tier(p, tier, b.tier);
    if let Some(g) = text {
        kit::shadow_galley(
            p,
            pos2(
                tier.right() + TEXT_GAP * k,
                pill.center().y - g.size().y / 2.0,
            ),
            g,
            LABEL_2,
            1.4 * k,
        );
    }
}

/// Draw the badges of a layer. `area` is the room in points (the whole window).
/// `origin` is the screen pixel of the top left corner of the window, and `ppp`
/// is its scale. The window over the game and the preview in the main window
/// both call this.
pub fn paint(p: &egui::Painter, area: Rect, layer: &Layer, origin: (i32, i32), ppp: f32) {
    let k = badge_scale(layer.geometry.game.h, ppp);
    let tops: Vec<(Pos2, &Badge)> = layer
        .badges
        .iter()
        .map(|b| (card_top(&layer.geometry, b.x, b.y, origin, ppp), b))
        .collect();
    // The top cards come last, so their glow lies over the others.
    for (top, b) in tops.iter().filter(|(_, b)| !b.top) {
        paint_badge(p, area, *top, k, b, layer.lang);
    }
    for (top, b) in tops.iter().filter(|(_, b)| b.top) {
        paint_badge(p, area, *top, k, b, layer.lang);
    }
}

// ---------------------------------------------------------------------------
// The window

/// What the badge window knows from one frame to the next. The callback of the window and the app share it.
#[derive(Default)]
struct Win {
    /// The native window. 0 until Hexsight finds it.
    hwnd: AtomicIsize,
    find_tries: AtomicU32,
    setup_tries: AtomicU32,
    /// The window is on the game, and hidden from captures.
    settled: AtomicBool,
    /// The game rect that the window was placed on last.
    placed: Mutex<Option<[i32; 4]>>,
    /// The test hook HEXSIGHT_BADGE_SHOT: when the window drew first, and whether it asked for its picture.
    first_draw: Mutex<Option<std::time::Instant>>,
    shot_asked: AtomicBool,
}

/// The badge window, seen from the app. Call `update` in each frame of the app.
#[derive(Default)]
pub struct Window {
    win: Arc<Win>,
    open: bool,
    last: Option<Arc<Layer>>,
}

fn viewport_id() -> egui::ViewportId {
    egui::ViewportId::from_hash_of("hexsight-badges")
}

impl Window {
    /// Show the window with this layer, or close it (`None`). egui closes a
    /// window that the app does not show in a frame.
    pub fn update(&mut self, ctx: &egui::Context, layer: Option<Layer>) {
        let Some(layer) = layer else {
            self.open = false;
            self.last = None;
            return;
        };
        if !self.open {
            // A new window: it has to find its native window again.
            self.open = true;
            self.win = Arc::new(Win::default());
        }
        let changed = self.last.as_deref() != Some(&layer);
        let layer = match (&self.last, changed) {
            (Some(old), false) => old.clone(),
            _ => Arc::new(layer),
        };
        // The first size and place. The window moves to the exact pixels when it
        // exists (`draw`), so the monitor scale cannot make it miss the game.
        let (ppp, g) = (ctx.pixels_per_point(), layer.geometry.game);
        let builder = egui::ViewportBuilder::default()
            .with_title(TITLE)
            .with_decorations(false)
            .with_transparent(true)
            .with_resizable(false)
            .with_always_on_top()
            .with_mouse_passthrough(true)
            .with_active(false)
            .with_taskbar(false)
            .with_position(pos2(g.x as f32 / ppp, g.y as f32 / ppp))
            .with_inner_size(vec2(g.w as f32 / ppp, g.h as f32 / ppp));
        let (shown, win) = (layer.clone(), self.win.clone());
        ctx.show_viewport_deferred(viewport_id(), builder, move |ui, _| draw(ui, &shown, &win));
        if changed {
            ctx.request_repaint_of(viewport_id());
        }
        self.last = Some(layer);
    }
}

/// One frame of the badge window.
fn draw(ui: &mut egui::Ui, layer: &Layer, win: &Win) {
    let ctx = ui.ctx().clone();
    let g = layer.geometry.game;
    let want = [g.x, g.y, g.x + g.w, g.y + g.h];
    let mut hwnd = win.hwnd.load(Ordering::Relaxed);
    if hwnd == 0 {
        // The first frame. Find the native window by its title before anything shows on it.
        if let Some(h) = crate::win::find_window(TITLE) {
            hwnd = h;
            win.hwnd.store(h, Ordering::Relaxed);
        }
    }
    if hwnd == 0 {
        // Without the native window, the window cannot hide from captures. It draws nothing.
        if win.find_tries.fetch_add(1, Ordering::Relaxed) < FIND_TRIES {
            ctx.request_repaint_after(Duration::from_millis(30));
        }
        return;
    }
    if !win.settled.load(Ordering::Relaxed) {
        // Move it over the game and hide it from captures. A frame without
        // drawing comes first, so nothing shows before the window is hidden.
        let changed = crate::win::fix_layer(hwnd, want);
        if changed && win.setup_tries.fetch_add(1, Ordering::Relaxed) + 1 < SETUP_FRAMES {
            ctx.request_repaint();
            return;
        }
        win.settled.store(true, Ordering::Relaxed);
        *win.placed.lock().unwrap() = Some(want);
    } else if win.placed.lock().unwrap().replace(want) != Some(want) {
        // The game moved. Follow it once.
        crate::win::fix_layer(hwnd, want);
        ctx.request_repaint();
    }
    // The real corner of the window gives the origin, so a small miss of the move does no harm.
    let origin = crate::win::window_rect(hwnd).map_or((g.x, g.y), |r| (r[0], r[1]));
    paint(
        ui.painter(),
        ui.max_rect(),
        layer,
        origin,
        ctx.pixels_per_point(),
    );
    // Test hook: HEXSIGHT_BADGE_SHOT=<png> saves what this window drew, once, after
    // HEXSIGHT_BADGE_SHOT_DELAY seconds (the stats need time to load). A screenshot
    // of the real window does not work on a locked PC.
    if let Some(path) = std::env::var_os("HEXSIGHT_BADGE_SHOT") {
        let delay: f32 = std::env::var("HEXSIGHT_BADGE_SHOT_DELAY")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(0.0);
        let since = win
            .first_draw
            .lock()
            .unwrap()
            .get_or_insert_with(std::time::Instant::now)
            .elapsed()
            .as_secs_f32();
        if since < delay {
            ctx.request_repaint_after(Duration::from_secs_f32(delay - since));
        } else if !win.shot_asked.swap(true, Ordering::Relaxed) {
            ctx.request_screenshot(move |img| save_shot(&img, std::path::Path::new(&path)));
        }
    }
}

/// Save a picture of the badge window as a PNG with a transparent background
/// (a test hook).
fn save_shot(img: &egui::ColorImage, path: &std::path::Path) {
    let [w, h] = img.size;
    let mut buf = Vec::with_capacity(w * h * 4);
    for c in &img.pixels {
        buf.extend_from_slice(&c.to_srgba_unmultiplied());
    }
    if let Some(shot) = image::RgbaImage::from_raw(w as u32, h as u32, buf) {
        let _ = shot.save(path);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::AugStat;
    use crate::ocr::{Card, Crop, crop_of};

    /// A 1920x1080 game at the screen origin, with the usual capture area.
    fn full_hd() -> Geometry {
        Geometry {
            game: Crop {
                x: 0,
                y: 0,
                w: 1920,
                h: 1080,
            },
            crop: crop_of(0, 0, 1920, 1080),
        }
    }

    fn near(a: f32, b: f32) -> bool {
        (a - b).abs() < 0.01
    }

    #[test]
    fn a_title_maps_from_the_capture_to_the_top_edge_of_its_card() {
        let g = full_hd();
        assert_eq!(
            g.crop,
            Crop {
                x: 192,
                y: 129,
                w: 1536,
                h: 712
            }
        );
        // x 0.5 is the middle of the capture: 192 + 768 = 960 pixels. The title
        // line at y 0.48 has its card top 0.30 higher, at 0.18: 129 + 128.16 pixels.
        let p = card_top(&g, 0.5, 0.48, (0, 0), 1.0);
        assert!(near(p.x, 960.0) && near(p.y, 257.16), "{p:?}");
        // Windows at 150 %: one point is 1.5 pixels.
        let p = card_top(&g, 0.5, 0.48, (0, 0), 1.5);
        assert!(near(p.x, 640.0) && near(p.y, 171.44), "{p:?}");
        // The window starts at the pixel (100, 50): everything moves by that.
        let p = card_top(&g, 0.5, 0.48, (100, 50), 1.0);
        assert!(near(p.x, 860.0) && near(p.y, 207.16), "{p:?}");
    }

    #[test]
    fn the_badge_sits_above_the_icon_and_not_on_the_title() {
        let g = full_hd();
        // The title line at y 0.48 is at pixel 129 + 0.48 * 712 = 470.8. The top edge of the
        // card is 0.30 * 712 = 213.6 pixels above it.
        let title = g.crop.at(0.5, 0.48).1;
        let top = card_top(&g, 0.5, 0.48, (0, 0), 1.0).y;
        assert!(near(title - top, CARD_TOP * 712.0), "{title} {top}");
        // The pill ends 22 pixels below the edge at scale 1 (it was 19.5, and 15 before): far above the title and its icon.
        let (pill, _) = pill_rects(pos2(960.0, top), 1.0, 40.0);
        assert!(
            pill.bottom() < top + 23.0 && pill.bottom() < title - 150.0,
            "{pill:?}"
        );
    }

    #[test]
    fn the_same_pixel_has_fewer_points_on_a_sharper_screen() {
        let g = full_hd();
        for ppp in [1.0, 1.25, 1.5, 1.75, 2.0, 3.0] {
            let p = card_top(&g, 0.2, 0.48, (0, 0), ppp);
            // Back to pixels: the scale gives the same pixel on every screen.
            assert!(near(p.x * ppp, 192.0 + 0.2 * 1536.0), "ppp {ppp}: {p:?}");
            assert!(near(p.y * ppp, 257.16), "ppp {ppp}: {p:?}");
        }
    }

    #[test]
    fn a_game_on_the_second_monitor_maps_to_its_own_window() {
        // A 2560x1440 game on a monitor to the right of the first one (1920 pixels wide).
        let game = Crop {
            x: 1920,
            y: 0,
            w: 2560,
            h: 1440,
        };
        let g = Geometry {
            game,
            crop: crop_of(1920, 0, 2560, 1440),
        };
        assert_eq!(
            g.crop,
            Crop {
                x: 2176,
                y: 172,
                w: 2048,
                h: 950
            }
        );
        // The window sits on the game, so its corner is the corner of the game.
        let p = card_top(&g, 0.2, 0.48, (1920, 0), 1.0);
        assert!(near(p.x, 256.0 + 0.2 * 2048.0), "{p:?}");
        assert!(near(p.y, 172.0 + 0.18 * 950.0), "{p:?}");
        // The same game, with the window 2 pixels off: the badge follows the real corner.
        let q = card_top(&g, 0.2, 0.48, (1922, 0), 1.0);
        assert!(near(q.x, p.x - 2.0), "{q:?}");
    }

    #[test]
    fn badges_grow_with_the_game_and_not_with_the_scale() {
        assert!(near(badge_scale(1080, 1.0), 1.0));
        // A 1440p game at 100 % has bigger cards than 1080p: bigger badges.
        assert!(near(badge_scale(1440, 1.0), 1.3333));
        // 4K at 150 %: the card is twice as high in pixels, a point is 1.5 pixels.
        assert!(near(badge_scale(2160, 1.5), 1.3333));
        // The badge in pixels is the same share of the game for every scale.
        for (h, ppp) in [
            (1080, 1.0f32),
            (1080, 1.25),
            (1440, 1.0),
            (1440, 1.5),
            (2160, 2.0),
            (2160, 1.5),
        ] {
            let pixels = badge_scale(h, ppp) * ppp;
            assert!(near(pixels, h as f32 / 1080.0), "{h} at {ppp}");
        }
        // A wild value stays in a sane range.
        assert_eq!(badge_scale(10, 1.0), 0.4);
        assert_eq!(badge_scale(100_000, 1.0), 4.0);
    }

    #[test]
    fn the_pill_sits_on_the_top_edge_of_the_card_and_centers_on_it() {
        let top = pos2(960.0, 460.0);
        let (pill, tier) = pill_rects(top, 1.0, 40.0);
        // The edge of the card runs through the middle of the pill.
        assert!(near(pill.center().y, top.y), "{pill:?}");
        assert!(near(pill.center().x, 960.0));
        // The tier square sits inside the pill, at its left end.
        assert!(pill.contains_rect(tier));
        assert!(tier.left() < pill.center().x);
        // The text makes the pill wider.
        let (plain, _) = pill_rects(top, 1.0, 0.0);
        assert!(plain.width() < pill.width());
        // At scale 2 the pill is twice as big, and it still sits on the same edge.
        let (big, _) = pill_rects(top, 2.0, 80.0);
        assert!(near(big.height(), pill.height() * 2.0));
        assert!(near(big.center().y, top.y));
    }

    #[test]
    fn the_tier_square_is_the_big_mark_of_the_badge_and_the_count_is_quiet() {
        // Before: a pill of 39 points with a square of 31 (and 30 and 24 before that).
        let (pill, tier) = pill_rects(pos2(500.0, 300.0), 1.0, 0.0);
        assert!(
            near(pill.height(), 44.0)
                && near(tier.width(), 36.0)
                && near(tier.left() - pill.left(), 6.5),
            "{pill:?} {tier:?}"
        );
        // The square keeps 4 points of room above and below, as before.
        assert!(
            near(tier.top() - pill.top(), 4.0) && near(pill.bottom() - tier.bottom(), 4.0),
            "{pill:?} {tier:?}"
        );
        // The tier letter is half the square: 18 points. The count is smaller, semibold, and grey.
        let f = text_font(1.0);
        assert!(near(f.size, 14.0) && f.family == semibold(), "{f:?}");
        assert!(
            f.size < tier.width() * 0.5,
            "the count must stay smaller than the tier letter"
        );
        // The text still sits inside the pill, with the room at its end.
        let (wide, _) = pill_rects(pos2(500.0, 300.0), 1.0, 50.0);
        assert!(
            near(wide.width(), 6.5 + 36.0 + 10.0 + 50.0 + 5.0 + 6.5),
            "{wide:?}"
        );
    }

    #[test]
    fn the_game_count_has_the_second_label_ink() {
        let ctx = egui::Context::default();
        crate::theme::setup(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
            ..Default::default()
        };
        let b = Badge {
            x: 0.5,
            y: 0.48,
            tier: Some(1),
            games: Some(5000),
            top: false,
            dim: false,
        };
        let mut inks = Vec::new();
        ctx.run_ui(input, |ui| {
            let g = badge_text(ui.painter(), Lang::En, &b, 1.0).unwrap();
            inks = g.job.sections.iter().map(|s| s.format.color).collect();
        })
        .drop_without_applying_deltas();
        assert!(
            !inks.is_empty() && inks.iter().all(|c| *c == LABEL_2),
            "the count must not use the white of the tier: {inks:?}"
        );
    }

    #[test]
    fn a_badge_near_the_edge_moves_inside_the_window() {
        let area = Rect::from_min_size(pos2(0.0, 0.0), vec2(1000.0, 500.0));
        let inside = Rect::from_min_size(pos2(100.0, 100.0), vec2(80.0, 26.0));
        assert_eq!(fit_in(inside, area), Vec2::ZERO);
        // Past the left and the top edge.
        let r = Rect::from_min_size(pos2(-30.0, -10.0), vec2(80.0, 26.0));
        let moved = r.translate(fit_in(r, area));
        assert!(
            near(moved.left(), 4.0) && near(moved.top(), 4.0),
            "{moved:?}"
        );
        // Past the right and the bottom edge.
        let r = Rect::from_min_size(pos2(950.0, 490.0), vec2(80.0, 26.0));
        let moved = r.translate(fit_in(r, area));
        assert!(
            near(moved.right(), 996.0) && near(moved.bottom(), 496.0),
            "{moved:?}"
        );
    }

    fn st(id: i64, tier: u8, win_rate: f32, games: u32) -> AugStat {
        AugStat {
            augment_id: id,
            win_rate,
            games,
            tier,
        }
    }

    fn offer(ids: [i64; 3], geometry: Option<Geometry>) -> Offer {
        let cards = ids
            .iter()
            .zip([0.2, 0.5, 0.8])
            .map(|(&id, x)| Card {
                id,
                x,
                y: 0.48,
                h: 0.03,
            })
            .collect();
        Offer {
            cards,
            screen: 1,
            geometry,
        }
    }

    fn stats() -> ChampStats {
        ChampStats {
            augments: vec![
                st(1, 1, 0.60, 5000),
                st(2, 2, 0.58, 5000),
                st(3, 5, 0.45, 100),
            ],
            ..Default::default()
        }
    }

    #[test]
    fn the_layer_marks_the_top_card_and_keeps_every_position() {
        let l = layer(
            Lang::En,
            Some(&stats()),
            &offer([3, 1, 2], Some(full_hd())),
            300,
            true,
        )
        .unwrap();
        assert_eq!(l.geometry, full_hd());
        // Screen order, as on the pick card. The card with 100 games is not eligible: only card 1 is on top.
        let order: Vec<(f32, Option<u8>, bool)> =
            l.badges.iter().map(|b| (b.x, b.tier, b.top)).collect();
        assert_eq!(
            order,
            vec![
                (0.2, Some(5), false),
                (0.5, Some(1), true),
                (0.8, Some(2), false)
            ]
        );
        // Each badge keeps the title line of its card, and shows its game count.
        assert!(l.badges.iter().all(|b| b.y == 0.48));
        assert_eq!(
            l.badges.iter().map(|b| b.games).collect::<Vec<_>>(),
            vec![Some(100), Some(5000), Some(5000)]
        );
        // The card with 100 games is under the minimum: dim.
        assert!(l.badges[0].dim && !l.badges[1].dim);
    }

    #[test]
    fn every_card_of_the_best_tier_is_marked() {
        let cs = ChampStats {
            augments: vec![
                st(1, 1, 0.60, 5000),
                st(2, 1, 0.58, 5000),
                st(3, 4, 0.45, 5000),
            ],
            ..Default::default()
        };
        let l = layer(
            Lang::En,
            Some(&cs),
            &offer([1, 3, 2], Some(full_hd())),
            300,
            true,
        )
        .unwrap();
        assert_eq!(
            l.badges.iter().map(|b| b.top).collect::<Vec<_>>(),
            vec![true, false, true]
        );
        // The same cards as the pick card marks.
        let top = top_tier(&cs, &offer([1, 3, 2], None).cards, 1, 300);
        assert_eq!(top.ids, vec![1, 2]);
        // Equal tiers: nobody is on top.
        let cs = ChampStats {
            augments: vec![
                st(1, 2, 0.60, 5000),
                st(2, 2, 0.58, 5000),
                st(3, 2, 0.45, 5000),
            ],
            ..Default::default()
        };
        let l = layer(
            Lang::En,
            Some(&cs),
            &offer([1, 2, 3], Some(full_hd())),
            300,
            true,
        )
        .unwrap();
        assert!(l.badges.iter().all(|b| !b.top));
    }

    #[test]
    fn the_green_edge_needs_the_highlight_setting() {
        // Off (the default of the setting): no card is on top. The tiers and the game counts stay.
        let l = layer(
            Lang::En,
            Some(&stats()),
            &offer([1, 2, 3], Some(full_hd())),
            300,
            false,
        )
        .unwrap();
        assert!(l.badges.iter().all(|b| !b.top));
        assert!(
            l.badges
                .iter()
                .all(|b| b.tier.is_some() && b.games.is_some())
        );
        assert_eq!(
            l.badges.iter().map(|b| b.tier).collect::<Vec<_>>(),
            vec![Some(1), Some(2), Some(5)]
        );
        // On: the best card is on top.
        let l = layer(
            Lang::En,
            Some(&stats()),
            &offer([1, 2, 3], Some(full_hd())),
            300,
            true,
        )
        .unwrap();
        assert_eq!(l.badges.iter().filter(|b| b.top).count(), 1);
    }

    #[test]
    fn a_badge_shows_no_win_rate_with_any_setting() {
        // The badge has no field for a win rate, and its text is the game count only.
        let ctx = egui::Context::default();
        crate::theme::setup(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(800.0, 600.0))),
            ..Default::default()
        };
        for (lang, highlight) in [
            (Lang::En, false),
            (Lang::En, true),
            (Lang::Tr, false),
            (Lang::Tr, true),
        ] {
            let l = layer(
                lang,
                Some(&stats()),
                &offer([1, 2, 3], Some(full_hd())),
                300,
                highlight,
            )
            .unwrap();
            let mut seen = Vec::new();
            ctx.run_ui(input.clone(), |ui| {
                for b in &l.badges {
                    seen.push(badge_text(ui.painter(), lang, b, 1.0).map(|g| g.text().to_string()));
                }
            })
            .drop_without_applying_deltas();
            let want: Vec<Option<String>> = l
                .badges
                .iter()
                .map(|b| b.games.map(|g| lang.games(g, &kit::fmt_n(lang, g))))
                .collect();
            assert_eq!(seen, want, "{lang:?} {highlight}");
            assert!(seen.iter().flatten().all(|t| !t.contains('%')), "{seen:?}");
        }
    }

    #[test]
    fn a_card_without_stats_gets_a_question_mark_and_no_edge() {
        // The champion has no stats at all: nobody is on top, cards keep their screen order.
        let l = layer(
            Lang::En,
            None,
            &offer([7, 8, 9], Some(full_hd())),
            300,
            true,
        )
        .unwrap();
        assert_eq!(
            l.badges.iter().map(|b| b.x).collect::<Vec<_>>(),
            vec![0.2, 0.5, 0.8]
        );
        assert!(
            l.badges
                .iter()
                .all(|b| b.tier.is_none() && b.games.is_none() && !b.top)
        );
        // One unknown card among known ones keeps its place and gets no edge.
        let l = layer(
            Lang::En,
            Some(&stats()),
            &offer([99, 2, 1], Some(full_hd())),
            300,
            true,
        )
        .unwrap();
        assert_eq!(
            l.badges.first().map(|b| (b.tier, b.top)),
            Some((None, false))
        );
        assert_eq!(
            l.badges.iter().map(|b| b.top).collect::<Vec<_>>(),
            vec![false, false, true]
        );
    }

    #[test]
    fn without_the_game_window_there_are_no_badges() {
        assert!(
            layer(
                Lang::En,
                Some(&stats()),
                &offer([1, 2, 3], None),
                300,
                false
            )
            .is_none()
        );
    }

    #[test]
    fn the_layer_keeps_the_language() {
        // A new language is a new layer, so the badge window draws again.
        let en = layer(
            Lang::En,
            Some(&stats()),
            &offer([1, 2, 3], Some(full_hd())),
            300,
            false,
        )
        .unwrap();
        let tr = layer(
            Lang::Tr,
            Some(&stats()),
            &offer([1, 2, 3], Some(full_hd())),
            300,
            false,
        )
        .unwrap();
        assert_eq!((en.lang, tr.lang), (Lang::En, Lang::Tr));
        assert_ne!(en, tr);
        assert_eq!(en.badges, tr.badges);
    }

    #[test]
    fn the_layer_paints_into_a_bare_context() {
        // The paint code runs without a window: three badges, one of them on top with the highlight on.
        for (lang, highlight) in [(Lang::En, false), (Lang::Tr, true), (Lang::En, true)] {
            let l = layer(
                lang,
                Some(&stats()),
                &offer([1, 2, 3], Some(full_hd())),
                300,
                highlight,
            )
            .unwrap();
            let ctx = egui::Context::default();
            // The badges use the app fonts (semibold and bold).
            crate::theme::setup(&ctx);
            let input = egui::RawInput {
                screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1920.0, 1080.0))),
                ..Default::default()
            };
            let out = ctx.run_ui(input, |ui| {
                paint(ui.painter(), ui.max_rect(), &l, (0, 0), 1.0)
            });
            // Each badge paints a shadow, a body, an edge, a tier badge, and its numbers.
            assert!(out.shapes.len() >= 20, "{lang:?}: {}", out.shapes.len());
            out.drop_without_applying_deltas();
        }
    }

    /// Paint a layer into a bare context and give back the shapes.
    fn shapes_of(l: &Layer) -> Vec<egui::epaint::ClippedShape> {
        let ctx = egui::Context::default();
        crate::theme::setup(&ctx);
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(1920.0, 1080.0))),
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| {
            paint(ui.painter(), ui.max_rect(), l, (0, 0), 1.0)
        });
        let shapes = out.shapes.clone();
        out.drop_without_applying_deltas();
        shapes
    }

    fn rects(shapes: &[egui::epaint::ClippedShape]) -> Vec<&egui::epaint::RectShape> {
        shapes
            .iter()
            .filter_map(|c| match &c.shape {
                egui::Shape::Rect(r) => Some(r),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn every_badge_sits_on_an_opaque_plate_with_a_light_outline() {
        let l = layer(
            Lang::En,
            Some(&stats()),
            &offer([1, 2, 3], Some(full_hd())),
            300,
            false,
        )
        .unwrap();
        let shapes = shapes_of(&l);
        let all = rects(&shapes);
        // One opaque plate for each pill: the art never shows through.
        let plates: Vec<_> = all.iter().filter(|r| r.fill == PLATE).collect();
        assert_eq!(plates.len(), 3);
        assert!(plates.iter().all(|r| r.fill.a() == 255));
        // Each pill with enough games has a solid outline of white at 150 of 255 (before: 50).
        let edges = all
            .iter()
            .filter(|r| {
                r.stroke.color == Color32::from_white_alpha(PLATE_EDGE_A) && r.stroke.width >= 1.25
            })
            .count();
        assert_eq!(edges, 2, "two pills have enough games, the third is dashed");
    }

    #[test]
    fn few_games_show_a_dashed_outline_and_not_a_faded_badge() {
        let enough = Layer {
            badges: vec![Badge {
                x: 0.5,
                y: 0.48,
                tier: Some(1),
                games: Some(5000),
                top: false,
                dim: false,
            }],
            ..layer(
                Lang::En,
                Some(&stats()),
                &offer([1, 2, 3], Some(full_hd())),
                300,
                false,
            )
            .unwrap()
        };
        let few = Layer {
            badges: vec![Badge {
                dim: true,
                games: Some(100),
                ..enough.badges[0]
            }],
            ..enough.clone()
        };
        let (a, b) = (shapes_of(&enough), shapes_of(&few));
        // The solid outline is one shape. The dashed one is many line pieces.
        assert!(b.len() > a.len() + 15, "{} against {}", b.len(), a.len());
        let dashes = b
            .iter()
            .filter(|c| {
                matches!(
                    c.shape,
                    egui::Shape::LineSegment { .. } | egui::Shape::Path(_)
                )
            })
            .count();
        assert!(dashes > 15, "{dashes}");
        // No fading: both badges use the same plate and the same tier color.
        let fills = |s: &[egui::epaint::ClippedShape]| {
            rects(s)
                .iter()
                .map(|r| r.fill)
                .filter(|c| *c != Color32::TRANSPARENT)
                .collect::<Vec<_>>()
        };
        assert_eq!(fills(&a), fills(&b));
        assert_eq!(kit::tier_ink(Some(1)), crate::theme::TIER_S);
    }

    #[test]
    fn the_top_card_keeps_its_solid_green_edge_and_a_stronger_glow() {
        let l = layer(
            Lang::En,
            Some(&stats()),
            &offer([1, 2, 3], Some(full_hd())),
            300,
            true,
        )
        .unwrap();
        let shapes = shapes_of(&l);
        let green = rects(&shapes)
            .iter()
            .filter(|r| r.stroke.color == GREEN && r.stroke.width >= 2.5)
            .count();
        assert_eq!(green, 1);
        // The glow is a shadow shape of green at 80 % (before 55 %).
        let glows = shapes.iter().filter(|c| matches!(&c.shape, egui::Shape::Rect(r) if r.fill == GREEN.gamma_multiply(0.8) && r.blur_width > 0.0)).count();
        assert_eq!(glows, 1);
    }
}
