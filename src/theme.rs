//! Look and feel: an Apple-style dark palette on Windows system fonts.
//! Fonts load from C:\Windows\Fonts at start, so the exe carries no font data.

use egui::{Color32, FontFamily, FontId, RichText};
use std::sync::Arc;

// Palette (Apple dark mode system colors).
pub const BG: Color32 = Color32::from_rgb(16, 16, 18);
pub const SURFACE: Color32 = Color32::from_rgb(28, 28, 30);
pub const SURFACE_2: Color32 = Color32::from_rgb(44, 44, 46);
pub const SURFACE_3: Color32 = Color32::from_rgb(58, 58, 60);
pub const LABEL: Color32 = Color32::from_rgb(245, 245, 247);
pub const LABEL_2: Color32 = Color32::from_rgba_premultiplied(156, 156, 163, 255);
/// Caption text. It has 5.2:1 contrast on SURFACE, above the 4.5:1 of WCAG AA.
pub const LABEL_3: Color32 = Color32::from_rgba_premultiplied(142, 142, 147, 255);
pub const HAIRLINE: Color32 = Color32::from_rgba_premultiplied(18, 18, 18, 18);
pub const BLUE: Color32 = Color32::from_rgb(10, 132, 255);
pub const GREEN: Color32 = Color32::from_rgb(48, 209, 88);
pub const RED: Color32 = Color32::from_rgb(255, 69, 58);
pub const ORANGE: Color32 = Color32::from_rgb(255, 159, 10);
pub const TEAL: Color32 = Color32::from_rgb(100, 210, 255);
pub const PRISM: Color32 = Color32::from_rgb(214, 150, 255);

pub const RADIUS: u8 = 12;

// The marks over the game art (badges, the overlay). The art is bright and busy, so
// every mark sits on an opaque dark plate with a thin light outline.
/// The plate under a mark. It is opaque: before, the badge plate was 236/255 and
/// the card art showed through a little.
pub const PLATE: Color32 = Color32::from_rgb(14, 14, 18);
/// Alpha of the thin white outline around a plate (before: 50).
pub const PLATE_EDGE_A: u8 = 150;
/// Alpha of the dark shadow behind the text of a mark.
pub const SHADOW_A: u8 = 210;

/// Semibold and bold faces as named families.
pub fn semibold() -> FontFamily {
    FontFamily::Name("semibold".into())
}
pub fn bold() -> FontFamily {
    FontFamily::Name("bold".into())
}
pub fn icons() -> FontFamily {
    FontFamily::Name("icons".into())
}

pub fn title(t: impl Into<String>, size: f32) -> RichText {
    RichText::new(t).family(bold()).size(size).color(LABEL)
}
pub fn strong(t: impl Into<String>, size: f32) -> RichText {
    RichText::new(t).family(semibold()).size(size).color(LABEL)
}
pub fn body(t: impl Into<String>) -> RichText {
    RichText::new(t).size(13.5).color(LABEL)
}
pub fn caption(t: impl Into<String>) -> RichText {
    RichText::new(t).size(11.5).color(LABEL_2)
}

/// Segoe Fluent Icons code points.
pub mod glyph {
    pub const SETTINGS: char = '\u{E713}';
    pub const CLOSE: char = '\u{E8BB}';
    pub const BACK: char = '\u{E76B}';
    pub const CHEVRON: char = '\u{E76C}';
    pub const CHEVRON_DOWN: char = '\u{E70D}';
    pub const CHEVRON_UP: char = '\u{E70E}';
    pub const CHECK: char = '\u{E73E}';
    pub const LINK: char = '\u{E71B}';
}

