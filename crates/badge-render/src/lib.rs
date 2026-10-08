//! Turns message text and images into badge [`Framebuffer`]s.

pub mod bitmap_font;
pub mod icons;
pub mod image_import;
pub mod led_look;
pub mod licenses;
pub mod preview;
pub mod preview_export;
pub mod system_font;
mod text;
mod webp_lossless;

pub use bitmap_font::FontLibrary;
pub use image_import::{ImportOptions, MonoThreshold, export_png, import_image};
pub use system_font::SystemFonts;
pub use text::{Token, parse_text};

use badge_core::{CardType, FontChoice, Framebuffer, LedColor, Message, Source};

/// Owns the font resources needed to render messages.
pub struct Renderer {
    pub builtin: FontLibrary,
    pub system: SystemFonts,
}

impl Renderer {
    pub fn new(builtin: FontLibrary) -> Self {
        Renderer {
            builtin,
            system: SystemFonts::load(),
        }
    }

    /// Renders a message's text for the given card. Bitmap-sourced messages
    /// are returned unchanged.
    pub fn render(&mut self, msg: &Message, card: CardType) -> Framebuffer {
        if msg.source == Source::Bitmap {
            let mut fb = msg.bitmap.clone();
            if fb.height() != card.height() {
                fb.set_height(card.height());
            }
            return fb;
        }
        self.render_text(msg, card)
    }

    pub fn render_text(&mut self, msg: &Message, card: CardType) -> Framebuffer {
        let height = card.height();
        let mut fb = Framebuffer::new(height);
        let color_of = |i: usize| -> LedColor {
            if card.is_color() {
                let c = msg.color_at(i);
                if c.is_on() { c } else { LedColor::RED }
            } else {
                LedColor::RED
            }
        };
        let mut x: i64 = 0;
        let y_off = -(msg.top_offset as i64);
        for token in parse_text(&msg.text) {
            match token {
                Token::Icon { index, char_index } => {
                    let glyph = icons::icon(index, height);
                    stamp(&mut fb, &glyph, x, y_off, color_of(char_index));
                    x += glyph.width as i64 + msg.letter_spacing as i64;
                }
                Token::Char { ch, char_index } => {
                    let color = color_of(char_index);
                    let glyph = match &msg.font {
                        FontChoice::Builtin => self
                            .builtin
                            .glyph(ch, height, msg.vertical)
                            .or_else(|| self.system.fallback_glyph(ch, height)),
                        // Missing characters (or Unifont on a badge under 16
                        // rows) come from the default font.
                        FontChoice::Pixel(font) => self
                            .builtin
                            .pixel_glyph(*font, ch, height, msg.vertical)
                            .or_else(|| self.builtin.glyph(ch, height, msg.vertical))
                            .or_else(|| self.system.fallback_glyph(ch, height)),
                        FontChoice::System {
                            family,
                            size_px,
                            bold,
                            italic,
                        } => self
                            .system
                            .glyph(family, *size_px, *bold, *italic, ch)
                            .or_else(|| self.system.fallback_glyph(ch, *size_px as usize)),
                    };
                    if let Some(g) = glyph {
                        stamp(&mut fb, &g, x, y_off, color);
                        x += g.advance as i64 + msg.letter_spacing as i64;
                    }
                }
            }
            if x >= badge_core::MAX_WIDTH as i64 {
                break;
            }
        }
        fb
    }
}

/// A 1-bit glyph image positioned relative to the pen.
#[derive(Clone, Debug, Default)]
pub struct Glyph {
    pub width: usize,
    pub height: usize,
    /// Offset of the image from the pen position (top of line = 0).
    pub left: i64,
    pub top: i64,
    pub advance: usize,
    /// Row-major on/off pixels, `width * height`.
    pub pixels: Vec<bool>,
}

impl Glyph {
    pub fn get(&self, x: usize, y: usize) -> bool {
        self.pixels[y * self.width + x]
    }
}

fn stamp(fb: &mut Framebuffer, g: &Glyph, pen_x: i64, pen_y: i64, color: LedColor) {
    for gy in 0..g.height {
        for gx in 0..g.width {
            if !g.get(gx, gy) {
                continue;
            }
            let x = pen_x + g.left + gx as i64;
            let y = pen_y + g.top + gy as i64;
            if x >= 0 && y >= 0 {
                fb.set(x as usize, y as usize, color);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn renderer() -> Renderer {
        Renderer::new(FontLibrary::embedded())
    }

    #[test]
    fn builtin_text_with_icon() {
        let mut r = renderer();
        let msg = Message {
            text: "Hi~0".into(),
            ..Message::default()
        };
        let fb = r.render(&msg, CardType::Mono11);
        // two 8px ASCII glyphs + one 11px icon
        assert!(fb.content_width() > 16 && fb.content_width() <= 27);
        assert_eq!(fb.height(), 11);
    }

    #[test]
    fn letter_spacing_widens_and_tightens() {
        let mut r = renderer();
        let mut width = |spacing| {
            let msg = Message {
                text: "HHHH".into(),
                letter_spacing: spacing,
                ..Message::default()
            };
            r.render(&msg, CardType::Mono11).content_width()
        };
        let (normal, wide, tight) = (width(0), width(3), width(-1));
        assert_eq!(wide, normal + 9);
        assert_eq!(tight, normal - 3);
    }

    #[test]
    fn color_spans() {
        let mut r = renderer();
        let msg = Message {
            text: "II".into(),
            colors: vec![LedColor::GREEN, LedColor::BLUE],
            ..Message::default()
        };
        let fb = r.render(&msg, CardType::Rgb12);
        let lit: Vec<LedColor> = (0..fb.width())
            .flat_map(|x| (0..12).map(move |y| (x, y)))
            .map(|(x, y)| fb.get(x, y))
            .filter(|c| c.is_on())
            .collect();
        assert!(lit.contains(&LedColor::GREEN) && lit.contains(&LedColor::BLUE));
    }

    #[test]
    fn system_font_renders_something() {
        let mut r = renderer();
        let family = r.system.default_family();
        let msg = Message {
            text: "Hello".into(),
            font: FontChoice::System {
                family,
                size_px: 12,
                bold: true,
                italic: false,
            },
            ..Message::default()
        };
        let fb = r.render(&msg, CardType::Mono12);
        if !r.system.families().is_empty() {
            assert!(fb.content_width() > 10, "width {}", fb.content_width());
        }
    }
}
