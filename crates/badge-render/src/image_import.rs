//! Image import (bmp/gif/jpg/png/ico) and PNG export.

use std::io::Cursor;
use std::path::Path;

use anyhow::{Context, Result};
use badge_core::{CardType, Framebuffer, LedColor, MAX_WIDTH};
use image::codecs::gif::GifDecoder;
use image::imageops::FilterType;
use image::{AnimationDecoder, DynamicImage, ImageDecoder, ImageFormat, Limits, Rgba, RgbaImage};

/// A decoded image (see [`decode_image`]).
pub use image::RgbaImage as DecodedImage;

/// Width of one animation frame when a GIF is laid out side by side
/// (the original software used 48 px).
const ANIMATION_FRAME_WIDTH: u32 = 48;

/// How a monochrome badge decides whether a pixel is lit.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum MonoThreshold {
    /// Brightness above `threshold`.
    #[default]
    Luminance,
    /// Red, green and blue all above `threshold` (the original behavior).
    AllChannels,
}

#[derive(Clone, Copy, Debug)]
pub struct ImportOptions {
    /// Scale images taller than the badge down to its height.
    pub fit_height: bool,
    pub invert: bool,
    /// 0..=255, compared with `>`.
    pub threshold: u8,
    pub mono: MonoThreshold,
}

impl Default for ImportOptions {
    fn default() -> Self {
        ImportOptions {
            fit_height: true,
            invert: false,
            threshold: 127,
            mono: MonoThreshold::Luminance,
        }
    }
}

/// Loads an image for `card`. Animated GIF frames are laid out every
/// [`ANIMATION_FRAME_WIDTH`] pixels for the Animation mode.
pub fn import_image(path: &Path, card: CardType, opts: &ImportOptions) -> Result<Framebuffer> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    import_image_bytes(&bytes, card, opts)
}

pub fn import_image_bytes(bytes: &[u8], card: CardType, opts: &ImportOptions) -> Result<Framebuffer> {
    Ok(image_to_leds(&decode_image(bytes, card, opts.fit_height)?, card, opts))
}

/// The slow half of an import: decodes and fits the image. Keep the result
/// to re-run [`image_to_leds`] cheaply.
pub fn decode_image(bytes: &[u8], card: CardType, fit_height: bool) -> Result<RgbaImage> {
    let height = card.height() as u32;
    let format = image::guess_format(bytes).context("unrecognised image format")?;
    let canvas = if format == ImageFormat::Gif {
        let mut decoder = GifDecoder::new(Cursor::new(bytes))?;
        decoder.set_limits(Limits::default())?;
        // Only as many frames as fit in a message are decoded, and each is
        // cut down to its tile as it comes: a long GIF's full-size frames
        // would take hundreds of megabytes all at once.
        let mut tiles = Vec::new();
        for frame in decoder.into_frames().take(MAX_WIDTH / ANIMATION_FRAME_WIDTH as usize) {
            let frame = frame.context("decoding GIF")?;
            let img = fit(DynamicImage::ImageRgba8(frame.into_buffer()), height, fit_height);
            let w = img.width().min(ANIMATION_FRAME_WIDTH);
            tiles.push(image::imageops::crop_imm(&img, 0, 0, w, img.height().min(height)).to_image());
        }
        if tiles.len() > 1 {
            let mut canvas = RgbaImage::new(ANIMATION_FRAME_WIDTH * tiles.len() as u32, height);
            for (i, tile) in tiles.iter().enumerate() {
                image::imageops::replace(&mut canvas, tile, (i as u32 * ANIMATION_FRAME_WIDTH) as i64, 0);
            }
            canvas
        } else {
            let img = image::load_from_memory_with_format(bytes, format)?;
            fit(img, height, fit_height)
        }
    } else {
        let img = image::load_from_memory_with_format(bytes, format)?;
        fit(img, height, fit_height)
    };
    Ok(canvas)
}

