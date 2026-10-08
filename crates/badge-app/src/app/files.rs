//! Project files: new, open, save, repair, recent files and drops.

use super::dialogs::sentence_case;
use super::*;

impl BadgeApp {
    /// Runs `then` once unsaved changes are dealt with: when there are
    /// some, asks whether to save them first (Save, Don't Save or Cancel).
    pub(super) fn ask_to_save_first(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        if !self.is_dirty() {
            then(self, window, cx);
            return;
        }
        let name = self.path.as_deref().map_or_else(
            || "this new project".to_string(),
            |p| format!("\u{201c}{}\u{201d}", file_name(p)),
        );
        // Either Save (once saved) or Don't Save goes on to `then`.
        let then = Rc::new(RefCell::new(Some(then)));
        let dont_save = then.clone();
        self.ask_or(
            &format!("Save the changes to {name}?"),
            "Your changes will be lost if you don't save them.",
            "Save",
            "Don't Save",
            window,
            cx,
            move |t, window, cx| {
                t.save_then(window, cx, move |t, window, cx| {
                    if let Some(f) = then.take() {
                        f(t, window, cx);
                    }
                });
            },
            move |t, window, cx| {
                if let Some(f) = dont_save.take() {
                    t.saved = Some(t.project.clone());
                    f(t, window, cx);
                }
            },
        );
    }

    /// Saves as Save does (choosing where for a new project), then runs
    /// `then` if the project was saved.
    fn save_then(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        match self.path.clone() {
            Some(p) if self.changed_elsewhere() => {
                // Keep Mine saves, Use Theirs leaves the file as it is; both
                // go on to `then`.
                let then = Rc::new(RefCell::new(Some(then)));
                let theirs = then.clone();
                self.ask_changed_elsewhere(
                    window,
                    cx,
                    move |t, window, cx| {
                        t.save_to(p, cx);
                        if !t.is_dirty()
                            && let Some(f) = then.take()
                        {
                            f(t, window, cx);
                        }
                    },
                    move |t, window, cx| {
                        if let Some(f) = theirs.take() {
                            t.saved = Some(t.project.clone());
                            f(t, window, cx);
                        }
                    },
                );
            }
            Some(p) => {
                self.save_to(p, cx);
                if !self.is_dirty() {
                    then(self, window, cx);
                }
            }
            None => self.save_as_then(window, cx, then),
        }
    }

    pub(super) fn new_project(&mut self, _: &NewProject, window: &mut Window, cx: &mut Context<Self>) {
        self.ask_to_save_first(window, cx, |this, _, cx| {
            this.replace_project(Project::new(this.project.card), None, cx);
            this.set_status("New project", Severity::Info);
            cx.notify();
        });
    }

