//! Project files: the native `.badge` format (JSON).
//!
//! Loading is forgiving but never silent:
//! - a value of the right kind but outside its range (speed 9, a 300 px
//!   font) is **repaired**, and every repair is reported ([`Loaded::fixes`]);
//! - anything without one sensible reading (broken JSON, text where a number
//!   belongs, an unknown mode or badge type, a newer file version) is
//!   **refused** with an error saying where the problem is.

pub mod dirs;
pub mod last_sent;

use std::io::Write;
use std::path::Path;

use anyhow::{Context, Result};
use badge_core::{CardType, FONT_SIZE_RANGE, LETTER_SPACING_RANGE, MESSAGE_COUNT, Project, TOP_OFFSET_RANGE};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

pub const PROJECT_EXT: &str = "badge";

const FORMAT_ID: &str = "led-badge-project";
const FORMAT_VERSION: u32 = 1;

#[derive(Serialize, Deserialize)]
struct ProjectFile {
    format: String,
    version: u32,
    project: Project,
}

/// A project read from a file, with what had to be repaired in it.
#[derive(Debug)]
pub struct Loaded {
    pub project: Project,
    /// Every repair made; empty when the file was fine.
    pub fixes: Vec<Fix>,
}

pub fn project_to_json(project: &Project) -> Result<String> {
    let mut project = project.clone();
    // Only mono badges have one LED color.
    if project.card.is_color() {
        project.led_color = None;
    }
    Ok(serde_json::to_string_pretty(&ProjectFile {
        format: FORMAT_ID.into(),
        version: FORMAT_VERSION,
        project,
    })?)
}

/// Why a project file can't be opened, in parts so the app can lay them out.
/// `Display` gives one line for the command line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileError {
    /// What is wrong ("the file ends too early").
    pub problem: String,
    /// Where in the file ("line 10, column 68", "message 2, \"text\"").
    pub location: Option<String>,
    /// The likely cause.
    pub cause: Option<String>,
    /// What to do about it.
    pub advice: Option<String>,
}

impl FileError {
    fn new(problem: impl Into<String>) -> Self {
        FileError {
            problem: problem.into(),
            location: None,
            cause: None,
            advice: None,
        }
    }

    fn at(mut self, location: impl Into<String>) -> Self {
        self.location = Some(location.into());
        self
    }

    fn cause(mut self, cause: impl Into<String>) -> Self {
        self.cause = Some(cause.into());
        self
    }

    fn advice(mut self, advice: impl Into<String>) -> Self {
        self.advice = Some(advice.into());
        self
    }
}

impl std::fmt::Display for FileError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.problem)?;
        if let Some(at) = &self.location {
            write!(f, ", at {at}")?;
        }
        for extra in [&self.cause, &self.advice].into_iter().flatten() {
            write!(f, "; {extra}")?;
        }
        Ok(())
    }
}

impl std::error::Error for FileError {}

/// serde_json's message without its own " at line L column C" suffix.
fn without_position(e: &serde_json::Error) -> String {
    let text = e.to_string();
    match text.rfind(" at line ") {
        Some(i) => text[..i].to_string(),
        None => text,
    }
}

/// Reads a project, repairing out-of-range values (see the module docs).
/// Errors are [`FileError`]s.
pub fn project_from_json_checked(s: &str) -> Result<Loaded> {
    let mut value: Value = serde_json::from_str(s).map_err(|e| {
        use serde_json::error::Category;
        let at = format!("line {}, column {}", e.line(), e.column());
        match e.classify() {
            Category::Eof if s.trim().is_empty() => FileError::new("the file is empty"),
            Category::Eof => FileError::new("the file ends too early")
                .at(at)
                .cause("it may have been cut off while it was being saved"),
            _ => FileError::new(format!("the text isn't valid JSON ({})", without_position(&e))).at(at),
        }
    })?;
    match value.get("format") {
        Some(Value::String(f)) if f == FORMAT_ID => {}
        _ => {
            return Err(FileError::new("this isn't a LED Badge Studio project file")
                .cause(format!("its \"format\" isn't \"{FORMAT_ID}\""))
                .into());
        }
    }
    if let Some(v) = value.get("version").and_then(Value::as_u64)
        && v > u64::from(FORMAT_VERSION)
    {
        return Err(
            FileError::new(format!("it was saved by a newer version of the app (file version {v})"))
                .advice("update LED Badge Studio to open it")
                .into(),
        );
    }
    let mut fixes = Vec::new();
    repair(&mut value, &mut fixes);
    let file: ProjectFile = serde_path_to_error::deserialize(value).map_err(|e| {
        FileError::new(format!("a setting has an unusable value ({})", e.inner()))
            .at(describe_path(&e.path().to_string()))
    })?;
    let mut project = file.project;
    project.normalize();
    if project.card.is_color() {
        project.led_color = None;
    }
    Ok(Loaded { project, fixes })
}

