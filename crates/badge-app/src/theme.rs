//! Colors and type sizes used across the UI, in a dark and a light version.
//! The LED preview and the Pixels canvas stay dark in both, like the badge.

use std::cell::Cell;

use badge_core::LedColor;
use gpui::{Hsla, Rgba, rgb, rgba};

/// Body text: labels on controls, menu items, the message box.
pub const TEXT: f32 = 14.;
/// Labels in front of controls, secondary lines.
pub const TEXT_SMALL: f32 = 13.;
/// Hints, captions and the status bar; the smallest text in the app.
pub const TEXT_TINY: f32 = 12.;
/// Card titles.
pub const TEXT_TITLE: f32 = 15.;
/// Dialog titles.
pub const TEXT_HEADING: f32 = 18.;

struct Palette {
    bg: u32,
    panel: u32,
    panel_alt: u32,
    border: u32,
    border_strong: u32,
    border_focus: u32,
    text: u32,
    text_muted: u32,
    text_dim: u32,
    accent: u32,
    accent_hover: u32,
    accent_border: u32,
    accent_soft: u32,
    hover: u32,
    selected: u32,
    danger: u32,
    danger_hover: u32,
    danger_border: u32,
    success: u32,
    warning: u32,
    /// RGBA.
    selection: u32,
    /// RGBA: dims the window behind a dialog.
    backdrop: u32,
    /// RGBA.
    shadow: u32,
}

const DARK: Palette = Palette {
    bg: 0x121316,
    panel: 0x1a1c20,
    panel_alt: 0x23262c,
    border: 0x2c2f36,
    border_strong: 0x3a3e47,
    border_focus: 0x6b8cff,
    text: 0xe8eaee,
    text_muted: 0xa4a9b4,
    text_dim: 0x757b87,
    accent: 0x3f6ff0,
    accent_hover: 0x5a84ff,
    accent_border: 0x2f57c9,
    accent_soft: 0x24304f,
    hover: 0x2a2d34,
    selected: 0x25304d,
    danger: 0xf2555a,
    danger_hover: 0xff6b70,
    danger_border: 0xb83f44,
    success: 0x3ccf7e,
    warning: 0xf0b44c,
    selection: 0x5b8cff55,
    backdrop: 0x0000008c,
    shadow: 0x00000073,
};

const LIGHT: Palette = Palette {
    bg: 0xf2f3f5,
    panel: 0xffffff,
    panel_alt: 0xf5f6f8,
    border: 0xdfe2e7,
    border_strong: 0xc9cdd5,
    border_focus: 0x3d6df2,
    text: 0x1b1e24,
    text_muted: 0x565d6b,
    text_dim: 0x878d99,
    accent: 0x3361e8,
    accent_hover: 0x4a75f2,
    accent_border: 0x2a50c4,
    accent_soft: 0xe6edfd,
    hover: 0xebedf1,
    selected: 0xe3eafc,
    danger: 0xd93a40,
    danger_hover: 0xe5484d,
    danger_border: 0xb12e33,
    success: 0x1f9d55,
    warning: 0xb87a12,
    selection: 0x3d6df240,
    backdrop: 0x0f141e59,
    shadow: 0x0f141e2e,
};

thread_local! {
    static DARK_MODE: Cell<bool> = const { Cell::new(true) };
}

/// Switches between the dark and light colors; redraw the windows after.
pub fn set_dark(dark: bool) {
    DARK_MODE.with(|d| d.set(dark));
}

pub fn is_dark() -> bool {
    DARK_MODE.with(Cell::get)
}

fn p() -> &'static Palette {
    if is_dark() { &DARK } else { &LIGHT }
}

