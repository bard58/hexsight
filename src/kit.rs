//! Small custom widgets in the app style: segmented control, toggle switch,
//! tier badge, grouped list, confidence bars, icon button, app mark.

use crate::i18n::Lang;
use crate::theme::{self, *};
use egui::{
    Color32, CornerRadius, Rect, Response, Sense, Stroke, StrokeKind, Ui, Vec2, pos2, vec2,
};

/// The room that a label needs around its text inside a segment, in points.
pub const SEGMENT_PAD: f32 = 2.0;

/// The width of each segment of a control that is `total` points wide, given
/// the width of each label. The segments share the room equally. When a label
/// needs more than its share (a long word in a narrow window), each segment
/// gets the width of its label plus an equal extra, so no label reaches the
/// next one. When the labels cannot fit at all, the segments shrink in the
/// same ratio, and each label ends with "…".
pub fn segment_widths(total: f32, text: &[f32]) -> Vec<f32> {
    let n = text.len().max(1) as f32;
    let share = total / n;
    if text.iter().all(|t| t + SEGMENT_PAD <= share) {
        return vec![share; text.len()];
    }
    let sum: f32 = text.iter().sum();
    if sum >= total {
        return text.iter().map(|t| t * total / sum.max(1.0)).collect();
    }
    let extra = (total - sum) / n;
    text.iter().map(|t| t + extra).collect()
}

/// Apple-style segmented control. The selected pill slides between segments.
/// The segments have equal widths when every label fits (`segment_widths`). A
/// label that is wider than its segment ends with "…", so it never reaches the next one.
pub fn segmented<T: PartialEq + Copy>(
    ui: &mut Ui,
    id: &str,
    value: &mut T,
    items: &[(T, &str)],
) -> bool {
    let h = 30.0;
    let w = ui.available_width();
    let (rect, _) = ui.allocate_exact_size(vec2(w, h), Sense::hover());
    let n = items.len();
    if n == 0 {
        return false;
    }
    // The widest case of each label: the selected segment has the semibold face.
    let text: Vec<f32> = items
        .iter()
        .map(|(_, label)| {
            ui.painter()
                .layout_no_wrap(
                    label.to_string(),
                    egui::FontId::new(12.5, semibold()),
                    LABEL,
                )
                .size()
                .x
        })
        .collect();
    let widths = segment_widths(rect.width() - 4.0, &text);
    let lefts: Vec<f32> = widths
        .iter()
        .scan(0.0, |at, w| {
            let left = *at;
            *at += w;
            Some(left)
        })
        .collect();
    let p = ui.painter();
    p.rect_filled(
        rect,
        CornerRadius::same(9),
        Color32::from_rgba_unmultiplied(118, 118, 128, 48),
    );
    let sel = items.iter().position(|(v, _)| v == value).unwrap_or(0) as f32;
    // The pill follows the index, so a resize of the window never leaves it behind.
    let at = ui
        .ctx()
        .animate_value_with_time(ui.scope_id().with(id), sel, 0.18)
        .clamp(0.0, (n - 1) as f32);
    let (i, f) = (at.floor() as usize, at.fract());
    let j = (i + 1).min(n - 1);
    let lerp = |v: &[f32]| v[i] + (v[j] - v[i]) * f;
    let pill = Rect::from_min_size(
        pos2(rect.left() + 2.0 + lerp(&lefts), rect.top() + 2.0),
        vec2(lerp(&widths), h - 4.0),
    );
    p.rect_filled(
        pill.translate(vec2(0.0, 1.0)),
        CornerRadius::same(7),
        Color32::from_black_alpha(60),
    );
    p.rect_filled(pill, CornerRadius::same(7), Color32::from_rgb(99, 99, 102));
    let mut changed = false;
    for (i, (v, label)) in items.iter().enumerate() {
        let r = Rect::from_min_size(
            pos2(rect.left() + 2.0 + lefts[i], rect.top()),
            vec2(widths[i], h),
        );
        let resp = ui.interact(r, ui.scope_id().with((id, i)), Sense::click());
        let selected = v == value;
        if resp.clicked() && !selected {
            *value = *v;
            changed = true;
        }
        let font = egui::FontId::new(
            12.5,
            if selected {
                semibold()
            } else {
                egui::FontFamily::Proportional
            },
        );
        let col = if selected || resp.hovered() {
            LABEL
        } else {
            LABEL_2
        };
        let mut job = egui::text::LayoutJob::single_section(
            label.to_string(),
            egui::TextFormat::simple(font, col),
        );
        job.wrap.max_width = (widths[i] - SEGMENT_PAD).max(10.0);
        job.wrap.max_rows = 1;
        job.wrap.break_anywhere = true;
        let galley = ui.painter().layout_job(job);
        ui.painter()
            .galley(r.center() - galley.size() / 2.0, galley, col);
    }
    changed
}

/// The width of the track of a toggle switch, in points (before: 44).
pub const SWITCH_W: f32 = 40.0;

/// The height of the track of a toggle switch, in points (before: 26).
pub const SWITCH_H: f32 = 24.0;

/// The font size of the label of a toggle row (before: 13.5, the size of `body`).
const TOGGLE_FONT: f32 = 13.0;

