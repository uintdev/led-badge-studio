//! App icons for every platform, made from one logo.
//!
//! Each size is made separately at its final pixel size: an SVG logo is
//! rendered straight from the vector art; a PNG logo is scaled down with a
//! Lanczos filter on premultiplied RGBA (so transparent edges don't get
//! dark fringes). Hand-made sizes in `pixel_sizes` win.
//!
//! All of that is the fallback: icon files placed in a platform's folder
//! (`assets/branding/linux`, `windows`, `macos`) are used for that platform
//! instead. macOS asset-catalog icons (Icon Composer `.icon`, Xcode
//! `.xcassets`) are compiled by `packaging/macos/build.sh`, which needs
//! Xcode's `actool`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use image::imageops::FilterType;
use image::{Rgba, Rgba32FImage, RgbaImage};

use crate::config::Branding;
use crate::write_if_changed;

/// Linux icon theme sizes (hicolor). Flathub accepts PNGs up to 512 px.
const LINUX: &[u32] = &[16, 22, 24, 32, 48, 64, 96, 128, 256, 512];
/// Windows: every size Explorer and the taskbar pick from at 100-200 %
/// display scaling; 256 is the largest an .ico holds.
const WINDOWS: &[u32] = &[16, 20, 24, 30, 32, 36, 40, 48, 60, 64, 72, 80, 96, 128, 256];
/// macOS .icns entries: (type, pixels). The @2x types are the Retina
/// versions of the size below them; ic10 (1024) is the largest.
const MACOS: &[(&[u8; 4], u32)] = &[
    (b"icp4", 16),
    (b"ic11", 32),
    (b"icp5", 32),
    (b"ic12", 64),
    (b"icp6", 64),
    (b"ic07", 128),
    (b"ic13", 256),
    (b"ic08", 256),
    (b"ic14", 512),
    (b"ic09", 512),
    (b"ic10", 1024),
];
/// Largest size any platform needs.
const LARGEST: u32 = 1024;

enum Logo {
    Svg(Box<resvg::usvg::Tree>),
    /// Square, premultiplied.
    Raster(Rgba32FImage),
}

struct Icons {
    logo: Logo,
    /// Draw the logo on a plain page (a document icon made from the logo).
    on_page: Option<Box<resvg::usvg::Tree>>,
    pixel_dir: Option<PathBuf>,
    cache: HashMap<u32, Vec<u8>>,
}

impl Icons {
    /// PNG bytes of the icon at `size` x `size` pixels.
    fn png(&mut self, size: u32) -> Result<Vec<u8>, String> {
        if let Some(png) = self.cache.get(&size) {
            return Ok(png.clone());
        }
        let png = match self.hand_made(size)? {
            Some(png) => png,
            None => encode(&self.render(size))?,
        };
        self.cache.insert(size, png.clone());
        Ok(png)
    }

    fn hand_made(&self, size: u32) -> Result<Option<Vec<u8>>, String> {
        let Some(dir) = &self.pixel_dir else {
            return Ok(None);
        };
        exact_png(&dir.join(format!("{size}.png")), size)
    }

    fn render(&self, size: u32) -> RgbaImage {
        let Some(page) = &self.on_page else {
            return self.render_logo(size);
        };
        let mut img = render_svg(page, size);
        let (x, y, side) = PAGE_LOGO;
        let px = |v: f32| (v * size as f32 / PAGE_VIEW).round() as i64;
        let logo = self.render_logo(px(side).max(1) as u32);
        image::imageops::overlay(&mut img, &logo, px(x), px(y));
        img
    }

