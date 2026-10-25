//! Crash recovery. While there's unsaved work that nothing else keeps, a
//! copy of it is kept in the app's data folder; if the app closes without
//! the work being saved or discarded (a crash, being forced to quit, the
//! computer shutting down), the copy is offered back when it next starts.
//!
//! Each running copy of the app keeps its own file, next to a lock file it
//! holds while it runs, so one copy never offers (or deletes) the unsaved
//! work of another that's still running.

use std::time::{Duration, SystemTime};

use super::files::file_name;
use super::*;

/// The folder of copies, one per running app.
const DIR_NAME: &str = "unsaved-work";
/// The single copy earlier versions kept; still offered.
const LEGACY_FILE_NAME: &str = "unsaved-work.json";

/// How often the copy is brought up to date.
const INTERVAL: Duration = Duration::from_secs(2);

/// Unsaved work found when the app started.
pub(super) struct Recovered {
    pub project: Project,
    /// The project's file, if it had one.
    pub file: Option<PathBuf>,
    pub when: SystemTime,
    /// The copy it was read from, removed once it's recovered or discarded.
    pub path: PathBuf,
}

/// Where this app keeps its copy, locked for as long as it runs; `None` in
/// tests, which set their own.
pub(super) fn default_path() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    static OWN: std::sync::OnceLock<Option<(PathBuf, std::fs::File)>> = std::sync::OnceLock::new();
    OWN.get_or_init(|| {
        let dir = copies_dir()?;
        std::fs::create_dir_all(&dir).ok()?;
        let started = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis();
        let id = format!("{}-{started}", std::process::id());
        let lock = std::fs::File::create(dir.join(format!("{id}.lock"))).ok()?;
        lock.try_lock().ok()?;
        Some((dir.join(format!("{id}.json")), lock))
    })
    .as_ref()
    .map(|(path, _)| path.clone())
}

fn copies_dir() -> Option<PathBuf> {
    use badge_formats::dirs;
    dirs::data_base().map(|base| dirs::user_file(&base, DIR_NAME))
}

/// The newest copy left by an app that isn't running any more (none in
/// tests).
pub(super) fn find_left_behind() -> Option<Recovered> {
    if cfg!(test) {
        return None;
    }
    use badge_formats::dirs;
    let own = default_path();
    let dir = copies_dir()?;
    // Lock files of apps that closed with nothing to keep.
    for entry in std::fs::read_dir(&dir).into_iter().flatten().flatten() {
        let lock = entry.path();
        if lock.extension().is_some_and(|x| x == "lock")
            && !lock.with_extension("json").exists()
            && !in_use(&lock.with_extension("json"))
        {
            let _ = std::fs::remove_file(lock);
        }
    }
    let mut found: Vec<Recovered> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json") && Some(p) != own.as_ref() && !in_use(p))
        .chain(dirs::data_base().map(|b| dirs::user_file(&b, LEGACY_FILE_NAME)))
        .filter_map(|p| read(&p))
        .collect();
    found.sort_by_key(|r| std::cmp::Reverse(r.when));
    found.into_iter().next()
}

/// Whether the app that keeps the copy at `path` is still running (holds
/// its lock).
fn in_use(path: &Path) -> bool {
    std::fs::File::open(path.with_extension("lock")).is_ok_and(|f| f.try_lock().is_err())
}

/// Removes a copy, and the lock file of the app that kept it.
fn remove(path: &Path) {
    let _ = std::fs::remove_file(path);
    if path.extension().is_some_and(|x| x == "json") && !in_use(path) {
        let _ = std::fs::remove_file(path.with_extension("lock"));
    }
}

/// Reads the copy at `path`; `None` if there's none (or it can't be read).
pub(super) fn read(path: &Path) -> Option<Recovered> {
    let text = std::fs::read_to_string(path).ok()?;
    let when = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let read = || -> anyhow::Result<Recovered> {
        let value: serde_json::Value = serde_json::from_str(&text)?;
        let project = badge_formats::project_from_json(&value["project"].to_string())?;
        let file = value["file"].as_str().map(PathBuf::from);
        Ok(Recovered {
            project,
            file,
            when,
            path: path.to_path_buf(),
        })
    };
    read()
        .inspect_err(|e| log::warn!("ignoring the unsaved-work copy {}: {e:#}", path.display()))
        .ok()
}

/// Writes the copy: the project as in a project file, and where its file
/// is.
fn write(path: &Path, project: &Project, file: Option<&Path>) -> anyhow::Result<()> {
    let project: serde_json::Value = serde_json::from_str(&badge_formats::project_to_json(project)?)?;
    let value = serde_json::json!({
        "file": file.map(|f| f.to_string_lossy()),
        "project": project,
    });
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir)?;
    }
    // Written aside and onto the disk, then moved over the old copy, so a
    // crash or power cut while writing leaves the old one.
    let tmp = path.with_extension("json.tmp");
    let mut out = std::fs::File::create(&tmp)?;
    std::io::Write::write_all(&mut out, serde_json::to_string(&value)?.as_bytes())?;
    out.sync_all()?;
    std::fs::rename(&tmp, path)?;
    Ok(())
}