/// The size of a toggle row for a label that is `text_h` points high: at least
/// 26 points, so the switch has room above and below.
fn toggle_row_height(text_h: f32) -> f32 {
    text_h.max(26.0)
}

/// iOS-style toggle switch with a label on the left. The whole row is the click
/// target, not only the switch: a click on the label flips it too. On hover the
/// row gets a light fill that reaches 6 points past the row on both sides.
pub fn toggle(ui: &mut Ui, on: &mut bool, label: &str) -> bool {
    let mut changed = false;
    // The label takes the room that the switch leaves. A long label wraps to a
    // second line in a narrow window, and the row grows with it.
    let room = (ui.available_width() - SWITCH_W - ui.spacing().item_spacing.x).max(40.0);
    let text = ui.painter().layout(
        label.to_string(),
        egui::FontId::proportional(TOGGLE_FONT),
        LABEL,
        room,
    );
    let row = vec2(ui.available_width(), toggle_row_height(text.size().y));
    let (rect, resp) = ui.allocate_exact_size(row, Sense::click());
    if resp.clicked() {
        *on = !*on;
        changed = true;
    }
    resp.widget_info(|| egui::WidgetInfo::selected(egui::Role::Switch, true, *on, label));
    let hover = ui
        .ctx()
        .animate_bool_with_time(resp.id.with("hover"), resp.hovered(), 0.12);
    let t = ui.ctx().animate_bool_with_time(resp.id, *on, 0.16);
    let p = ui.painter();
    if hover > 0.0 {
        p.rect_filled(
            rect.expand2(vec2(6.0, 0.0)),
            CornerRadius::same(8),
            Color32::from_white_alpha((8.0 * hover).round() as u8),
        );
    }
    p.galley(
        pos2(rect.left(), rect.center().y - text.size().y / 2.0),
        text,
        LABEL,
    );
    let track = Rect::from_center_size(
        pos2(rect.right() - SWITCH_W / 2.0, rect.center().y),
        vec2(SWITCH_W, SWITCH_H),
    );
    let off = Color32::from_rgba_unmultiplied(120, 120, 128, 80);
    p.rect_filled(
        track,
        CornerRadius::same((SWITCH_H / 2.0) as u8),
        lerp_color(off, GREEN, t),
    );
    // The knob is 2 points smaller than the track on each side.
    let knob = SWITCH_H / 2.0 - 2.0;
    let cx = egui::lerp(track.left() + knob + 2.0..=track.right() - knob - 2.0, t);
    p.circle_filled(
        pos2(cx, track.center().y + 1.0),
        knob,
        Color32::from_black_alpha(50),
    );
    p.circle_filled(pos2(cx, track.center().y), knob, Color32::WHITE);
    changed
}

fn lerp_color(a: Color32, b: Color32, t: f32) -> Color32 {
    let l = |x: u8, y: u8| (x as f32 + (y as f32 - x as f32) * t) as u8;
    Color32::from_rgba_premultiplied(
        l(a.r(), b.r()),
        l(a.g(), b.g()),
        l(a.b(), b.b()),
        l(a.a(), b.a()),
    )
}

/// Rounded tier badge: "S+", "S", "A" ... in a tinted square. `dim` marks a
/// sample with few games. The badge keeps the tier color and gets a dashed edge,
/// the same rule as `tier_ink` for the marks over the game. Before, `dim` painted
/// the whole badge in `LABEL_3`, a grey as light as the B tier, so a thin S looked
/// like a B or a C.
pub fn tier_badge(ui: &mut Ui, tier: u8, size: f32, dim: bool) -> Response {
    badge(
        ui,
        crate::model::TIER_NAMES[tier.min(5) as usize],
        size,
        theme::tier_color(tier),
        dim,
    )
}

/// Dim badge with a question mark. The augment has no stats.
pub fn unknown_badge(ui: &mut Ui, size: f32) -> Response {
    badge(ui, "?", size, LABEL_3, false)
}

fn badge(ui: &mut Ui, text: &str, size: f32, c: Color32, dashed: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(size), Sense::hover());
    paint_badge(ui.painter(), rect, text, c, dashed);
    resp
}

/// The ink of a tier mark over the game: the full tier color, never dimmed. A
/// sample with few games is shown by a dashed outline (`paint_dashed`), not by
/// fading. Without stats the mark is a "?" in a light grey.
pub fn tier_ink(tier: Option<u8>) -> Color32 {
    tier.map_or(LABEL_2, theme::tier_color)
}

/// Paint a badge into a square rect, without a widget. The badges over the game
/// and the bench icons use it. `tier` is a tier index, or `None` for the "?".
pub fn paint_tier(p: &egui::Painter, rect: Rect, tier: Option<u8>) {
    let text = tier.map_or("?", |t| crate::model::TIER_NAMES[t.min(5) as usize]);
    paint_badge(p, rect, text, tier_ink(tier), false);
}

/// Paint the muted badge of a champion without stats: a dash, not a tier.
pub fn paint_no_stats(p: &egui::Painter, rect: Rect) {
    paint_badge(p, rect, "—", LABEL_2, false);
}

/// The length of a dash on the edge of a badge, as a part of its size.
const BADGE_DASH: f32 = 0.18;

