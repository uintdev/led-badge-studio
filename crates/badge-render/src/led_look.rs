//! How LEDs look: the one description shared by everything that draws them,
//! the app's preview and Pixels canvas (on the GPU) and the exported images
//! (`preview_export`, on the CPU), so an export shows exactly what the
//! preview shows.
//!
//! To add an effect, add a field to [`LedLook`] and the math for it here.
//! Both renderers list every field of `LedLook` (they destructure it without
//! `..`), so the build fails until each of them draws the new effect; and
//! a test in `preview_export` checks that every field changes the exported
//! picture.

use badge_core::LedColor;

/// An sRGB color, each channel 0 to 1.
pub type Rgb = [f32; 3];

/// Everything about how the LEDs look.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LedLook {
    /// Pixels per LED.
    pub cell: f32,
    /// Round LEDs (else squares with slightly rounded corners).
    pub round: bool,
    /// Color badge (each LED its own color) or mono (every LED `tint`).
    pub color_card: bool,
    /// The LED color of a mono badge.
    pub tint: LedColor,
    /// A soft glow in each lit LED's color around it, spilling onto the
    /// board and the LEDs next to it.
    pub glow: bool,
    /// The badge's brightness, 0.25 to 1: how strong and how wide the glow
    /// is (and how dim the LEDs are with `dim`).
    pub level: f32,
    /// Lit LEDs dimmed to `level` (the preview's Show brightness).
    pub dim: bool,
}

/// The board behind the LEDs.
pub const BOARD: Rgb = [11. / 255., 12. / 255., 14. / 255.];
/// An unlit LED of a color badge (clear, neutral).
const OFF_RGB_LED: Rgb = [0x24 as f32 / 255., 0x26 as f32 / 255., 0x2c as f32 / 255.];
/// A lit mono LED is warmer than its color at the edge and nearly white in
/// the middle.
const WARM: Rgb = [1., 0.745, 0.353];
const HOT: Rgb = [1., 0.925, 0.745];

/// `a` mixed with `b`: `amount` 1 gives `a`, 0 gives `b`.
pub fn mix(a: Rgb, b: Rgb, amount: f32) -> Rgb {
    [0, 1, 2].map(|i| b[i] + (a[i] - b[i]) * amount)
}

/// The on-screen color of an LED color: its channels on, the others dim
/// (softened, so white and yellow don't glare).
pub fn soft(c: LedColor) -> Rgb {
    c.to_rgb().map(|v| if v > 0 { 1. } else { 0x30 as f32 / 255. })
}

/// An LED color at full strength: its channels on, the others nearly off.
fn saturated(c: LedColor) -> Rgb {
    c.to_rgb().map(|v| if v > 0 { 1. } else { 0.06 })
}

/// How bright an LED driven at `level` (0-1) looks: eyes see the badge's
/// low settings as brighter than their power (75% ≈ 0.93, 50% ≈ 0.84,
/// 25% ≈ 0.71).
pub fn perceived(level: f32) -> f32 {
    level.clamp(0., 1.).powf(0.25)
}

/// Where an LED sits in its cell, in pixels.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Geometry {
    /// Space between the cell's edge and the LED.
    pub gap: f32,
    /// The LED's width and height.
    pub size: f32,
    /// Its corner radius.
    pub radius: f32,
    /// The hot core's width and height (centered on the LED), and corner
    /// radius; `None` when LEDs are too small to show one.
    pub core: Option<(f32, f32)>,
}

/// One layer of the glow: the LED grown by `spread`, blurred (Gaussian,
/// sigma `blur`) and painted at `alpha` in the LED's color.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct GlowLayer {
    pub alpha: f32,
    pub blur: f32,
    pub spread: f32,
}

impl LedLook {
    pub fn geometry(&self) -> Geometry {
        let cell = self.cell;
        let gap = if cell >= 8. {
            (cell * 0.14).max(1.)
        } else if cell >= 4. {
            0.5
        } else {
            0.
        };
        let size = cell - 2. * gap;
        let radius = if self.round { size / 2. } else { (size * 0.15).min(2.) };
        let core = (cell >= 6.).then(|| {
            let core = size * 0.5;
            (core, if self.round { core / 2. } else { radius * 0.5 })
        });
        Geometry {
            gap,
            size,
            radius,
            core,
        }
    }

    /// How bright the LEDs look at the badge's brightness.
    pub fn seen(&self) -> f32 {
        perceived(self.level)
    }

    /// An unlit LED: on mono badges a dark version of the LED color (the
    /// color of its package), on color badges neutral.
    pub fn off(&self) -> Rgb {
        if self.color_card {
            OFF_RGB_LED
        } else {
            mix(soft(self.tint), BOARD, 0.22)
        }
    }

    /// The color an LED shows as (`c` on color badges, the tint on mono).
    pub fn led_color(&self, c: LedColor) -> Rgb {
        soft(if self.color_card { c } else { self.tint })
    }

    /// A lit LED's body: on mono badges a strong, saturated, slightly warm
    /// version of the LED color; on color badges a little deeper than the
    /// pure color. Dimmed toward unlit with `dim`.
    pub fn body(&self, c: LedColor) -> Rgb {
        let body = if self.color_card {
            mix(soft(c), BOARD, 0.86)
        } else {
            mix(mix(saturated(self.tint), WARM, 0.94), BOARD, 0.92)
        };
        if self.dim {
            mix(body, self.off(), self.seen())
        } else {
            body
        }
    }

    /// The hot, nearly white core of a lit LED with `body`.
    pub fn core(&self, body: Rgb) -> Rgb {
        mix(HOT, body, if self.dim { 0.35 * self.seen() } else { 0.35 })
    }

    /// The glow's layers (none when it's off): a light ring and a very light
    /// wash on the board. Each starts from an area a little larger than the
    /// LED before blurring; blurring the LED alone over a wide area would
    /// spread its light too thin to see. Lower brightness makes the glow
    /// weaker and a quarter shorter (sizes are for 9 px LEDs, scaled).
    pub fn glow_layers(&self) -> Vec<GlowLayer> {
        if !self.glow {
            return Vec::new();
        }
        let cell = self.cell;
        let t = ((self.level - 0.25) / 0.75).clamp(0., 1.);
        let unit = cell / 9. * (0.75 + 0.25 * t);
        let seen = self.seen();
        vec![
            GlowLayer {
                alpha: 0.16 * seen,
                blur: 3.2 * unit,
                spread: 1.3 * unit,
            },
            GlowLayer {
                alpha: 0.10 * seen,
                blur: cell * 1.15,
                spread: cell * 0.6,
            },
        ]
    }
}
