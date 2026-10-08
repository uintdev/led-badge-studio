//! Crash recovery. While there's unsaved work that nothing else keeps, a
//! copy of it is kept in the app's data folder; if the app closes without
//! the work being saved or discarded (a crash, being forced to quit, the
//! computer shutting down), the copy is offered back when it next starts.
//!

use std::time::{Duration, SystemTime};

use super::files::file_name;
use super::*;

const FILE_NAME: &str = "unsaved-work.json";

/// How often the copy is brought up to date.
const INTERVAL: Duration = Duration::from_secs(2);

/// Unsaved work found when the app started.
pub(super) struct Recovered {
    pub project: Project,
    /// The project's file, if it had one.
    pub file: Option<PathBuf>,
    pub when: SystemTime,
}

/// Where the copy is kept; `None` in tests, which set their own.
pub(super) fn default_path() -> Option<PathBuf> {
    if cfg!(test) {
        return None;
    }
    use badge_formats::dirs;
    dirs::data_base().map(|base| dirs::user_file(&base, FILE_NAME))
}

/// Reads the copy at `path`; `None` if there's none (or it can't be read).
pub(super) fn read(path: &Path) -> Option<Recovered> {
    let text = std::fs::read_to_string(path).ok()?;
    let when = std::fs::metadata(path).and_then(|m| m.modified()).ok()?;
    let read = || -> anyhow::Result<Recovered> {
        let value: serde_json::Value = serde_json::from_str(&text)?;
        let project = badge_formats::project_from_json(&value["project"].to_string())?;
        let file = value["file"].as_str().map(PathBuf::from);
        Ok(Recovered { project, file, when })
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
    // Written aside, then moved over the old copy, so a crash while writing
    // leaves the old one.
    let tmp = path.with_extension("json.tmp");
    std::fs::write(&tmp, serde_json::to_string(&value)?)?;
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
    /// nothing to keep. Left alone while a copy from before is offered,
    /// which would be lost.
    pub(super) fn keep_recovery_copy(&mut self) {
        let Some(path) = self.recovery_path.clone() else {
            return;
        };
        if self.recovered.is_some() {
            return;
        }
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
            t.forget_recovery_copy();
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
        self.recovered = None;
        self.forget_recovery_copy();
        self.keep_recovery_copy();
        cx.notify();
    }

    fn forget_recovery_copy(&mut self) {
        self.recovery_written = None;
        if let Some(path) = &self.recovery_path {
            let _ = std::fs::remove_file(path);
        }
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