/// The length of a gap on the edge of a badge, as a part of its size.
const BADGE_GAP: f32 = 0.12;

/// A badge: an opaque plate tinted with the color, a full-strength edge, and the
/// text in the same color with a dark shadow. Before, the fill had 18 % of the
/// color on a see-through plate, the edge 45 %, and the text no shadow. With
/// `dashed`, the edge is a dashed line (`paint_dashed_rounded`) in the same color.
fn paint_badge(p: &egui::Painter, rect: Rect, text: &str, c: Color32, dashed: bool) {
    let size = rect.width();
    let radius = (size * 0.28) as u8;
    let stroke = Stroke::new((size * 0.05).max(1.25), c);
    p.rect_filled(rect, CornerRadius::same(radius), lerp_color(PLATE, c, 0.26));
    if dashed {
        paint_dashed_rounded(
            p,
            rect,
            f32::from(radius),
            stroke,
            size * BADGE_DASH,
            size * BADGE_GAP,
        );
    } else {
        p.rect_stroke(rect, CornerRadius::same(radius), stroke, StrokeKind::Inside);
    }
    shadow_text(
        p,
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        egui::FontId::new(size * 0.5, bold()),
        c,
        size * 0.04,
    );
}

/// Paint a laid-out text with a dark shadow `off` points down and right.
pub fn shadow_galley(
    p: &egui::Painter,
    pos: egui::Pos2,
    galley: std::sync::Arc<egui::Galley>,
    color: Color32,
    off: f32,
) {
    p.galley_with_override_text_color(
        pos + vec2(off, off),
        galley.clone(),
        Color32::from_black_alpha(SHADOW_A),
    );
    p.galley(pos, galley, color);
}

/// Paint one line of text with a dark shadow. `anchor` works as in `Painter::text`.
pub fn shadow_text(
    p: &egui::Painter,
    pos: egui::Pos2,
    anchor: egui::Align2,
    text: &str,
    font: egui::FontId,
    color: Color32,
    off: f32,
) {
    let galley = p.layout_no_wrap(text.to_string(), font, color);
    let at = anchor.anchor_size(pos, galley.size()).min;
    shadow_galley(p, at, galley, color, off);
}

/// The outline of a capsule (a rect with fully round ends) as a closed path of
/// points, clockwise from the top left. `arc` is the number of steps in each end.
pub fn capsule_path(rect: Rect, arc: usize) -> Vec<egui::Pos2> {
    let r = rect.height() / 2.0;
    let (left, right) = (
        pos2(rect.left() + r, rect.center().y),
        pos2(rect.right() - r, rect.center().y),
    );
    let mut path = Vec::with_capacity(arc * 2 + 4);
    for (c, from) in [(right, -90.0f32), (left, 90.0)] {
        for i in 0..=arc {
            let a = (from + 180.0 * i as f32 / arc as f32).to_radians();
            path.push(pos2(c.x + r * a.cos(), c.y + r * a.sin()));
        }
    }
    path.push(path[0]);
    path
}

/// A dashed outline around a capsule: the sign of a sample with few games, in
/// place of fading the mark. The outline runs inside the rect.
pub fn paint_dashed(p: &egui::Painter, rect: Rect, stroke: Stroke, dash: f32, gap: f32) {
    let path = capsule_path(rect.shrink(stroke.width / 2.0), 10);
    p.extend(egui::Shape::dashed_line(&path, stroke, dash, gap));
}

/// The outline of a rect with round corners of `radius` as a closed path of
/// points, clockwise from the top left. `arc` is the number of steps in each corner.
pub fn rounded_path(rect: Rect, radius: f32, arc: usize) -> Vec<egui::Pos2> {
    let r = radius.clamp(0.0, rect.width().min(rect.height()) / 2.0);
    let steps = arc.max(1);
    let mut path = Vec::with_capacity(steps * 4 + 5);
    let corners = [
        (pos2(rect.right() - r, rect.top() + r), -90.0f32),
        (pos2(rect.right() - r, rect.bottom() - r), 0.0),
        (pos2(rect.left() + r, rect.bottom() - r), 90.0),
        (pos2(rect.left() + r, rect.top() + r), 180.0),
    ];
    for (c, from) in corners {
        for i in 0..=steps {
            let a = (from + 90.0 * i as f32 / steps as f32).to_radians();
            path.push(pos2(c.x + r * a.cos(), c.y + r * a.sin()));
        }
    }
    path.push(path[0]);
    path
}

/// A dashed outline around a rect with round corners: the edge of a badge with
/// few games. `radius` is the corner radius of the plate. The outline runs inside
/// the rect, as the solid edge of a badge does. `paint_dashed` draws a capsule,
/// which would be a circle on a square badge and leave the plate corners bare.
pub fn paint_dashed_rounded(
    p: &egui::Painter,
    rect: Rect,
    radius: f32,
    stroke: Stroke,
    dash: f32,
    gap: f32,
) {
    let inset = stroke.width / 2.0;
    let path = rounded_path(rect.shrink(inset), (radius - inset).max(0.0), 6);
    p.extend(egui::Shape::dashed_line(&path, stroke, dash, gap));
}