/// The fast half of an import: thresholds a decoded image into LEDs.
pub fn image_to_leds(canvas: &RgbaImage, card: CardType, opts: &ImportOptions) -> Framebuffer {
    threshold(canvas, card, opts)
}

fn fit(img: DynamicImage, height: u32, fit_height: bool) -> RgbaImage {
    if fit_height && img.height() > height {
        let w = ((img.width() as f64 * height as f64 / img.height() as f64).round() as u32).max(1);
        img.resize_exact(w, height, FilterType::Triangle).to_rgba8()
    } else {
        img.to_rgba8()
    }
}

fn threshold(img: &RgbaImage, card: CardType, opts: &ImportOptions) -> Framebuffer {
    let mut fb = Framebuffer::new(card.height());
    let t = opts.threshold;
    for (x, y, &Rgba([r, g, b, a])) in img.enumerate_pixels() {
        if x as usize >= MAX_WIDTH || y as usize >= card.height() {
            continue;
        }
        let opaque = a >= 128;
        let c = if card.is_color() {
            let (r, g, b) = if opts.invert {
                (255 - r, 255 - g, 255 - b)
            } else {
                (r, g, b)
            };
            if !opaque {
                LedColor::OFF
            } else {
                LedColor::new(((r > t) as u8) << 2 | ((g > t) as u8) << 1 | (b > t) as u8)
            }
        } else {
            let lit = opaque
                && match opts.mono {
                    MonoThreshold::Luminance => (r as u32 * 299 + g as u32 * 587 + b as u32 * 114) / 1000 > t as u32,
                    MonoThreshold::AllChannels => r > t && g > t && b > t,
                };
            let lit = if opts.invert { opaque && !lit } else { lit };
            if lit { LedColor::RED } else { LedColor::OFF }
        };
        if c.is_on() {
            fb.set(x as usize, y as usize, c);
        }
    }
    fb
}

/// Renders a framebuffer to an RGBA image, `scale` pixels per LED.
/// Monochrome badges are drawn in red.
pub fn to_image(fb: &Framebuffer, color: bool, scale: u32) -> RgbaImage {
    let w = fb.used_width().max(8) as u32;
    let h = fb.height() as u32;
    let scale = scale.max(1);
    let mut img = RgbaImage::from_pixel(w * scale, h * scale, Rgba([0, 0, 0, 255]));
    for y in 0..h {
        for x in 0..w {
            let c = fb.get(x as usize, y as usize);
            if !c.is_on() {
                continue;
            }
            let [r, g, b] = if color { c.to_rgb() } else { [255, 0, 0] };
            for dy in 0..scale {
                for dx in 0..scale {
                    img.put_pixel(x * scale + dx, y * scale + dy, Rgba([r, g, b, 255]));
                }
            }
        }
    }
    img
}