    pub(super) fn open_project(&mut self, _: &OpenProject, window: &mut Window, cx: &mut Context<Self>) {
        // Unsaved changes are asked about only once the file has been read.
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open".into()),
        });
        let rx = self.during_system_dialog(rx, cx);
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(p) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| this.open_requested(p, window, cx))
                    .ok();
            }
        })
        .detach();
    }

    pub(super) fn notice(
        &mut self,
        title: impl Into<SharedString>,
        detail: impl Into<SharedString>,
        items: Vec<(&str, String)>,
        error: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        self.dialog = Some(Dialog::Notice {
            title: title.into(),
            detail: detail.into(),
            items: items
                .into_iter()
                .map(|(label, value)| (label.to_string().into(), value.into()))
                .collect(),
            error,
        });
        self.pending_confirm = None;
        self.pending_cancel = None;
        self.pending_alternate = None;
        self.focus_dialog(window, cx);
        cx.notify();
    }

    /// Shows the project read from `path`. A repaired one is saved back with
    /// `save`, else it stays modified.
    pub(super) fn show_loaded(
        &mut self,
        path: PathBuf,
        loaded: badge_formats::Loaded,
        save: bool,
        cx: &mut Context<Self>,
    ) {
        let name = file_name(&path);
        let repaired = !loaded.fixes.is_empty();
        self.replace_project(loaded.project, Some(path.clone()), cx);
        self.settings.last_dir = path.parent().map(Path::to_path_buf);
        self.settings.add_recent(path.clone());
        self.settings.save();
        if !repaired {
            self.set_status(format!("Opened {name}"), Severity::Success);
        } else if save {
            self.save_to(path, cx);
            if !self.is_dirty() {
                self.set_status(format!("Repaired, saved and opened {name}"), Severity::Success);
            }
        } else {
            self.saved = None;
            self.set_status(
                format!("Opened a repaired copy of {name}; save to keep the repairs (the file is unchanged)"),
                Severity::Info,
            );
        }
        cx.notify();
    }

    /// File → Check Project File.
    pub(super) fn check_project_file(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let rx = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Check".into()),
        });
        let rx = self.during_system_dialog(rx, cx);
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(paths))) = rx.await
                && let Some(p) = paths.into_iter().next()
            {
                this.update_in(cx, |this, window, cx| this.check_file(p, window, cx))
                    .ok();
            }
        })
        .detach();
    }

    pub(super) fn check_file(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match badge_formats::open_checked(&path) {
            Ok(loaded) if loaded.fixes.is_empty() => {
                let name = file_name(&path);
                self.notice(
                    format!("No problems in {name}"),
                    "Every value in it is something the badge supports.",
                    vec![("File", path.display().to_string())],
                    false,
                    window,
                    cx,
                );
            }
            Ok(loaded) => self.offer_repair(path, loaded, false, window, cx),
            Err(e) => self.show_open_error(&path, &e, window, cx),
        }
    }

    pub(super) fn offer_repair(
        &mut self,
        path: PathBuf,
        loaded: badge_formats::Loaded,
        opening: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.menu = None;
        self.dialog = Some(Dialog::Repair {
            path,
            fixes: loaded.fixes.clone(),
            opening,
        });
        self.pending_repair = Some(loaded);
        self.pending_confirm = None;
        self.pending_cancel = None;
        self.pending_alternate = None;
        self.focus_dialog(window, cx);
        cx.notify();
    }

    /// The repair dialog's choice: open the repaired project (saving it first
    /// with `save`), or only save it.
    pub(super) fn finish_repair(&mut self, open: bool, save: bool, window: &mut Window, cx: &mut Context<Self>) {
        let Some(Dialog::Repair { path, .. }) = self.dialog.clone() else {
            return;
        };
        let Some(loaded) = self.pending_repair.take() else {
            return;
        };
        self.close_dialog(window, cx);
        let name = file_name(&path);
        if open {
            self.ask_to_save_first(window, cx, move |t, _, cx| t.show_loaded(path, loaded, save, cx));
        } else {
            // The file as it was becomes an earlier version (File → Revert
            // To), as when saving: the repairs can drop what's out of range.
            self.keep_version(&path);
            match badge_formats::save_project(&loaded.project, &path) {
                Ok(()) => {
                    // The open project's own file was saved by the app, not
                    // changed by another program.
                    if self.path.as_ref() == Some(&path) {
                        self.note_file_stamp();
                        self.refresh_file_versions();
                    }
                    self.set_status(format!("Repaired and saved {name}"), Severity::Success);
                }
                Err(e) => self.set_status(format!("Couldn't save {name}: {e:#}"), Severity::Error),
            }
            cx.notify();
        }
    }

    pub(super) fn show_open_error(
        &mut self,
        path: &Path,
        e: &anyhow::Error,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let name = file_name(path);
        self.set_status(format!("Could not open {name}"), Severity::Error);
        let mut items = Vec::new();
        let headline = match e.downcast_ref::<badge_formats::FileError>() {
            Some(fe) => {
                if let Some(at) = &fe.location {
                    items.push(("Where", sentence_case(at, false)));
                }
                if let Some(cause) = &fe.cause {
                    items.push(("Likely cause", sentence_case(cause, true)));
                }
                if let Some(advice) = &fe.advice {
                    items.push(("What to do", sentence_case(advice, true)));
                }
                sentence_case(&fe.problem, true)
            }
            None => sentence_case(&format!("{e:#}"), true),
        };
        items.push(("File", path.display().to_string()));
        self.notice(format!("Couldn't open {name}"), headline, items, true, window, cx);
    }

    pub(super) fn save_project(&mut self, _: &SaveProject, window: &mut Window, cx: &mut Context<Self>) {
        match self.path.clone() {
            Some(p) if self.changed_elsewhere() => {
                let theirs = p.clone();
                self.ask_changed_elsewhere(
                    window,
                    cx,
                    move |t, _, cx| t.save_to(p, cx),
                    move |t, window, cx| t.reload(theirs, window, cx),
                );
            }
            Some(p) => self.save_to(p, cx),
            None => self.save_project_as(&SaveProjectAs, window, cx),
        }
    }

    /// Whether another program changed the project's file since the app
    /// read or wrote it: its contents differ. They're only read when its
    /// time changed (which alone isn't proof: a program may write a file
    /// again as it was).
    pub(super) fn changed_elsewhere(&self) -> bool {
        let (Some(path), Some(stamp)) = (self.path.as_deref(), self.file_stamp) else {
            return false;
        };
        if stamp.modified.is_some() && modified_time(path) == stamp.modified {
            return false;
        }
        let Ok(bytes) = std::fs::read(path) else {
            return false;
        };
        let hash = content_hash(&bytes);
        let changed = hash != stamp.hash;
        if changed {
            log::info!("{} was changed by another program", path.display());
        }
        changed
    }

    pub(super) fn note_file_stamp(&mut self) {
        self.file_stamp = self.path.as_deref().and_then(FileStamp::read);
        self.told_changed_elsewhere = false;
    }

    /// When another program changed the project's file, the status bar says
    /// so. The file isn't opened again on its own, even with nothing unsaved
    /// here: what's shown stays as it is, and saving asks whose changes to
    /// keep.
    pub(super) fn check_file_changed_elsewhere(&mut self, cx: &mut Context<Self>) {
        if self.told_changed_elsewhere || self.dialog.is_some() || !self.changed_elsewhere() {
            return;
        }
        let Some(path) = self.path.as_deref() else {
            return;
        };
        self.told_changed_elsewhere = true;
        self.set_status(
            format!(
                "Another program changed {}; saving will ask whose changes to keep",
                file_name(path)
            ),
            Severity::Info,
        );
        cx.notify();
    }

    /// Asks whose version to keep when the file was changed elsewhere:
    /// `keep_mine` saves over it, `use_theirs` leaves it as it is.
    fn ask_changed_elsewhere(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        keep_mine: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
        use_theirs: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let name = self.path.as_deref().map(file_name).unwrap_or_default();
        self.ask_or(
            &format!("\u{201c}{name}\u{201d} was changed by another program"),
            "The file changed after you opened or last saved it here. Keep your version to replace those \
             changes, or use the file as it is now: the changes you made here since then will be lost.",
            "Keep Mine",
            "Use Theirs",
            window,
            cx,
            keep_mine,
            use_theirs,
        );
    }

    /// Opens the project's file again, as it is now on disk.
    pub(super) fn reload(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match badge_formats::open_checked(&path) {
            Ok(loaded) => {
                self.show_loaded(path.clone(), loaded, false, cx);
                if self.saved.is_some() {
                    self.set_status(format!("Reloaded {}", file_name(&path)), Severity::Success);
                }
            }
            Err(e) => self.show_open_error(&path, &e, window, cx),
        }
    }

    pub(super) fn save_project_as(&mut self, _: &SaveProjectAs, window: &mut Window, cx: &mut Context<Self>) {
        self.save_as_then(window, cx, |_, _, _| {});
    }

    /// Asks where to save, saves, then runs `then` if the project was saved.
    fn save_as_then(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        let dir = self.default_dir();
        let rx = cx.prompt_for_new_path(&dir, Some("messages.badge"));
        let rx = self.during_system_dialog(rx, cx);
        cx.spawn_in(window, async move |this, cx| {
            if let Ok(Ok(Some(mut p))) = rx.await {
                if p.extension().is_none() {
                    p.set_extension(badge_formats::PROJECT_EXT);
                }
                this.update_in(cx, |this, window, cx| {
                    this.save_to(p, cx);
                    if !this.is_dirty() {
                        then(this, window, cx);
                    }
                })
                .ok();
            }
        })
        .detach();
    }

    pub(super) fn save_to(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        // The file as last saved becomes an earlier version (File → Revert
        // To).
        if self.is_dirty() && self.path.as_ref() == Some(&path) {
            self.keep_version(&path);
        }
        match badge_formats::save_project(&self.project, &path) {
            Ok(()) => {
                self.saved = Some(self.project.clone());
                self.file_stamp = FileStamp::read(&path);
                self.told_changed_elsewhere = false;
                self.saved_here = true;
                self.set_status(format!("Saved {}", path.display()), Severity::Success);
                self.settings.last_dir = path.parent().map(Path::to_path_buf);
                self.settings.add_recent(path.clone());
                self.settings.save();
                self.path = Some(path);
                self.refresh_file_versions();
                self.keep_recovery_copy();
            }
            Err(e) => self.set_status(format!("Save failed: {e:#}"), Severity::Error),
        }
        cx.notify();
    }

    pub(super) fn default_dir(&self) -> PathBuf {
        self.path
            .as_ref()
            .and_then(|p| p.parent().map(Path::to_path_buf))
            .or_else(|| self.settings.last_dir.clone())
            .or_else(home_dir)
            .unwrap_or_else(|| PathBuf::from("."))
    }

    /// Dropped images go into the current message; other files open as
    /// projects.
    pub(super) fn handle_drop(&mut self, paths: &[PathBuf], window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = paths.first().cloned() else {
            return;
        };
        let is_image = path.extension().and_then(|e| e.to_str()).is_some_and(|e| {
            matches!(
                e.to_ascii_lowercase().as_str(),
                "png" | "bmp" | "gif" | "jpg" | "jpeg" | "ico"
            )
        });
        if is_image {
            self.import_image_from(&path, cx);
        } else {
            self.open_requested(path, window, cx);
        }
    }

    /// Opens a project from outside the app (a drop, or the file manager).
    /// The file is read before asking about unsaved changes.
    pub fn open_requested(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        match badge_formats::open_checked(&path) {
            Ok(loaded) if loaded.fixes.is_empty() => {
                self.ask_to_save_first(window, cx, move |t, _, cx| t.show_loaded(path, loaded, false, cx));
            }
            Ok(loaded) => self.offer_repair(path, loaded, true, window, cx),
            Err(e) => {
                // A moved or deleted recent file leaves the list.
                if !path.exists() && self.settings.recent_files.contains(&path) {
                    self.settings.recent_files.retain(|f| f != &path);
                    self.settings.save();
                }
                self.show_open_error(&path, &e, window, cx);
            }
        }
    }

    pub(super) fn clear_recent_files(&mut self) {
        self.settings.recent_files.clear();
        self.settings.save();
        self.set_status("Recent files cleared.", Severity::Info);
    }

    pub(super) fn recent_items(&self, cx: &mut Context<Self>) -> Vec<MenuItem> {
        let files = &self.settings.recent_files;
        let mut items: Vec<MenuItem> = if files.is_empty() {
            vec![menu_item("No recent files", false, |_, _| {}).disabled(true)]
        } else {
            recent_labels(files)
                .into_iter()
                .zip(files.iter().cloned())
                .map(|((name, folder), path)| {
                    let full = path.display().to_string();
                    menu_item(
                        name,
                        false,
                        self.act(cx, move |t, w, cx| t.open_requested(path.clone(), w, cx)),
                    )
                    .hint(folder)
                    .tooltip(full)
                })
                .collect()
        };
        items.push(
            menu_item(
                "Clear Recent Files",
                false,
                self.act(cx, |t, _, _| t.clear_recent_files()),
            )
            .disabled(files.is_empty())
            .separator(),
        );
        items
    }
}

