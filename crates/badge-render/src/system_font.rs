//! System TrueType/OpenType fonts rasterized to 1-bit glyphs ("Manual" font).

use std::collections::HashMap;
use std::sync::Arc;

use fontdb::{Database, Family, ID, Query, Stretch, Style, Weight};
use swash::FontRef;
use swash::scale::{Render, ScaleContext, Source};
use swash::zeno::{Angle, Format, Transform};

use crate::Glyph;

/// Families that are tried, in order, when a character is missing.
const FALLBACK_FAMILIES: &[&str] = &[
    "DejaVu Sans",
    "Noto Sans",
    "Liberation Sans",
    "Arial",
    "Segoe UI",
    "Noto Sans CJK SC",
    "Microsoft YaHei",
];

/// TrueType/OpenType fonts compiled into the app, listed under Built-in.
/// None yet: add `include_bytes!("../../../assets/fonts/app/<file>")` here,
/// with its license in `assets/fonts/licenses/` and `licenses.rs`.
pub const APP_FONTS: &[&[u8]] = &[];

pub struct SystemFonts {
    db: Database,
    /// The computer's own font families, sorted.
    families: Vec<String>,
    /// The families of [`APP_FONTS`], sorted.
    app_families: Vec<String>,
    ctx: ScaleContext,
    fallback: HashMap<char, Option<ID>>,
    /// Font files already used, kept in memory: rereading them per glyph is
    /// slow, and a deleted font keeps working until the list is refreshed.
    loaded: HashMap<ID, Option<FontData>>,
}

/// A font file's bytes and the face's index in it.
type FontData = (Arc<Vec<u8>>, u32);

impl SystemFonts {
    pub fn load() -> Self {
        let mut db = Database::new();
        db.load_system_fonts();
        let app_ids: Vec<ID> = APP_FONTS
            .iter()
            .flat_map(|data| db.load_font_source(fontdb::Source::Binary(Arc::new(data.to_vec()))))
            .collect();
        Self::from_db(db, &app_ids)
    }

    /// `app_ids`: the faces that came with the app (their own group).
    fn from_db(db: Database, app_ids: &[ID]) -> Self {
        let sorted = |mut v: Vec<String>| {
            v.sort_by_key(|s| s.to_lowercase());
            v.dedup();
            v
        };
        let family = |f: &fontdb::FaceInfo| f.families.first().map(|(n, _)| n.clone());
        let app_families = sorted(
            app_ids
                .iter()
                .filter_map(|id| db.face(*id))
                .filter_map(family)
                .collect(),
        );
        let families = sorted(
            db.faces()
                .filter(|f| !app_ids.contains(&f.id))
                .filter_map(family)
                .filter(|n| !app_families.contains(n))
                .collect(),
        );
        SystemFonts {
            db,
            families,
            app_families,
            ctx: ScaleContext::new(),
            fallback: HashMap::new(),
            loaded: HashMap::new(),
        }
    }

    pub fn families(&self) -> &[String] {
        &self.families
    }

    pub fn app_families(&self) -> &[String] {
        &self.app_families
    }

    /// Whether `family` is installed and its file still exists.
    pub fn is_installed(&self, family: &str) -> bool {
        let Some(id) = self.db.query(&Query {
            families: &[Family::Name(family)],
            ..Query::default()
        }) else {
            return false;
        };
        match self.db.face(id).map(|f| &f.source) {
            Some(fontdb::Source::File(path)) => path.exists(),
            Some(_) => true,
            None => false,
        }
    }

    /// Changes whenever fonts are installed or removed.
    pub fn fingerprint(&self) -> u64 {
        use std::hash::{Hash, Hasher};
        let mut paths: Vec<String> = self
            .db
            .faces()
            .map(|f| match &f.source {
                fontdb::Source::File(p) => p.to_string_lossy().into_owned(),
                _ => f.families.first().map(|(n, _)| n.clone()).unwrap_or_default(),
            })
            .collect();
        paths.sort();
        let mut h = std::collections::hash_map::DefaultHasher::new();
        paths.hash(&mut h);
        h.finish()
    }

    /// A reasonable default family for new messages.
    pub fn default_family(&self) -> String {
        FALLBACK_FAMILIES
            .iter()
            .find(|f| self.families.iter().any(|x| x == *f))
            .map(std::string::ToString::to_string)
            .or_else(|| self.families.first().cloned())
            .unwrap_or_else(|| "Sans".into())
    }

    pub fn glyph(&mut self, family: &str, size_px: u32, bold: bool, italic: bool, ch: char) -> Option<Glyph> {
        let id = self.db.query(&Query {
            families: &[Family::Name(family)],
            weight: if bold { Weight::BOLD } else { Weight::NORMAL },
            stretch: Stretch::Normal,
            style: if italic { Style::Italic } else { Style::Normal },
        })?;
        let face = self.db.face(id)?;
        let fake_bold = bold && face.weight.0 < 600;
        let fake_italic = italic && face.style == Style::Normal;
        self.render(id, size_px, fake_bold, fake_italic, ch)
    }

    /// Glyph for `ch` from any installed font that has it, sized for a
    /// line `size_px` tall.
    pub fn fallback_glyph(&mut self, ch: char, size_px: usize) -> Option<Glyph> {
        let id = match self.fallback.get(&ch) {
            Some(id) => *id,
            None => {
                let id = self.find_face_with(ch);
                self.fallback.insert(ch, id);
                id
            }
        }?;
        self.render(id, size_px as u32, false, false, ch)
    }

