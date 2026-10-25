//! User preferences persisted between runs.

use std::path::PathBuf;

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
        let Some(p) = path() else {
            return (Settings::default(), None);
        };
        let Ok(text) = std::fs::read_to_string(&p) else {
            return (Settings::default(), None); // first run
        };
        match serde_json::from_str::<Settings>(&text) {
            Ok(s) => (s.repaired(), None),
            Err(e) => {
                let kept = p.with_extension("json.damaged");
                log::warn!(
                    "settings file {} is damaged ({e}); moved to {}",
                    p.display(),
                    kept.display()
                );
                let moved = std::fs::rename(&p, &kept).is_ok();
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

    pub fn save(&self) {
        let Some(p) = path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        if let Ok(s) = serde_json::to_string_pretty(self) {
            // Whole or not at all, so a crash can't leave half a file.
            if let Err(e) = badge_core::files::replace_file(&p, s.as_bytes()) {
                log::warn!("could not save settings to {}: {e}", p.display());
            }
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
}
