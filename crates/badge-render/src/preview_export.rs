//! Saves the preview as an animated GIF, APNG or WebP, or as a still: one
//! cycle of each message at the badge's timing, drawn as the app draws LEDs.

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result, bail};
use badge_core::Framebuffer;
use image::codecs::gif::{GifEncoder, Repeat};
use image::{Delay, Frame, Rgba, RgbaImage};

use crate::led_look::{BOARD, Rgb, mix};
use crate::preview::{self, PreviewParams};
use crate::webp_lossless;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Format {
    Gif,
    Png,
    WebP,
}

impl Format {
    pub const ALL: [Format; 3] = [Format::Gif, Format::Png, Format::WebP];

    /// From a file name's extension (`.gif`, `.png`, `.apng`, `.webp`).
    pub fn from_path(path: &Path) -> Option<Format> {
        let ext = path.extension()?.to_str()?.to_ascii_lowercase();
        match ext.as_str() {
            "gif" => Some(Format::Gif),
            "png" | "apng" => Some(Format::Png),
            "webp" => Some(Format::WebP),
            _ => None,
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Format::Gif => "gif",
            Format::Png => "png",
            Format::WebP => "webp",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Format::Gif => "GIF",
            Format::Png => "PNG",
            Format::WebP => "WebP",
        }
    }
}

/// How the LEDs are drawn (`cell` is rounded to whole pixels).
pub use crate::led_look::LedLook as Look;

pub struct Part<'a> {
    pub bitmap: &'a Framebuffer,
    pub params: PreviewParams,
}

/// Viewers show shorter frames slowly, so faster steps are merged.
const MIN_FRAME_MS: f64 = 20.;
const MAX_FRAMES: usize = 4000;

pub fn save_animation(parts: &[Part], look: &Look, format: Format, path: &Path) -> Result<()> {
    let painter = Painter::new(look);
    let mut steps = Vec::new();
    for part in parts {
        collect_frames(part, &mut steps);
    }
    if steps.is_empty() {
        bail!("nothing to animate");
    }
    // Identical neighbors (one message's end, the next one's start) merge.
    let mut frames: Vec<(RgbaImage, f64)> = Vec::new();
    for ((fb, cols), ms) in steps.into_iter().take(MAX_FRAMES) {
        let image = painter.paint(&fb, cols);
        match frames.last_mut() {
            Some((last, last_ms)) if *last == image => *last_ms += ms,
            _ => frames.push((image, ms)),
        }
    }
    write(path, |out| match format {
        Format::Gif => write_gif(out, &frames),
        Format::Png => write_apng(out, &frames),
        Format::WebP => write_animated_webp(out, &frames),
    })
}

pub fn save_still(frame: &Framebuffer, cols: usize, look: &Look, format: Format, path: &Path) -> Result<()> {
    let image = Painter::new(look).paint(frame, cols);
    write(path, |out| match format {
        Format::Gif => write_gif(out, &[(image.clone(), 0.)]),
        Format::Png => {
            image.write_to(&mut std::io::Cursor::new(out), image::ImageFormat::Png)?;
            Ok(())
        }
        Format::WebP => {
            let (w, h) = image.dimensions();
            let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
            out.extend(webp_lossless::encode_still(&rgb, w, h));
            Ok(())
        }
    })
}

/// The frame with the most lit LEDs: a good still when none was chosen (the
/// first step is often blank).
pub fn busiest_frame(part: &Part) -> Framebuffer {
    let lit = |fb: &Framebuffer| -> usize {
        (0..fb.height())
            .map(|y| (0..part.params.width).filter(|&x| fb.get(x, y).is_on()).count())
            .sum()
    };
    (0..preview::cycle_len(part.bitmap, &part.params))
        .map(|step| preview::frame(part.bitmap, &part.params, step))
        .max_by_key(lit)
        .unwrap_or_else(|| Framebuffer::new(part.bitmap.height()))
}

/// Encodes in memory first, then writes beside `path` and moves it over,
/// so a failed export leaves no half file and keeps an earlier one.
fn write(path: &Path, encode: impl FnOnce(&mut Vec<u8>) -> Result<()>) -> Result<()> {
    let mut bytes = Vec::new();
    encode(&mut bytes).with_context(|| format!("encoding {}", path.display()))?;
    let mut tmp = path.as_os_str().to_owned();
    tmp.push(".tmp");
    let tmp = std::path::PathBuf::from(tmp);
    let written = std::fs::write(&tmp, bytes).and_then(|()| std::fs::rename(&tmp, path));
    if written.is_err() {
        let _ = std::fs::remove_file(&tmp);
    }
    written.with_context(|| format!("writing {}", path.display()))
}

