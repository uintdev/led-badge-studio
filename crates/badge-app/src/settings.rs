//! User preferences persisted between runs.

use std::path::{Path, PathBuf};

use badge_core::{CardType, LedColor};
use serde::{Deserialize, Serialize};

/// The display widths the preview offers, in LEDs (the vendor app's range).
pub const PREVIEW_WIDTHS: (usize, usize) = (16, 132);

/// Light or dark colors.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Appearance {
    /// As the system is set; dark when the system doesn't say.
    #[default]
    System,
    Light,
    Dark,
}

impl Appearance {
    pub const ALL: [Appearance; 3] = [Appearance::System, Appearance::Light, Appearance::Dark];

    pub fn label(self) -> &'static str {
        match self {
            Appearance::System => "System",
            Appearance::Light => "Light",
            Appearance::Dark => "Dark",
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct Settings {
    pub card: CardType,
    /// LED color used to draw monochrome badges on screen.
    pub mono_tint: LedColor,
    pub round_leds: bool,
    /// Lit LEDs glow onto their surroundings (preview and pixel canvas).
    pub preview_glow: bool,
    /// LEDs are dimmed to the badge's brightness setting (preview and
    /// pixel canvas).
    pub preview_brightness: bool,
    /// The preview's display width in LEDs (0: the badge type's own),
    /// within `PREVIEW_WIDTHS`.
    pub preview_width: usize,
    pub recent_files: Vec<PathBuf>,
    pub last_dir: Option<PathBuf>,
    pub import_fit_height: bool,
    pub import_invert: bool,
    pub import_threshold: u8,
    pub last_device: Option<String>,
    /// Preview plays every ticked message in turn (instead of the current one).
    pub preview_all: bool,
    pub reopen_last: bool,
    /// Preview → Export: the format ("gif", "png" or "webp").
    pub export_format: String,
    /// Preview → Export: one frame instead of the animation.
    pub export_still: bool,
    pub appearance: Appearance,
    /// A badge has been connected at some point: the getting-started card
    /// isn't shown any more.
    pub seen_badge: bool,
    /// The getting-started card was closed.
    pub hide_getting_started: bool,
    /// The recent files as this app last read or wrote them, to tell which
    /// another running copy added since.
    #[serde(skip)]
    recent_seen: Vec<PathBuf>,
}

impl Default for Settings {
    fn default() -> Self {
        Settings {
            card: CardType::Mono11,
            mono_tint: LedColor::RED,
            round_leds: true,
            preview_glow: true,
            preview_brightness: true,
            preview_width: 0,
            recent_files: Vec::new(),
            last_dir: None,
            import_fit_height: true,
            import_invert: false,
            import_threshold: 127,
            last_device: None,
            preview_all: false,
            reopen_last: true,
            export_format: "gif".into(),
            export_still: false,
            appearance: Appearance::System,
            seen_badge: false,
            hide_getting_started: false,
            recent_seen: Vec::new(),
        }
    }
}

/// How many recent files are remembered (File → Open Recent).
pub const MAX_RECENT: usize = 10;

fn path() -> Option<PathBuf> {
    // Tests run with the defaults and never touch the real settings.
    if cfg!(test) {
        return None;
    }
    use badge_formats::dirs;
    Some(dirs::user_file(&dirs::config_base()?, "settings.json"))
}

impl Settings {
    /// Loads the settings. A damaged file is moved aside and its new path
    /// returned; the defaults are used.
    pub fn load() -> (Self, Option<PathBuf>) {
        match path() {
            Some(p) => Self::load_from(&p),
            None => (Settings::default(), None),
        }
    }

    fn load_from(p: &Path) -> (Self, Option<PathBuf>) {
        let mut read = std::fs::read(p);
        for _ in 0..3 {
            match &read {
                Err(e) if e.kind() != std::io::ErrorKind::NotFound => {}
                _ => break,
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
            read = std::fs::read(p);
        }
        if read.as_ref().is_err_and(|e| e.kind() == std::io::ErrorKind::NotFound) {
            return (Settings::default(), None); // first run
        }
        let parsed = read
            .map_err(|e| e.to_string())
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).map_err(|e| e.to_string()));
        match parsed {
            Ok(s) => {
                let mut s = s.repaired();
                s.recent_seen = s.recent_files.clone();
                (s, None)
            }
            Err(e) => {
                let kept = (1..100)
                    .map(|n| match n {
                        1 => p.with_extension("json.damaged"),
                        n => p.with_extension(format!("json.damaged-{n}")),
                    })
                    .find(|k| !k.exists())
                    .unwrap_or_else(|| p.with_extension("json.damaged"));
                log::warn!(
                    "settings file {} is damaged ({e}); moved to {}",
                    p.display(),
                    kept.display()
                );
                let moved = std::fs::rename(p, &kept).is_ok();
                (Settings::default(), moved.then_some(kept))
            }
        }
    }

    /// Puts values a hand-edited file may have outside their range back to
    /// the defaults.
    fn repaired(mut self) -> Self {
        // Off isn't an LED color.
        if !self.mono_tint.is_on() {
            self.mono_tint = Settings::default().mono_tint;
        }
        self
    }

    pub fn save(&mut self) {
        if let Some(p) = path() {
            self.save_to(&p);
        }
    }

    fn save_to(&mut self, p: &Path) {
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        // One running app at a time: two saving at once would write through
        // the same temporary file. Where locks don't work, it's saved
        // anyway.
        let lock = std::fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(false)
            .open(p.with_extension("json.lock"));
        let _held = lock.ok().filter(|f| f.lock().is_ok());
        // Files another running copy opened since stay in the list.
        if let Some(on_disk) = std::fs::read(p)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<Settings>(&bytes).ok())
        {
            for file in on_disk.recent_files {
                if !self.recent_seen.contains(&file) && !self.recent_files.contains(&file) {
                    self.recent_files.push(file);
                }
            }
            self.recent_files.truncate(MAX_RECENT);
        }
        // A path that isn't UTF-8 can't be written to the file.
        let mut stored = self.clone();
        stored.recent_files.retain(|f| f.to_str().is_some());
        stored.last_dir = stored.last_dir.filter(|d| d.to_str().is_some());
        let Ok(text) = serde_json::to_string_pretty(&stored) else {
            return;
        };
        // Whole or not at all, so a crash can't leave half a file.
        match badge_core::files::replace_file(p, text.as_bytes()) {
            Ok(()) => self.recent_seen = stored.recent_files,
            Err(e) => log::warn!("could not save settings to {}: {e}", p.display()),
        }
    }