    fn find_face_with(&self, ch: char) -> Option<ID> {
        let has = |id: ID| {
            self.db
                .with_face_data(id, |data, index| {
                    FontRef::from_index(data, index as usize).is_some_and(|f| f.charmap().map(ch) != 0)
                })
                .unwrap_or(false)
        };
        let preferred = FALLBACK_FAMILIES.iter().filter_map(|name| {
            self.db.query(&Query {
                families: &[Family::Name(name)],
                ..Query::default()
            })
        });
        let generic = self.db.query(&Query {
            families: &[Family::SansSerif],
            ..Query::default()
        });
        preferred.chain(generic).find(|&id| has(id)).or_else(|| {
            self.db
                .faces()
                .filter(|f| f.style == Style::Normal && f.weight == Weight::NORMAL)
                .map(|f| f.id)
                .find(|&id| has(id))
        })
    }

    /// The font file behind face `id` and the face's index in it, read once.
    fn face_data(&mut self, id: ID) -> Option<FontData> {
        let db = &self.db;
        self.loaded
            .entry(id)
            .or_insert_with(|| db.with_face_data(id, |data, index| (Arc::new(data.to_vec()), index)))
            .clone()
    }

    fn render(&mut self, id: ID, size_px: u32, fake_bold: bool, fake_italic: bool, ch: char) -> Option<Glyph> {
        let (data, index) = self.face_data(id)?;
        let ctx = &mut self.ctx;
        Some(&*data).and_then(|data| {
            let font = FontRef::from_index(data, index as usize)?;
            let gid = font.charmap().map(ch);
            if gid == 0 && !ch.is_whitespace() {
                return None;
            }
            let size = size_px.max(1) as f32;
            let metrics = font.metrics(&[]).scale(size);
            let advance = font.glyph_metrics(&[]).scale(size).advance_width(gid).round().max(0.0) as usize;
            // Center the line box (ascent + descent) on the em box so that
            // a font sized to the badge height fills it.
            let line = metrics.ascent + metrics.descent;
            let baseline = ((size - line) / 2.0 + metrics.ascent).round() as i64;

            let mut scaler = ctx.builder(font).size(size).hint(true).build();
            let mut render = Render::new(&[Source::Outline]);
            render.format(Format::Alpha);
            if fake_bold {
                render.embolden((size / 24.0).max(0.6));
            }
            if fake_italic {
                render.transform(Some(Transform::skew(Angle::from_degrees(12.0), Angle::ZERO)));
            }
            let img = render.render(&mut scaler, gid);
            let (width, height, left, top, pixels) = match img {
                Some(img) if img.placement.width > 0 && img.placement.height > 0 => {
                    let p = img.placement;
                    let pixels = img.data.iter().map(|&a| a >= 128).collect();
                    (
                        p.width as usize,
                        p.height as usize,
                        p.left as i64,
                        baseline - p.top as i64,
                        pixels,
                    )
                }
                _ => (0, 0, 0, 0, Vec::new()),
            };
            Some(Glyph {
                width,
                height,
                left,
                top,
                advance: advance + usize::from(fake_bold),
                pixels,
            })
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_deleted_font_is_reported_missing() {
        // Any installed font file, copied so it can be deleted.
        let system = SystemFonts::load();
        let Some((path, family)) = system.db.faces().find_map(|f| match &f.source {
            fontdb::Source::File(p) => Some((p.clone(), f.families.first()?.0.clone())),
            _ => None,
        }) else {
            return; // no fonts installed
        };
        let dir = std::env::temp_dir().join(format!("led-badge-font-test-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let copy = dir.join(path.file_name().unwrap());
        std::fs::copy(&path, &copy).unwrap();

        let mut db = Database::new();
        db.load_font_file(&copy).unwrap();
        let mut fonts = SystemFonts::from_db(db, &[]);
        assert!(fonts.is_installed(&family));
        let drawn = fonts.glyph(&family, 12, false, false, 'A').is_some();
        assert!(!fonts.is_installed("No Such Font Family"));
        let before = fonts.fingerprint();

        std::fs::remove_dir_all(&dir).unwrap();
        assert!(!fonts.is_installed(&family), "deleted after loading");
        // A font already drawn with stays in memory and keeps working.
        if drawn {
            assert!(fonts.glyph(&family, 12, false, false, 'A').is_some());
        }
        assert_ne!(SystemFonts::from_db(Database::new(), &[]).fingerprint(), before);
    }

    #[test]
    fn app_fonts_are_their_own_group() {
        // Any font on this computer, loaded as if it came with the app.
        let mut system = Database::new();
        system.load_system_fonts();
        let Some(path) = system.faces().find_map(|f| match &f.source {
            fontdb::Source::File(p) => Some(p.clone()),
            _ => None,
        }) else {
            return; // no fonts to test with
        };
        let mut db = Database::new();
        let ids = db.load_font_source(fontdb::Source::Binary(Arc::new(std::fs::read(path).unwrap())));
        let fonts = SystemFonts::from_db(db, &ids);
        assert!(!fonts.app_families().is_empty());
        assert!(fonts.families().is_empty(), "app fonts aren't listed as the computer's");
        assert!(fonts.is_installed(&fonts.app_families()[0]));
    }
}