fn write_gif(out: &mut Vec<u8>, frames: &[(RgbaImage, f64)]) -> Result<()> {
    let mut encoder = GifEncoder::new_with_speed(out, 10);
    if frames.len() > 1 {
        encoder.set_repeat(Repeat::Infinite)?;
    }
    // GIF delays are in centiseconds: carry the rounding over.
    let mut owed = 0.;
    for (image, ms) in frames {
        let wanted = ms + owed;
        let cs = (wanted / 10.).round().max(2.);
        owed = wanted - cs * 10.;
        encoder.encode_frame(Frame::from_parts(
            image.clone(),
            0,
            0,
            Delay::from_numer_denom_ms(cs as u32 * 10, 1),
        ))?;
    }
    Ok(())
}

fn write_apng(out: &mut Vec<u8>, frames: &[(RgbaImage, f64)]) -> Result<()> {
    let (w, h) = frames[0].0.dimensions();
    let mut encoder = png::Encoder::new(out, w, h);
    encoder.set_color(png::ColorType::Rgb);
    encoder.set_depth(png::BitDepth::Eight);
    encoder.set_compression(png::Compression::High);
    encoder.set_animated(frames.len() as u32, 0)?;
    let mut writer = encoder.write_header()?;
    let mut owed = 0.;
    for (image, ms) in frames {
        let wanted = ms + owed;
        let d = wanted.round().clamp(1., f64::from(u16::MAX));
        owed = wanted - d;
        writer.set_frame_delay(d as u16, 1000)?;
        let rgb = image::DynamicImage::ImageRgba8(image.clone()).to_rgb8();
        writer.write_image_data(&rgb)?;
    }
    writer.finish()?;
    Ok(())
}

/// An animated WebP: each frame's changed rectangle encoded losslessly, in
/// RIFF "WEBP" with VP8X, ANIM and one ANMF chunk per frame.
fn write_animated_webp(out: &mut Vec<u8>, frames: &[(RgbaImage, f64)]) -> Result<()> {
    use webp_lossless::riff_chunk;
    let (w, h) = frames[0].0.dimensions();
    let u24 = |v: u32| v.to_le_bytes()[..3].to_vec();
    let mut body = Vec::new();
    // VP8X: animation flag; canvas size minus one.
    let mut vp8x = vec![0b0000_0010, 0, 0, 0];
    vp8x.extend(u24(w - 1));
    vp8x.extend(u24(h - 1));
    body.extend(riff_chunk(b"VP8X", &vp8x));
    // ANIM: background color (BGRA), loop forever.
    body.extend(riff_chunk(b"ANIM", &[0, 0, 0, 255, 0, 0]));
    // Durations are whole milliseconds: carry the rounding over.
    let mut owed = 0.;
    let mut previous: Option<&RgbaImage> = None;
    for (image, ms) in frames {
        let wanted = ms + owed;
        let d = wanted.round().clamp(1., f64::from(0xFF_FFFF));
        owed = wanted - d;
        // Frame offsets must be even.
        let (x0, y0, x1, y1) = match previous {
            None => (0, 0, w, h),
            Some(prev) => changed_rect(prev, image).unwrap_or((0, 0, 2.min(w), 2.min(h))),
        };
        let (x0, y0) = (x0 & !1, y0 & !1);
        let mut rgb = Vec::with_capacity(((x1 - x0) * (y1 - y0) * 3) as usize);
        for y in y0..y1 {
            for x in x0..x1 {
                rgb.extend_from_slice(&image.get_pixel(x, y).0[..3]);
            }
        }
        let (fw, fh) = (x1 - x0, y1 - y0);
        let mut anmf = Vec::new();
        anmf.extend(u24(x0 / 2));
        anmf.extend(u24(y0 / 2));
        anmf.extend(u24(fw - 1));
        anmf.extend(u24(fh - 1));
        anmf.extend(u24(d as u32));
        anmf.push(0b0000_0010); // replace the rectangle (no blending); keep it
        anmf.extend(riff_chunk(b"VP8L", &webp_lossless::encode_vp8l(&rgb, fw, fh)));
        body.extend(riff_chunk(b"ANMF", &anmf));
        previous = Some(image);
    }
    out.write_all(b"RIFF")?;
    out.write_all(&(body.len() as u32 + 4).to_le_bytes())?;
    out.write_all(b"WEBP")?;
    out.write_all(&body)?;
    Ok(())
}