    pub fn add_recent(&mut self, file: PathBuf) {
        self.recent_files.retain(|f| f != &file);
        self.recent_files.insert(0, file);
        self.recent_files.truncate(MAX_RECENT);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_off_mono_tint_is_put_back_to_the_default() {
        let with = |mono_tint| Settings {
            mono_tint,
            ..Default::default()
        };
        assert_eq!(with(LedColor::OFF).repaired().mono_tint, LedColor::RED);
        assert_eq!(with(LedColor::BLUE).repaired().mono_tint, LedColor::BLUE);
    }

    fn temp_file(test: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("led-badge-studio-settings-{test}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn a_file_that_cant_be_read_is_moved_aside() {
        let p = temp_file("unreadable");
        std::fs::write(&p, b"{\"appearance\": \"dark\", \"last_device\": \"caf\xe9\"}").unwrap();
        let (settings, kept) = Settings::load_from(&p);
        assert_eq!(settings.appearance, Appearance::System);
        let kept = kept.expect("moved aside");
        assert!(kept.exists() && !p.exists());
        std::fs::write(&p, b"{").unwrap();
        let again = Settings::load_from(&p).1.expect("moved aside");
        assert_ne!(again, kept);
        assert!(kept.exists() && again.exists());
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[test]
    fn files_another_running_copy_opened_stay_recent() {
        let p = temp_file("two-copies");
        let (mut a, _) = Settings::load_from(&p);
        let (mut b, _) = Settings::load_from(&p);
        a.add_recent("/a.badge".into());
        a.save_to(&p);
        b.add_recent("/b.badge".into());
        b.save_to(&p);
        let (read, _) = Settings::load_from(&p);
        assert_eq!(read.recent_files, [PathBuf::from("/b.badge"), "/a.badge".into()]);

        // A file this copy took out of the list stays out.
        b.recent_files.clear();
        b.save_to(&p);
        assert!(Settings::load_from(&p).0.recent_files.is_empty());
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }

    #[cfg(unix)]
    #[test]
    fn a_path_that_isnt_utf8_doesnt_stop_saving() {
        use std::os::unix::ffi::OsStrExt;
        let p = temp_file("not-utf8");
        let (mut s, _) = Settings::load_from(&p);
        s.add_recent(std::ffi::OsStr::from_bytes(b"/caf\xe9.badge").into());
        s.add_recent("/fine.badge".into());
        s.appearance = Appearance::Dark;
        s.save_to(&p);
        let (read, _) = Settings::load_from(&p);
        assert_eq!(read.appearance, Appearance::Dark);
        assert_eq!(read.recent_files, [PathBuf::from("/fine.badge")]);
        let _ = std::fs::remove_dir_all(p.parent().unwrap());
    }
}
