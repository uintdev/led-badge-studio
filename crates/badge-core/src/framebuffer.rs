use serde::{Deserialize, Deserializer, Serialize, Serializer};

use crate::LedColor;

/// Widest message the original software could hold (512 bytes x 8 pixels).
pub const MAX_WIDTH: usize = 4096;
/// Tallest badge display (rows).
pub const MAX_HEIGHT: usize = 16;

/// Which color plane to pack when encoding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Plane {
    /// Any lit LED (monochrome badges).
    Mono,
    Red,
    Green,
    Blue,
}

/// Pixels of one message: `height` rows by up to [`MAX_WIDTH`] columns, one
/// [`LedColor`] per pixel. Storage grows to the right on demand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Framebuffer {
    height: usize,
    width: usize,
    /// Row-major, `height * width` entries, each a 3-bit color index.
    data: Vec<u8>,
}

impl Framebuffer {
    pub fn new(height: usize) -> Self {
        Framebuffer {
            height,
            width: 0,
            data: Vec::new(),
        }
    }

    pub fn height(&self) -> usize {
        self.height
    }

    /// Allocated width (may include unlit columns on the right).
    pub fn width(&self) -> usize {
        self.width
    }

    pub fn get(&self, x: usize, y: usize) -> LedColor {
        if x >= self.width || y >= self.height {
            return LedColor::OFF;
        }
        LedColor::new(self.data[y * self.width + x])
    }

    pub fn is_on(&self, x: usize, y: usize) -> bool {
        self.get(x, y).is_on()
    }

    /// Sets a pixel. Writes outside the height or beyond [`MAX_WIDTH`] are ignored.
    pub fn set(&mut self, x: usize, y: usize, c: LedColor) {
        if y >= self.height || x >= MAX_WIDTH {
            return;
        }
        if x >= self.width {
            if !c.is_on() {
                return;
            }
            self.resize_width((x + 1).next_multiple_of(8));
        }
        self.data[y * self.width + x] = c.index();
    }

    pub fn clear(&mut self) {
        self.width = 0;
        self.data.clear();
    }

    /// Changes the number of rows, keeping the top-left content.
    pub fn set_height(&mut self, height: usize) {
        let mut next = Framebuffer::new(height);
        next.resize_width(self.width);
        for y in 0..height.min(self.height) {
            let src = &self.data[y * self.width..(y + 1) * self.width];
            next.data[y * self.width..(y + 1) * self.width].copy_from_slice(src);
        }
        *self = next;
    }

    fn resize_width(&mut self, new_width: usize) {
        let new_width = new_width.min(MAX_WIDTH);
        if new_width == self.width {
            return;
        }
        let mut data = vec![0u8; self.height * new_width];
        let copy = self.width.min(new_width);
        for y in 0..self.height {
            data[y * new_width..y * new_width + copy]
                .copy_from_slice(&self.data[y * self.width..y * self.width + copy]);
        }
        self.width = new_width;
        self.data = data;
    }

    /// Rightmost lit column + 1 (0 when empty), without rounding.
    pub fn content_width(&self) -> usize {
        for x in (0..self.width).rev() {
            if (0..self.height).any(|y| self.data[y * self.width + x] != 0) {
                return x + 1;
            }
        }
        0
    }

    /// Width that will be sent: content width rounded up to a whole byte
    /// (the original's `MaxLen`).
    pub fn used_width(&self) -> usize {
        self.content_width().next_multiple_of(8)
    }

    /// Packs 8 horizontal pixels starting at `byte_col * 8` on `row` into a
    /// byte, MSB = leftmost pixel.
    pub fn column_byte(&self, plane: Plane, byte_col: usize, row: usize) -> u8 {
        let mut b = 0u8;
        for bit in 0..8 {
            let c = self.get(byte_col * 8 + bit, row);
            let on = match plane {
                Plane::Mono => c.is_on(),
                Plane::Red => c.red(),
                Plane::Green => c.green(),
                Plane::Blue => c.blue(),
            };
            if on {
                b |= 0x80 >> bit;
            }
        }
        b
    }

    /// Replaces every lit pixel's color with `c`.
    pub fn recolor(&mut self, c: LedColor) {
        for p in &mut self.data {
            if *p != 0 {
                *p = c.index();
            }
        }
    }

    /// Text form used by the project file: one string per row, one hex digit
    /// (color index) per pixel, trimmed to [`Self::content_width`].
    pub fn to_rows(&self) -> Vec<String> {
        let w = self.content_width();
        (0..self.height)
            .map(|y| {
                (0..w)
                    .map(|x| char::from_digit(self.get(x, y).index() as u32, 8).unwrap())
                    .collect()
            })
            .collect()
    }

    pub fn from_rows(height: usize, rows: &[String]) -> Self {
        let mut fb = Framebuffer::new(height);
        for (y, row) in rows.iter().enumerate().take(height) {
            for (x, ch) in row.chars().take(MAX_WIDTH).enumerate() {
                let v = ch.to_digit(8).unwrap_or(0) as u8;
                fb.set(x, y, LedColor::new(v));
            }
        }
        fb
    }
}

#[derive(Serialize, Deserialize)]
struct FramebufferRepr {
    height: usize,
    rows: Vec<String>,
}

impl Serialize for Framebuffer {
    fn serialize<S: Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        FramebufferRepr {
            height: self.height,
            rows: self.to_rows(),
        }
        .serialize(s)
    }
}

impl<'de> Deserialize<'de> for Framebuffer {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let r = FramebufferRepr::deserialize(d)?;
        // Bounded for damaged files; the project sets the real height later.
        Ok(Framebuffer::from_rows(r.height.min(MAX_HEIGHT), &r.rows))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_get_and_width() {
        let mut fb = Framebuffer::new(11);
        assert_eq!(fb.used_width(), 0);
        fb.set(9, 3, LedColor::RED);
        assert!(fb.is_on(9, 3));
        assert_eq!(fb.content_width(), 10);
        assert_eq!(fb.used_width(), 16);
        fb.set(5000, 0, LedColor::RED);
        assert_eq!(fb.width(), 16);
    }

    #[test]
    fn column_bytes_msb_first() {
        let mut fb = Framebuffer::new(2);
        fb.set(0, 0, LedColor::RED);
        fb.set(7, 0, LedColor::GREEN);
        fb.set(8, 1, LedColor::WHITE);
        assert_eq!(fb.column_byte(Plane::Mono, 0, 0), 0x81);
        assert_eq!(fb.column_byte(Plane::Red, 0, 0), 0x80);
        assert_eq!(fb.column_byte(Plane::Green, 0, 0), 0x01);
        assert_eq!(fb.column_byte(Plane::Blue, 1, 1), 0x80);
    }

    #[test]
    fn rows_roundtrip_and_height_change() {
        let mut fb = Framebuffer::new(3);
        fb.set(1, 0, LedColor::RED);
        fb.set(2, 2, LedColor::CYAN);
        let back = Framebuffer::from_rows(3, &fb.to_rows());
        assert_eq!(back.to_rows(), fb.to_rows());
        let mut taller = fb.clone();
        taller.set_height(5);
        assert_eq!(taller.get(2, 2), LedColor::CYAN);
        taller.set_height(2);
        assert_eq!(taller.content_width(), 2);
    }
}
