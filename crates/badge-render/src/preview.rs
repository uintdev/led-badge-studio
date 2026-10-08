//! Simulation of the badge's display modes for the on-screen preview.
//!
//! Every frame is a pure function of a step counter; the UI advances `step`
//! every [`step_interval_us`].

use badge_core::{CardType, Framebuffer, LedColor, Mode};

/// Visible width of the physical display for a badge type.
pub fn display_width(card: CardType) -> usize {
    match card {
        CardType::Mono11 => 44,
        _ => 48,
    }
}

/// The badge's timer unit in microseconds; every step is a whole number of
/// them. Measured with `badgectl calibrate` on the 11x44 badge (11086.9 ±1.5).
pub const TICK_US: u64 = 11_087;

/// Units per step for speed 1..=8.
const TICKS: [u64; 8] = [18, 12, 9, 7, 5, 3, 2, 1];

const ANIMATION_STEPS: usize = 4;

pub fn units_per_step(speed: u8) -> u64 {
    TICKS[(speed.clamp(1, 8) - 1) as usize]
}

pub const ANIMATION_STEPS_PER_PICTURE: usize = ANIMATION_STEPS;

pub fn animation_frame_us(speed: u8) -> u64 {
    ANIMATION_STEPS as u64 * TICKS[(speed.clamp(1, 8) - 1) as usize] * TICK_US
}

/// Microseconds per step: one column of a scroll or the laser, a quarter of
/// an Animation picture.
pub fn step_interval_us(p: &PreviewParams) -> u64 {
    TICKS[(p.speed.clamp(1, 8) - 1) as usize] * TICK_US
}

/// Whether `mode` shows a message narrower than the display centered on it.
pub fn centers_short_messages(mode: Mode) -> bool {
    matches!(mode, Mode::Fixed | Mode::Snow | Mode::Curtain | Mode::Laser)
}

/// Blank columns before a message narrower than the display, which the modes
/// showing whole pages (Fixed, Snow, Curtain, Laser) center on it; 0 in the
/// other modes, and for an empty message. The editor's canvas shows the same.
pub fn centering_offset(mode: Mode, content_width: usize, display_width: usize) -> usize {
    if centers_short_messages(mode) && content_width > 0 && content_width < display_width {
        (display_width - content_width) / 2
    } else {
        0
    }
}

/// Width of each Animation picture: the display width rounded up to whole
/// bytes. On the 44-wide badge that's 48, so columns 44-47 of every picture
/// are never shown.
pub fn animation_frame_width(display_width: usize) -> usize {
    display_width.max(1).div_ceil(8) * 8
}

/// What to simulate.
#[derive(Clone, Copy, Debug)]
pub struct PreviewParams {
    pub mode: Mode,
    pub flash: bool,
    pub marquee: bool,
    /// Visible display width in pixels.
    pub width: usize,
    /// Message speed 1..=8 (laser pauses depend on it).
    pub speed: u8,
}

/// Steps before the animation repeats.
pub fn cycle_len(fb: &Framebuffer, p: &PreviewParams) -> usize {
    let w = p.width.max(1);
    let hold = HOLD_STEPS;
    let h = fb.height();
    let cw = fb.content_width();
    let pages = cw.div_ceil(w).max(1);
    match p.mode {
        Mode::Left | Mode::Right => cw + w,
        Mode::Up | Mode::Down => pages * (2 * h + hold),
        Mode::Fixed => pages * hold,
        Mode::Animation => cw.div_ceil(animation_frame_width(w)).max(1) * ANIMATION_STEPS,
        Mode::Snow => {
            let (pile, fall) = snow_lengths(h);
            pages * (pile + hold) + fall
        }
        // Each page opens and holds; after the last, the display is cleared
        // from the center out, at the opening's pace.
        Mode::Curtain => pages * (w / 2 + 1 + hold) + (w / 2 + 1),
        Mode::Laser => {
            let lp = laser_pages(cw, w);
            lp.count * (lp.width + hold) + lp.width
        }
    }
    .max(1)
}

