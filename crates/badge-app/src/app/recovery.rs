//! Crash recovery. While there's unsaved work that nothing else keeps, a
//! copy of it is kept in the app's data folder; if the app closes without
//! the work being saved or discarded (a crash, being forced to quit, the
//! computer shutting down), the copy is offered back when it next starts.
//!
//! Each running copy of the app keeps its own file, next to a lock file it
//! holds while it runs, so one copy never offers (or deletes) the unsaved
//! work of another that's still running. The copy is written on a thread
//! of its own: it waits until the copy is on the disk, which can take a
//! moment, and the window mustn't.

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::SyncSender;
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
        let own = || -> std::io::Result<(PathBuf, std::fs::File)> {
            std::fs::create_dir_all(&dir)?;
            let started = SystemTime::now()
                .duration_since(SystemTime::UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis();
            // A lock file no other app has: the process id isn't enough on
            // its own (each Flatpak sandbox counts its own from 1).
            for n in 0..100 {
                let id = format!("{}-{started}-{n}", std::process::id());
                let lock_path = dir.join(format!("{id}.lock"));
                let lock = match std::fs::OpenOptions::new()
                    .write(true)
                    .create_new(true)
                    .open(&lock_path)
                {
                    Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => continue,
                    opened => opened?,
                };
                // Nothing else looks at a new lock file (`clean_up` waits a
                // minute), so it fails only where locks don't work.
                if let Err(e) = lock.try_lock() {
                    let _ = std::fs::remove_file(&lock_path);
                    return Err(e.into());
                }
                return Ok((dir.join(format!("{id}.json")), lock));
            }
            Err(std::io::Error::other("no free name for a lock file"))
        };
        own()
            .inspect_err(|e| log::warn!("no copies of unsaved work will be kept in {}: {e}", dir.display()))
            .ok()
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
    clean_up(&dir);
    let modified = |p: &Path| std::fs::metadata(p).and_then(|m| m.modified()).ok();
    let mut copies: Vec<(SystemTime, PathBuf)> = std::fs::read_dir(&dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|e| e.path())
        .filter(|p| p.extension().is_some_and(|x| x == "json") && Some(p) != own.as_ref() && !in_use(p))
        .chain(dirs::data_base().map(|b| dirs::user_file(&b, LEGACY_FILE_NAME)))
        .filter_map(|p| Some((modified(&p)?, p)))
        .collect();
    // Only the newest that can be read is opened.
    copies.sort_by_key(|(when, _)| std::cmp::Reverse(*when));
    copies.into_iter().find_map(|(_, p)| read(&p))
}

/// Removes what apps that are gone left behind besides copies: lock files
/// of ones that closed with nothing to keep, and half-written copies of
/// ones that crashed while writing. Only files a minute old or older, so
/// an app starting at the same moment keeps its new lock file.
fn clean_up(dir: &Path) {
    let old = |p: &Path| {
        std::fs::metadata(p)
            .and_then(|m| m.modified())
            .is_ok_and(|t| t.elapsed().is_ok_and(|age| age >= Duration::from_secs(60)))
    };
    for entry in std::fs::read_dir(dir).into_iter().flatten().flatten() {
        let path = entry.path();
        let name = path.file_name().and_then(|n| n.to_str()).unwrap_or_default();
        // The copy each leftover belongs to.
        let copy = if let Some(id) = name.strip_suffix(".lock") {
            dir.join(format!("{id}.json"))
        } else if let Some(id) = name.strip_suffix(".json.tmp") {
            dir.join(format!("{id}.json"))
        } else {
            continue;
        };
        let lone_lock = name.ends_with(".lock") && !copy.exists();
        if (lone_lock || name.ends_with(".tmp")) && old(&path) && !in_use(&copy) {
            let _ = std::fs::remove_file(&path);
        }
    }
}