/// The smallest rectangle (x0, y0, x1, y1; end exclusive) covering every
/// pixel that differs.
fn changed_rect(a: &RgbaImage, b: &RgbaImage) -> Option<(u32, u32, u32, u32)> {
    let mut rect: Option<(u32, u32, u32, u32)> = None;
    for (x, y, p) in b.enumerate_pixels() {
        if a.get_pixel(x, y) != p {
            rect = Some(match rect {
                None => (x, y, x + 1, y + 1),
                Some((x0, y0, x1, y1)) => (x0.min(x), y0.min(y), x1.max(x + 1), y1.max(y + 1)),
            });
        }
    }
    rect
}

/// The distinct pictures of one cycle and how long each shows; pictures too
/// brief for viewers to show are dropped.
fn collect_frames(part: &Part, out: &mut Vec<((Framebuffer, usize), f64)>) {
    let step_ms = preview::step_interval_us(&part.params) as f64 / 1000.;
    let width = part.params.width;
    let mut pending: Option<(Framebuffer, f64)> = None;
    for step in 0..preview::cycle_len(part.bitmap, &part.params) {
        let frame = preview::frame(part.bitmap, &part.params, step);
        pending = Some(match pending.take() {
            Some((f, ms)) if f == frame => (f, ms + step_ms),
            Some((f, ms)) if ms >= MIN_FRAME_MS => {
                out.push(((f, width), ms));
                (frame, step_ms)
            }
            Some((_, ms)) => (frame, ms + step_ms),
            None => (frame, step_ms),
        });
    }
    if let Some((f, ms)) = pending {
        out.push(((f, width), ms));
    }
}

/// Draws frames from a [`Look`] as the app's `led_view` does.
struct Painter {
    look: Look,
    n: usize,
    off: Rgb,
    /// LED shape coverage, `n` x `n`, and the hot core's.
    led_mask: Vec<f32>,
    core_mask: Option<Vec<f32>>,
    /// The glow layers around one LED.
    glow: Vec<Glow>,
}

/// One glow layer as coverage over a square `size` wide whose top left is
/// `offset` pixels from the LED's cell.
struct Glow {
    alpha: f32,
    offset: i64,
    size: usize,
    coverage: Vec<f32>,
}

/// Coverage of GPUI's blurred rounded-rectangle shadow (its `fs_shadow`
/// shader) at `p`, measured from the shadow's center.
fn shadow_alpha(p: (f32, f32), half: f32, corner: f32, sigma: f32) -> f32 {
    let erf = |v: f32| {
        let a = v.abs();
        let r1 = 1. + (0.278393 + (0.230389 + (0.000972 + 0.078108 * a) * a) * a) * a;
        let r2 = r1 * r1;
        v.signum() - v.signum() / (r2 * r2)
    };
    let gaussian = |x: f32| (-(x * x) / (2. * sigma * sigma)).exp() / ((2. * std::f32::consts::PI).sqrt() * sigma);
    let blur_along_x = |x: f32, y: f32| {
        let delta = (half - corner - y.abs()).min(0.);
        let curved = half - corner + (corner * corner - delta * delta).max(0.).sqrt();
        let k = 0.5f32.sqrt() / sigma;
        (0.5 + 0.5 * erf((x + curved) * k)) - (0.5 + 0.5 * erf((x - curved) * k))
    };
    let (low, high) = (p.1 - half, p.1 + half);
    let start = (-3. * sigma).clamp(low, high);
    let end = (3. * sigma).clamp(low, high);
    let step = (end - start) / 4.;
    let mut y = start + step * 0.5;
    let mut alpha = 0.;
    for _ in 0..4 {
        alpha += blur_along_x(p.0, p.1 - y) * gaussian(y) * step;
        y += step;
    }
    alpha
}

