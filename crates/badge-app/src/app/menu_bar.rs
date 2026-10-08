//! The macOS menu bar's side of the main window (the menus themselves are
//! made in `app_menu` in main.rs). Only built on macOS: on Linux and Windows
//! the app's own menus are the only ones.

use super::*;

// Actions only the menu bar sends; the others it uses (New, Open, Save,
// Settings, Keyboard Shortcuts…) are the app's usual ones.
actions!(
    menu_bar,
    [
        CloseWindow,
        LoadLastSent,
        CheckProjectFile,
        ClearRecentFiles,
        ShowAbout,
        ShowLicenses,
        RevertToSaved
    ]
);

/// File → Revert To → an earlier version macOS kept of the file.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = menu_bar, no_json)]
pub struct RevertToVersion {
    /// Where the version's contents can be read.
    pub path: PathBuf,
}

/// What the menu bar's File menu lists; the menus are made again when it
/// changes.
#[derive(Clone, PartialEq)]
pub struct FileMenu {
    /// Open Recent: a label and path per file, newest first.
    pub recent: Vec<(String, PathBuf)>,
    /// Revert To's first item, "Last Opened Version" or "Last Saved
    /// Version", and whether there's anything to revert.
    pub revert_label: &'static str,
    pub can_revert: bool,
    /// Revert To's earlier versions: when each was saved, and where its
    /// contents can be read.
    pub versions: Vec<(String, PathBuf)>,
}

/// File → Open Recent → a file.
#[derive(Clone, Debug, PartialEq, gpui::Action)]
#[action(namespace = menu_bar, no_json)]
pub struct OpenRecent {
    pub path: PathBuf,
}

impl BadgeApp {
    /// The main window's handlers for the menu bar's own actions.
    pub(super) fn on_menu_bar_actions(
        d: gpui::Stateful<gpui::Div>,
        cx: &mut Context<Self>,
    ) -> gpui::Stateful<gpui::Div> {
        d.on_action(cx.listener(|t, _: &CloseWindow, w, cx| t.request_close(w, cx)))
            .on_action(cx.listener(|t, _: &LoadLastSent, w, cx| t.load_last_sent(w, cx)))
            .on_action(cx.listener(|t, _: &CheckProjectFile, w, cx| t.check_project_file(w, cx)))
            .on_action(cx.listener(|t, _: &ClearRecentFiles, _, _| t.clear_recent_files()))
            .on_action(cx.listener(|t, a: &OpenRecent, w, cx| t.open_requested(a.path.clone(), w, cx)))
            .on_action(cx.listener(|t, _: &ShowAbout, w, cx| t.open_dialog(Dialog::About, w, cx)))
            .on_action(cx.listener(|t, _: &ShowLicenses, w, cx| t.open_dialog(Dialog::Licenses(0), w, cx)))
            .on_action(cx.listener(|t, _: &RevertToSaved, _, cx| t.revert_to_saved(cx)))
            .on_action(cx.listener(|t, a: &RevertToVersion, w, cx| t.revert_to_version(a.path.clone(), w, cx)))
    }

    /// The menu bar's File menu contents.
    pub fn file_menu(&self) -> FileMenu {
        FileMenu {
            recent: self.recent_menu_entries(),
            revert_label: self.revert_label(),
            can_revert: self.can_revert(),
            versions: self.file_versions.clone(),
        }
    }

    /// Runs a File or application menu action (macOS menu bar) that was
    /// sent while another window, the pop-out preview, was in front; the
    /// main window handles them itself otherwise. Other actions are ignored.
    pub fn run_menu_action(&mut self, action: &dyn gpui::Action, window: &mut Window, cx: &mut Context<Self>) {
        let a = action.as_any();
        if a.is::<NewProject>() {
            self.new_project(&NewProject, window, cx);
        } else if a.is::<OpenProject>() {
            self.open_project(&OpenProject, window, cx);
        } else if let Some(open) = a.downcast_ref::<OpenRecent>() {
            self.open_requested(open.path.clone(), window, cx);
        } else if a.is::<SaveProject>() {
            self.save_project(&SaveProject, window, cx);
        } else if a.is::<SaveProjectAs>() {
            self.save_project_as(&SaveProjectAs, window, cx);
        } else if a.is::<LoadLastSent>() {
            self.load_last_sent(window, cx);
        } else if a.is::<CheckProjectFile>() {
            self.check_project_file(window, cx);
        } else if a.is::<ClearRecentFiles>() {
            self.clear_recent_files();
        } else if a.is::<ShowAbout>() {
            self.open_dialog(Dialog::About, window, cx);
        } else if a.is::<ShowSettings>() {
            self.open_dialog(Dialog::Settings, window, cx);
        } else if a.is::<ShowShortcuts>() {
            self.open_dialog(Dialog::Shortcuts, window, cx);
        } else if a.is::<ShowLicenses>() {
            self.open_dialog(Dialog::Licenses(0), window, cx);
        } else if a.is::<RevertToSaved>() {
            self.revert_to_saved(cx);
        } else if let Some(revert) = a.downcast_ref::<RevertToVersion>() {
            self.revert_to_version(revert.path.clone(), window, cx);
        }
    }

    /// File → Open Recent for the macOS menu bar: a label and path per file,
    /// newest first. The label is the file name, with as much of its folder
    /// as tells it apart from another file of the same name.
    pub fn recent_menu_entries(&self) -> Vec<(String, PathBuf)> {
        let files = &self.settings.recent_files;
        let labels = files::recent_labels(files);
        labels
            .iter()
            .zip(files)
            .map(|((name, folder), path)| {
                let shared = labels.iter().filter(|(n, _)| n == name).count() > 1;
                let label = if shared {
                    format!("{name} ({folder})")
                } else {
                    name.clone()
                };
                (label, path.clone())
            })
            .collect()
    }
}