/// Whether the app that keeps the copy at `path` is still running (holds
/// its lock).
fn in_use(path: &Path) -> bool {
    // Opened for writing: on NFS, locks are made with ones that need it. A
    // folder where locks fail can't have copies of running apps (they
    // couldn't lock their own), so only a lock that's held counts.
    std::fs::OpenOptions::new()
        .write(true)
        .open(path.with_extension("lock"))
        .is_ok_and(|f| matches!(f.try_lock(), Err(std::fs::TryLockError::WouldBlock)))
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

/// The copy: the project as in a project file, and where its file is.
fn to_json(project: &Project, file: Option<&Path>) -> anyhow::Result<Vec<u8>> {
    let project: serde_json::Value = serde_json::from_str(&badge_formats::project_to_json(project)?)?;
    let value = serde_json::json!({
        "file": file.map(|f| f.to_string_lossy()),
        "project": project,
    });
    Ok(serde_json::to_vec(&value)?)
}

/// A change to this app's copy.
enum Change {
    Write(PathBuf, Vec<u8>),
    Remove(PathBuf),
}

/// A change for the thread that makes them (or none), and whom to tell,
/// once it's made, whether every change since the last telling worked.
type Job = (Option<Change>, Option<SyncSender<bool>>);

/// A write failed since `keep_recovery_copy` last looked: it writes again.
static WRITE_FAILED: AtomicBool = AtomicBool::new(false);

/// Makes jobs' changes in order, keeping count of failures between tellings.
struct Worker {
    ok: bool,
}

impl Worker {
    fn run(&mut self, (change, done): Job) {
        if let Some(change) = change {
            self.ok &= apply(change);
        }
        if let Some(done) = done {
            let _ = done.send(self.ok);
            self.ok = true;
        }
    }
}

fn apply(change: Change) -> bool {
    match change {
        Change::Write(path, bytes) => {
            // Whole or not at all: a crash or power cut while writing leaves
            // the old copy.
            let written = path
                .parent()
                .map_or(Ok(()), std::fs::create_dir_all)
                .and_then(|()| badge_core::files::replace_file(&path, &bytes));
            if let Err(e) = &written {
                log::warn!("couldn't keep a copy of the unsaved work in {}: {e}", path.display());
                WRITE_FAILED.store(true, Ordering::Relaxed);
            }
            written.is_ok()
        }
        Change::Remove(path) => {
            !matches!(std::fs::remove_file(&path), Err(e) if e.kind() != std::io::ErrorKind::NotFound)
        }
    }
}

/// Hands a job to the thread (made in place in tests, or if the thread
/// couldn't start).
fn submit(job: Job) {
    thread_local! {
        static HERE: std::cell::RefCell<Worker> = const { std::cell::RefCell::new(Worker { ok: true }) };
    }
    #[cfg(not(test))]
    let job = {
        static JOBS: std::sync::OnceLock<std::sync::mpsc::Sender<Job>> = std::sync::OnceLock::new();
        let jobs = JOBS.get_or_init(|| {
            let (tx, rx) = std::sync::mpsc::channel::<Job>();
            let started = std::thread::Builder::new().name("unsaved-work".into()).spawn(move || {
                let mut worker = Worker { ok: true };
                rx.into_iter().for_each(|job| worker.run(job));
            });
            if let Err(e) = started {
                log::warn!("couldn't start the unsaved-work thread: {e}");
            }
            tx
        });
        match jobs.send(job) {
            Ok(()) => return,
            Err(std::sync::mpsc::SendError(job)) => job,
        }
    };
    HERE.with_borrow_mut(|worker| worker.run(job));
}

/// Waits (briefly) until the changes to this app's copy so far are made;
/// whether they all worked.
fn settle() -> bool {
    let (done, told) = std::sync::mpsc::sync_channel(1);
    submit((None, Some(done)));
    told.recv_timeout(Duration::from_secs(5)).unwrap_or(false)
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
        if WRITE_FAILED.swap(false, Ordering::Relaxed) {
            self.recovery_written = None;
        }
        if !self.is_dirty() {
            if self.recovery_written.take().is_some() {
                submit((Some(Change::Remove(path)), None));
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
        match to_json(&self.project, self.path.as_deref()) {
            Ok(json) => {
                submit((Some(Change::Write(path, json)), None));
                self.recovery_written = Some((self.project.clone(), self.path.clone()));
            }
            Err(e) => log::warn!("couldn't keep a copy of the unsaved work: {e:#}"),
        }
    }

    /// Like `keep_recovery_copy`, and waits until it's on the disk (before
    /// quitting, or removing a copy it takes over from); whether it is.
    pub(super) fn keep_recovery_copy_now(&mut self) -> bool {
        self.keep_recovery_copy();
        settle()
    }

    /// The banner's Recover: the work as it was, unsaved (saving it is up to
    /// you), in place of the current project (saved or asked about first).
    pub(super) fn recover_unsaved_work(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.ask_to_save_first(window, cx, |t, _, cx| {
            let Some(recovered) = t.recovered.take() else {
                return;
            };
            t.replace_project(recovered.project, recovered.file, cx);
            // Unsaved until saved.
            t.saved = None;
            t.set_status("Recovered the unsaved work; save it to keep it", Severity::Success);
            // The copy it came from goes once this app keeps the work in its
            // own; without that, it stays for the next start.
            let kept = t.keep_recovery_copy_now()
                && t.recovery_written
                    .as_ref()
                    .is_some_and(|(project, file)| project == &t.project && file == &t.path);
            if kept && t.recovery_path.as_deref() != Some(recovered.path.as_path()) {
                remove(&recovered.path);
            }
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
            submit((Some(Change::Remove(path.to_path_buf())), None));
        } else {
            remove(path);
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

    #[test]
    fn only_old_leftovers_of_apps_that_are_gone_are_cleaned_up() {
        let dir = std::env::temp_dir().join(format!("lbs-recovery-clean-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let make = |name: &str, age_secs: u64| {
            let path = dir.join(name);
            let file = std::fs::File::create(&path).unwrap();
            file.set_modified(SystemTime::now() - Duration::from_secs(age_secs))
                .unwrap();
            path
        };
        let gone_lock = make("1-1.lock", 120);
        let new_lock = make("2-2.lock", 0);
        let kept_copy_lock = make("3-3.lock", 120);
        let kept_copy = make("3-3.json", 120);
        let gone_tmp = make("4-4.json.tmp", 120);
        let held_tmp = make("5-5.json.tmp", 120);
        let held_lock = make("5-5.lock", 120);
        let lock = std::fs::File::open(&held_lock).unwrap();
        lock.try_lock().unwrap();
        clean_up(&dir);
        assert!(!gone_lock.exists(), "a lone, old lock file");
        assert!(new_lock.exists(), "an app may be starting");
        assert!(
            kept_copy_lock.exists() && kept_copy.exists(),
            "its copy is still offered"
        );
        assert!(!gone_tmp.exists(), "a half-written copy");
        assert!(held_tmp.exists(), "its app is running");
        drop(lock);
        let _ = std::fs::remove_dir_all(dir);
    }

    #[test]
    fn failed_writes_are_told() {
        let dir = std::env::temp_dir().join(format!("lbs-recovery-worker-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        let mut worker = Worker { ok: true };
        let tell = |worker: &mut Worker| {
            let (done, told) = std::sync::mpsc::sync_channel(1);
            worker.run((None, Some(done)));
            told.recv().unwrap()
        };
        let copy = dir.join("1-2-0.json");
        worker.run((Some(Change::Write(copy.clone(), b"{}".to_vec())), None));
        assert!(tell(&mut worker));
        assert_eq!(std::fs::read(&copy).unwrap(), b"{}");
        // A file where its folder should be.
        let blocked = copy.join("x.json");
        worker.run((Some(Change::Write(blocked, b"{}".to_vec())), None));
        worker.run((Some(Change::Remove(copy.clone())), None));
        assert!(!tell(&mut worker), "one of them failed");
        assert!(!copy.exists());
        assert!(tell(&mut worker), "counted afresh");
        let _ = std::fs::remove_dir_all(dir);
    }
}