/// Reads a project; repairs are made but not reported.
pub fn project_from_json(s: &str) -> Result<Project> {
    project_from_json_checked(s).map(|l| l.project)
}

pub fn save_project(project: &Project, path: &Path) -> Result<()> {
    let json = project_to_json(project)?;
    let tmp = path.with_extension("badge.tmp");
    // On the disk before it replaces the file, so a power cut right after
    // can't leave an empty file in its place.
    let write = || -> std::io::Result<()> {
        let mut file = std::fs::File::create(&tmp)?;
        file.write_all(json.as_bytes())?;
        file.sync_all()
    };
    write().with_context(|| format!("writing {}", tmp.display()))?;
    std::fs::rename(&tmp, path).with_context(|| format!("writing {}", path.display()))?;
    Ok(())
}

/// Opens a `.badge` project file, reporting repairs.
pub fn open_checked(path: &Path) -> Result<Loaded> {
    let bytes = std::fs::read(path).with_context(|| format!("reading {}", path.display()))?;
    let text = String::from_utf8_lossy(&bytes);
    // Tolerate a UTF-8 byte order mark added by some editors.
    project_from_json_checked(text.trim_start_matches('\u{feff}'))
}

/// Opens a `.badge` project file; repairs are made but not reported.
pub fn open(path: &Path) -> Result<Project> {
    open_checked(path).map(|l| l.project)
}

/// "project.messages[1].font" → "message 2, font"; "." → "the file".
fn describe_path(path: &str) -> String {
    let path = path.strip_prefix("project.").unwrap_or(path);
    if let Some(rest) = path.strip_prefix("messages[")
        && let Some((n, tail)) = rest.split_once(']')
        && let Ok(n) = n.parse::<usize>()
    {
        let tail = tail.trim_start_matches('.');
        return if tail.is_empty() {
            format!("message {}", n + 1)
        } else {
            format!("message {}, setting \"{tail}\"", n + 1)
        };
    }
    if path == "." || path.is_empty() {
        "the file".into()
    } else {
        format!("setting \"{path}\"")
    }
}

// Repairs are made on the raw JSON, before it's read into a `Project`.

/// One value that was outside what the badge supports, and what it became.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Fix {
    /// Message number (1-8), or `None` for the project as a whole.
    pub message: Option<usize>,
    /// The setting, as the app names it ("Speed").
    pub setting: String,
    /// The value in the file ("9").
    pub before: String,
    /// What it becomes ("8").
    pub after: String,
    /// Why ("allowed: 1 to 8").
    pub reason: String,
}

impl Fix {
    /// "Message 2" or "Project".
    pub fn place(&self) -> String {
        self.message
            .map_or_else(|| "Project".into(), |n| format!("Message {n}"))
    }
}

impl std::fmt::Display for Fix {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "{}: {}: {} → {} ({})",
            self.place().to_lowercase(),
            self.setting,
            self.before,
            self.after,
            self.reason
        )
    }
}

/// Shows a JSON number the way it was written (9, 2.5, -1).
fn num(x: f64) -> String {
    format!("{x}")
}

fn color_name(index: u8) -> String {
    badge_core::LedColor::new(index).name().to_lowercase()
}

/// Collects the repairs for one message.
struct Fixes<'a> {
    message: Option<usize>,
    out: &'a mut Vec<Fix>,
}

impl Fixes<'_> {
    fn add(&mut self, setting: &str, before: String, after: String, reason: String) {
        self.out.push(Fix {
            message: self.message,
            setting: setting.into(),
            before,
            after,
            reason,
        });
    }

    /// Rounds and clamps a number field into `lo..=hi`.
    fn clamp(&mut self, obj: &mut Map<String, Value>, key: &str, setting: &str, (lo, hi): (i64, i64)) {
        let Some(v) = obj.get_mut(key) else { return };
        let Some(x) = v.as_f64() else { return }; // refused when read
        let fixed = (x.round() as i64).clamp(lo, hi);
        if x != fixed as f64 {
            self.add(setting, num(x), fixed.to_string(), format!("allowed: {lo} to {hi}"));
            *v = fixed.into();
        }
    }
}

fn is_color(v: &Value) -> bool {
    v.as_f64().is_some_and(|x| x.fract() == 0.0 && (0.0..=7.0).contains(&x))
}