/// The user's home folder: `HOME`, or on Windows (where it usually isn't
/// set) `USERPROFILE`.
fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

/// Labels for the recent files: (file name, folder). The folder shows as
/// little of the path as tells files with the same name apart: just the
/// parent folder ("Downloads") when the name is unique, more ("…/a/x" vs
/// "…/b/x") when it isn't. The home folder shows as "~".
pub(super) fn recent_labels(paths: &[PathBuf]) -> Vec<(String, String)> {
    let home = home_dir();
    // Each file's folder as path components, with the home folder as "~".
    let folders: Vec<Vec<String>> = paths
        .iter()
        .map(|p| {
            let parent = p.parent().unwrap_or(Path::new(""));
            let (prefix, rest) = match home.as_deref().and_then(|h| parent.strip_prefix(h).ok()) {
                Some(rest) => (Some("~".to_string()), rest),
                None => (None, parent),
            };
            prefix
                .into_iter()
                .chain(rest.components().map(|c| c.as_os_str().to_string_lossy().into_owned()))
                .collect()
        })
        .collect();
    let names: Vec<String> = paths.iter().map(|p| file_name(p)).collect();
    // Each label shows one folder more than it has in common (counting from
    // the end) with any other file of the same name.
    let common = |a: &[String], b: &[String]| a.iter().rev().zip(b.iter().rev()).take_while(|(x, y)| x == y).count();
    let depth: Vec<usize> = (0..paths.len())
        .map(|i| {
            let shared = (0..paths.len())
                .filter(|&j| j != i && names[j] == names[i])
                .map(|j| common(&folders[i], &folders[j]))
                .max()
                .unwrap_or(0);
            (shared + 1).min(folders[i].len()).max(1)
        })
        .collect();
    let label = |i: usize, short: bool| {
        let folder = &folders[i];
        let start = folder.len().saturating_sub(depth[i]);
        let part = |c: &String| if short { shorten(c, MAX_FOLDER_CHARS) } else { c.clone() };
        let shown = folder[start..]
            .iter()
            .map(part)
            .collect::<Vec<_>>()
            .join("/")
            .replace("//", "/");
        let folder = if start > 0 && depth[i] > 1 {
            format!("\u{2026}/{shown}")
        } else {
            shown
        };
        let name = if short {
            shorten_name(&names[i], MAX_NAME_CHARS)
        } else {
            names[i].clone()
        };
        (name, folder)
    };
    let short: Vec<(String, String)> = (0..paths.len()).map(|i| label(i, true)).collect();
    // Shortening could make two entries look the same; those keep the
    // whole names (the menu just gets wider).
    (0..paths.len())
        .map(|i| {
            let clash = (0..paths.len()).any(|j| j != i && short[j] == short[i]);
            if clash { label(i, false) } else { short[i].clone() }
        })
        .collect()
}

