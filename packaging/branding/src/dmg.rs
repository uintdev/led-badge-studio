//! The macOS disk image's window background: `assets/branding/dmg-background.svg`
//! drawn at its own size and at twice that (for Retina screens), into
//! `assets/icons/`. The macOS build combines the two into the image's
//! background (see `packaging/macos/dmg-settings.py`).

use std::path::{Path, PathBuf};

use image::RgbaImage;

use crate::icons::encode;
use crate::write_if_changed;

const SOURCE: &str = "assets/branding/dmg-background.svg";

/// The window size the layout in `dmg-settings.py` is made for.
const SIZE: (u32, u32) = (640, 400);

pub fn write(root: &Path, changed: &mut Vec<PathBuf>) -> Result<(), String> {
    let path = root.join(SOURCE);
    let bytes = std::fs::read(&path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    let tree = resvg::usvg::Tree::from_data(&bytes, &resvg::usvg::Options::default())
        .map_err(|e| format!("{}: {e}", path.display()))?;
    let size = tree.size();
    if (size.width().round() as u32, size.height().round() as u32) != SIZE {
        return Err(format!(
            "{} must be {} x {} (the disk image window's size), not {} x {}",
            path.display(),
            SIZE.0,
            SIZE.1,
            size.width(),
            size.height()
        ));
    }
    let out = root.join("assets/icons");
    for (scale, name) in [(1, "dmg-background.png"), (2, "dmg-background@2x.png")] {
        let img = render(&tree, scale);
        write_if_changed(&out.join(name), &encode(&img)?, changed)?;
    }
    Ok(())
}

fn render(tree: &resvg::usvg::Tree, scale: u32) -> RgbaImage {
    let (w, h) = (SIZE.0 * scale, SIZE.1 * scale);
    let mut pixmap = resvg::tiny_skia::Pixmap::new(w, h).expect("size > 0");
    let t = resvg::tiny_skia::Transform::from_scale(scale as f32, scale as f32);
    resvg::render(tree, t, &mut pixmap.as_mut());
    // tiny-skia's pixels are premultiplied; the background is opaque, so
    // they're the same as straight alpha.
    RgbaImage::from_raw(w, h, pixmap.take()).expect("pixmap size matches")
}
