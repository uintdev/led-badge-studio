//! The last project uploaded from this computer (badges can't be read back).

use std::path::PathBuf;
use std::time::SystemTime;

use badge_core::protocol::encode_usb;
use badge_core::{AddressMode, Project, Source, Timestamp};

#[derive(Clone)]
pub struct LastSent {
    pub project: Project,
    pub when: SystemTime,
}

/// Exactly what the badge receives, ignoring the timestamp and addressing.
fn upload_bytes(p: &Project) -> Vec<u8> {
    encode_usb(p, AddressMode::Ignore, Timestamp::default())
}

impl LastSent {
    fn path() -> Option<PathBuf> {
        crate::dirs::data_base().map(|b| crate::dirs::user_file(&b, "last-sent.badge"))
    }

    /// Saves `project` as the last upload. Failures are ignored (best effort).
    pub fn record(project: &Project) {
        let Some(p) = Self::path() else { return };
        if let Some(dir) = p.parent() {
            let _ = std::fs::create_dir_all(dir);
        }
        let _ = crate::save_project(project, &p);
    }

    pub fn load() -> Option<Self> {
        let p = Self::path()?;
        let when = std::fs::metadata(&p).and_then(|m| m.modified()).ok()?;
        let project = match crate::open(&p) {
            Ok(p) => p,
            Err(e) => {
                eprintln!("ignoring the last-sent record {}: {e:#}", p.display());
                return None;
            }
        };
        Some(LastSent { project, when })
    }

    /// True when sending `project` would put the same content on the badge.
    pub fn matches(&self, project: &Project) -> bool {
        upload_bytes(&self.project) == upload_bytes(project)
    }

    pub fn when_text(&self) -> String {
        let secs = self.when.elapsed().map_or(0, |d| d.as_secs());
        match secs {
            0..60 => "just now".into(),
            60..3600 => format!("{} min ago", secs / 60),
            3600..86400 => format!("{} h ago", secs / 3600),
            _ => format!("{} days ago", secs / 86400),
        }
    }

    pub fn summary(&self) -> String {
        let texts: Vec<String> = self
            .project
            .messages
            .iter()
            .filter(|m| m.has_content())
            .map(|m| {
                if m.source == Source::Bitmap || m.text.is_empty() {
                    "(drawing)".to_string()
                } else {
                    format!("“{}”", m.text.chars().take(24).collect::<String>())
                }
            })
            .collect();
        format!(
            "Badge connected. It differs from the editor; last sent from this computer {}: {}",
            self.when_text(),
            if texts.is_empty() {
                "(nothing)".into()
            } else {
                texts.join(", ")
            }
        )
    }
}
