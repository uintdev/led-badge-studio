//! Painting framebuffers as LED matrices.

use std::cell::Cell;
use std::rc::Rc;

use badge_core::Framebuffer;
use badge_render::led_look::{BOARD, LedLook, Rgb};
use gpui::prelude::*;
use gpui::{Bounds, Hsla, Pixels, canvas, div, fill, point, px, size};

use crate::theme;

/// The smallest LED size, in window pixels, before the preview scrolls.
pub const MIN_CELL: f32 = 2.;

/// The LED size for `cols` x `rows` LEDs in `room`: `max` when it fits, else
/// the largest whole number of screen pixels that fits, but at least
/// `MIN_CELL`.
pub fn fit_cell(max: f32, room: (f32, f32), cols: usize, rows: usize, sf: f32) -> f32 {
    let fit = (room.0 / cols.max(1) as f32).min(room.1 / rows.max(1) as f32);
    let cell = (fit.min(max) * sf).floor() / sf;
    cell.max(MIN_CELL)
}

/// How a matrix is drawn: the shared LED look plus screen-only extras.
#[derive(Clone, Copy)]
pub struct LedStyle {
    pub look: LedLook,
    /// Draw byte (8-column) guides and grid lines (editor only).
    pub guides: bool,
    /// Column after which a "display width" marker is drawn.
    pub display_width: Option<usize>,
    /// Draw unlit LEDs (off for thumbnails, which is much cheaper).
    pub draw_off: bool,
}

fn hsla(c: Rgb) -> Hsla {
    gpui::Rgba {
        r: c[0],
        g: c[1],
        b: c[2],
        a: 1.,
    }
    .into()
}

/// An LED matrix `cols` wide showing `fb`. When `bounds_out` is given the
/// painted bounds are written there (for mouse hit-testing).
pub fn led_matrix(
    fb: Framebuffer,
    cols: usize,
    style: LedStyle,
    bounds_out: Option<Rc<Cell<Option<Bounds<Pixels>>>>>,
) -> gpui::Div {
    let rows = fb.height();
    let w = cols as f32 * style.look.cell;
    let h = rows as f32 * style.look.cell;
    div().w(px(w)).h(px(h)).flex_none().child(
        canvas(
            move |bounds, _, _| {
                if let Some(out) = &bounds_out {
                    out.set(Some(bounds));
                }
                bounds
            },
            move |bounds, _, window, _| {
                paint_matrix(&fb, cols, &style, bounds, window);
            },
        )
        .size_full(),
    )
}