pub fn frame(fb: &Framebuffer, p: &PreviewParams, step: usize) -> Framebuffer {
    let w = p.width.max(1);
    let hold = HOLD_STEPS;
    let h = fb.height();
    let cw = fb.content_width();
    let cycle = cycle_len(fb, p);
    let step = step % cycle;
    let mut out = Framebuffer::new(h);
    let src = |x: i64, y: i64| -> LedColor {
        if x < 0 || y < 0 {
            LedColor::OFF
        } else {
            fb.get(x as usize, y as usize)
        }
    };
    let paged = |per_page: usize| (step / per_page, step % per_page);
    let pad = centering_offset(p.mode, cw, w) as i64;
    let put = |out: &mut Framebuffer, x: usize, y: usize, c: LedColor| {
        if c.is_on() && x < w && y < h {
            out.set(x, y, c);
        }
    };

    let mut holding = false;
    match p.mode {
        Mode::Left => {
            for x in 0..w {
                for y in 0..h {
                    put(&mut out, x, y, src(x as i64 + step as i64 - w as i64, y as i64));
                }
            }
        }
        Mode::Right => {
            let s = step as i64 - cw as i64;
            for x in 0..w {
                for y in 0..h {
                    put(&mut out, x, y, src(x as i64 - s, y as i64));
                }
            }
        }
        Mode::Up | Mode::Down => {
            // Each page enters from one edge, pauses in place, then leaves
            // through the opposite edge. `offset` is how far the page sits
            // below its resting position (negative = above).
            let (page, i) = paged(2 * h + hold);
            let h_i = h as i64;
            let offset = if i < h {
                h_i - i as i64 // entering
            } else if i < h + hold {
                holding = true;
                0
            } else {
                -((i - h - hold) as i64 + 1) // leaving
            };
            let offset = if p.mode == Mode::Up { offset } else { -offset };
            for x in 0..w {
                for y in 0..h {
                    let c = src((page * w + x) as i64, y as i64 - offset);
                    put(&mut out, x, y, c);
                }
            }
        }
        Mode::Fixed => {
            let (page, _) = paged(hold);
            holding = true;
            for x in 0..w {
                for y in 0..h {
                    put(&mut out, x, y, src((page * w + x) as i64 - pad, y as i64));
                }
            }
        }
        Mode::Animation => {
            let (page, _) = paged(ANIMATION_STEPS);
            let fw = animation_frame_width(w);
            holding = true;
            for x in 0..w {
                for y in 0..h {
                    put(&mut out, x, y, src((page * fw + x) as i64, y as i64));
                }
            }
        }
        Mode::Snow => {
            // Rows fall one at a time, bottom row first, SNOW_SPACING steps
            // apart. After the last page the display falls away the same way.
            let (pile, _) = snow_lengths(h);
            let per = pile + hold;
            let last_page = cw.div_ceil(w).max(1) - 1;
            let (page, i, breaking) = if step >= per * (last_page + 1) {
                (last_page, step - per * (last_page + 1), true)
            } else {
                (step / per, step % per, false)
            };
            holding = !breaking && i >= pile;
            for x in 0..w {
                let sx = (page * w + x) as i64 - pad;
                for y in 0..h {
                    let c = src(sx, y as i64);
                    if !c.is_on() {
                        continue;
                    }
                    let row_order = h - 1 - y;
                    if breaking {
                        let release = row_order * SNOW_SPACING;
                        let pos = y + i.saturating_sub(release);
                        put(&mut out, x, pos, c);
                    } else {
                        let start = row_order * SNOW_SPACING;
                        if i > start {
                            let pos = (i - start - 1).min(y);
                            put(&mut out, x, pos, c);
                        }
                    }
                }
            }
        }
        Mode::Curtain => 'curtain: {
            let half = w / 2;
            let per_page = half + 1 + hold;
            let pages = cw.div_ceil(w).max(1);
            if step >= pages * per_page {
                // After the last page, two lines move out from the center,
                // clearing the display.
                let cleared = (step - pages * per_page) as i64;
                let page = pages - 1;
                let line = fb_color_hint(fb);
                for x in 0..w {
                    let d = (x as i64 - half as i64).abs();
                    if d == cleared + 1 {
                        for y in 0..h {
                            put(&mut out, x, y, line);
                        }
                    } else if d > cleared + 1 {
                        for y in 0..h {
                            put(&mut out, x, y, src((page * w + x) as i64 - pad, y as i64));
                        }
                    }
                }
                break 'curtain;
            }
            let (page, i) = paged(per_page);
            holding = i > half;
            let open = i.min(half) as i64;
            for x in 0..w {
                let d = (x as i64 - half as i64).abs();
                if d <= open {
                    for y in 0..h {
                        put(&mut out, x, y, src((page * w + x) as i64 - pad, y as i64));
                    }
                }
            }
            if !holding {
                // The curtain edges, just outside the opening.
                let line = fb_color_hint(fb);
                for edge in [half as i64 - open - 1, half as i64 + open + 1] {
                    if (0..w as i64).contains(&edge) {
                        for y in 0..h {
                            put(&mut out, edge as usize, y, line);
                        }
                    }
                }
            }
        }
        Mode::Laser => {
            // Pages are drawn column by column from the left; each lit pixel
            // of the column being drawn beams to the right edge. After the
            // last page the columns are erased from the left, beaming back to
            // the left edge.
            let lp = laser_pages(cw, w);
            let per_page = lp.width + hold;
            let (page, i) = (step / per_page, step % per_page);
            let column = |out: &mut Framebuffer, page: usize, x: usize, beam: std::ops::Range<usize>| {
                for y in 0..h {
                    let c = src((page * lp.width + x) as i64 - pad, y as i64);
                    for bx in beam.clone() {
                        put(out, bx, y, c);
                    }
                }
            };
            if page < lp.count {
                let drawn = i;
                holding = drawn >= lp.width;
                for x in 0..drawn.min(lp.width) {
                    column(&mut out, page, x, x..x + 1);
                }
                if !holding {
                    column(&mut out, page, drawn, drawn..w);
                }
            } else {
                let erased = step - lp.count * per_page;
                let page = lp.count - 1;
                for x in erased + 1..lp.width {
                    column(&mut out, page, x, x..x + 1);
                }
                column(&mut out, page, erased, 0..erased + 1);
            }
        }
    }

    let scrolling = matches!(p.mode, Mode::Left | Mode::Right);
    if p.flash && (holding || scrolling) && (step / 8) % 2 == 1 {
        out.clear();
    }
    if p.marquee {
        draw_marquee(&mut out, w, h, step);
    }
    out
}