/// The font of a pill.
fn pill_font() -> egui::FontId {
    egui::FontId::new(11.5, semibold())
}

/// The room around the text of a pill: left and right, and top and bottom.
fn pill_pad(dot: bool) -> Vec2 {
    vec2(if dot { 22.0 } else { 16.0 }, 8.0)
}

/// Capsule label, for example a status or a rarity.
pub fn pill(ui: &mut Ui, text: &str, color: Color32, dot: bool) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), pill_font(), color);
    let pad = pill_pad(dot);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + pad, Sense::hover());
    let p = ui.painter();
    p.rect_filled(rect, CornerRadius::same(255), color.gamma_multiply(0.16));
    let mut x = rect.left() + 8.0;
    if dot {
        p.circle_filled(pos2(x + 3.0, rect.center().y), 3.0, color);
        x += 9.0;
    }
    p.galley(
        pos2(x, rect.center().y - galley.size().y / 2.0),
        galley,
        color,
    );
    resp
}

/// Inset grouped container (like iOS Settings).
pub fn group<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(SURFACE)
        .corner_radius(CornerRadius::same(RADIUS))
        .stroke(Stroke::new(1.0, HAIRLINE))
        .inner_margin(egui::Margin::symmetric(12, 10))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// A card whose rows reach its edges: each row paints its own padding, so a
/// highlighted row can fill the card from side to side.
pub fn list<R>(ui: &mut Ui, add: impl FnOnce(&mut Ui) -> R) -> R {
    egui::Frame::new()
        .fill(SURFACE)
        .corner_radius(CornerRadius::same(RADIUS))
        .stroke(Stroke::new(1.0, HAIRLINE))
        .inner_margin(egui::Margin::symmetric(6, 6))
        .show(ui, |ui| {
            ui.set_width(ui.available_width());
            add(ui)
        })
        .inner
}

/// The green frame of a top-tier card or row: a light fill (18 %, before 12 %)
/// and a 1.5 point edge (before 1 point) at 90 % of the green (before 55 %).
/// The text "top" goes with it, so color is never the only cue.
pub fn paint_top(p: &egui::Painter, rect: Rect, radius: u8) {
    p.rect_filled(rect, CornerRadius::same(radius), GREEN.gamma_multiply(0.18));
    p.rect_stroke(
        rect,
        CornerRadius::same(radius),
        Stroke::new(1.5, GREEN.gamma_multiply(0.9)),
        StrokeKind::Inside,
    );
}

/// The width of the three confidence bars, in points.
pub const BARS_W: f32 = 34.0;

/// The height of a confidence bar (before: 4 points).
pub const BAR_H: f32 = 6.0;

/// Alpha of an unlit bar (before: 41 of 255).
pub const BAR_OFF_A: u8 = 90;

/// Three small bars that show how much data stands behind a number. `on` of
/// them (0 to 3) are lit. `at` is the top left corner of a 4 point bar: the
/// taller bar grows one point up and one down, so the middle stays.
pub fn paint_bars(p: &egui::Painter, at: egui::Pos2, on: u8) {
    for i in 0..3u8 {
        let bar = Rect::from_min_size(
            pos2(at.x + f32::from(i) * 12.0, at.y - (BAR_H - 4.0) / 2.0),
            vec2(10.0, BAR_H),
        );
        p.rect_filled(
            bar,
            CornerRadius::same(3),
            if i < on {
                Color32::WHITE
            } else {
                Color32::from_white_alpha(BAR_OFF_A)
            },
        );
    }
}

/// Section header above a group. The caller gives the text in capitals
/// (`Lang::upper`): Turkish capitals differ from the default rule.
pub fn section(ui: &mut Ui, text: &str) {
    ui.add_space(6.0);
    ui.label(
        egui::RichText::new(text)
            .size(11.0)
            .family(semibold())
            .color(LABEL_3),
    );
    ui.add_space(2.0);
}

pub fn hairline(ui: &mut Ui, inset: f32) {
    let r = ui.available_rect_before_wrap();
    let y = ui.cursor().top();
    ui.painter().hline(
        (r.left() + inset)..=r.right(),
        y,
        Stroke::new(1.0, HAIRLINE),
    );
    ui.add_space(1.0);
}

