//! The built-in pixel fonts, embedded in the binary.
//!
//! * ASCII: the IBM PC VGA ROM fonts, as shipped with the vendor software
//!   (`assets/fonts/ascii-8x{11,12,16}.bin`, one byte per row).
//! * Chinese, Japanese and Korean: Fusion Pixel Font (11- and 12-row badges)
//!   and GNU Unifont (16 rows), converted by `packaging/fonts/build_fonts.py`.
//!   As converted copies they are Modified Versions under the SIL OFL and
//!   must not use the original fonts' reserved names.
//!
//! `LED_BADGE_STUDIO_FONTLIB` can point at the vendor's own CJK libraries
//! (`HZK12`, `JPK16V`, …), which aren't bundled: they appear to be renders of
//! Windows system fonts.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use encoding_rs::{BIG5, EUC_KR, Encoding, GBK, SHIFT_JIS};

use badge_core::PixelFont;

use crate::Glyph;

static ASCII_11: &[u8] = include_bytes!("../../../assets/fonts/ascii-8x11.bin");
static ASCII_12: &[u8] = include_bytes!("../../../assets/fonts/ascii-8x12.bin");
static ASCII_16: &[u8] = include_bytes!("../../../assets/fonts/ascii-8x16.bin");

/// Regional glyph style for CJK characters.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Script {
    /// Simplified Chinese (GBK).
    ZhHans,
    /// Traditional Chinese (Big5).
    ZhHant,
    /// Japanese (Shift-JIS).
    Ja,
    /// Korean (EUC-KR).
    Ko,
}

impl Script {
    const ALL: [Script; 4] = [Script::ZhHans, Script::ZhHant, Script::Ja, Script::Ko];

    fn encoding(self) -> &'static Encoding {
        match self {
            Script::ZhHans => GBK,
            Script::ZhHant => BIG5,
            Script::Ja => SHIFT_JIS,
            Script::Ko => EUC_KR,
        }
    }

    /// File-name prefix of the vendor's library for this script.
    fn vendor_prefix(self) -> &'static str {
        match self {
            Script::ZhHans => "HZK",
            Script::ZhHant => "TWK",
            Script::Ja => "JPK",
            Script::Ko => "KRK",
        }
    }
}

macro_rules! table_bytes {
    ($size:literal, $script:literal) => {
        include_bytes!(concat!("../../../assets/fonts/cjk-", $size, "-", $script, ".lbg"))
    };
}

/// (cell size, script) -> compressed table. The zh-hans table is the base;
/// the others only hold glyphs that differ from it.
static TABLE_DATA: [(usize, Script, &[u8]); 12] = [
    (10, Script::ZhHans, table_bytes!("10", "zh-hans")),
    (10, Script::ZhHant, table_bytes!("10", "zh-hant")),
    (10, Script::Ja, table_bytes!("10", "ja")),
    (10, Script::Ko, table_bytes!("10", "ko")),
    (12, Script::ZhHans, table_bytes!("12", "zh-hans")),
    (12, Script::ZhHant, table_bytes!("12", "zh-hant")),
    (12, Script::Ja, table_bytes!("12", "ja")),
    (12, Script::Ko, table_bytes!("12", "ko")),
    (16, Script::ZhHans, table_bytes!("16", "zh-hans")),
    (16, Script::ZhHant, table_bytes!("16", "zh-hant")),
    (16, Script::Ja, table_bytes!("16", "ja")),
    (16, Script::Ko, table_bytes!("16", "ko")),
];

struct Table {
    cell: usize,
    code_points: Vec<u32>,
    advances: Vec<u8>,
    rows: Vec<u16>,
}