pub fn setup(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let dir = r"C:\Windows\Fonts\";
    let mut load = |key: &str, files: &[&str]| -> bool {
        for f in files {
            if let Ok(bytes) = std::fs::read(format!("{dir}{f}")) {
                fonts
                    .font_data
                    .insert(key.into(), Arc::new(egui::FontData::from_owned(bytes)));
                return true;
            }
        }
        false
    };
    let regular = load("regular", &["SegUIVar.ttf", "segoeui.ttf"]);
    let semi = load("semibold", &["seguisb.ttf", "segoeuib.ttf"]);
    let bolder = load("bold", &["segoeuib.ttf", "seguisb.ttf"]);
    let icons_ok = load("icons", &["SegoeIcons.ttf", "segmdl2.ttf"]);

    // Every family falls back to the egui default fonts for missing glyphs.
    let fallback: Vec<String> = fonts.families[&FontFamily::Proportional].clone();
    let chain = |first: Option<&str>| -> Vec<String> {
        let mut v: Vec<String> = first.map(|s| vec![s.to_string()]).unwrap_or_default();
        v.extend(fallback.iter().cloned());
        v
    };
    let reg = chain(regular.then_some("regular"));
    fonts.families.insert(FontFamily::Proportional, reg.clone());
    fonts.families.insert(
        semibold(),
        if semi {
            chain(Some("semibold"))
        } else {
            reg.clone()
        },
    );
    fonts.families.insert(
        bold(),
        if bolder {
            chain(Some("bold"))
        } else {
            reg.clone()
        },
    );
    fonts
        .families
        .insert(icons(), if icons_ok { chain(Some("icons")) } else { reg });
    ctx.set_fonts(fonts);

    ctx.set_theme(egui::ThemePreference::Dark);
    ctx.all_styles_mut(|s| {
        s.spacing.item_spacing = egui::vec2(8.0, 6.0);
        s.spacing.button_padding = egui::vec2(10.0, 5.0);
        s.spacing.interact_size.y = 26.0;
        s.spacing.scroll.bar_width = 6.0;
        s.spacing.scroll.floating = true;
        s.text_styles
            .insert(egui::TextStyle::Body, FontId::proportional(13.5));
        s.text_styles
            .insert(egui::TextStyle::Button, FontId::proportional(13.0));
        s.text_styles
            .insert(egui::TextStyle::Small, FontId::proportional(11.0));
        s.text_styles
            .insert(egui::TextStyle::Heading, FontId::new(20.0, bold()));
        let v = &mut s.visuals;
        v.dark_mode = true;
        v.override_text_color = None;
        v.panel_fill = BG;
        v.window_fill = SURFACE;
        v.extreme_bg_color = SURFACE_2;
        v.faint_bg_color = Color32::from_white_alpha(6);
        v.selection.bg_fill = BLUE.gamma_multiply(0.5);
        v.selection.stroke = egui::Stroke::new(1.0, BLUE);
        v.hyperlink_color = TEAL;
        v.window_corner_radius = egui::CornerRadius::same(RADIUS);
        v.menu_corner_radius = egui::CornerRadius::same(10);
        v.popup_shadow = egui::epaint::Shadow {
            offset: [0, 8],
            blur: 24,
            spread: 0,
            color: Color32::from_black_alpha(120),
        };
        let w = &mut v.widgets;
        for (st, fill) in [
            (&mut w.noninteractive, Color32::TRANSPARENT),
            (&mut w.inactive, Color32::from_white_alpha(14)),
            (&mut w.hovered, Color32::from_white_alpha(26)),
            (&mut w.active, Color32::from_white_alpha(36)),
            (&mut w.open, Color32::from_white_alpha(26)),
        ] {
            st.bg_fill = fill;
            st.weak_bg_fill = fill;
            st.bg_stroke = egui::Stroke::NONE;
            st.corner_radius = egui::CornerRadius::same(8);
            st.expansion = 0.0;
        }
        w.noninteractive.fg_stroke = egui::Stroke::new(1.0, LABEL);
        w.noninteractive.bg_stroke = egui::Stroke::new(1.0, HAIRLINE);
        w.inactive.fg_stroke = egui::Stroke::new(1.0, LABEL);
        w.hovered.fg_stroke = egui::Stroke::new(1.0, LABEL);
        w.active.fg_stroke = egui::Stroke::new(1.0, LABEL);
    });
}