    fn render_logo(&self, size: u32) -> RgbaImage {
        match &self.logo {
            Logo::Svg(tree) => render_svg(tree, size),
            Logo::Raster(src) => {
                let small = image::imageops::resize(src, size, size, FilterType::Lanczos3);
                let mut img = RgbaImage::new(size, size);
                for (dst, p) in img.pixels_mut().zip(small.pixels()) {
                    let a = p[3].clamp(0.0, 1.0);
                    let un = |v: f32| {
                        let v = if a > 0.0 { v / a } else { 0.0 };
                        (v.clamp(0.0, 1.0) * 255.0).round() as u8
                    };
                    *dst = Rgba([un(p[0]), un(p[1]), un(p[2]), (a * 255.0).round() as u8]);
                }
                img
            }
        }
    }
}

/// `tree` centered on a `size` square, keeping its aspect ratio.
fn render_svg(tree: &resvg::usvg::Tree, size: u32) -> RgbaImage {
    let mut pixmap = resvg::tiny_skia::Pixmap::new(size, size).expect("icon size > 0");
    let s = tree.size();
    let scale = size as f32 / s.width().max(s.height());
    let (dx, dy) = (
        (size as f32 - s.width() * scale) / 2.0,
        (size as f32 - s.height() * scale) / 2.0,
    );
    let t = resvg::tiny_skia::Transform::from_scale(scale, scale).post_translate(dx, dy);
    resvg::render(tree, t, &mut pixmap.as_mut());
    let mut img = RgbaImage::new(size, size);
    for (dst, p) in img.pixels_mut().zip(pixmap.pixels()) {
        let c = p.demultiply();
        *dst = Rgba([c.red(), c.green(), c.blue(), c.alpha()]);
    }
    img
}

/// The page a document icon is drawn on when no document art is set: a
/// sheet with a folded corner, in the proportions file managers use for
/// documents.
const PAGE_SVG: &str = r##"<svg xmlns="http://www.w3.org/2000/svg" width="256" height="256" viewBox="0 0 256 256">
<defs>
<linearGradient id="paper" x1="0" y1="0" x2="0" y2="1">
<stop offset="0" stop-color="#fbfbfa"/><stop offset="1" stop-color="#e8e7e4"/>
</linearGradient>
<filter id="shadow" x="-20%" y="-20%" width="140%" height="140%">
<feGaussianBlur stdDeviation="3"/>
</filter>
</defs>
<path d="M44 22h120l48 48v166a8 8 0 0 1-8 8H44a8 8 0 0 1-8-8V30a8 8 0 0 1 8-8z" fill="#000" opacity=".22" filter="url(#shadow)" transform="translate(0 3)"/>
<path d="M44 18h120l48 48v166a8 8 0 0 1-8 8H44a8 8 0 0 1-8-8V26a8 8 0 0 1 8-8z" fill="url(#paper)" stroke="#9a9996" stroke-width="2"/>
<path d="M164 18v40a8 8 0 0 0 8 8h40" fill="#d6d5d1" stroke="#9a9996" stroke-width="2" stroke-linejoin="round"/>
</svg>"##;
/// `PAGE_SVG`'s size, and where the logo goes on it: (x, y, side).
const PAGE_VIEW: f32 = 256.0;
const PAGE_LOGO: (f32, f32, f32) = (58.0, 96.0, 132.0);

fn page() -> Box<resvg::usvg::Tree> {
    Box::new(resvg::usvg::Tree::from_str(PAGE_SVG, &resvg::usvg::Options::default()).expect("PAGE_SVG is valid"))
}

pub fn encode(img: &RgbaImage) -> Result<Vec<u8>, String> {
    use image::ImageEncoder;
    use image::codecs::png::{CompressionType, FilterType as PngFilter, PngEncoder};
    let mut out = Vec::new();
    PngEncoder::new_with_quality(&mut out, CompressionType::Best, PngFilter::Adaptive)
        .write_image(img.as_raw(), img.width(), img.height(), image::ExtendedColorType::Rgba8)
        .map_err(|e| format!("encoding PNG: {e}"))?;
    Ok(out)
}