pub fn bg() -> Rgba {
    rgb(p().bg)
}
/// Cards, the sidebars, the toolbar.
pub fn panel() -> Rgba {
    rgb(p().panel)
}
/// Buttons, fields and other controls.
pub fn panel_alt() -> Rgba {
    rgb(p().panel_alt)
}
pub fn border() -> Rgba {
    rgb(p().border)
}
/// Control outlines, a little stronger than card edges.
pub fn border_strong() -> Rgba {
    rgb(p().border_strong)
}
pub fn border_focus() -> Rgba {
    rgb(p().border_focus)
}
pub fn text() -> Rgba {
    rgb(p().text)
}
pub fn text_muted() -> Rgba {
    rgb(p().text_muted)
}
pub fn text_dim() -> Rgba {
    rgb(p().text_dim)
}
pub fn accent() -> Rgba {
    rgb(p().accent)
}
pub fn accent_hover() -> Rgba {
    rgb(p().accent_hover)
}
/// A darker edge for blue buttons; a bright one made them look taller.
pub fn accent_border() -> Rgba {
    rgb(p().accent_border)
}
/// A faint accent background (notices, highlighted rows).
pub fn accent_soft() -> Rgba {
    rgb(p().accent_soft)
}
pub fn hover() -> Rgba {
    rgb(p().hover)
}
pub fn selected() -> Rgba {
    rgb(p().selected)
}
pub fn danger() -> Rgba {
    rgb(p().danger)
}
pub fn danger_hover() -> Rgba {
    rgb(p().danger_hover)
}
pub fn danger_border() -> Rgba {
    rgb(p().danger_border)
}
pub fn success() -> Rgba {
    rgb(p().success)
}
pub fn warning() -> Rgba {
    rgb(p().warning)
}
/// A faint warning background, for notices.
pub fn warning_soft() -> Hsla {
    Hsla::from(warning()).opacity(if is_dark() { 0.12 } else { 0.1 })
}
pub fn warning_edge() -> Hsla {
    Hsla::from(warning()).opacity(0.45)
}
pub fn selection() -> Rgba {
    rgba(p().selection)
}
pub fn backdrop() -> Rgba {
    rgba(p().backdrop)
}
pub fn shadow() -> Hsla {
    rgba(p().shadow).into()
}
/// Behind code and license text.
pub fn code_bg() -> Rgba {
    rgb(if is_dark() { 0x0e0f12 } else { 0xf6f7f9 })
}

/// The board behind the LEDs (from the shared LED look), in both themes.
pub fn led_board() -> Rgba {
    let [r, g, b] = badge_render::led_look::BOARD;
    Rgba { r, g, b, a: 1. }
}
/// The byte guides drawn on the LED board, which is dark in both themes.
pub fn led_guide() -> Rgba {
    rgb(0x343946)
}
/// The visible-width line on the LED board.
pub fn led_marker() -> Rgba {
    rgb(0xf0b44c)
}
/// The badge's plastic around the LEDs in the preview.
pub fn badge_body() -> Rgba {
    rgb(0x1c1d21)
}
pub fn badge_edge() -> Rgba {
    rgb(if is_dark() { 0x34363d } else { 0x2a2c31 })
}

/// A soft drop shadow for cards and popovers.
pub fn card_shadow() -> Vec<gpui::BoxShadow> {
    vec![gpui::BoxShadow {
        color: shadow().opacity(if is_dark() { 0.35 } else { 0.5 }),
        offset: gpui::point(gpui::px(0.), gpui::px(1.)),
        blur_radius: gpui::px(3.),
        spread_radius: gpui::px(0.),
        inset: false,
    }]
}

pub fn popover_shadow() -> Vec<gpui::BoxShadow> {
    vec![gpui::BoxShadow {
        color: shadow(),
        offset: gpui::point(gpui::px(0.), gpui::px(8.)),
        blur_radius: gpui::px(24.),
        spread_radius: gpui::px(0.),
        inset: false,
    }]
}

/// On-screen color of a lit LED. Monochrome badges use `mono_tint`.
pub fn led(c: LedColor, color_card: bool, mono_tint: LedColor) -> Hsla {
    let c = if color_card { c } else { mono_tint };
    let [r, g, b] = c.to_rgb();
    // Slightly softened so white/yellow don't glare.
    let soften = |v: u8| if v > 0 { 0xff } else { 0x30 };
    let v = (soften(r) as u32) << 16 | (soften(g) as u32) << 8 | soften(b) as u32;
    rgb(v).into()
}

/// A message-box text color for LED color `c`, mixed with white or black to
/// the same luminance for every color, so no color's letters look thinner.
/// On the light theme bright colors are darkened instead, to read on white.
pub fn text_color_for(c: LedColor) -> Hsla {
    let dark = is_dark();
    let target: f32 = if dark { 0.45 } else { 0.16 };
    let to_linear = |v: f32| {
        if v <= 0.04045 {
            v / 12.92
        } else {
            ((v + 0.055) / 1.055).powf(2.4)
        }
    };
    let to_srgb = |v: f32| {
        if v <= 0.0031308 {
            v * 12.92
        } else {
            1.055 * v.powf(1. / 2.4) - 0.055
        }
    };
    let [r, g, b] = c.to_rgb().map(|v| to_linear(f32::from(v) / 255.));
    let lum = 0.2126 * r + 0.7152 * g + 0.0722 * b;
    let adjust = |v: f32| {
        if lum < target && !dark {
            v
        } else if lum < target {
            let t = (target - lum) / (1. - lum);
            v + (1. - v) * t
        } else {
            v * target / lum
        }
    };
    gpui::Rgba {
        r: to_srgb(adjust(r)),
        g: to_srgb(adjust(g)),
        b: to_srgb(adjust(b)),
        a: 1.,
    }
    .into()
}

/// Swatch color for palette buttons.
pub fn swatch(c: LedColor) -> Hsla {
    led(c, true, c)
}