impl Painter {
    fn new(look: &Look) -> Self {
        // No `..`, so a new field can't be left out.
        let Look {
            cell,
            round: _,
            color_card: _,
            tint: _,
            glow: _,
            level: _,
            dim: _,
        } = *look;
        let n = (cell.round() as usize).max(1);
        let look = Look {
            cell: n as f32,
            ..*look
        };
        let g = look.geometry();
        // 4x4 supersampling.
        let mask = |size: f32, corner: f32| -> Vec<f32> {
            let (c, r) = (n as f32 / 2., size / 2.);
            let mut m = vec![0.; n * n];
            for y in 0..n {
                for x in 0..n {
                    let mut hits = 0;
                    for sy in 0..4 {
                        for sx in 0..4 {
                            let px = x as f32 + (sx as f32 + 0.5) / 4. - c;
                            let py = y as f32 + (sy as f32 + 0.5) / 4. - c;
                            let qx = (px.abs() - (r - corner)).max(0.);
                            let qy = (py.abs() - (r - corner)).max(0.);
                            if px.abs() <= r && py.abs() <= r && qx * qx + qy * qy <= corner * corner {
                                hits += 1;
                            }
                        }
                    }
                    m[y * n + x] = hits as f32 / 16.;
                }
            }
            m
        };
        let center = n as f32 / 2.;
        let glow = look
            .glow_layers()
            .into_iter()
            .map(|layer| {
                let half = g.size / 2. + layer.spread;
                let reach = (half + 3. * layer.blur).ceil() as i64;
                let offset = center.floor() as i64 - reach;
                let size = (2 * reach + 2) as usize;
                let mut coverage = vec![0.; size * size];
                for ky in 0..size {
                    for kx in 0..size {
                        let p = (
                            (offset + kx as i64) as f32 + 0.5 - center,
                            (offset + ky as i64) as f32 + 0.5 - center,
                        );
                        coverage[ky * size + kx] = shadow_alpha(p, half, g.radius + layer.spread, layer.blur);
                    }
                }
                Glow {
                    alpha: layer.alpha,
                    offset,
                    size,
                    coverage,
                }
            })
            .collect();
        Painter {
            look,
            n,
            off: look.off(),
            led_mask: mask(g.size, g.radius),
            core_mask: g.core.map(|(size, corner)| mask(size, corner)),
            glow,
        }
    }