fn load_logo(path: &Path) -> Result<Logo, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("reading logo {}: {e}", path.display()))?;
    let is_svg = path
        .extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("svg") || e.eq_ignore_ascii_case("svgz"));
    if is_svg {
        let mut opts = resvg::usvg::Options {
            resources_dir: path.parent().map(Path::to_path_buf),
            ..Default::default()
        };
        // Text in the logo needs fonts.
        opts.fontdb_mut().load_system_fonts();
        let tree = resvg::usvg::Tree::from_data(&bytes, &opts).map_err(|e| format!("logo {}: {e}", path.display()))?;
        return Ok(Logo::Svg(Box::new(tree)));
    }
    let img = image::load_from_memory(&bytes)
        .map_err(|e| format!("logo {}: {e} (use an SVG or PNG file)", path.display()))?
        .to_rgba32f();
    let side = img.width().max(img.height());
    if side < LARGEST {
        eprintln!(
            "branding: warning: the logo is {}x{} pixels; icons up to {LARGEST}x{LARGEST} are made, \
             so the largest will be blurry. Use an SVG or a PNG of at least {LARGEST}x{LARGEST}.",
            img.width(),
            img.height()
        );
    }
    if img.width() != img.height() {
        eprintln!("branding: note: the logo isn't square; it is centered on a transparent square.");
    }
    // Square canvas, premultiplied for filtering.
    let mut square = Rgba32FImage::new(side, side);
    let (ox, oy) = ((side - img.width()) / 2, (side - img.height()) / 2);
    for (x, y, p) in img.enumerate_pixels() {
        let a = p[3];
        square.put_pixel(x + ox, y + oy, Rgba([p[0] * a, p[1] * a, p[2] * a, a]));
    }
    Ok(Logo::Raster(square))
}

fn ico(icons: &mut Icons) -> Result<Vec<u8>, String> {
    // PNG-compressed entries (Windows Vista and later).
    let pngs: Vec<(u32, Vec<u8>)> = WINDOWS
        .iter()
        .map(|&s| icons.png(s).map(|p| (s, p)))
        .collect::<Result<_, _>>()?;
    let mut head = Vec::new();
    head.extend_from_slice(&[0, 0, 1, 0]);
    head.extend_from_slice(&(pngs.len() as u16).to_le_bytes());
    let mut offset = 6 + 16 * pngs.len() as u32;
    let mut data = Vec::new();
    for (s, png) in &pngs {
        let dim = if *s >= 256 { 0 } else { *s as u8 };
        head.extend_from_slice(&[dim, dim, 0, 0]);
        head.extend_from_slice(&1u16.to_le_bytes()); // planes
        head.extend_from_slice(&32u16.to_le_bytes()); // bits per pixel
        head.extend_from_slice(&(png.len() as u32).to_le_bytes());
        head.extend_from_slice(&offset.to_le_bytes());
        offset += png.len() as u32;
        data.extend_from_slice(png);
    }
    head.extend(data);
    Ok(head)
}

fn icns(icons: &mut Icons) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    for (kind, size) in MACOS {
        let png = icons.png(*size)?;
        body.extend_from_slice(*kind);
        body.extend_from_slice(&(png.len() as u32 + 8).to_be_bytes());
        body.extend_from_slice(&png);
    }
    let mut out = b"icns".to_vec();
    out.extend_from_slice(&(body.len() as u32 + 8).to_be_bytes());
    out.extend(body);
    Ok(out)
}

/// Folder of per-platform icon overrides.
const PLATFORM_DIR: &str = "assets/branding";

/// The single file with extension `ext` in `dir` (not counting the
/// document icon's `document.<ext>`), if any.
fn find_one(dir: &Path, ext: &str) -> Result<Option<PathBuf>, String> {
    let Ok(rd) = std::fs::read_dir(dir) else {
        return Ok(None);
    };
    let mut found: Vec<PathBuf> = rd
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|e| e.eq_ignore_ascii_case(ext)))
        // document.ico / document.icns are the document icon's overrides.
        .filter(|p| p.file_stem().is_none_or(|s| !s.eq_ignore_ascii_case(DOCUMENT)))
        .collect();
    if found.len() > 1 {
        return Err(format!(
            "{} has {} .{ext} files; keep only one",
            dir.display(),
            found.len()
        ));
    }
    Ok(found.pop())
}