/// Saves a framebuffer as PNG/BMP (chosen from the extension), replacing an
/// earlier file whole or not at all, as exports do.
pub fn export_png(fb: &Framebuffer, color: bool, path: &Path, scale: u32) -> Result<()> {
    let format = ImageFormat::from_path(path).with_context(|| format!("writing {}", path.display()))?;
    crate::preview_export::write(path, |out| {
        Ok(to_image(fb, color, scale).write_to(&mut Cursor::new(out), format)?)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn png_roundtrip_mono() {
        let mut img = RgbaImage::from_pixel(20, 11, Rgba([0, 0, 0, 255]));
        img.put_pixel(3, 4, Rgba([255, 255, 255, 255]));
        img.put_pixel(19, 10, Rgba([200, 200, 200, 255]));
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png).unwrap();
        let fb = import_image_bytes(&bytes, CardType::Mono11, &ImportOptions::default()).unwrap();
        assert!(fb.is_on(3, 4));
        assert!(fb.is_on(19, 10));
        assert_eq!(fb.used_width(), 24);
    }

    #[test]
    fn decoding_once_gives_the_same_leds() {
        // A gradient, so different thresholds light different pixels.
        let img = RgbaImage::from_fn(300, 40, |x, _| Rgba([(x % 256) as u8, 90, 200, 255]));
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png).unwrap();
        for card in [CardType::Mono11, CardType::Rgb16] {
            let decoded = decode_image(&bytes, card, true).unwrap();
            for threshold in [10, 127, 240] {
                for invert in [false, true] {
                    let opts = ImportOptions {
                        threshold,
                        invert,
                        ..ImportOptions::default()
                    };
                    assert_eq!(
                        image_to_leds(&decoded, card, &opts),
                        import_image_bytes(&bytes, card, &opts).unwrap()
                    );
                }
            }
        }
    }

    /// Run with `cargo test --release -- --ignored --nocapture` to time it.
    #[test]
    #[ignore]
    fn time_decode_and_threshold() {
        let img = RgbaImage::from_fn(4000, 3000, |x, y| Rgba([(x ^ y) as u8, (x * 3) as u8, y as u8, 255]));
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png).unwrap();
        let t = std::time::Instant::now();
        let decoded = decode_image(&bytes, CardType::Mono11, true).unwrap();
        let decode = t.elapsed();
        let t = std::time::Instant::now();
        let _ = image_to_leds(&decoded, CardType::Mono11, &ImportOptions::default());
        println!(
            "4000x3000 photo: decode+fit {decode:?}, threshold only {:?}",
            t.elapsed()
        );
    }

    /// Each frame of an animated GIF becomes a tile of its own, cut to the
    /// tile's width.
    #[test]
    fn gif_frames_are_laid_out_side_by_side() {
        use image::codecs::gif::GifEncoder;
        use image::{Delay, Frame};
        let mut bytes = Vec::new();
        {
            let mut encoder = GifEncoder::new(&mut bytes);
            for i in 0..3u32 {
                let mut img = RgbaImage::from_pixel(60, 11, Rgba([0, 0, 0, 255]));
                img.put_pixel(i * 5, i, Rgba([255, 255, 255, 255]));
                // Beyond the tile's width: cut off.
                img.put_pixel(55, 10, Rgba([255, 255, 255, 255]));
                encoder
                    .encode_frame(Frame::from_parts(img, 0, 0, Delay::from_numer_denom_ms(100, 1)))
                    .unwrap();
            }
        }
        let fb = import_image_bytes(&bytes, CardType::Mono11, &ImportOptions::default()).unwrap();
        let lit: Vec<(usize, usize)> = (0..fb.width())
            .flat_map(|x| (0..fb.height()).map(move |y| (x, y)))
            .filter(|&(x, y)| fb.is_on(x, y))
            .collect();
        assert_eq!(lit, [(0, 0), (48 + 5, 1), (96 + 10, 2)]);
    }

    #[test]
    fn color_threshold() {
        let mut img = RgbaImage::from_pixel(4, 4, Rgba([0, 0, 0, 0]));
        img.put_pixel(0, 0, Rgba([255, 255, 0, 255]));
        let mut bytes = Vec::new();
        img.write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png).unwrap();
        let fb = import_image_bytes(&bytes, CardType::Rgb12, &ImportOptions::default()).unwrap();
        assert_eq!(fb.get(0, 0), LedColor::YELLOW);
    }

    #[test]
    fn saving_an_image_replaces_the_file_whole() {
        let dir = std::env::temp_dir().join(format!("lbs-save-image-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("message1.png");
        std::fs::write(&path, b"earlier").unwrap();
        let mut fb = Framebuffer::new(11);
        fb.set(2, 3, LedColor::RED);
        // A format it can't tell keeps the earlier file.
        assert!(export_png(&fb, false, &dir.join("message1.xyz"), 1).is_err());
        export_png(&fb, false, &path, 1).unwrap();
        let saved = image::load_from_memory(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(saved.to_rgba8().get_pixel(2, 3), &Rgba([255, 0, 0, 255]));
        assert!(!dir.join("message1.png.tmp").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