impl Table {
    fn parse(compressed: &[u8]) -> Table {
        let data = miniz_oxide::inflate::decompress_to_vec_zlib(compressed).expect("corrupt embedded font table");
        assert_eq!(&data[..4], b"LBGF", "bad embedded font table");
        let cell = data[5] as usize;
        let count = u32::from_le_bytes(data[8..12].try_into().unwrap()) as usize;
        let mut pos = 12;
        let code_points = (0..count)
            .map(|i| u32::from_le_bytes(data[pos + i * 4..pos + i * 4 + 4].try_into().unwrap()))
            .collect();
        pos += count * 4;
        let advances = data[pos..pos + count].to_vec();
        pos += count;
        let rows = data[pos..pos + count * cell * 2]
            .as_chunks::<2>()
            .0
            .iter()
            .map(|b| u16::from_le_bytes(*b))
            .collect();
        Table {
            cell,
            code_points,
            advances,
            rows,
        }
    }

    fn glyph(&self, ch: char) -> Option<(u8, &[u16])> {
        let i = self.code_points.binary_search(&(ch as u32)).ok()?;
        Some((self.advances[i], &self.rows[i * self.cell..(i + 1) * self.cell]))
    }
}

/// Each pixel font's own ASCII glyphs (the default font uses IBM VGA ones).
static ASCII_TABLE_DATA: [(usize, &[u8]); 3] = [
    (10, include_bytes!("../../../assets/fonts/ascii-fusion-10.lbg")),
    (12, include_bytes!("../../../assets/fonts/ascii-fusion-12.lbg")),
    (16, include_bytes!("../../../assets/fonts/ascii-unifont-16.lbg")),
];

fn ascii_table(cell: usize) -> &'static Table {
    static TABLES: [OnceLock<Table>; 3] = [const { OnceLock::new() }; 3];
    let i = ASCII_TABLE_DATA
        .iter()
        .position(|&(c, _)| c == cell)
        .expect("no such font table");
    TABLES[i].get_or_init(|| Table::parse(ASCII_TABLE_DATA[i].1))
}

/// A glyph from a table's `rows` (`cell` x `cell`), placed in the middle of
/// a `slot` rows tall, turned sideways with `vertical`.
fn cell_glyph(advance: u8, rows: &[u16], cell: usize, slot: usize, vertical: bool) -> Glyph {
    let rows: Vec<u16> = if vertical {
        rotate_ccw(rows, cell)
    } else {
        rows.to_vec()
    };
    let width = if vertical {
        cell
    } else {
        (advance as usize).clamp(1, cell)
    };
    let mut pixels = vec![false; width * cell];
    for (y, row) in rows.iter().enumerate() {
        for x in 0..width {
            pixels[y * width + x] = row & (0x8000 >> x) != 0;
        }
    }
    Glyph {
        width,
        height: cell,
        left: 0,
        top: ((slot - cell) / 2) as i64,
        advance: width,
        pixels,
    }
}

fn table(cell: usize, script: Script) -> &'static Table {
    static TABLES: [OnceLock<Table>; 12] = [const { OnceLock::new() }; 12];
    let i = TABLE_DATA
        .iter()
        .position(|&(c, s, _)| c == cell && s == script)
        .expect("no such font table");
    TABLES[i].get_or_init(|| Table::parse(TABLE_DATA[i].2))
}

/// Embedded font cell size used for a badge-height glyph slot.
fn embedded_cell(slot: usize) -> usize {
    match slot {
        11 => 10,
        12 => 12,
        _ => 16,
    }
}

/// Rotates an n x n glyph 90 degrees counterclockwise (the vendor's
/// vertical "Fall" glyphs are rotated this way).
fn rotate_ccw(rows: &[u16], n: usize) -> Vec<u16> {
    let bit = |x: usize, y: usize| rows[y] & (0x8000 >> x) != 0;
    (0..n)
        .map(|y| (0..n).fold(0u16, |acc, x| if bit(n - 1 - y, x) { acc | (0x8000 >> x) } else { acc }))
        .collect()
}