fn is_lit_color(v: &Value) -> bool {
    v.as_f64().is_some_and(|x| x.fract() == 0.0 && (1.0..=7.0).contains(&x))
}

/// Red: what invalid colors become (also the default of new messages).
const DEFAULT_COLOR: u8 = 4;

fn repair(root: &mut Value, out: &mut Vec<Fix>) {
    let card = root
        .pointer("/project/card")
        .and_then(|c| serde_json::from_value::<CardType>(c.clone()).ok())
        .unwrap_or_default();
    let card_height = card.height();
    // The LED color of a mono badge: 1-7 (an LED color, not off). Color
    // badges don't use one, so it's ignored there whatever it holds.
    if let Some(project) = root.get_mut("project").and_then(Value::as_object_mut) {
        if card.is_color() {
            project.remove("led_color");
        } else if let Some(v) = project
            .get_mut("led_color")
            .filter(|v| v.is_number() && !is_lit_color(v))
        {
            let before = if v.as_f64() == Some(0.0) {
                "off".to_string()
            } else {
                v.to_string()
            };
            Fixes {
                message: None,
                out: &mut *out,
            }
            .add(
                "LED color",
                before,
                color_name(DEFAULT_COLOR),
                "LED colors are 1 to 7".into(),
            );
            *v = DEFAULT_COLOR.into();
        }
    }
    let Some(msgs) = root.pointer_mut("/project/messages").and_then(Value::as_array_mut) else {
        return;
    };
    if msgs.len() > MESSAGE_COUNT {
        Fixes {
            message: None,
            out: &mut *out,
        }
        .add(
            "Messages",
            msgs.len().to_string(),
            format!("{MESSAGE_COUNT} (the first {MESSAGE_COUNT} are kept)"),
            format!("a badge holds {MESSAGE_COUNT}"),
        );
        msgs.truncate(MESSAGE_COUNT);
    }
    for (i, m) in msgs.iter_mut().enumerate() {
        let Some(m) = m.as_object_mut() else { continue };
        let mut fx = Fixes {
            message: Some(i + 1),
            out: &mut *out,
        };
        let range = |r: (i32, i32)| (i64::from(r.0), i64::from(r.1));
        fx.clamp(m, "speed", "Speed", (1, 8));
        fx.clamp(m, "top_offset", "Shift up", range(TOP_OFFSET_RANGE));
        fx.clamp(m, "letter_spacing", "Spacing", range(LETTER_SPACING_RANGE));
        if let Some(sys) = m
            .get_mut("font")
            .and_then(|f| f.get_mut("System"))
            .and_then(Value::as_object_mut)
        {
            let r = (i64::from(FONT_SIZE_RANGE.0), i64::from(FONT_SIZE_RANGE.1));
            fx.clamp(sys, "size_px", "Font size", r);
        }

        // Colors: 0-7 (off, blue, green, cyan, red, magenta, yellow, white).
        // Anything else becomes red.
        if let Some(v) = m.get_mut("default_color").filter(|v| v.is_number() && !is_color(v)) {
            fx.add(
                "Default text color",
                v.to_string(),
                color_name(DEFAULT_COLOR),
                "colors are 0 to 7".into(),
            );
            *v = DEFAULT_COLOR.into();
        }
        if let Some(colors) = m.get_mut("colors").and_then(Value::as_array_mut) {
            let mut bad = Vec::new();
            for c in colors.iter_mut().filter(|c| c.is_number() && !is_color(c)) {
                bad.push(c.to_string());
                *c = DEFAULT_COLOR.into();
            }
            if !bad.is_empty() {
                let shown = if bad.len() > 5 {
                    format!("{}, … ({} letters)", bad[..5].join(", "), bad.len())
                } else {
                    bad.join(", ")
                };
                fx.add(
                    "Letter colors",
                    shown,
                    color_name(DEFAULT_COLOR),
                    "colors are 0 to 7".into(),
                );
            }
        }

        // Pixels: at most 16 rows of up to 4096 columns, digits 0-7.
        let drawing = m.get("source").and_then(Value::as_str) == Some("Bitmap");
        if let Some(bitmap) = m.get_mut("bitmap").and_then(Value::as_object_mut) {
            if let Some(h) = bitmap.get("height").and_then(Value::as_u64)
                && h > 16
            {
                fx.add(
                    "Drawing height",
                    format!("{h} rows"),
                    format!("{card_height} rows"),
                    "badges have at most 16 rows".into(),
                );
            }
            if let Some(rows) = bitmap.get("rows").and_then(Value::as_array) {
                let lit_rows = rows
                    .iter()
                    .filter_map(Value::as_str)
                    .enumerate()
                    .filter(|(_, r)| r.chars().any(|c| c != '0'))
                    .map(|(y, _)| y + 1)
                    .max()
                    .unwrap_or(0);
                if drawing && lit_rows > card_height {
                    fx.add(
                        "Drawing",
                        format!("uses {lit_rows} rows"),
                        format!("the top {card_height} rows (the rest is cut off)"),
                        format!("this badge type has {card_height} rows"),
                    );
                }
                let strs = || rows.iter().filter_map(Value::as_str);
                let widest = strs().map(|r| r.chars().count()).max().unwrap_or(0);
                if widest > badge_core::MAX_WIDTH {
                    fx.add(
                        "Drawing width",
                        format!("{widest} columns"),
                        format!("{} columns (the rest is cut off)", badge_core::MAX_WIDTH),
                        "the most a message can hold".into(),
                    );
                }
                let bad: usize = strs()
                    .map(|r| r.chars().filter(|c| !('0'..='7').contains(c)).count())
                    .sum();
                if bad > 0 {
                    fx.add(
                        "Pixels",
                        format!("{bad} with a value outside 0 to 7"),
                        "off".into(),
                        "pixel colors are 0 to 7".into(),
                    );
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use badge_core::{CardType, LedColor, Source};
    use std::assert_matches;

    #[test]
    fn json_roundtrip() {
        let mut p = Project::new(CardType::Rgb16);
        p.messages[3].bitmap.set(5, 5, LedColor::CYAN);
        p.messages[3].source = Source::Bitmap;
        p.messages[1].colors = vec![LedColor::GREEN];
        p.messages[2].font = badge_core::FontChoice::Pixel(badge_core::PixelFont::Unifont);
        let json = project_to_json(&p).unwrap();
        let back = project_from_json_checked(&json).unwrap();
        assert_eq!(back.project, p);
        assert!(back.fixes.is_empty(), "{:?}", back.fixes);
    }

    #[test]
    fn the_led_color_of_mono_badges_is_kept() {
        // Saved and read back for a mono badge.
        let mut p = Project::new(CardType::Mono11);
        p.led_color = Some(LedColor::BLUE);
        let json = project_to_json(&p).unwrap();
        assert!(json.contains("\"led_color\": 1"), "{json}");
        let back = project_from_json_checked(&json).unwrap();
        assert_eq!(back.project.led_color, Some(LedColor::BLUE));
        assert!(back.fixes.is_empty());

        // Not chosen: not written, and files without it still open.
        let p = Project::new(CardType::Mono11);
        let json = project_to_json(&p).unwrap();
        assert!(!json.contains("led_color"));
        assert_eq!(project_from_json(&json).unwrap().led_color, None);

        // Color badges have no single LED color: not written, ignored on
        // reading whatever it holds, and not reported as damage.
        let mut p = Project::new(CardType::Rgb12);
        p.led_color = Some(LedColor::BLUE);
        assert!(!project_to_json(&p).unwrap().contains("led_color"));
        let with = |card: &str, value: &str| {
            let base = project_to_json(&Project::new(CardType::Mono11)).unwrap();
            let mut v: Value = serde_json::from_str(&base).unwrap();
            v["project"]["card"] = serde_json::from_str(card).unwrap();
            v["project"]["led_color"] = serde_json::from_str(value).unwrap();
            v.to_string()
        };
        for value in ["3", "99", "\"blue\""] {
            let loaded = project_from_json_checked(&with("\"Rgb16\"", value)).unwrap();
            assert_eq!(loaded.project.led_color, None);
            assert!(loaded.fixes.is_empty(), "{value}: {:?}", loaded.fixes);
        }
    }

    #[test]
    fn an_invalid_led_color_becomes_red() {
        let with = |value: &str| {
            let base = project_to_json(&Project::new(CardType::Mono12)).unwrap();
            let mut v: Value = serde_json::from_str(&base).unwrap();
            v["project"]["led_color"] = serde_json::from_str(value).unwrap();
            v.to_string()
        };
        for (value, before) in [("0", "off"), ("9", "9"), ("-1", "-1"), ("2.5", "2.5")] {
            let loaded = project_from_json_checked(&with(value)).unwrap();
            assert_eq!(loaded.project.led_color, Some(LedColor::RED), "{value}");
            assert_eq!(loaded.fixes.len(), 1, "{value}");
            let fix = &loaded.fixes[0];
            assert_eq!((fix.message, fix.setting.as_str()), (None, "LED color"));
            assert_eq!((fix.before.as_str(), fix.after.as_str()), (before, "red"));
        }
        // Valid colors are kept; the wrong kind of value is refused.
        let loaded = project_from_json_checked(&with("7")).unwrap();
        assert_eq!(loaded.project.led_color, Some(LedColor::WHITE));
        assert!(loaded.fixes.is_empty());
        assert!(project_from_json_checked(&with("\"blue\"")).is_err());
        assert_eq!(project_from_json(&with("null")).unwrap().led_color, None);
    }

    #[test]
    fn damaged_values_are_repaired_and_reported() {
        let s = r#"{"format":"led-badge-project","version":1,"project":{"card":"Rgb12",
            "messages":[{"text":"Hi","colors":[200,-1,3],"default_color":250,"speed":300,
            "top_offset":-500,"letter_spacing":2.5,
            "font":{"System":{"family":"X","size_px":100000,"bold":false,"italic":false}},
            "source":"Bitmap",
            "bitmap":{"height":1000000000,"rows":["9871","0","0","0","0","0","0","0","0","0","0","0","0","1"]}}]}}"#;
        let l = project_from_json_checked(s).unwrap();
        let m = &l.project.messages[0];
        // Invalid colors become red.
        assert_eq!(m.colors.iter().map(|c| c.index()).collect::<Vec<_>>(), [4, 4, 3]);
        assert_eq!(m.default_color.index(), 4);
        assert_eq!(m.speed, 8);
        assert_eq!((m.top_offset, m.letter_spacing), (-8, 3));
        assert_matches!(m.font, badge_core::FontChoice::System { size_px: 64, .. });
        assert_eq!(m.bitmap.height(), 12);
        let all = l.fixes.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n");
        for expected in [
            "message 1: Speed: 300 → 8",
            "Shift up: -500 → -8",
            "Spacing: 2.5 → 3",
            "Font size: 100000 → 64",
            "Default text color: 250 → red",
            "Letter colors: 200, -1 → red",
            "Drawing height: 1000000000 rows → 12 rows",
            "Drawing: uses 14 rows",
            "Pixels: 2 with a value",
        ] {
            assert!(all.contains(expected), "missing {expected:?} in:\n{all}");
        }
        // Saving it again gives a clean file.
        let again = project_from_json_checked(&project_to_json(&l.project).unwrap()).unwrap();
        assert!(again.fixes.is_empty(), "{:?}", again.fixes);
    }

    #[test]
    fn extra_messages_are_reported() {
        let msgs = [r#"{"text":"a"}"#; 10].join(",");
        let s = format!(r#"{{"format":"led-badge-project","version":1,"project":{{"messages":[{msgs}]}}}}"#);
        let l = project_from_json_checked(&s).unwrap();
        assert_eq!(l.project.messages.len(), MESSAGE_COUNT);
        assert_eq!(l.fixes[0].message, None);
        assert_eq!(
            (l.fixes[0].before.as_str(), l.fixes[0].setting.as_str()),
            ("10", "Messages")
        );
    }

    #[test]
    fn unreadable_files_are_refused_with_the_place() {
        let err = |s: &str| format!("{:#}", project_from_json_checked(s).unwrap_err());
        assert!(err("{").contains("ends too early, at line 1"));
        assert!(err("").contains("empty"));
        let e = err("{]");
        assert!(
            e.contains("isn't valid JSON") && e.contains("at line 1, column 2"),
            "{e}"
        );
        assert!(err("{}").contains("isn't a LED Badge Studio project"));
        assert!(err(r#"{"format":"something-else","version":1,"project":{}}"#).contains("isn't a LED"));
        assert!(err(r#"{"format":"led-badge-project","version":9,"project":{}}"#).contains("newer"));
        let e = err(r#"{"format":"led-badge-project","version":1,"project":{"messages":[{},{"text":42}]}}"#);
        assert!(e.contains("message 2, setting \"text\""), "{e}");
        let e = err(r#"{"format":"led-badge-project","version":1,"project":{"messages":[{"mode":"Spin"}]}}"#);
        assert!(e.contains("message 1, setting \"mode\"") && e.contains("Spin"), "{e}");
        let e = err(r#"{"format":"led-badge-project","version":1,"project":{"card":"Mono99"}}"#);
        assert!(e.contains("setting \"card\""), "{e}");
        // The parts are available for laying out.
        let fe = project_from_json_checked("{\"a\": ").unwrap_err();
        let fe = fe.downcast_ref::<FileError>().unwrap();
        assert_eq!(fe.problem, "the file ends too early");
        assert!(fe.location.as_deref().unwrap().starts_with("line 1"));
        assert!(fe.cause.is_some());
    }
}