pub(super) const MAX_FOLDER_CHARS: usize = 24;

pub(super) const MAX_NAME_CHARS: usize = 40;

/// `text` cut to `max` characters by removing the middle ("my-very-lo…2025").
pub(super) fn shorten(text: &str, max: usize) -> String {
    let chars: Vec<char> = text.chars().collect();
    if chars.len() <= max {
        return text.to_string();
    }
    let keep = max - 1;
    let head = keep.div_ceil(2);
    let tail = keep - head;
    chars[..head]
        .iter()
        .chain(std::iter::once(&'\u{2026}'))
        .chain(&chars[chars.len() - tail..])
        .collect()
}

/// Like [`shorten`], but always keeps a file's extension.
pub(super) fn shorten_name(name: &str, max: usize) -> String {
    match name.rsplit_once('.') {
        Some((stem, ext)) if ext.len() < 10 && name.chars().count() > max => {
            let room = max.saturating_sub(ext.chars().count() + 1).max(4);
            format!("{}.{ext}", shorten(stem, room))
        }
        _ => shorten(name, max),
    }
}

/// A project file as the app last read or wrote it.
#[derive(Clone, Copy)]
pub(super) struct FileStamp {
    /// When it was changed then, if known.
    pub modified: Option<std::time::SystemTime>,
    /// A fingerprint of its contents.
    pub hash: u64,
}

impl FileStamp {
    pub fn read(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        Some(FileStamp {
            modified: modified_time(path),
            hash: content_hash(&bytes),
        })
    }
}

/// A fingerprint of a file's contents, to tell whether it changed.
pub(super) fn content_hash(bytes: &[u8]) -> u64 {
    use std::hash::{DefaultHasher, Hasher};
    let mut hasher = DefaultHasher::new();
    hasher.write(bytes);
    hasher.finish()
}

/// When `path` was last changed; `None` if it can't be read.
fn modified_time(path: &Path) -> Option<std::time::SystemTime> {
    std::fs::metadata(path).and_then(|m| m.modified()).ok()
}

pub(super) fn file_name(path: &Path) -> String {
    path.file_name()
        .map_or_else(|| path.display().to_string(), |n| n.to_string_lossy().into_owned())
}