    fn paint(&self, frame: &Framebuffer, cols: usize) -> RgbaImage {
        let n = self.n;
        let rows = frame.height();
        let (w, h) = (cols * n, rows * n);
        let mut canvas = vec![BOARD; w * h];
        let lit = |x: usize, y: usize| Some(frame.get(x, y)).filter(|c| c.is_on());
        let stamp = |canvas: &mut Vec<Rgb>, lx: usize, ly: usize, color: Rgb, mask: &[f32]| {
            for y in 0..n {
                for x in 0..n {
                    let i = (ly * n + y) * w + lx * n + x;
                    canvas[i] = mix(color, canvas[i], mask[y * n + x]);
                }
            }
        };
        // Unlit LEDs first, so the glow can spill over them.
        for lx in 0..cols {
            for ly in (0..rows).filter(|&ly| lit(lx, ly).is_none()) {
                stamp(&mut canvas, lx, ly, self.off, &self.led_mask);
            }
        }
        for lx in 0..cols {
            for ly in 0..rows {
                let Some(c) = lit(lx, ly) else { continue };
                let color = self.look.led_color(c);
                for g in &self.glow {
                    let (ox, oy) = ((lx * n) as i64 + g.offset, (ly * n) as i64 + g.offset);
                    for ky in 0..g.size {
                        let y = oy + ky as i64;
                        if y < 0 || y >= h as i64 {
                            continue;
                        }
                        for kx in 0..g.size {
                            let x = ox + kx as i64;
                            if x < 0 || x >= w as i64 {
                                continue;
                            }
                            let a = g.coverage[ky * g.size + kx] * g.alpha;
                            if a > 0. {
                                let i = y as usize * w + x as usize;
                                canvas[i] = mix(color, canvas[i], a);
                            }
                        }
                    }
                }
            }
        }
        for lx in 0..cols {
            for ly in 0..rows {
                if let Some(c) = lit(lx, ly) {
                    let body = self.look.body(c);
                    stamp(&mut canvas, lx, ly, body, &self.led_mask);
                    if let Some(core) = &self.core_mask {
                        stamp(&mut canvas, lx, ly, self.look.core(body), core);
                    }
                }
            }
        }
        let to8 = |v: f32| (v.clamp(0., 1.) * 255.).round() as u8;
        RgbaImage::from_fn(w as u32, h as u32, |x, y| {
            let c = canvas[y as usize * w + x as usize];
            Rgba([to8(c[0]), to8(c[1]), to8(c[2]), 255])
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use badge_core::LedColor;
    use badge_core::Mode;
    use image::AnimationDecoder;

    #[test]
    fn writing_replaces_the_file_whole_or_not_at_all() {
        let dir = std::env::temp_dir().join(format!("lbs-export-write-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let path = dir.join("out.gif");
        std::fs::write(&path, b"earlier").unwrap();
        // An encoder that fails keeps the earlier file.
        assert!(write(&path, |_| anyhow::bail!("no")).is_err());
        assert_eq!(std::fs::read(&path).unwrap(), b"earlier");
        write(&path, |out| {
            out.extend_from_slice(b"new");
            Ok(())
        })
        .unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), b"new");
        assert!(!dir.join("out.gif.tmp").exists());
        // A write that fails leaves nothing behind.
        let missing = dir.join("missing/out.gif");
        assert!(write(&missing, |_| Ok(())).is_err());
        assert!(!dir.join("missing").exists());
        let _ = std::fs::remove_dir_all(dir);
    }

    fn scrolling() -> (Framebuffer, PreviewParams) {
        let mut fb = Framebuffer::new(11);
        for y in 0..11 {
            fb.set(2, y, LedColor::RED);
        }
        let params = PreviewParams {
            mode: Mode::Left,
            flash: false,
            marquee: false,
            width: 44,
            speed: 5,
        };
        (fb, params)
    }

    #[test]
    fn frames_follow_the_badge_pace() {
        let (fb, params) = scrolling();
        let mut frames = Vec::new();
        collect_frames(&Part { bitmap: &fb, params }, &mut frames);
        // Each step changes the picture; the total is one cycle's time.
        let total: f64 = frames.iter().map(|(_, ms)| ms).sum();
        let cycle = preview::cycle_len(&fb, &params) as f64 * preview::step_interval_us(&params) as f64 / 1000.;
        assert!((total - cycle).abs() < 0.01, "{total} vs {cycle}");
        assert!(frames.iter().all(|(_, ms)| *ms >= MIN_FRAME_MS));

        // Speed 8 steps are shorter than a frame can show: they are merged.
        let fast = PreviewParams { speed: 8, ..params };
        let mut fast_frames = Vec::new();
        collect_frames(
            &Part {
                bitmap: &fb,
                params: fast,
            },
            &mut fast_frames,
        );
        assert!(fast_frames.len() < preview::cycle_len(&fb, &fast));
    }

    /// Every format, animated and still, decodes back with the right size,
    /// frame count and (for the animations) total time.
    #[test]
    fn every_format_round_trips() {
        let (fb, params) = scrolling();
        let part = Part { bitmap: &fb, params };
        let mut expected = Vec::new();
        collect_frames(&part, &mut expected);
        let total_ms: f64 = expected.iter().map(|(_, ms)| ms).sum();
        let look = Look {
            cell: 6.,
            round: true,
            color_card: false,
            tint: LedColor::RED,
            glow: true,
            level: 1.,
            dim: false,
        };
        let (w, h) = (44 * 6, 11 * 6);
        let dir = std::env::temp_dir();
        for format in Format::ALL {
            let path = dir.join(format!(
                "led-badge-export-test-{}.{}",
                std::process::id(),
                format.extension()
            ));
            save_animation(&[Part { bitmap: &fb, params }], &look, format, &path).unwrap();
            let bytes = std::fs::read(&path).unwrap();
            let frames: Vec<image::Frame> = match format {
                Format::Gif => image::codecs::gif::GifDecoder::new(std::io::Cursor::new(&bytes))
                    .unwrap()
                    .into_frames()
                    .collect_frames()
                    .unwrap(),
                Format::Png => image::codecs::png::PngDecoder::new(std::io::Cursor::new(&bytes))
                    .unwrap()
                    .apng()
                    .unwrap()
                    .into_frames()
                    .collect_frames()
                    .unwrap(),
                Format::WebP => {
                    let mut d = image_webp::WebPDecoder::new(std::io::Cursor::new(&bytes)).unwrap();
                    assert!(d.is_animated(), "{format:?}");
                    assert_eq!(d.dimensions(), (w, h));
                    let mut buf = vec![0; d.output_buffer_size().unwrap()];
                    let mut n = 0;
                    let mut ms = 0u32;
                    while let Ok(delay) = d.read_frame(&mut buf) {
                        n += 1;
                        ms += delay;
                    }
                    assert_eq!(n, expected.len());
                    assert!((f64::from(ms) - total_ms).abs() <= 1., "{ms} vs {total_ms}");
                    std::fs::remove_file(&path).ok();
                    continue;
                }
            };
            assert_eq!(frames.len(), expected.len(), "{format:?}");
            assert_eq!(frames[0].buffer().dimensions(), (w, h), "{format:?}");
            let ms: f64 = frames
                .iter()
                .map(|f| {
                    let (n, d) = f.delay().numer_denom_ms();
                    f64::from(n) / f64::from(d)
                })
                .sum();
            // GIF delays are in hundredths of a second.
            let tolerance = if format == Format::Gif { 10. } else { 1. };
            assert!((ms - total_ms).abs() <= tolerance, "{format:?}: {ms} vs {total_ms}");

            // A still: one picture of the same size.
            let still = busiest_frame(&part);
            save_still(&still, 44, &look, format, &path).unwrap();
            let img = image::open(&path).unwrap();
            assert_eq!((img.width(), img.height()), (w, h), "{format:?} still");
            std::fs::remove_file(&path).ok();
        }
        // The WebP still too.
        let path = dir.join(format!("led-badge-export-test-{}.webp", std::process::id()));
        save_still(&busiest_frame(&part), 44, &look, Format::WebP, &path).unwrap();
        let d = image_webp::WebPDecoder::new(std::io::Cursor::new(std::fs::read(&path).unwrap())).unwrap();
        assert!(!d.is_animated());
        assert_eq!(d.dimensions(), (w, h));
        std::fs::remove_file(&path).ok();
    }

    #[test]
    fn the_busiest_frame_shows_the_most() {
        let (fb, params) = scrolling();
        let frame = busiest_frame(&Part { bitmap: &fb, params });
        let lit: usize = (0..11)
            .map(|y| (0..44).filter(|&x| frame.get(x, y).is_on()).count())
            .sum();
        assert_eq!(lit, 11);
    }

    /// Every part of the look shows in the exported picture, so an effect
    /// the preview draws can't be missing from the export. A new field in
    /// `LedLook` doesn't compile here until it's added below.
    #[test]
    fn every_part_of_the_look_shows_in_the_export() {
        let mut fb = Framebuffer::new(11);
        for (x, y) in [(2, 3), (3, 3), (6, 5), (10, 8)] {
            fb.set(x, y, LedColor::GREEN);
        }
        let base = Look {
            cell: 10.,
            round: true,
            color_card: false,
            tint: LedColor::RED,
            glow: true,
            level: 0.5,
            dim: false,
        };
        let picture = |look: Look| Painter::new(&look).paint(&fb, 16);
        let before = picture(base);
        let changes = [
            ("cell", Look { cell: 12., ..base }),
            ("round", Look { round: false, ..base }),
            (
                "color_card",
                Look {
                    color_card: true,
                    ..base
                },
            ),
            (
                "tint",
                Look {
                    tint: LedColor::BLUE,
                    ..base
                },
            ),
            ("glow", Look { glow: false, ..base }),
            ("level", Look { level: 1., ..base }),
            ("dim", Look { dim: true, ..base }),
        ];
        let Look {
            cell: _,
            round: _,
            color_card: _,
            tint: _,
            glow: _,
            level: _,
            dim: _,
        } = base;
        for (name, look) in changes {
            assert_ne!(
                picture(look),
                before,
                "changing {name} doesn't change the exported picture"
            );
        }
    }

    #[test]
    fn formats_come_from_the_extension() {
        assert_eq!(Format::from_path(Path::new("a.GIF")), Some(Format::Gif));
        assert_eq!(Format::from_path(Path::new("a.apng")), Some(Format::Png));
        assert_eq!(Format::from_path(Path::new("a.webp")), Some(Format::WebP));
        assert_eq!(Format::from_path(Path::new("a.bmp")), None);
    }
}