/// Steps a page stays still in the paged modes (laser, up, down, fixed, snow,
/// curtain). Measured at speeds 5 and 7.
const HOLD_STEPS: usize = 32;

/// How the laser mode splits the text into pages.
struct LaserPages {
    /// Columns per page: the full display width (on a real badge, a 44-pixel
    /// display shows columns 0-43, 44-87, …, whatever the characters are).
    width: usize,
    count: usize,
}

fn laser_pages(content_width: usize, display_width: usize) -> LaserPages {
    let width = display_width.max(1);
    LaserPages {
        width,
        count: content_width.div_ceil(width).max(1),
    }
}

/// Steps between consecutive rows starting to fall.
const SNOW_SPACING: usize = 2;

/// (steps to pile one page in, steps for the final page to fall away).
fn snow_lengths(h: usize) -> (usize, usize) {
    let last_start = h.saturating_sub(1) * SNOW_SPACING;
    (last_start + h + 1, last_start + h + 1)
}

fn fb_color_hint(fb: &Framebuffer) -> LedColor {
    for x in 0..fb.width() {
        for y in 0..fb.height() {
            let c = fb.get(x, y);
            if c.is_on() {
                return c;
            }
        }
    }
    LedColor::RED
}

/// Dots chasing around the border.
fn draw_marquee(out: &mut Framebuffer, w: usize, h: usize, step: usize) {
    if w < 2 || h < 2 {
        return;
    }
    let mut perimeter = Vec::with_capacity(2 * (w + h));
    perimeter.extend((0..w).map(|x| (x, 0)));
    perimeter.extend((1..h).map(|y| (w - 1, y)));
    perimeter.extend((0..w - 1).rev().map(|x| (x, h - 1)));
    perimeter.extend((1..h - 1).rev().map(|y| (0, y)));
    for (i, (x, y)) in perimeter.into_iter().enumerate() {
        if (i + step).is_multiple_of(4) {
            out.set(x, y, LedColor::WHITE);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> Framebuffer {
        let mut fb = Framebuffer::new(11);
        for y in 0..11 {
            fb.set(0, y, LedColor::RED);
            fb.set(60, y, LedColor::GREEN);
        }
        fb
    }

    #[test]
    fn every_mode_produces_frames() {
        let fb = sample();
        for mode in Mode::ALL {
            let p = PreviewParams {
                mode,
                flash: true,
                marquee: true,
                width: 44,
                speed: 5,
            };
            let n = cycle_len(&fb, &p);
            let lit: usize = (0..n).map(|s| frame(&fb, &p, s).content_width()).sum();
            assert!(lit > 0, "{mode:?}");
        }
    }

    /// Topmost lit row of the frame, if any.
    fn top_row(fb: &Framebuffer) -> Option<usize> {
        (0..fb.height()).find(|&y| (0..fb.width()).any(|x| fb.is_on(x, y)))
    }

    #[test]
    fn vertical_scrolls_enter_pause_and_leave() {
        // A single dot on row 5.
        let mut fb = Framebuffer::new(11);
        fb.set(3, 5, LedColor::RED);
        let up = PreviewParams {
            mode: Mode::Up,
            flash: false,
            marquee: false,
            width: 44,
            speed: 5,
        };
        let down = PreviewParams { mode: Mode::Down, ..up };
        let h = 11;
        // Up: rises from below into place, holds, then leaves upward.
        assert_eq!(top_row(&frame(&fb, &up, 0)), None);
        assert_eq!(top_row(&frame(&fb, &up, h - 3)), Some(8));
        assert_eq!(top_row(&frame(&fb, &up, h)), Some(5));
        assert_eq!(top_row(&frame(&fb, &up, h + HOLD_STEPS - 1)), Some(5));
        assert_eq!(top_row(&frame(&fb, &up, h + HOLD_STEPS + 2)), Some(2));
        assert_eq!(top_row(&frame(&fb, &up, h + HOLD_STEPS + 7)), None);
        // Down: starts above, falls into place, holds, continues downward.
        assert_eq!(top_row(&frame(&fb, &down, h - 3)), Some(2));
        assert_eq!(top_row(&frame(&fb, &down, h)), Some(5));
        assert_eq!(top_row(&frame(&fb, &down, h + HOLD_STEPS + 2)), Some(8));
        assert_eq!(top_row(&frame(&fb, &down, h + HOLD_STEPS + 7)), None);
    }

    #[test]
    fn snow_piles_up_then_breaks_apart() {
        let mut fb = Framebuffer::new(11);
        for x in 0..10 {
            for y in 0..11 {
                fb.set(x, y, LedColor::RED);
            }
        }
        let p = PreviewParams {
            mode: Mode::Snow,
            flash: false,
            marquee: false,
            width: 44,
            speed: 5,
        };
        let (pile, fall) = snow_lengths(11);
        let lit = |s| {
            let f = frame(&fb, &p, s);
            (0..44)
                .flat_map(|x| (0..11).map(move |y| (x, y)))
                .filter(|&(x, y)| f.is_on(x, y))
                .count()
        };
        assert_eq!(lit(0), 0, "starts empty");
        assert!(lit(pile / 2) > 0 && lit(pile / 2) < 110, "partly piled");
        assert_eq!(lit(pile + 1), 110, "fully piled while holding");
        assert_eq!(cycle_len(&fb, &p), pile + HOLD_STEPS + fall);
        assert_eq!(lit(pile + HOLD_STEPS + fall - 1), 0, "fallen off the bottom");
    }

    #[test]
    fn curtain_draws_edge_lines_while_opening() {
        let fb = sample();
        let p = PreviewParams {
            mode: Mode::Curtain,
            flash: false,
            marquee: false,
            width: 44,
            speed: 5,
        };
        let full_column = |f: &Framebuffer, x: usize| (0..11).all(|y| f.is_on(x, y));
        // Step 3: opening spans columns 19..=25; edges at 18 and 26.
        let f = frame(&fb, &p, 3);
        assert!(full_column(&f, 18) && full_column(&f, 26));
        assert!(!f.is_on(17, 5) && !f.is_on(27, 5));
        // While holding, no curtain lines.
        let f = frame(&fb, &p, 22 + 1 + 5);
        assert!(!full_column(&f, 21));
    }

    #[test]
    fn laser_draws_display_wide_pages_with_beams() {
        // A dot at (2, 3) on page 1 and at (44 + 5, 6) on page 2.
        let mut fb = Framebuffer::new(11);
        fb.set(2, 3, LedColor::RED);
        fb.set(49, 6, LedColor::GREEN);
        let p = PreviewParams {
            mode: Mode::Laser,
            flash: false,
            marquee: false,
            width: 44,
            speed: 5,
        };
        let lp = laser_pages(fb.content_width(), 44);
        assert_eq!((lp.width, lp.count), (44, 2), "pages are the display width");

        // Drawing the empty first column: blank.
        assert_eq!(frame(&fb, &p, 0).content_width(), 0);
        // Drawing column 2: its pixel beams to the right edge of the display.
        let f = frame(&fb, &p, 2);
        assert!((2..44).all(|x| f.get(x, 3) == LedColor::RED));
        assert!(!f.get(1, 3).is_on());
        // Drawing column 3 (empty): column 2 stays, no beam.
        let f = frame(&fb, &p, 3);
        assert!(f.get(2, 3).is_on() && !f.get(3, 3).is_on());
        // Holding the finished page: just the content.
        let f = frame(&fb, &p, 44 + 5);
        assert!(f.get(2, 3).is_on() && !f.get(43, 3).is_on());

        // The second page shows columns 44.. at the left edge, unshifted.
        let per_page = 44 + HOLD_STEPS;
        let f = frame(&fb, &p, per_page + 44);
        assert!(f.get(5, 6) == LedColor::GREEN && f.content_width() == 6);

        // After the last page: erased from the left, beaming to the left edge.
        let f = frame(&fb, &p, 2 * per_page + 5);
        assert!((0..=5).all(|x| f.get(x, 6) == LedColor::GREEN));
        assert!(!f.get(6, 6).is_on());
        let f = frame(&fb, &p, 2 * per_page + 6);
        assert_eq!(f.content_width(), 0);
        assert_eq!(cycle_len(&fb, &p), 2 * per_page + 44);
    }

    /// Step time of a scrolling or paged mode at `speed`.
    fn step_us(speed: u8) -> u64 {
        step_interval_us(&PreviewParams {
            mode: Mode::Laser,
            flash: false,
            marquee: false,
            width: 44,
            speed,
        })
    }

    #[test]
    fn animation_frames_are_whole_bytes_wide_four_steps_each() {
        // Three 48-column pictures, each with one lit column at a different
        // place; shown on the 44-wide badge.
        let mut fb = Framebuffer::new(11);
        for (i, x) in [0usize, 10, 20].into_iter().enumerate() {
            for y in 0..11 {
                fb.set(i * 48 + x, y, LedColor::RED);
            }
        }
        let p = PreviewParams {
            mode: Mode::Animation,
            flash: false,
            marquee: false,
            width: 44,
            speed: 5,
        };
        assert_eq!(cycle_len(&fb, &p), 3 * ANIMATION_STEPS);
        for (i, x) in [0usize, 10, 20].into_iter().enumerate() {
            let f = frame(&fb, &p, i * ANIMATION_STEPS + 1);
            let lit: Vec<usize> = (0..44).filter(|&c| f.get(c, 5).is_on()).collect();
            assert_eq!(lit, vec![x], "picture {i}");
        }
        assert_eq!(animation_frame_width(48), 48);
        assert_eq!(animation_frame_width(44), 48);
        // As measured on the badge (badgectl calibrate, 30 taps a speed):
        // ms a picture, each within 0.15% of the model.
        let measured = [798.20, 532.09, 399.15, 310.40, 221.70, 133.00, 88.82, 44.36];
        for (sp, ms) in (1..=8u8).zip(measured) {
            let ours = animation_frame_us(sp) as f64 / 1000.;
            assert!(
                (ours - ms).abs() / ms < 0.002,
                "speed {sp}: {ours} ms, measured {ms} ms"
            );
        }
    }

    #[test]
    fn curtain_clears_from_the_center_after_the_last_page() {
        // Two pages of a fully lit message on the 44-wide badge.
        let mut fb = Framebuffer::new(11);
        for x in 0..88 {
            for y in 0..11 {
                fb.set(x, y, LedColor::RED);
            }
        }
        let p = PreviewParams {
            mode: Mode::Curtain,
            flash: false,
            marquee: false,
            width: 44,
            speed: 5,
        };
        let per_page = 22 + 1 + HOLD_STEPS;
        assert_eq!(cycle_len(&fb, &p), 2 * per_page + 23);
        let lit = |step: usize| -> Vec<usize> {
            let f = frame(&fb, &p, step);
            (0..44).filter(|&x| f.get(x, 5).is_on()).collect()
        };
        // The last page, fully open.
        assert_eq!(lit(2 * per_page - 1).len(), 44);
        // Clearing: a gap growing from the center, a line at each side of it.
        let clearing = lit(2 * per_page + 5);
        assert!(!clearing.contains(&22) && !clearing.contains(&17) && !clearing.contains(&27));
        assert!(clearing.contains(&16) && clearing.contains(&28) && clearing.contains(&0) && clearing.contains(&43));
        // Fully cleared at the end, then the first page opens from the center.
        assert!(lit(2 * per_page + 22).is_empty());
        assert_eq!(lit(cycle_len(&fb, &p)), vec![21, 22, 23]);
    }

    #[test]
    fn pause_follows_the_badge() {
        let pause_ms = |speed| (HOLD_STEPS as u64 * step_us(speed)) / 1000;
        // 1.778 s at speed 5 (measured); 32 of the measured steps at speeds
        // 1 and 8 (6.4 s and 0.35 s).
        assert!((1765..=1785).contains(&pause_ms(5)), "{}", pause_ms(5));
        assert!((340..=370).contains(&pause_ms(8)), "{}", pause_ms(8));
        assert!((6300..=6450).contains(&pause_ms(1)), "{}", pause_ms(1));
        // Shorter at every higher speed.
        assert!((1..8).all(|sp| pause_ms(sp) > pause_ms(sp + 1)));
    }

    #[test]
    fn step_times_match_the_badge() {
        // Speed 5 measured on video: 55.42 ms per step.
        assert!((55_300..=55_550).contains(&step_us(5)), "{}", step_us(5));
        assert!((1..=8).all(|sp| step_us(sp).is_multiple_of(TICK_US)));
        assert!((1..8).all(|sp| step_us(sp) > step_us(sp + 1)));
    }

    #[test]
    fn fixed_pages_change_after_the_shared_pause() {
        let fb = sample(); // content on two 44-wide pages
        let p = PreviewParams {
            mode: Mode::Fixed,
            flash: false,
            marquee: false,
            width: 44,
            speed: 5,
        };
        assert_eq!(cycle_len(&fb, &p), 2 * HOLD_STEPS);
        assert_ne!(frame(&fb, &p, HOLD_STEPS - 1), frame(&fb, &p, HOLD_STEPS));
    }

    #[test]
    fn short_messages_are_centered_in_the_page_modes() {
        // A 10-column block on the 44-wide display: 17 blank columns each side.
        let mut fb = Framebuffer::new(11);
        for x in 0..10 {
            for y in 0..11 {
                fb.set(x, y, LedColor::RED);
            }
        }
        let p = |mode| PreviewParams {
            mode,
            flash: false,
            marquee: false,
            width: 44,
            speed: 5,
        };
        let lit = |f: &Framebuffer| (0..44).filter(|&x| f.get(x, 10).is_on()).collect::<Vec<_>>();
        let centered: Vec<usize> = (17..27).collect();
        // A step where each mode shows the whole page, holding.
        let (pile, _) = snow_lengths(11);
        for (mode, step) in [
            (Mode::Fixed, 0),
            (Mode::Snow, pile),
            (Mode::Curtain, 23),
            (Mode::Laser, 44),
        ] {
            assert_eq!(lit(&frame(&fb, &p(mode), step)), centered, "{mode:?}");
        }
        // A message as wide as the display isn't moved.
        let mut full = Framebuffer::new(11);
        (0..44).for_each(|x| full.set(x, 10, LedColor::RED));
        for (mode, step) in [
            (Mode::Fixed, 0),
            (Mode::Snow, pile),
            (Mode::Curtain, 23),
            (Mode::Laser, 44),
        ] {
            assert_eq!(lit(&frame(&full, &p(mode), step)).len(), 44, "{mode:?}");
        }
    }

    #[test]
    fn left_scroll_enters_from_the_right() {
        let fb = sample();
        let p = PreviewParams {
            mode: Mode::Left,
            flash: false,
            marquee: false,
            width: 44,
            speed: 5,
        };
        assert_eq!(frame(&fb, &p, 0).content_width(), 0);
        assert!(frame(&fb, &p, 1).is_on(43, 0));
        assert!(frame(&fb, &p, 44).is_on(0, 0));
    }
}
