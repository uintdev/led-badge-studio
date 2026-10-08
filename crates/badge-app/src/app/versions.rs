//! File → Revert To: back to the project as last opened or saved, or to an
//! earlier save of its file. Reverting is an edit (Undo brings the changes
//! back); the file is unchanged until saved.
//!
//! Each save keeps the file as it was, as an earlier version: on macOS in
//! the system's version store (native_document.rs), as other Mac apps do;
//! on Linux and Windows, which have no such store for apps, in the app's
//! data folder, the last [`KEEP`] per project file.

use std::time::{Duration, SystemTime, UNIX_EPOCH};

use super::*;

/// How many earlier versions of each project file are kept (Linux and
/// Windows).
const KEEP: usize = 10;

/// Where the app keeps earlier versions (Linux and Windows); `None` on
/// macOS, which keeps them itself, and in tests, which set their own.
pub(super) fn default_dir() -> Option<PathBuf> {
    if cfg!(test) || cfg!(target_os = "macos") {
        return None;
    }
    use badge_formats::dirs;
    dirs::data_base().map(|base| dirs::user_file(&base, "versions"))
}

impl BadgeApp {
    /// Keeps `path` as it is now as an earlier version; just before saving
    /// over it.
    pub(super) fn keep_version(&self, path: &Path) {
        #[cfg(all(target_os = "macos", not(test)))]
        super::native_document::keep_version(path);
        #[cfg(not(all(target_os = "macos", not(test))))]
        if let Some(dir) = &self.versions_dir {
            store::keep(dir, path);
        }
    }

    /// Reads the earlier versions of the project's file.
    pub(super) fn refresh_file_versions(&mut self) {
        let Some(path) = self.path.as_deref() else {
            self.file_versions.clear();
            return;
        };
        #[cfg(all(target_os = "macos", not(test)))]
        {
            self.file_versions = super::native_document::file_versions(path);
        }
        #[cfg(not(all(target_os = "macos", not(test))))]
        {
            self.file_versions = self
                .versions_dir
                .as_deref()
                .map(|dir| store::list(dir, path))
                .unwrap_or_default();
        }
    }

    /// Revert To's first item.
    pub(super) fn revert_label(&self) -> &'static str {
        if self.saved_here {
            "Last Saved Version"
        } else {
            "Last Opened Version"
        }
    }

    /// Whether there's anything to go back to the version last opened or
    /// saved from.
    pub(super) fn can_revert(&self) -> bool {
        self.saved.is_some() && self.is_dirty()
    }

    /// File → Revert To in the app's own File menu (Linux and Windows).
    #[cfg_attr(target_os = "macos", allow(dead_code))]
    pub(super) fn revert_items(&self, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let mut items = vec![
            menu_item(
                self.revert_label(),
                false,
                self.act(cx, |t, _, cx| t.revert_to_saved(cx)),
            )
            .disabled(!self.can_revert()),
        ];
        for (i, (label, path)) in self.file_versions.iter().enumerate() {
            let path = path.clone();
            let item = menu_item(
                label.clone(),
                false,
                self.act(cx, move |t, w, cx| t.revert_to_version(path.clone(), w, cx)),
            );
            items.push(if i == 0 { item.separator() } else { item });
        }
        items
    }

    /// Revert To → Last Opened (or Saved) Version: the project as it was
    /// then.
    pub(super) fn revert_to_saved(&mut self, cx: &mut Context<Self>) {
        let Some(saved) = self.saved.clone() else {
            return;
        };
        self.apply_as_edit(saved, cx);
        self.set_status("Reverted to the version last opened or saved", Severity::Success);
    }

    /// Revert To → an earlier version.
    pub(super) fn revert_to_version(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match badge_formats::open_checked(&path) {
            Ok(loaded) => {
                let label = self
                    .file_versions
                    .iter()
                    .find(|(_, p)| p == &path)
                    .map(|(label, _)| label.clone());
                self.apply_as_edit(loaded.project, cx);
                self.set_status(
                    match label {
                        Some(label) => format!("Reverted to the version saved {label}"),
                        None => "Reverted to an earlier version".into(),
                    },
                    Severity::Success,
                );
            }
            Err(e) => self.show_open_error(&path, &e, window, cx),
        }
    }

    /// Shows `project` in place of the current one, as one undoable edit.
    fn apply_as_edit(&mut self, mut project: Project, cx: &mut Context<Self>) {
        project.normalize();
        self.project = project;
        self.fill_led_color();
        self.current = self.current.min(self.project.messages.len().saturating_sub(1));
        self.rerender_all();
        self.sync_input(cx);
        self.commit_history();
        cx.notify();
    }
}