/// An override file's bytes, checked to start with `magic`.
fn read_override(path: &Path, magic: &[u8], kind: &str) -> Result<Vec<u8>, String> {
    let bytes = std::fs::read(path).map_err(|e| format!("reading {}: {e}", path.display()))?;
    if !bytes.starts_with(magic) {
        return Err(format!("{} isn't a valid {kind} file", path.display()));
    }
    println!("Using {}", path.display());
    Ok(bytes)
}

/// A PNG of exactly `size` pixels at `path`, if the file exists, re-encoded
/// so every icon is plain 8-bit RGBA.
fn exact_png(path: &Path, size: u32) -> Result<Option<Vec<u8>>, String> {
    let Ok(bytes) = std::fs::read(path) else {
        return Ok(None);
    };
    let img = image::load_from_memory(&bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    if img.width() != size || img.height() != size {
        return Err(format!(
            "{} is {}x{} pixels; it must be {size}x{size}",
            path.display(),
            img.width(),
            img.height()
        ));
    }
    Ok(Some(encode(&img.to_rgba8())?))
}

/// One icon set: the app icon, or the `.badge` document icon.
struct Set<'a> {
    /// Logo file (relative to the project folder).
    logo: &'a str,
    /// Draw `logo` on a plain page.
    on_page: bool,
    /// Folder of hand-made `<pixels>.png` sizes ("" for none).
    pixel_sizes: &'a str,
    /// Linux icon-theme context and name: `apps` / `<app id>` or
    /// `mimetypes` / `<app id>.Project`.
    linux_context: &'a str,
    linux_name: String,
    /// Override locations inside assets/branding/<platform>/.
    linux_overrides: PathBuf,
    windows_override: Option<PathBuf>,
    macos_override: Option<PathBuf>,
    /// Make a macOS `.icns` (else macOS draws the icon itself).
    macos: bool,
    /// Output file names in assets/icons/.
    ico: &'a str,
    icns: &'a str,
}

fn load_set(root: &Path, set: &Set) -> Result<Icons, String> {
    let pixel_dir = (!set.pixel_sizes.trim().is_empty()).then(|| root.join(set.pixel_sizes));
    if let Some(dir) = &pixel_dir
        && !dir.is_dir()
    {
        return Err(format!("{} is not a folder", dir.display()));
    }
    Ok(Icons {
        logo: load_logo(&root.join(set.logo))?,
        on_page: set.on_page.then(page),
        pixel_dir,
        cache: HashMap::new(),
    })
}