fn paint_matrix(fb: &Framebuffer, cols: usize, style: &LedStyle, bounds: Bounds<Pixels>, window: &mut gpui::Window) {
    // No `..`, so a new field can't be left out.
    let LedLook {
        cell,
        round: _,
        color_card: _,
        tint: _,
        glow: _,
        level: _,
        dim: _,
    } = style.look;
    let look = &style.look;
    window.paint_quad(fill(bounds, hsla(BOARD)));
    let clip = window.content_mask().bounds;
    let left = f32::from(clip.left() - bounds.left()).max(0.0);
    let right = f32::from(clip.right() - bounds.left()).max(0.0);
    let first = (left / cell).floor() as usize;
    let last = ((right / cell).ceil() as usize).min(cols);
    let g = look.geometry();
    let (gap, d, radius) = (g.gap, g.size, g.radius);
    let led_bounds = |x: usize, y: usize| {
        let origin = point(
            bounds.left() + px(x as f32 * cell + gap),
            bounds.top() + px(y as f32 * cell + gap),
        );
        gpui::Bounds::new(origin, size(px(d), px(d)))
    };
    let lit = |x: usize, y: usize| Some(fb.get(x, y)).filter(|c| c.is_on());

    // Unlit LEDs first, so the glow can spill over them.
    if style.draw_off {
        let off = hsla(look.off());
        for x in first..last {
            for y in (0..fb.height()).filter(|&y| lit(x, y).is_none()) {
                window.paint_quad(fill(led_bounds(x, y), off).corner_radii(px(radius)));
            }
        }
    }
    let layers = look.glow_layers();
    if !layers.is_empty() {
        // Columns just outside the visible area still glow into it.
        let reach = 3;
        window.with_content_mask(Some(gpui::ContentMask { bounds }), |window| {
            for x in first.saturating_sub(reach)..(last + reach).min(cols) {
                for y in 0..fb.height() {
                    if let Some(c) = lit(x, y) {
                        let color = hsla(look.led_color(c));
                        for layer in &layers {
                            window.paint_drop_shadows(
                                led_bounds(x, y),
                                gpui::Corners::all(px(radius + layer.spread)),
                                &[gpui::BoxShadow {
                                    color: color.opacity(layer.alpha),
                                    offset: point(px(0.), px(0.)),
                                    blur_radius: px(layer.blur),
                                    spread_radius: px(layer.spread),
                                    inset: false,
                                }],
                            );
                        }
                    }
                }
            }
        });
    }
    for x in first..last {
        for y in 0..fb.height() {
            if let Some(c) = lit(x, y) {
                let b = led_bounds(x, y);
                let body = look.body(c);
                window.paint_quad(fill(b, hsla(body)).corner_radii(px(radius)));
                if let Some((core, core_radius)) = g.core {
                    let inset = (d - core) / 2.;
                    let core_bounds = gpui::Bounds::new(
                        point(b.left() + px(inset), b.top() + px(inset)),
                        size(px(core), px(core)),
                    );
                    window.paint_quad(fill(core_bounds, hsla(look.core(body))).corner_radii(px(core_radius)));
                }
            }
        }
    }
    if style.guides {
        let guide: Hsla = theme::led_guide().into();
        for x in first..=last {
            if x % 8 == 0 && x > 0 {
                let line = gpui::Bounds::new(
                    point(bounds.left() + px(x as f32 * cell - 0.5), bounds.top()),
                    size(px(1.0), bounds.size.height),
                );
                window.paint_quad(fill(line, guide));
            }
        }
    }
    if let Some(dw) = style.display_width
        && dw > first
        && dw <= last
    {
        let line = gpui::Bounds::new(
            point(bounds.left() + px(dw as f32 * cell - 1.0), bounds.top()),
            size(px(2.0), bounds.size.height),
        );
        window.paint_quad(fill(line, theme::led_marker()));
    }
}

/// Converts a window position into a (column, row) on a matrix painted at
/// `bounds` with `cell`-sized LEDs.
pub fn hit(bounds: Bounds<Pixels>, cell: f32, pos: gpui::Point<Pixels>, rows: usize) -> Option<(usize, usize)> {
    let x = f32::from(pos.x - bounds.left());
    let y = f32::from(pos.y - bounds.top());
    if x < 0.0 || y < 0.0 || x >= f32::from(bounds.size.width) {
        return None;
    }
    let (cx, cy) = ((x / cell) as usize, (y / cell) as usize);
    (cy < rows).then_some((cx, cy))
}

#[cfg(test)]
mod fit_tests {
    use super::{MIN_CELL, fit_cell};

    #[test]
    fn wide_displays_get_smaller_whole_pixel_leds() {
        // Fits at the normal size: unchanged.
        assert_eq!(fit_cell(9., (900., f32::INFINITY), 44, 11, 1.), 9.);
        // 256 LEDs in 940 px: 3 px each (3.67 rounded down), so it fits.
        let cell = fit_cell(9., (940., f32::INFINITY), 256, 11, 1.);
        assert_eq!(cell, 3.);
        assert!(256. * cell <= 940.);
        // At 1.5x scaling: whole screen pixels (5 of them), not window pixels.
        assert_eq!(fit_cell(9., (940., f32::INFINITY), 256, 11, 1.5), 5. / 1.5);
        // The pop-out also fits the height.
        assert_eq!(fit_cell(120., (2000., 110.), 44, 11, 1.), 10.);
        // Never below the smallest size (the preview scrolls instead).
        assert_eq!(fit_cell(9., (300., f32::INFINITY), 256, 11, 1.), MIN_CELL);
    }
}