/// When a version was saved, as "Today at 14:15", "Yesterday at 9:02" or
/// "6 Oct 2026 at 14:15".
#[cfg_attr(all(target_os = "macos", not(test)), allow(dead_code))]
pub(super) fn when_label(when: SystemTime) -> String {
    use chrono::{DateTime, Days, Local};
    let when: DateTime<Local> = when.into();
    let today = Local::now().date_naive();
    let day = when.date_naive();
    let time = when.format("%H:%M");
    if day == today {
        format!("Today at {time}")
    } else if today.checked_sub_days(Days::new(1)) == Some(day) {
        format!("Yesterday at {time}")
    } else {
        format!("{} at {time}", when.format("%-d %b %Y"))
    }
}

/// The app's own version store: a folder per project file, named after its
/// path, holding copies named after when they were kept.
#[cfg_attr(all(target_os = "macos", not(test)), allow(dead_code))]
pub(super) mod store {
    use super::*;

    /// The folder for `file`'s versions.
    fn folder(dir: &Path, file: &Path) -> PathBuf {
        // FNV-1a: the same name in every version of the app (unlike the
        // standard library's hasher).
        let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
        for b in file.to_string_lossy().bytes() {
            hash = (hash ^ u64::from(b)).wrapping_mul(0x0100_0000_01b3);
        }
        dir.join(format!("{hash:016x}"))
    }

    /// Copies `file` in as its newest version, then lets go of all but the
    /// newest [`KEEP`]. Best effort: a failure only means one fewer
    /// version.
    pub fn keep(dir: &Path, file: &Path) {
        if !file.is_file() {
            return;
        }
        let folder = folder(dir, file);
        let kept = (|| -> std::io::Result<()> {
            std::fs::create_dir_all(&folder)?;
            // Which file these are of, for anyone looking.
            std::fs::write(folder.join("file.txt"), file.to_string_lossy().as_bytes())?;
            let millis = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            std::fs::copy(file, folder.join(format!("{millis:020}.badge")))?;
            Ok(())
        })();
        if let Err(e) = kept {
            log::warn!("couldn't keep the earlier version of {}: {e}", file.display());
            return;
        }
        for (_, old) in versions(&folder).into_iter().skip(KEEP) {
            let _ = std::fs::remove_file(old);
        }
    }

    /// `file`'s earlier versions, newest first: when each was kept, and
    /// where.
    pub fn list(dir: &Path, file: &Path) -> Vec<(String, PathBuf)> {
        versions(&folder(dir, file))
            .into_iter()
            .map(|(when, path)| (when_label(when), path))
            .collect()
    }

    fn versions(folder: &Path) -> Vec<(SystemTime, PathBuf)> {
        let Ok(entries) = std::fs::read_dir(folder) else {
            return Vec::new();
        };
        let mut found: Vec<(SystemTime, PathBuf)> = entries
            .flatten()
            .filter_map(|e| {
                let path = e.path();
                let millis: u64 = path
                    .file_stem()?
                    .to_str()?
                    .parse()
                    .ok()
                    .filter(|_| path.extension().is_some_and(|x| x == "badge"))?;
                Some((UNIX_EPOCH + Duration::from_millis(millis), path))
            })
            .collect();
        found.sort_by_key(|v| std::cmp::Reverse(v.0));
        found
    }
}