fn write_set(root: &Path, set: &Set, changed: &mut Vec<PathBuf>) -> Result<(), String> {
    let mut icons = load_set(root, set)?;
    let logo_path = root.join(set.logo);
    let out = root.join("assets/icons");

    // Linux: <N>.png and scalable.svg in the override folder win.
    let hicolor = out.join("hicolor");
    let (ctx, name) = (set.linux_context, &set.linux_name);
    for &s in LINUX {
        let png = match exact_png(&set.linux_overrides.join(format!("{s}.png")), s)? {
            Some(png) => png,
            None => icons.png(s)?,
        };
        write_if_changed(&hicolor.join(format!("{s}x{s}/{ctx}/{name}.png")), &png, changed)?;
    }
    // Linux also takes vector art as is; a PNG logo, or one put on a page,
    // has no scalable icon.
    let scalable = hicolor.join(format!("scalable/{ctx}/{name}.svg"));
    let svg_source = Some(set.linux_overrides.join("scalable.svg"))
        .filter(|p| p.is_file())
        .or_else(|| (matches!(icons.logo, Logo::Svg(_)) && !set.on_page).then(|| logo_path.clone()));
    if let Some(src) = svg_source {
        let svg = std::fs::read(&src).map_err(|e| format!("reading {}: {e}", src.display()))?;
        write_if_changed(&scalable, &svg, changed)?;
    } else if scalable.exists() {
        std::fs::remove_file(&scalable).map_err(|e| format!("removing {}: {e}", scalable.display()))?;
        // Leave no empty folder behind (fails, harmlessly, if it isn't empty).
        if let Some(dir) = scalable.parent() {
            let _ = std::fs::remove_dir(dir);
        }
        changed.push(scalable);
    }

    // Windows and macOS: a whole .ico / .icns override wins.
    let ico_bytes = match &set.windows_override {
        Some(p) => read_override(p, &[0, 0, 1, 0], ".ico")?,
        None => ico(&mut icons)?,
    };
    write_if_changed(&out.join(set.ico), &ico_bytes, changed)?;
    let icns_path = out.join(set.icns);
    if !set.macos {
        if icns_path.exists() {
            std::fs::remove_file(&icns_path).map_err(|e| format!("removing {}: {e}", icns_path.display()))?;
            changed.push(icns_path);
        }
        return Ok(());
    }
    let icns_bytes = match &set.macos_override {
        Some(p) => read_override(p, b"icns", ".icns")?,
        None => icns(&mut icons)?,
    };
    write_if_changed(&icns_path, &icns_bytes, changed)
}

/// Whether macOS draws the `.badge` file icon itself (the app icon on its
/// own page): when there's no document art and no macOS override for it.
pub fn macos_draws_document(root: &Path, b: &Branding) -> bool {
    b.icon.document.trim().is_empty() && !root.join(PLATFORM_DIR).join("macos/document.icns").is_file()
}

/// Name of the document icon override in a platform folder.
const DOCUMENT: &str = "document";

pub fn write_all(root: &Path, b: &Branding, app_id: &str, changed: &mut Vec<PathBuf>) -> Result<(), String> {
    let platform = root.join(PLATFORM_DIR);
    let (linux, windows, macos) = (platform.join("linux"), platform.join("windows"), platform.join("macos"));
    let named = |dir: &Path, file: &str| Some(dir.join(file)).filter(|p| p.is_file());

    // The app icon. Its overrides are any .ico / .icns other than the
    // document ones.
    write_set(
        root,
        &Set {
            logo: &b.icon.logo,
            on_page: false,
            pixel_sizes: &b.icon.pixel_sizes,
            linux_context: "apps",
            linux_name: app_id.to_string(),
            linux_overrides: linux.clone(),
            windows_override: find_one(&windows, "ico")?,
            macos_override: find_one(&macos, "icns")?,
            macos: true,
            ico: "led-badge-studio.ico",
            icns: "led-badge-studio.icns",
        },
        changed,
    )?;

    // The .badge document icon. On Linux it's named after the app ID so the
    // Flatpak may export it; the file-type definition points to it. Without
    // document art, it's the logo on a plain page, except on macOS, which
    // draws its own from the app icon.
    let own_art = !b.icon.document.trim().is_empty();
    write_set(
        root,
        &Set {
            logo: if own_art { &b.icon.document } else { &b.icon.logo },
            on_page: !own_art,
            pixel_sizes: &b.icon.document_pixel_sizes,
            linux_context: "mimetypes",
            linux_name: format!("{app_id}.Project"),
            linux_overrides: linux.join(DOCUMENT),
            windows_override: named(&windows, "document.ico"),
            macos_override: named(&macos, "document.icns"),
            macos: !macos_draws_document(root, b),
            ico: "led-badge-studio-document.ico",
            icns: "led-badge-studio-document.icns",
        },
        changed,
    )?;

    for ext in ["icon", "xcassets"] {
        if let Some(p) = find_one(&macos, ext)? {
            println!(
                "Found {}: it becomes the macOS app icon when building on a Mac with Xcode \
                 (packaging/macos/build.sh); the .icns stays as the fallback.",
                p.display()
            );
        }
    }
    Ok(())
}