/// The badge font. Cheap to create; tables are decoded on first use.
pub struct FontLibrary {
    /// Folder with the vendor's own libraries, if the user chose to use them.
    vendor_dir: Option<PathBuf>,
    libs: HashMap<String, Option<Vec<u8>>>,
}

impl FontLibrary {
    /// Embedded fonts, or the vendor libraries if `LED_BADGE_STUDIO_FONTLIB` points
    /// at a folder containing them.
    pub fn discover() -> Self {
        let dir = std::env::var_os("LED_BADGE_STUDIO_FONTLIB")
            .map(PathBuf::from)
            .filter(|d| find_case_insensitive(d, "HZK12").is_some())
            .map(|d| std::fs::canonicalize(&d).unwrap_or(d));
        FontLibrary::with_vendor_dir(dir)
    }

    /// Embedded fonts only.
    pub fn embedded() -> Self {
        FontLibrary::with_vendor_dir(None)
    }

    pub fn with_vendor_dir(vendor_dir: Option<PathBuf>) -> Self {
        FontLibrary {
            vendor_dir,
            libs: HashMap::new(),
        }
    }

    pub fn vendor_dir(&self) -> Option<&Path> {
        self.vendor_dir.as_deref()
    }

    /// The font's name for a badge `height` rows tall, after its sources.
    pub fn display_name(&self, height: usize) -> String {
        let rest = if self.vendor_dir.is_some() {
            "vendor fonts"
        } else if glyph_size(height) >= 16 {
            "Unifont"
        } else {
            "Fusion Pixel"
        };
        format!("IBM VGA + {rest}")
    }

    pub fn description(&self) -> String {
        match &self.vendor_dir {
            Some(d) => format!("vendor libraries from {}", d.display()),
            None => "built in (ASCII derived from the IBM VGA fonts; Chinese, Japanese and Korean derived from \
                     Fusion Pixel Font and GNU Unifont, SIL OFL 1.1)"
                .into(),
        }
    }

    fn vendor_lib(&mut self, name: &str) -> Option<&[u8]> {
        let dir = self.vendor_dir.clone()?;
        self.libs
            .entry(name.to_string())
            .or_insert_with(|| std::fs::read(find_case_insensitive(&dir, name)?).ok())
            .as_deref()
    }

    /// Glyph for `ch` on a badge `height` rows tall, or `None` when the font
    /// doesn't cover it (the renderer then falls back to a computer font).
    pub fn glyph(&mut self, ch: char, height: usize, vertical: bool) -> Option<Glyph> {
        let slot = glyph_size(height);
        if (ch as u32) < 128 {
            return Some(ascii_glyph(ch as u8, slot));
        }
        let classified = classify(ch);
        if self.vendor_dir.is_some()
            && let Some((script, code)) = classified
            && let Some(g) = self.vendor_glyph(script, code, slot, vertical)
        {
            return Some(g);
        }
        let script = classified.map_or(Script::ZhHans, |(s, _)| s);
        let cell = embedded_cell(slot);
        let (advance, rows) = table(cell, script)
            .glyph(ch)
            .or_else(|| table(cell, Script::ZhHans).glyph(ch))?;
        Some(cell_glyph(advance, rows, cell, slot, vertical))
    }

    /// Glyph for `ch` in one of the other built-in pixel fonts; `None` when it
    /// lacks `ch` or doesn't fit the badge. Only non-ASCII characters turn
    /// with `vertical`, as in the default font.
    pub fn pixel_glyph(&self, font: PixelFont, ch: char, height: usize, vertical: bool) -> Option<Glyph> {
        if !font.fits(height) {
            return None;
        }
        let slot = glyph_size(height);
        let cell = match font {
            PixelFont::FusionPixel if slot <= 11 => 10,
            PixelFont::FusionPixel => 12,
            PixelFont::Unifont => 16,
        };
        if (ch as u32) < 128 {
            let (advance, rows) = ascii_table(cell).glyph(ch)?;
            return Some(cell_glyph(advance, rows, cell, slot, false));
        }
        let script = classify(ch).map_or(Script::ZhHans, |(s, _)| s);
        let (advance, rows) = table(cell, script)
            .glyph(ch)
            .or_else(|| table(cell, Script::ZhHans).glyph(ch))?;
        Some(cell_glyph(advance, rows, cell, slot, vertical))
    }