// The tier ramp. It has no green and no orange: green means take or gain, red
// means loss, and orange means warning. Each tier has one color on every tab.
pub const TIER_S_PLUS: Color32 = Color32::from_rgb(165, 132, 255);
pub const TIER_S: Color32 = Color32::from_rgb(236, 192, 80);
pub const TIER_A: Color32 = Color32::from_rgb(120, 186, 255);
pub const TIER_B: Color32 = Color32::from_rgb(128, 142, 170);
pub const TIER_C: Color32 = Color32::from_rgb(160, 160, 166);
pub const TIER_D: Color32 = Color32::from_rgb(196, 98, 98);

/// The color of a tier: 0 is S+, 5 is D.
pub fn tier_color(t: u8) -> Color32 {
    match t {
        0 => TIER_S_PLUS,
        1 => TIER_S,
        2 => TIER_A,
        3 => TIER_B,
        4 => TIER_C,
        _ => TIER_D,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Relative luminance of a color (WCAG 2.1).
    fn luminance(c: Color32) -> f32 {
        let f = |v: u8| {
            let v = v as f32 / 255.0;
            if v <= 0.03928 {
                v / 12.92
            } else {
                ((v + 0.055) / 1.055).powf(2.4)
            }
        };
        0.2126 * f(c.r()) + 0.7152 * f(c.g()) + 0.0722 * f(c.b())
    }

    /// Contrast ratio of two colors (WCAG 2.1): 1 to 21.
    fn contrast(a: Color32, b: Color32) -> f32 {
        let (x, y) = (luminance(a), luminance(b));
        (x.max(y) + 0.05) / (x.min(y) + 0.05)
    }

    #[test]
    fn caption_text_passes_wcag_aa_on_the_dark_surfaces() {
        for bg in [SURFACE, BG] {
            assert!(
                contrast(LABEL_3, bg) >= 4.5,
                "LABEL_3 on {bg:?}: {:.2}",
                contrast(LABEL_3, bg)
            );
            assert!(contrast(LABEL_2, bg) >= 4.5);
        }
    }

    #[test]
    fn marks_over_the_game_art_read_at_full_strength_on_their_plate() {
        // The plate hides the art: it has no alpha.
        assert_eq!(PLATE.a(), 255);
        // Every tier color and the text colors pass WCAG AA on the plate, with no dimming.
        for t in 0..6 {
            assert!(
                contrast(tier_color(t), PLATE) >= 4.5,
                "tier {t}: {:.2}",
                contrast(tier_color(t), PLATE)
            );
        }
        for ink in [LABEL, LABEL_2, GREEN, ORANGE, TEAL] {
            assert!(
                contrast(ink, PLATE) >= 4.5,
                "{ink:?}: {:.2}",
                contrast(ink, PLATE)
            );
        }
        // The outline and the shadow are strong, not a faint hint.
        const { assert!(PLATE_EDGE_A >= 120 && SHADOW_A >= 180) };
    }

    #[test]
    fn tiers_have_their_own_ramp_without_green_or_orange() {
        let ramp: Vec<Color32> = (0..6).map(tier_color).collect();
        // Six different colors.
        for (i, a) in ramp.iter().enumerate() {
            for b in &ramp[i + 1..] {
                assert_ne!(a, b);
            }
        }
        for (tier, c) in ramp.iter().enumerate() {
            // The colors with a meaning of their own stay away from the tiers.
            for meaning in [GREEN, ORANGE, RED, TEAL] {
                assert_ne!(*c, meaning, "tier {tier}");
            }
            // No tier color is green: its green channel never leads.
            assert!(c.g() < c.r().max(c.b()), "tier {tier} looks green: {c:?}");
        }
        // The ramp from the owner: S+ violet, S gold, A light blue, B slate, C grey, D muted red.
        assert!(ramp[0].b() > ramp[0].g() && ramp[0].r() > ramp[0].g());
        assert!(ramp[1].r() > ramp[1].g() && ramp[1].g() > ramp[1].b());
        assert!(ramp[2].b() > ramp[2].g() && ramp[2].g() > ramp[2].r());
        assert!(ramp[3].b() > ramp[3].g() && ramp[3].g() > ramp[3].r());
        assert!(ramp[4].r().abs_diff(ramp[4].g()) < 8);
        assert!(ramp[5].r() > ramp[5].g() + 60 && ramp[5].r() < RED.r());
    }
}