/// Round icon button with a hover fill.
pub fn icon_button(ui: &mut Ui, g: char, tip: &str, active: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(Vec2::splat(28.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let p = ui.painter();
    if t > 0.0 || active {
        let a = if active { 40 } else { (28.0 * t) as u8 };
        p.rect_filled(
            rect,
            CornerRadius::same(8),
            Color32::from_rgba_premultiplied(a, a, a, a),
        );
    }
    let col = if active {
        BLUE
    } else if resp.hovered() {
        LABEL
    } else {
        LABEL_2
    };
    p.text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        g,
        egui::FontId::new(13.0, icons()),
        col,
    );
    resp.on_hover_text(tip)
}

/// The font of an accent button.
fn button_font() -> egui::FontId {
    egui::FontId::new(12.5, semibold())
}

/// The width of an accent button with this text, in points.
pub fn accent_button_width(ui: &Ui, text: &str) -> f32 {
    ui.painter()
        .layout_no_wrap(text.to_string(), button_font(), Color32::WHITE)
        .size()
        .x
        + 24.0
}

/// Filled capsule button with an accent color.
pub fn accent_button(ui: &mut Ui, text: &str, color: Color32) -> Response {
    let galley = ui
        .painter()
        .layout_no_wrap(text.to_string(), button_font(), Color32::WHITE);
    let (rect, resp) = ui.allocate_exact_size(galley.size() + vec2(24.0, 10.0), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.12);
    let fill = color.gamma_multiply(0.85 + 0.15 * t);
    ui.painter()
        .rect_filled(rect, CornerRadius::same(255), fill);
    ui.painter()
        .galley(rect.center() - galley.size() / 2.0, galley, Color32::WHITE);
    resp
}

/// The height of a `text_button`, in points.
pub const TEXT_BUTTON_H: f32 = 30.0;

/// A button that is only text: the text is blue and centered in a row as wide as the card, and
/// a light fill shows under the mouse. For a quiet action at the end of a list ("Show all").
pub fn text_button(ui: &mut Ui, text: &str) -> Response {
    let (rect, resp) =
        ui.allocate_exact_size(vec2(ui.available_width(), TEXT_BUTTON_H), Sense::click());
    let t = ui
        .ctx()
        .animate_bool_with_time(resp.id, resp.hovered(), 0.1);
    if t > 0.0 {
        ui.painter().rect_filled(
            rect.expand2(vec2(6.0, 0.0)),
            CornerRadius::same(8),
            Color32::from_white_alpha((10.0 * t) as u8),
        );
    }
    ui.painter().text(
        rect.center(),
        egui::Align2::CENTER_CENTER,
        text,
        button_font(),
        BLUE,
    );
    if resp.hovered() {
        ui.ctx().set_cursor_icon(egui::CursorIcon::PointingHand);
    }
    resp.widget_info(|| egui::WidgetInfo::labeled(egui::Role::Button, true, text));
    resp
}

/// The app mark in the title bar and the overlay: the 64 px app icon, drawn small.
/// Mipmaps keep it smooth at 18 to 30 px. A new logo needs only new icon files
/// (tools\logo.py), no code change.
pub fn app_mark(ui: &Ui, rect: Rect) {
    let opts = egui::TextureOptions {
        mipmap_mode: Some(egui::TextureFilter::Linear),
        ..egui::TextureOptions::LINEAR
    };
    egui::Image::new(egui::include_image!("../assets/icon-64.png"))
        .texture_options(opts)
        .paint_at(ui, rect);
}

/// The width of the first ring of `glow`.
const GLOW_STROKE: f32 = 1.5;

/// How far `glow` reaches out of its rect. The last ring ends 7 points out. The
/// layout counts on 7.5, as before, so the gaps between cards stay as they are.
pub const GLOW_REACH: f32 = 6.0 + GLOW_STROKE;

/// The rings of `glow`, from the rect outwards: how far out the ring starts, its
/// width, and the part of the color that it keeps. The first ring is the edge of
/// the glow. The five thin rings after it fade out. Before, there were three hard
/// rings (1, 3 and 6 points out, 1.5 points wide, 50 %, 14 % and 6 %), and the eye
/// read them as a double or triple border.
const GLOW_RINGS: [(f32, f32, f32); 6] = [
    (1.0, GLOW_STROKE, 0.6),
    (2.0, 1.0, 0.22),
    (3.0, 1.0, 0.14),
    (4.0, 1.0, 0.09),
    (5.0, 1.0, 0.05),
    (6.0, 1.0, 0.025),
];

// The last ring must end inside the room that the layout keeps free (`GLOW_REACH`).
const _: () =
    assert!(GLOW_RINGS[GLOW_RINGS.len() - 1].0 + GLOW_RINGS[GLOW_RINGS.len() - 1].1 <= GLOW_REACH);

/// Soft glow around a rect (selected card): one clear ring and a fading halo.
/// It is made of strokes, not a filled shadow, so it draws on top of the icon
/// that the card already painted.
pub fn glow(ui: &Ui, rect: Rect, color: Color32) {
    for (out, width, keep) in GLOW_RINGS {
        ui.painter().rect_stroke(
            rect.expand(out),
            CornerRadius::same((RADIUS as f32 + out) as u8),
            Stroke::new(width, color.gamma_multiply(keep)),
            StrokeKind::Outside,
        );
    }
}

/// A count in short form: "950", "1.2k", "47k", "5.2M". Turkish writes "1,2 bin",
/// "47 bin" and "5,2M".
pub fn fmt_n(lang: Lang, n: u32) -> String {
    let thousand = if lang == Lang::Tr { " bin" } else { "k" };
    match n {
        n if n >= 1_000_000 => format!("{}M", lang.num(n as f32 / 1e6, 1)),
        n if n >= 10_000 => format!("{}{thousand}", n / 1000),
        n if n >= 1000 => format!("{}{thousand}", lang.num(n as f32 / 1e3, 1)),
        n => n.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn segments_share_the_width_while_every_label_fits() {
        let w = segment_widths(300.0, &[40.0, 30.0, 50.0, 60.0]);
        assert_eq!(w, vec![75.0; 4]);
        // A label that needs the whole share and the pad does not fit.
        assert_eq!(
            segment_widths(300.0, &[73.0, 30.0, 50.0, 60.0]),
            vec![75.0; 4]
        );
        assert_ne!(
            segment_widths(300.0, &[74.0, 30.0, 50.0, 60.0]),
            vec![75.0; 4]
        );
    }

    #[test]
    fn a_long_label_makes_the_segments_follow_their_labels() {
        // One long word: each segment gets its text and an equal extra, and the sum stays the same.
        let text = [87.0, 39.0, 48.0, 49.0, 53.0];
        let w = segment_widths(308.0, &text);
        assert!((w.iter().sum::<f32>() - 308.0).abs() < 0.01, "{w:?}");
        let extra = (308.0 - text.iter().sum::<f32>()) / 5.0;
        for (width, t) in w.iter().zip(text) {
            assert!((width - (t + extra)).abs() < 0.01, "{w:?}");
            assert!(*width >= t + SEGMENT_PAD, "{w:?}");
        }
        // No room for the labels: the segments shrink in the same ratio and fill the control.
        let w = segment_widths(100.0, &[80.0, 40.0]);
        assert!((w.iter().sum::<f32>() - 100.0).abs() < 0.01, "{w:?}");
        assert!((w[0] / w[1] - 2.0).abs() < 0.01, "{w:?}");
        // No label at all is no segment.
        assert!(segment_widths(100.0, &[]).is_empty());
    }

    #[test]
    fn counts_come_in_short_form() {
        assert_eq!(fmt_n(Lang::En, 950), "950");
        assert_eq!(fmt_n(Lang::En, 1234), "1.2k");
        assert_eq!(fmt_n(Lang::En, 47_000), "47k");
        assert_eq!(fmt_n(Lang::En, 5_200_000), "5.2M");
        // Turkish writes the decimal comma and the word "bin" (thousand).
        assert_eq!(fmt_n(Lang::Tr, 1234), "1,2 bin");
        assert_eq!(fmt_n(Lang::Tr, 5_200_000), "5,2M");
        assert_eq!(fmt_n(Lang::Tr, 47_000), "47 bin");
        assert_eq!(fmt_n(Lang::Tr, 950), "950");
    }

    #[test]
    fn a_tier_mark_has_the_full_tier_color_and_no_dimming() {
        for t in 0..6 {
            assert_eq!(tier_ink(Some(t)), theme::tier_color(t));
        }
        // No stats: a light grey "?", not the old dim caption grey.
        assert_eq!(tier_ink(None), LABEL_2);
    }

    #[test]
    fn a_capsule_path_closes_and_hugs_the_rect() {
        let r = Rect::from_min_size(pos2(10.0, 20.0), vec2(100.0, 30.0));
        let path = capsule_path(r, 10);
        assert_eq!(path.first(), path.last());
        for q in &path {
            assert!(r.expand(0.01).contains(*q), "{q:?}");
        }
        // The path touches the top, the bottom, and both ends.
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(
            path.iter().any(|q| near(q.y, r.top())) && path.iter().any(|q| near(q.y, r.bottom()))
        );
        assert!(
            path.iter().any(|q| near(q.x, r.left())) && path.iter().any(|q| near(q.x, r.right()))
        );
        // Its length is two straight lines and a circle: 2 * (100 - 30) + pi * 30 (about 234.2).
        let len: f32 = path.windows(2).map(|w| (w[1] - w[0]).length()).sum();
        assert!(
            (len - (140.0 + std::f32::consts::PI * 30.0)).abs() < 1.0,
            "{len}"
        );
    }

    #[test]
    fn a_dashed_outline_paints_many_short_pieces() {
        let ctx = egui::Context::default();
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 200.0))),
            ..Default::default()
        };
        let r = Rect::from_min_size(pos2(10.0, 10.0), vec2(120.0, 39.0));
        let out = ctx.run_ui(input, |ui| {
            paint_dashed(ui.painter(), r, Stroke::new(1.5, Color32::WHITE), 5.0, 3.5)
        });
        // About 270 points of path in steps of 8.5: more than 20 dashes.
        assert!(out.shapes.len() > 20, "{}", out.shapes.len());
        out.drop_without_applying_deltas();
    }

    /// The shapes that one frame paints, with the nested `Shape::Vec` flattened.
    fn shapes_of(mut add: impl FnMut(&mut Ui)) -> Vec<egui::Shape> {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        frame(&ctx, vec![], &mut add)
    }

    /// Run one frame of `add` with these input events on this context.
    fn frame(
        ctx: &egui::Context,
        events: Vec<egui::Event>,
        add: &mut impl FnMut(&mut Ui),
    ) -> Vec<egui::Shape> {
        fn flat(s: &egui::Shape, out: &mut Vec<egui::Shape>) {
            match s {
                egui::Shape::Vec(v) => v.iter().for_each(|s| flat(s, out)),
                s => out.push(s.clone()),
            }
        }
        let input = egui::RawInput {
            screen_rect: Some(Rect::from_min_size(pos2(0.0, 0.0), vec2(400.0, 200.0))),
            events,
            ..Default::default()
        };
        let out = ctx.run_ui(input, |ui| add(ui));
        let mut shapes = Vec::new();
        out.shapes.iter().for_each(|c| flat(&c.shape, &mut shapes));
        out.drop_without_applying_deltas();
        shapes
    }

    /// The visible edges of the rects (the root of a test frame has an invisible one), as (rect, stroke color), in paint order.
    fn edges(shapes: &[egui::Shape]) -> Vec<(Rect, Color32)> {
        shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Rect(r) if r.stroke.width > 0.0 && r.stroke.color.a() > 0 => {
                    Some((r.rect, r.stroke.color))
                }
                _ => None,
            })
            .collect()
    }

    /// The colors of the dashes (`Shape::dashed_line` makes one line segment for each piece).
    fn dashes(shapes: &[egui::Shape]) -> Vec<Color32> {
        shapes
            .iter()
            .filter_map(|s| {
                if let egui::Shape::LineSegment { stroke, .. } = s {
                    Some(stroke.color)
                } else {
                    None
                }
            })
            .collect()
    }

    /// The colors of the filled rects (an edge is a rect with a see-through fill).
    fn fills(shapes: &[egui::Shape]) -> Vec<Color32> {
        shapes
            .iter()
            .filter_map(|s| {
                if let egui::Shape::Rect(r) = s {
                    Some(r.fill)
                } else {
                    None
                }
            })
            .filter(|c| c.a() > 0)
            .collect()
    }

    /// The color of each text, with the dark shadow of a mark left out.
    fn inks(shapes: &[egui::Shape]) -> Vec<Color32> {
        shapes
            .iter()
            .filter_map(|s| match s {
                egui::Shape::Text(t) => Some(
                    t.override_text_color
                        .unwrap_or_else(|| t.galley.job.sections[0].format.color),
                ),
                _ => None,
            })
            .filter(|c| *c != Color32::from_black_alpha(SHADOW_A))
            .collect()
    }

    #[test]
    fn a_dimmed_tier_badge_keeps_the_tier_color_and_draws_a_dashed_edge() {
        for tier in 0..6u8 {
            let c = theme::tier_color(tier);
            // The grey of the old dim badge is the color of no tier.
            assert_ne!(c, LABEL_3, "tier {tier}");
            let solid = shapes_of(|ui| {
                tier_badge(ui, tier, 30.0, false);
            });
            let dim = shapes_of(|ui| {
                tier_badge(ui, tier, 30.0, true);
            });
            // A full sample: one solid edge in the tier color.
            assert_eq!(
                edges(&solid).iter().map(|e| e.1).collect::<Vec<_>>(),
                vec![c],
                "tier {tier}"
            );
            assert!(dashes(&solid).is_empty(), "tier {tier}");
            // A thin sample: no solid edge, many dashes, all in the tier color.
            assert!(edges(&dim).is_empty(), "tier {tier}");
            let pieces = dashes(&dim);
            assert!(pieces.len() >= 8, "tier {tier}: {} dashes", pieces.len());
            assert!(pieces.iter().all(|p| *p == c), "tier {tier}: {pieces:?}");
            // The plate and the letter are the same in both, and none of them is grey.
            assert_eq!(fills(&dim), fills(&solid), "tier {tier}");
            assert_eq!(inks(&dim), inks(&solid), "tier {tier}");
            assert!(
                inks(&dim).contains(&c) && !inks(&dim).contains(&LABEL_3),
                "tier {tier}"
            );
        }
    }

    #[test]
    fn a_rounded_path_closes_and_hugs_the_rect() {
        let r = Rect::from_min_size(pos2(10.0, 20.0), vec2(30.0, 30.0));
        let path = rounded_path(r, 8.0, 6);
        assert_eq!(path.first(), path.last());
        for q in &path {
            assert!(r.expand(0.01).contains(*q), "{q:?}");
        }
        // The path touches the middle of all four sides.
        let near = |a: f32, b: f32| (a - b).abs() < 0.01;
        assert!(path.iter().any(|q| near(q.y, r.top()) && near(q.x, 18.0)));
        assert!(
            path.iter()
                .any(|q| near(q.y, r.bottom()) && near(q.x, 18.0))
        );
        assert!(
            path.iter().any(|q| near(q.x, r.left())) && path.iter().any(|q| near(q.x, r.right()))
        );
        // The corners are round: no point sits near the corner of the rect (the arc comes within 4.7 of it, a sharp corner within 0).
        assert!(
            path.iter()
                .all(|q| (q.x - r.left()).abs() + (q.y - r.top()).abs() > 4.0)
        );
        // Its length is four straight sides and a circle: 4 * (30 - 16) + 2 * pi * 8 (about 106.3).
        let len: f32 = path.windows(2).map(|w| (w[1] - w[0]).length()).sum();
        assert!(
            (len - (56.0 + 2.0 * std::f32::consts::PI * 8.0)).abs() < 1.0,
            "{len}"
        );
        // A radius larger than the rect shrinks to a half of the short side.
        let big = rounded_path(r, 99.0, 6);
        assert!(big.iter().all(|q| r.expand(0.01).contains(*q)));
    }

    #[test]
    fn the_switch_is_40_by_24_and_the_label_has_the_rest_of_the_row() {
        assert_eq!((SWITCH_W, SWITCH_H), (40.0, 24.0));
        let shapes = shapes_of(|ui| {
            toggle(ui, &mut true, "Label");
        });
        // The track is the only filled rect with this size, with the full round ends of a 24 point high capsule.
        let track: Vec<_> = shapes
            .iter()
            .filter_map(|s| {
                if let egui::Shape::Rect(r) = s {
                    Some(r)
                } else {
                    None
                }
            })
            .filter(|r| {
                (r.rect.width() - SWITCH_W).abs() < 0.01
                    && (r.rect.height() - SWITCH_H).abs() < 0.01
            })
            .collect();
        assert_eq!(track.len(), 1);
        assert_eq!(track[0].corner_radius, CornerRadius::same(12));
        // A one-line label keeps the 26 points of the row. A wrapped label makes the row taller.
        assert_eq!(toggle_row_height(16.0), 26.0);
        assert_eq!(toggle_row_height(34.0), 34.0);
    }

    #[test]
    fn a_click_anywhere_in_a_toggle_row_flips_it() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let (mut on, mut changes) = (false, 0);
        let mut row = |ui: &mut Ui| {
            if toggle(ui, &mut on, "Label") {
                changes += 1;
            }
        };
        // Frame 1 lays the row out. The track tells where the row ends.
        let shapes = frame(&ctx, vec![], &mut row);
        let track = shapes
            .iter()
            .find_map(|s| match s {
                egui::Shape::Rect(r) if (r.rect.width() - SWITCH_W).abs() < 0.01 => Some(r.rect),
                _ => None,
            })
            .expect("a track");
        // Press and release in the empty middle of the row: not on the label and not on the switch.
        let at = pos2(track.left() - 60.0, track.center().y);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&ctx, vec![egui::Event::PointerMoved(at)], &mut row);
        frame(&ctx, vec![button(true)], &mut row);
        frame(&ctx, vec![button(false)], &mut row);
        frame(&ctx, vec![], &mut row);
        assert_eq!(changes, 1);
        assert!(on);
    }

    #[test]
    fn a_text_button_is_blue_text_in_a_row_of_30_points_and_a_click_anywhere_in_it_answers() {
        let ctx = egui::Context::default();
        theme::setup(&ctx);
        let (mut clicks, mut size) = (0, Vec2::ZERO);
        let mut row = |ui: &mut Ui| {
            let r = text_button(ui, "Show all");
            size = r.rect.size();
            if r.clicked() {
                clicks += 1;
            }
        };
        let shapes = frame(&ctx, vec![], &mut row);
        // One blue text, and no fill at rest.
        assert_eq!(inks(&shapes), vec![BLUE]);
        assert!(fills(&shapes).is_empty(), "{:?}", fills(&shapes));
        // The press and the release sit at the far left of the row, far from the text.
        let at = pos2(20.0, TEXT_BUTTON_H / 2.0);
        let button = |pressed| egui::Event::PointerButton {
            pos: at,
            button: egui::PointerButton::Primary,
            pressed,
            modifiers: egui::Modifiers::NONE,
        };
        frame(&ctx, vec![egui::Event::PointerMoved(at)], &mut row);
        frame(&ctx, vec![button(true)], &mut row);
        frame(&ctx, vec![button(false)], &mut row);
        frame(&ctx, vec![], &mut row);
        assert_eq!(clicks, 1);
        // The row is as wide as the room and 30 points high.
        assert_eq!(size.y, TEXT_BUTTON_H);
        assert!(size.x > 300.0, "{size:?}");
    }

    #[test]
    fn the_glow_is_one_clear_ring_and_a_fading_halo() {
        // The first ring is the strongest, the rest fade out with the distance.
        assert!(GLOW_RINGS[0].2 >= 0.5);
        for w in GLOW_RINGS.windows(2) {
            assert!(w[1].0 > w[0].0 && w[1].2 < w[0].2, "{w:?}");
        }
        // No ring reaches past the room that the layout keeps free.
        for (out, width, _) in GLOW_RINGS {
            assert!(out + width <= GLOW_REACH, "{out} {width}");
        }
        let card = Rect::from_min_size(pos2(40.0, 40.0), vec2(100.0, 60.0));
        let shapes = shapes_of(|ui| glow(ui, card, GREEN));
        let rings = edges(&shapes);
        assert_eq!(rings.len(), GLOW_RINGS.len());
        // The rings grow outwards and their alpha falls.
        for w in rings.windows(2) {
            assert!(
                w[1].0.width() > w[0].0.width() && w[1].1.a() < w[0].1.a(),
                "{w:?}"
            );
        }
        assert_eq!(rings[0].0, card.expand(1.0));
        assert_eq!(rings[0].1, GREEN.gamma_multiply(0.6));
    }

    #[test]
    fn the_confidence_bars_are_taller_and_the_unlit_ones_show() {
        const { assert!(BAR_H > 4.0 && BAR_OFF_A > 41) };
    }

    #[test]
    fn the_confidence_bars_fill_the_width_that_the_layout_counts_on() {
        // Three bars of 10 points with two gaps of 2.
        assert_eq!(BARS_W, 3.0 * 10.0 + 2.0 * 2.0);
    }
}