    fn vendor_glyph(&mut self, script: Script, code: [u8; 2], size: usize, vertical: bool) -> Option<Glyph> {
        let index = (code[0] as usize).checked_sub(0x81)? * 191 + (code[1] as usize).checked_sub(0x40)?;
        let name = format!("{}{}{}", script.vendor_prefix(), size, if vertical { "V" } else { "" });
        let stride = size * 2;
        let bytes = self.vendor_lib(&name)?.get(index * stride..(index + 1) * stride)?;
        let mut pixels = vec![false; size * size];
        for y in 0..size {
            let row = u16::from_be_bytes([bytes[y * 2], bytes[y * 2 + 1]]);
            for x in 0..size {
                pixels[y * size + x] = row & (0x8000 >> x) != 0;
            }
        }
        Some(Glyph {
            width: size,
            height: size,
            left: 0,
            top: 0,
            advance: size,
            pixels,
        })
    }
}

/// Glyph slot height used for a badge height (11, 12 or 16), as in the original.
fn glyph_size(height: usize) -> usize {
    match height {
        0..=11 => 11,
        12..=15 => 12,
        _ => 16,
    }
}

fn ascii_glyph(code: u8, size: usize) -> Glyph {
    let table = match size {
        11 => ASCII_11,
        12 => ASCII_12,
        _ => ASCII_16,
    };
    let rows = &table[code as usize * size..(code as usize + 1) * size];
    let mut pixels = vec![false; 8 * size];
    for (y, row) in rows.iter().enumerate() {
        for x in 0..8 {
            pixels[y * 8 + x] = row & (0x80 >> x) != 0;
        }
    }
    Glyph {
        width: 8,
        height: size,
        left: 0,
        top: 0,
        advance: 8,
        pixels,
    }
}

/// Picks the regional style for a non-ASCII character (the same order the
/// original software used) and returns its two-byte code in that encoding.
fn classify(ch: char) -> Option<(Script, [u8; 2])> {
    let cp = ch as u32;
    let order: &[Script] = if (0x3040..=0x30FF).contains(&cp) || (0x31F0..=0x31FF).contains(&cp) {
        &[Script::Ja, Script::ZhHans]
    } else if (0xAC00..=0xD7AF).contains(&cp) || (0x3130..=0x318F).contains(&cp) {
        &[Script::Ko]
    } else {
        &Script::ALL
    };
    let mut buf = [0u8; 4];
    let s = ch.encode_utf8(&mut buf);
    order.iter().find_map(|&script| {
        let (bytes, _, had_errors) = script.encoding().encode(s);
        if !had_errors && bytes.len() == 2 && bytes[0] >= 0x81 && bytes[1] >= 0x40 {
            Some((script, [bytes[0], bytes[1]]))
        } else {
            None
        }
    })
}