impl BadgeApp {
    /// Keeps the copy up to date every few seconds.
    pub(super) fn start_recovery_timer(cx: &mut Context<Self>) {
        cx.spawn(async move |this: WeakEntity<Self>, cx| {
            loop {
                cx.background_executor().timer(INTERVAL).await;
                if this.update(cx, |t, _| t.keep_recovery_copy()).is_err() {
                    break;
                }
            }
        })
        .detach();
    }

    /// Writes the copy if the work changed since, or removes it once there's
    /// nothing to keep. A copy from before that's offered is another file,
    /// so it's kept meanwhile.
    pub(super) fn keep_recovery_copy(&mut self) {
        let Some(path) = self.recovery_path.clone() else {
            return;
        };
        if !self.is_dirty() {
            if self.recovery_written.take().is_some() {
                let _ = std::fs::remove_file(&path);
            }
            return;
        }
        let current = self
            .recovery_written
            .as_ref()
            .is_some_and(|(project, file)| project == &self.project && file == &self.path);
        if current {
            return;
        }
        match write(&path, &self.project, self.path.as_deref()) {
            Ok(()) => self.recovery_written = Some((self.project.clone(), self.path.clone())),
            Err(e) => log::warn!("couldn't keep a copy of the unsaved work in {}: {e:#}", path.display()),
        }
    }

    /// The banner's Recover: the work as it was, unsaved (saving it is up to
    /// you), in place of the current project (saved or asked about first).
    pub(super) fn recover_unsaved_work(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ask_to_save_first(window, cx, |t, _, cx| {
            let Some(recovered) = t.recovered.take() else {
                return;
            };
            t.forget_recovered(&recovered.path);
            t.replace_project(recovered.project, recovered.file, cx);
            // Unsaved until saved.
            t.saved = None;
            t.set_status("Recovered the unsaved work; save it to keep it", Severity::Success);
            t.keep_recovery_copy();
            cx.notify();
        });
    }

    /// The banner's Discard.
    pub(super) fn discard_recovered_work(&mut self, cx: &mut Context<Self>) {
        if let Some(recovered) = self.recovered.take() {
            self.forget_recovered(&recovered.path);
        }
        self.keep_recovery_copy();
        cx.notify();
    }

    /// Removes the copy a recovered work came from; if it was this app's
    /// own, the next update writes it again.
    fn forget_recovered(&mut self, path: &Path) {
        if self.recovery_path.as_deref() == Some(path) {
            self.recovery_written = None;
        }
        remove(path);
    }

    /// The banner offering back work from when the app last closed without
    /// it being saved.
    pub(super) fn render_recovery_banner(&self, cx: &mut Context<Self>) -> Option<impl IntoElement + use<>> {
        let recovered = self.recovered.as_ref()?;
        let what = recovered.file.as_deref().map_or_else(
            || "a new project was saved".to_string(),
            |f| format!("changes to {} were saved", file_name(f)),
        );
        let text = SharedString::from(format!(
            "{} closed before {what} ({}).",
            env!("BRAND_NAME"),
            ago(recovered.when)
        ));
        Some(
            div()
                .flex()
                .flex_row()
                .items_center()
                .gap_2()
                .px_5()
                .py_2()
                .flex_none()
                .bg(theme::accent_soft())
                .border_b_1()
                .border_color(theme::border())
                .text_body()
                // One line, cut to the window's width; the whole text on
                // hover.
                .child(
                    div()
                        .id("recovery-summary")
                        .flex_1()
                        .min_w_0()
                        .truncate()
                        .child(text.clone())
                        .tooltip(move |_, cx| crate::widgets::tooltip(text.clone(), cx)),
                )
                .child(
                    button("recover-work", "Recover")
                        .primary()
                        .compact()
                        .tooltip("Open the unsaved work, in place of the current project")
                        .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.recover_unsaved_work(w, cx))),
                )
                .child(
                    button("discard-recovered", "Discard")
                        .compact()
                        .tooltip("Throw the unsaved work away")
                        .on_click(cx.listener(|t, _: &ClickEvent, _, cx| t.discard_recovered_work(cx))),
                ),
        )
    }
}

/// "2 min ago" and the like.
fn ago(when: SystemTime) -> String {
    let secs = when.elapsed().map_or(0, |d| d.as_secs());
    match secs {
        0..60 => "just now".into(),
        60..3600 => format!("{} min ago", secs / 60),
        3600..86400 => format!("{} h ago", secs / 3600),
        _ => format!("{} days ago", secs / 86400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_copy_is_in_use_while_its_app_holds_the_lock() {
        let dir = std::env::temp_dir().join(format!("lbs-recovery-lock-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let copy = dir.join("1-2.json");
        std::fs::write(&copy, "{}").unwrap();
        // No lock file: an app from before, or one that closed.
        assert!(!in_use(&copy));
        let lock = std::fs::File::create(copy.with_extension("lock")).unwrap();
        lock.try_lock().unwrap();
        assert!(in_use(&copy));
        drop(lock);
        assert!(!in_use(&copy));
        remove(&copy);
        assert!(!copy.exists() && !copy.with_extension("lock").exists());
        let _ = std::fs::remove_dir_all(dir);
    }
}