fn find_case_insensitive(dir: &Path, name: &str) -> Option<PathBuf> {
    let exact = dir.join(name);
    if exact.is_file() {
        return Some(exact);
    }
    std::fs::read_dir(dir)
        .ok()?
        .flatten()
        .find(|e| e.file_name().to_string_lossy().eq_ignore_ascii_case(name))
        .map(|e| e.path())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn art(g: &Glyph) -> Vec<String> {
        (0..g.height)
            .map(|y| (0..g.width).map(|x| if g.get(x, y) { '#' } else { '.' }).collect())
            .collect()
    }

    #[test]
    fn ascii_is_ibm_vga_style() {
        let g = FontLibrary::embedded().glyph('A', 12, false).unwrap();
        assert_eq!(art(&g)[1], "..###...");
        assert_eq!(art(&g)[5], "#######.");
    }

    #[test]
    fn every_badge_height_has_cjk() {
        let mut f = FontLibrary::embedded();
        for height in [11, 12, 16] {
            for ch in ['中', '國', 'あ', 'ア', '한', '永'] {
                let g = f
                    .glyph(ch, height, false)
                    .unwrap_or_else(|| panic!("{ch} missing at {height}"));
                assert!(g.pixels.iter().any(|&p| p), "{ch} blank at {height}");
                assert!(g.top as usize + g.height <= height, "{ch} too tall at {height}");
            }
        }
    }

    #[test]
    fn common_chinese_is_covered_at_every_size() {
        // The 3,500 most common Simplified Chinese characters are in the
        // GB2312 level-1 block (0xB0A1..0xD7F9).
        let mut f = FontLibrary::embedded();
        for height in [11, 12, 16] {
            let mut missing = 0;
            for b1 in 0xB0u8..=0xD7 {
                for b2 in 0xA1u8..=0xFE {
                    let bytes = [b1, b2];
                    let (s, _, bad) = GBK.decode(&bytes);
                    let ch = s.chars().next().unwrap();
                    if !bad && f.glyph(ch, height, false).is_none() {
                        missing += 1;
                    }
                }
            }
            assert!(missing < 10, "{missing} common characters missing at {height} rows");
        }
    }

    #[test]
    fn vertical_glyphs_are_rotated() {
        let mut f = FontLibrary::embedded();
        let n = f.glyph('中', 12, false).unwrap();
        let v = f.glyph('中', 12, true).unwrap();
        let cell = v.height;
        // Rotating four times gives the original back.
        let rows: Vec<u16> = (0..cell)
            .map(|y| (0..n.width).fold(0u16, |a, x| if n.get(x, y) { a | (0x8000 >> x) } else { a }))
            .collect();
        let mut r = rows.clone();
        for _ in 0..4 {
            r = rotate_ccw(&r, cell);
        }
        assert_eq!(r, rows);
        assert_ne!(rotate_ccw(&rows, cell), rows);
    }

    #[test]
    fn the_built_in_font_is_named_after_its_sources() {
        let f = FontLibrary::embedded();
        assert_eq!(f.display_name(11), "IBM VGA + Fusion Pixel");
        assert_eq!(f.display_name(12), "IBM VGA + Fusion Pixel");
        assert_eq!(f.display_name(16), "IBM VGA + Unifont");
    }

    #[test]
    fn fusion_pixel_and_unifont_work_on_their_own() {
        let f = FontLibrary::embedded();
        for (font, height, cell) in [
            (PixelFont::FusionPixel, 11, 10),
            (PixelFont::FusionPixel, 12, 12),
            (PixelFont::FusionPixel, 16, 12),
            (PixelFont::Unifont, 16, 16),
        ] {
            // Their own letters, a space with a width, and the rest.
            for ch in ['A', 'g', '1', '!', ' ', 'é', '中', 'あ', '한'] {
                let g = f
                    .pixel_glyph(font, ch, height, false)
                    .unwrap_or_else(|| panic!("{font:?} {height}: {ch}"));
                assert_eq!(g.height, cell, "{font:?} {height}: {ch}");
                assert!(g.advance > 0);
                assert_eq!(g.pixels.iter().any(|p| *p), ch != ' ', "{font:?} {height}: {ch:?}");
            }
        }
        // The letters differ from the default font's IBM VGA ones.
        let mut def = FontLibrary::embedded();
        assert_ne!(
            f.pixel_glyph(PixelFont::FusionPixel, 'A', 12, false).unwrap().pixels,
            def.glyph('A', 12, false).unwrap().pixels
        );
        // Unifont is 16 px: not on smaller badges.
        assert!(f.pixel_glyph(PixelFont::Unifont, 'A', 11, false).is_none());
    }
}
