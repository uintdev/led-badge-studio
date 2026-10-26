//! The main window: its state, layout and key bindings. Each section of the
//! window has its own module.

mod device;
mod dialogs;
#[cfg(all(target_os = "macos", not(test)))]
mod dock_progress;
mod editor;
mod files;
mod history;
#[cfg(target_os = "linux")]
mod launcher_entry;
#[cfg(target_os = "macos")]
mod menu_bar;
mod menus;
mod message;
#[cfg(all(target_os = "macos", not(test)))]
mod native_document;
#[cfg(all(target_os = "macos", not(test)))]
pub(crate) use native_document::approve_quit;
mod preview_section;
mod recovery;
#[cfg(all(windows, not(test)))]
mod taskbar;
mod toolbar;
mod versions;
#[cfg(test)]
mod window_tests;

#[cfg(target_os = "macos")]
pub use menu_bar::{
    CheckProjectFile, ClearRecentFiles, CloseWindow, FileMenu, LoadLastSent, OpenRecent, RevertToSaved,
    RevertToVersion, ShowAbout, ShowLicenses,
};

use message::font_notice_text;

use std::cell::{Cell, RefCell};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::time::Duration;

use badge_core::{
    AddressMode, Brightness, CardType, FontChoice, Framebuffer, LedColor, MAX_ADDRESS, MESSAGE_COUNT, Mode, Project,
    Source,
};
use badge_io::{DeviceInfo, DeviceKind};
use badge_render::led_look::LedLook;
use badge_render::preview::{self, PreviewParams};
use badge_render::preview_export::Format as ExportFormat;
use badge_render::{FontLibrary, ImportOptions, Renderer};
use futures::StreamExt;
use gpui::prelude::*;
use gpui::{
    App, Bounds, ClickEvent, Context, Entity, ExternalPaths, FocusHandle, Focusable, MouseButton, MouseDownEvent,
    MouseMoveEvent, PathPromptOptions, Pixels, SharedString, Subscription, WeakEntity, Window, actions, deferred, div,
    px,
};

use badge_formats::last_sent::LastSent;

use crate::icons::{Icon, icon};
use crate::led_view::{self, LedStyle};
use crate::settings::Settings;
use crate::text_input::{InputEvent, TextInput};
use crate::theme;
use crate::widgets::{
    self, COMPACT_H, CONTROL_H, MenuItem, TextStyle, button, checkbox, divider, label, menu_item, menu_with_highlight,
    row_label, section, segmented, stepper, subheading, warning_chip,
};

actions!(
    badge,
    [
        NewProject,
        OpenProject,
        SaveProject,
        SaveProjectAs,
        SendToBadge,
        Quit,
        ShowSettings,
        ShowShortcuts
    ]
);
actions!(modal, [ModalCancel, ModalConfirm, FocusNext, FocusPrev]);
actions!(
    menu_keys,
    [MenuUp, MenuDown, MenuActivate, MenuClose, MenuRight, MenuLeft]
);
actions!(edit, [Undo, Redo]);

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
enum Menu {
    File,
    /// File → Open Recent (shown beside the File menu).
    Recent,
    /// File → Revert To (shown beside the File menu).
    Revert,
    Card,
    Brightness,
    Device,
    Address,
    FontFamily,
    Icons,
    MonoTint,
    /// The actions of message `n` (its ⋯ button, or a right-click).
    MessageActions(usize),
}

#[derive(Clone, PartialEq, Eq)]
enum Dialog {
    DeviceAccess,
    Settings,
    /// Every keyboard shortcut.
    Shortcuts,
    /// Preview → Export.
    Export,
    About,
    /// Font and third-party licenses; the index selects the entry shown.
    Licenses(usize),
    /// A message with an OK button.
    Notice {
        title: SharedString,
        detail: SharedString,
        /// Labeled details under `detail` ("Where", "File", …).
        items: Vec<(SharedString, SharedString)>,
        error: bool,
    },
    /// A project file with values the badge doesn't support.
    Repair {
        path: PathBuf,
        fixes: Vec<badge_formats::Fix>,
        /// Opening the file (true), or File → Check Project File (false).
        opening: bool,
    },
    /// Yes/no question; the action runs on confirm.
    Confirm {
        title: SharedString,
        detail: SharedString,
        confirm_label: SharedString,
        cancel_label: SharedString,
        /// A third choice, between Cancel and the confirm button.
        alternate_label: Option<SharedString>,
        destructive: bool,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Tool {
    Draw,
    Erase,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Severity {
    Info,
    Success,
    Error,
}

enum SendState {
    Idle,
    Sending {
        done: usize,
        total: usize,
    },
    /// Briefly shown on the button after a successful upload.
    Sent,
}

enum SendMsg {
    Progress(usize, usize),
    Done(Result<(), (String, bool)>),
}

const EDITOR_CELL: f32 = 14.0;
/// Window width from which the right-hand panel is shown.
const WIDE_LAYOUT: f32 = 1260.;
/// The right-hand panel's width.
const INSPECTOR_W: f32 = 320.;
/// Row of Open Recent in the File menu.
const OPEN_RECENT_ROW: usize = 2;
/// File → Revert To's row in the File menu.
const REVERT_ROW: usize = 5;
const PREVIEW_CELL: f32 = 9.0;

/// Deferred action run when a confirmation dialog is accepted.
type ConfirmAction = Box<dyn FnOnce(&mut BadgeApp, &mut Window, &mut Context<BadgeApp>)>;

pub struct BadgeApp {
    project: Project,
    current: usize,
    renderer: Renderer,
    settings: Settings,
    path: Option<PathBuf>,
    /// The project as last saved or opened; unsaved changes are any
    /// difference from it, so undoing back to it counts as unchanged. `None`
    /// when opened with repairs that aren't written yet.
    saved: Option<Project>,
    /// The project's file as the app last read or wrote it, to notice
    /// another program changing it (see `changed_elsewhere`).
    file_stamp: Option<files::FileStamp>,
    /// Told (in the status bar) that the file was changed elsewhere while
    /// there were unsaved changes here; not again until it's read or saved.
    told_changed_elsewhere: bool,
    /// The earlier versions of the project's file (File → Revert To), newest
    /// first: when each was saved, and where its contents can be read.
    file_versions: Vec<(String, PathBuf)>,
    /// Saved since opened, for File → Revert To's wording.
    saved_here: bool,
    /// Windows: the recent files last put in the taskbar's Jump List.
    #[cfg(all(windows, not(test)))]
    jump_list: Option<Vec<PathBuf>>,
    /// Where the app keeps earlier versions itself (versions.rs).
    #[cfg_attr(all(target_os = "macos", not(test)), allow(dead_code))]
    versions_dir: Option<PathBuf>,
    /// Where a copy of unsaved work is kept (recovery.rs); `None` in tests.
    recovery_path: Option<PathBuf>,
    /// What the copy holds: the project and its file.
    recovery_written: Option<(Project, Option<PathBuf>)>,
    /// Unsaved work found at the start, offered back.
    recovered: Option<recovery::Recovered>,
    input: Entity<TextInput>,
    _subs: Vec<Subscription>,
    devices: Vec<DeviceInfo>,
    device: Option<String>,
    /// The device was picked in the device menu in this session (not just
    /// remembered from the last send or chosen by the app).
    device_chosen: bool,
    address_kind: u8,
    address: u16,
    menu: Option<Menu>,
    dialog: Option<Dialog>,
    /// System file dialogs (Open, Save…) waiting for an answer; the preview
    /// pauses meanwhile (see `during_system_dialog`).
    system_dialogs: usize,
    /// What to run when a `Dialog::Confirm` is accepted.
    pending_confirm: Option<ConfirmAction>,
    /// What to run when a `Dialog::Confirm` is canceled.
    pending_cancel: Option<ConfirmAction>,
    /// What to run when a `Dialog::Confirm`'s third choice is chosen.
    pending_alternate: Option<ConfirmAction>,
    modal_focus: FocusHandle,
    /// Dialog buttons, in order (Tab moves between them).
    dialog_buttons: Vec<FocusHandle>,
    /// The dialog whose default button was last focused.
    focused_dialog: Option<Dialog>,
    /// Keyboard focus while a menu is open.
    menu_focus: FocusHandle,
    /// Where the keyboard goes back to when a menu or dialog closes.
    return_focus: Option<FocusHandle>,
    /// Where each dropdown's button was last drawn.
    menu_anchors: Rc<RefCell<HashMap<Menu, Bounds<Pixels>>>>,
    viewport_h: f32,
    /// The preview frame and its width when Export was opened.
    export_frame: Option<(Framebuffer, usize)>,
    /// The message list's rows.
    message_focus: Vec<FocusHandle>,
    /// The keyboard position in the open menu.
    menu_highlight: Option<usize>,
    /// The open menu's items as last drawn: (disabled, opens a submenu,
    /// action).
    menu_actions: std::cell::RefCell<Vec<(bool, bool, widgets::WindowAction)>>,
    tool: Tool,
    paint_color: LedColor,
    /// Color being painted during a drag, and the last cell painted.
    drag: Option<(LedColor, (usize, usize))>,
    editor_bounds: Rc<Cell<Option<Bounds<Pixels>>>>,
    /// The room the preview has, measured while painting.
    preview_room: Rc<Cell<Option<f32>>>,
    preview_step: usize,
    playing: bool,
    send: SendState,
    status: Option<(SharedString, Severity)>,
    /// The selected badge was found but this user can't write to it.
    access_missing: bool,
    setting_up_access: bool,
    /// The udev rule's state, checked in the background since the Badge
    /// Access dialog opened (in a Flatpak that runs commands on the host).
    udev_state: Option<badge_io::UdevRuleState>,
    udev_checking: bool,
    /// Whether the rule can be changed from here.
    can_set_up_access: bool,
    /// The command block whose Copy button was just clicked, and a counter
    /// so only the latest click clears it.
    copied: Option<&'static str>,
    copied_gen: u64,
    /// The repaired project waiting for a choice in `Dialog::Repair`.
    pending_repair: Option<badge_formats::Loaded>,
    /// The message and frame the preview last showed.
    last_preview: Option<(usize, Framebuffer)>,
    preview_window: Option<gpui::WindowHandle<crate::preview_window::PreviewWindow>>,
    fonts_refreshing: bool,
    /// The title last given to the window, and the (dirty, path) it shows.
    window_title: SharedString,
    title_inputs: Option<(bool, Option<PathBuf>)>,
    /// Bumped on each threshold change; only the latest schedules a save.
    threshold_save_gen: u64,
    last_sent_offer: Option<LastSent>,
    /// The missing-font banner was dismissed (until another project opens).
    font_notice_dismissed: bool,
    history: History,
    /// What kind of edit the next history entry is, for grouping.
    edit_hint: Option<EditKind>,
    stroke_id: u64,
    /// The last imported image, so threshold/invert/fit can be re-applied.
    import_source: Option<ImportSource>,
    /// Counts images loaded, and decodes for new import options, which
    /// run in the background: a result that a later one has replaced is
    /// dropped.
    import_gen: u64,
    /// The message an image being loaded goes into: it follows the message
    /// when it's moved, and is cleared when the message is replaced (New,
    /// Open, Undo, a paste), which drops the image.
    import_into: Option<usize>,
    decode_gen: u64,
    /// Message the preview is playing (differs from `current` in "all ticked" mode).
    preview_msg: usize,
    /// Third-party notices, parsed when the Licenses view first opens.
    third_party: Option<Rc<Vec<badge_render::licenses::Library>>>,
    focus: FocusHandle,
    /// The system's light (false) or dark (true) setting; `None` when it
    /// doesn't say.
    system_dark: Option<bool>,
}

#[derive(Clone)]
struct Snapshot {
    project: Project,
    current: usize,
}

/// Snapshot undo history: a change is any difference from `baseline`, so
/// every kind of edit is covered.
struct History {
    undo: Vec<Snapshot>,
    redo: Vec<Snapshot>,
    baseline: Snapshot,
    last_kind: Option<EditKind>,
    last_at: std::time::Instant,
}

const HISTORY_LIMIT: usize = 200;

/// Edits of the same kind in quick succession undo as one step.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
enum EditKind {
    Text,
    Stroke(u64),
    Slider,
    Other,
}

struct ImportSource {
    msg: usize,
    bytes: Vec<u8>,
    result: Framebuffer,
    /// The decoded, fitted image, so moving the threshold slider only redoes
    /// the thresholding.
    decoded: Option<(CardType, bool, badge_render::image_import::DecodedImage)>,
}

impl Focusable for BadgeApp {
    fn focus_handle(&self, _: &App) -> FocusHandle {
        self.focus.clone()
    }
}

impl BadgeApp {
    pub fn new(open: Option<PathBuf>, window: &mut Window, cx: &mut Context<Self>) -> Self {
        let (settings, damaged_settings) = Settings::load();
        let renderer = Renderer::new(FontLibrary::discover());
        let input = cx.new(|cx| TextInput::new("Type your message…  (~0 … ~f insert icons)", cx));
        let subs = vec![
            cx.subscribe_in(&input, window, Self::on_input_event),
            // Fonts or badge access may have changed while the app was in the
            // background.
            cx.observe_window_activation(window, |this, window, cx| {
                if window.is_window_active() {
                    this.refresh_fonts(cx);
                    this.update_access(cx);
                }
            }),
        ];
        let mut project = Project::new(settings.card);
        project.normalize();
        if !project.card.is_color() {
            project.led_color = Some(settings.mono_tint);
        }
        let history = History {
            undo: Vec::new(),
            redo: Vec::new(),
            baseline: Snapshot {
                project: project.clone(),
                current: 0,
            },
            last_kind: None,
            last_at: std::time::Instant::now(),
        };

        let saved = Some(project.clone());
        let mut app = BadgeApp {
            project,
            current: 0,
            renderer,
            device: settings.last_device.clone(),
            device_chosen: false,
            settings,
            path: None,
            saved,
            input,
            _subs: subs,
            devices: Vec::new(),
            address_kind: 0,
            address: 1,
            menu: None,
            dialog: None,
            system_dialogs: 0,
            pending_confirm: None,
            pending_cancel: None,
            pending_alternate: None,
            modal_focus: cx.focus_handle(),
            dialog_buttons: (0..4).map(|i| cx.focus_handle().tab_index(i).tab_stop(true)).collect(),
            focused_dialog: None,
            return_focus: None,
            menu_anchors: Rc::default(),
            viewport_h: 0.,
            export_frame: None,
            message_focus: (0..badge_core::MESSAGE_COUNT).map(|_| cx.focus_handle()).collect(),
            menu_focus: cx.focus_handle(),
            menu_highlight: None,
            menu_actions: Default::default(),
            tool: Tool::Draw,
            paint_color: LedColor::RED,
            drag: None,
            editor_bounds: Rc::new(Cell::new(None)),
            preview_room: Rc::new(Cell::new(None)),
            preview_step: 0,
            playing: true,
            send: SendState::Idle,
            status: None,
            file_stamp: None,
            told_changed_elsewhere: false,
            file_versions: Vec::new(),
            saved_here: false,
            versions_dir: versions::default_dir(),
            #[cfg(all(windows, not(test)))]
            jump_list: None,
            recovery_path: recovery::default_path(),
            recovery_written: None,
            recovered: recovery::find_left_behind(),
            last_sent_offer: None,
            font_notice_dismissed: false,
            history,
            edit_hint: None,
            stroke_id: 0,
            import_source: None,
            import_gen: 0,
            import_into: None,
            decode_gen: 0,
            preview_msg: 0,
            third_party: None,
            access_missing: false,
            setting_up_access: false,
            udev_state: None,
            udev_checking: false,
            can_set_up_access: false,
            copied: None,
            copied_gen: 0,
            fonts_refreshing: false,
            preview_window: None,
            last_preview: None,
            pending_repair: None,
            window_title: SharedString::default(),
            title_inputs: None,
            threshold_save_gen: 0,
            focus: cx.focus_handle(),
            system_dark: (!cfg!(target_os = "linux")).then(|| crate::system_theme::is_dark(window.appearance())),
        };
        if cfg!(target_os = "linux") {
            // In tests the system setting stays unknown (the app is dark).
            #[cfg(all(target_os = "linux", not(test)))]
            {
                let this = cx.entity().downgrade();
                crate::system_theme::watch_linux(cx, move |dark, cx| {
                    this.update(cx, |t, cx| {
                        t.system_dark = dark;
                        t.apply_theme(None, cx);
                    })
                    .ok();
                });
            }
        } else {
            app._subs.push(cx.observe_window_appearance(window, |t, window, cx| {
                t.system_dark = Some(crate::system_theme::is_dark(window.appearance()));
                t.apply_theme(Some(window), cx);
            }));
        }
        #[cfg(all(windows, not(test)))]
        taskbar::set_window(window);
        // Coming back to the window: the project's file may have been changed
        // by another program meanwhile.
        app._subs.push(cx.observe_window_activation(window, |t, window, cx| {
            if window.is_window_active() {
                t.check_file_changed_elsewhere(cx);
            }
        }));
        // However the app ends, the unsaved-work copy is settled and the lock
        // file removed: when the window goes (it quits the app), or when the
        // app quits with the window still open (its own Quit, or on macOS
        // from the Dock or by logging out). Doing it twice changes nothing.
        cx.on_release(|t, _| t.settle_recovery_before_quitting()).detach();
        app._subs.push(cx.on_app_quit(|t, _| {
            t.settle_recovery_before_quitting();
            async {}
        }));
        app.apply_theme(Some(window), cx);
        app.rerender_all();
        // After redrawing: pixels redrawn from text aren't a change.
        app.saved = Some(app.project.clone());
        app.history.baseline.project = app.project.clone();
        app.sync_input(cx);
        if let Some(path) = open {
            app.open_requested(path, window, cx);
        } else if app.settings.reopen_last
            && let Some(last) = app.settings.recent_files.first().cloned()
        {
            if last.is_file() {
                app.open_requested(last, window, cx);
            } else {
                let name = last
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                app.set_status(
                    format!("Your last project ({name}) couldn't be found, so a new project was started."),
                    Severity::Info,
                );
                app.settings.recent_files.retain(|f| f != &last);
                app.settings.save();
            }
        }
        app.reset_history();
        if let Some(kept) = damaged_settings {
            app.notice(
                "Your settings were reset",
                "The settings file couldn't be read (it may have been damaged or edited by hand), \
                 so the app started with its default settings. The old file was kept.",
                vec![("Old settings", kept.display().to_string())],
                false,
                window,
                cx,
            );
        }
        // Nothing visibly focused at the start; shortcuts still work.
        if app.dialog.is_none() {
            window.focus(&app.focus, cx);
        }

        if !cfg!(test) {
            // Before the preview's timer, which redraws less often when
            // rendering on the CPU.
            crate::gpu::note_renderer(window);
            Self::start_device_watch(cx);
            Self::start_preview_timer(window, cx);
            Self::start_recovery_timer(cx);
        }
        app
    }

    /// Replaces the whole project (New, Open, Load What Was Last Sent), with a
    /// fresh undo history. Its badge type becomes the default for new ones.
    fn replace_project(&mut self, mut project: Project, path: Option<PathBuf>, cx: &mut Context<Self>) {
        project.normalize();
        self.project = project;
        self.fill_led_color();
        self.path = path;
        self.note_file_stamp();
        self.saved_here = false;
        self.refresh_file_versions();
        self.current = 0;
        self.preview_step = 0;
        self.rerender_all();
        // After redrawing: pixels redrawn from text aren't a change.
        self.saved = Some(self.project.clone());
        self.font_notice_dismissed = false;
        self.sync_input(cx);
        self.reset_history();
        self.settings.card = self.project.card;
        self.keep_recovery_copy();
        cx.notify();
    }

    fn msg(&self) -> &badge_core::Message {
        &self.project.messages[self.current]
    }

    fn msg_mut(&mut self) -> &mut badge_core::Message {
        &mut self.project.messages[self.current]
    }

    fn rerender(&mut self, i: usize) {
        let card = self.project.card;
        let m = &self.project.messages[i];
        if m.source == Source::Text {
            let fb = self.renderer.render(m, card);
            self.project.messages[i].bitmap = fb;
        }
    }

    fn rerender_all(&mut self) {
        for i in 0..MESSAGE_COUNT {
            self.rerender(i);
        }
    }

    /// Call after changing anything in the current message.
    fn changed(&mut self, cx: &mut Context<Self>) {
        self.rerender(self.current);
        self.preview_step = 0;
        cx.notify();
    }

    fn set_card(&mut self, card: CardType, cx: &mut Context<Self>) {
        if card == self.project.card {
            return;
        }
        if let Some(why) = self.card_overflow(card) {
            self.set_status(why, Severity::Error);
            cx.notify();
            return;
        }
        let was_color = self.project.card.is_color();
        self.project.set_card(card);
        self.fill_led_color();
        if was_color && !card.is_color() {
            for m in &mut self.project.messages {
                m.bitmap.recolor(LedColor::RED);
            }
        }
        self.settings.card = card;
        self.settings.save();
        self.set_status(
            format!(
                "Badge type set to {}. If the badge then shows misaligned or garbled pixels, the type doesn't match it.",
                card.label()
            ),
            Severity::Info,
        );
        self.rerender_all();
        self.sync_input(cx);
        cx.notify();
    }

    /// Why the messages don't fit as `card` (re-drawn for its height, text
    /// can get wider): a message would reach the widest a message can be, or
    /// the ticked messages would need more than the badge's memory (and more
    /// than now, so a project already over it can still switch).
    fn card_overflow(&mut self, card: CardType) -> Option<String> {
        let mut trial = self.project.clone();
        trial.set_card(card);
        for (i, m) in trial.messages.iter_mut().enumerate() {
            if m.source != Source::Text {
                continue;
            }
            m.bitmap = self.renderer.render(m, card);
            let before = self.project.messages[i].bitmap.used_width();
            if m.bitmap.used_width() >= badge_core::MAX_WIDTH && before < badge_core::MAX_WIDTH {
                return Some(format!(
                    "Can't switch to {}: message {} would be wider than a message can be ({} columns). \
                     Shorten it first.",
                    card.label(),
                    i + 1,
                    badge_core::MAX_WIDTH
                ));
            }
        }
        let used = trial.used_columns();
        let cap = badge_core::protocol::MAX_TOTAL_COLUMNS;
        (used > cap && used > self.project.used_columns()).then(|| {
            format!(
                "Can't switch to {}: the ticked messages would need {} columns, more than the {} the badge \
                 holds. Untick or shorten messages first.",
                card.label(),
                used * 8,
                cap * 8
            )
        })
    }

    fn set_status(&mut self, text: impl Into<SharedString>, sev: Severity) {
        self.status = Some((text.into(), sev));
    }

    /// Whether the app shows its dark colors: as chosen in Settings, or as
    /// the system is set (dark when it doesn't say).
    fn wants_dark(&self) -> bool {
        match self.settings.appearance {
            crate::settings::Appearance::Dark => true,
            crate::settings::Appearance::Light => false,
            crate::settings::Appearance::System => self.system_dark.unwrap_or(true),
        }
    }

    /// Applies the light or dark colors to every window, and to the native
    /// window frames where the system draws them.
    fn apply_theme(&mut self, window: Option<&Window>, cx: &mut Context<Self>) {
        use crate::settings::Appearance;
        let dark = self.wants_dark();
        theme::set_dark(dark);
        crate::system_theme::set_app_appearance(match self.settings.appearance {
            Appearance::System => None,
            Appearance::Light => Some(false),
            Appearance::Dark => Some(true),
        });
        if let Some(window) = window {
            crate::system_theme::set_native_frame(window, dark);
        }
        self.input.update(cx, |_, cx| cx.notify());
        cx.refresh_windows();
    }

    fn is_dirty(&self) -> bool {
        self.saved.as_ref() != Some(&self.project)
    }

    fn title(&self) -> String {
        let name = self
            .path
            .as_ref()
            .and_then(|p| p.file_name())
            .map_or_else(|| "Untitled".into(), |n| n.to_string_lossy().into_owned());
        // On macOS the document's title is shown by macOS (see
        // native_document.rs): the file name, its icon, and "Edited".
        if cfg!(target_os = "macos") {
            return name;
        }
        format!(
            "{}{} - {}",
            if self.is_dirty() { "• " } else { "" },
            name,
            env!("BRAND_NAME")
        )
    }
}

/// A label and its control, kept together when rows wrap.
fn labeled(text: &'static str, control: impl IntoElement) -> gpui::Div {
    div()
        .flex()
        .flex_row()
        .flex_none()
        .items_center()
        .gap_2()
        .child(label(text))
        .child(control)
}

/// Space between one label-and-control set and the next in a row.
const SET_GAP: f32 = 24.;

fn rgb_black() -> gpui::Rgba {
    gpui::rgb(0x050506)
}

impl Render for BadgeApp {
    fn render(&mut self, window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        self.commit_history();
        // Runs every animation frame: only update the title when it changes.
        let dirty = self.is_dirty();
        if self
            .title_inputs
            .as_ref()
            .is_none_or(|(was_dirty, path)| *was_dirty != dirty || *path != self.path)
        {
            self.title_inputs = Some((dirty, self.path.clone()));
            self.window_title = self.title().into();
            #[cfg(not(all(target_os = "macos", not(test))))]
            window.set_window_title(&self.window_title);
        }
        #[cfg(all(target_os = "macos", not(test)))]
        native_document::sync(self, window, cx);
        // The taskbar's Jump List shows the recent files.
        #[cfg(all(windows, not(test)))]
        if self.jump_list.as_ref() != Some(&self.settings.recent_files) {
            self.jump_list = Some(self.settings.recent_files.clone());
            taskbar::set_recent_files(&self.settings.recent_files);
        }
        let title = self.window_title.clone();
        let content = self.render_content(window, cx);
        let this = cx.entity().downgrade();
        crate::chrome::decorate(content, title, window, move |window, cx| {
            this.update(cx, |t, cx| t.request_close(window, cx)).ok();
        })
    }
}

impl BadgeApp {
    /// Closes the window, asking first if there are unsaved changes.
    fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.before_quitting(window, cx, |t, window, cx| {
            t.saved = Some(t.project.clone());
            #[cfg(all(target_os = "linux", not(test)))]
            launcher_entry::clear_before_quitting();
            // Closing this window quits the app. On Windows, quitting
            // straight away: removing the window first has GPUI log errors
            // about it as Windows deactivates it.
            if cfg!(windows) {
                cx.quit();
            } else {
                window.remove_window();
            }
        });
    }

    /// Quits, asking first if there are unsaved changes.
    fn quit(&mut self, _: &Quit, window: &mut Window, cx: &mut Context<Self>) {
        self.before_quitting(window, cx, |t, _, cx| {
            t.saved = Some(t.project.clone());
            #[cfg(all(target_os = "linux", not(test)))]
            launcher_entry::clear_before_quitting();
            #[cfg(all(target_os = "macos", not(test)))]
            native_document::approve_quit();
            cx.quit();
        });
    }

    /// Runs `then` once a send in progress and unsaved changes are dealt
    /// with. Quitting mid-send cuts the upload off, which leaves the badge
    /// ignoring everything until it's power-cycled, so that's asked first.
    fn before_quitting(
        &mut self,
        window: &mut Window,
        cx: &mut Context<Self>,
        then: impl FnOnce(&mut Self, &mut Window, &mut Context<Self>) + 'static,
    ) {
        if !self.is_sending() {
            self.ask_to_save_first(window, cx, then);
            return;
        }
        self.ask(
            "A message is still being sent",
            "Quitting now stops the upload partway, and the badge then ignores everything until it's \
             unplugged and plugged back in.",
            "Quit Anyway",
            true,
            window,
            cx,
            move |t, window, cx| t.ask_to_save_first(window, cx, then),
        );
    }

    fn is_sending(&self) -> bool {
        matches!(self.send, SendState::Sending { .. })
    }

    /// Hook for the window manager's close request (Alt+F4, taskbar…).
    pub fn install_close_guard(window: &mut Window, cx: &mut Context<Self>) {
        let this = cx.entity().downgrade();
        window.on_window_should_close(cx, move |window, cx| {
            let Some(this) = this.upgrade() else {
                return true;
            };
            // On Windows, letting the close go ahead has Windows destroy the
            // window itself, and GPUI then logs errors destroying it again;
            // so there it's always closed the app's way.
            let app = this.read(cx);
            if app.is_dirty() || app.is_sending() || cfg!(windows) {
                this.update(cx, |t, cx| t.request_close(window, cx));
                return false;
            }
            true
        });
    }

    fn render_content(&mut self, window: &mut Window, cx: &mut Context<Self>) -> gpui::Stateful<gpui::Div> {
        let menu_open = self.menu.is_some();
        let dialog = self.dialog.clone();
        self.viewport_h = f32::from(window.viewport_size().height);
        div()
            .id("root")
            .key_context("BadgeApp")
            .track_focus(&self.focus)
            .on_action(cx.listener(Self::new_project))
            .on_action(cx.listener(Self::open_project))
            .on_action(cx.listener(Self::save_project))
            .on_action(cx.listener(Self::save_project_as))
            .on_action(cx.listener(Self::send))
            .on_action(cx.listener(Self::undo))
            .on_action(cx.listener(Self::redo))
            .on_action(cx.listener(Self::quit))
            .on_action(cx.listener(|t, _: &ShowSettings, w, cx| t.open_dialog(Dialog::Settings, w, cx)))
            .on_action(cx.listener(|t, _: &ShowShortcuts, w, cx| t.open_dialog(Dialog::Shortcuts, w, cx)))
            .map(|d| {
                // Actions only the macOS menu bar sends.
                #[cfg(target_os = "macos")]
                let d = Self::on_menu_bar_actions(d, cx);
                d
            })
            .on_action(cx.listener(|_, _: &FocusNext, window, cx| {
                widgets::set_focus_visible(true);
                window.focus_next(cx);
                cx.notify();
            }))
            .on_action(cx.listener(|_, _: &FocusPrev, window, cx| {
                widgets::set_focus_visible(true);
                window.focus_prev(cx);
                cx.notify();
            }))
            // A click anywhere hides the focus ring until the keyboard is used.
            .child(
                gpui::canvas(
                    |_, _, _| {},
                    |_, _, window, _| {
                        window.on_mouse_event(|_: &MouseDownEvent, phase, window, _| {
                            if phase == gpui::DispatchPhase::Capture && widgets::focus_visible() {
                                widgets::set_focus_visible(false);
                                window.refresh();
                            }
                        });
                    },
                )
                .absolute()
                .size_0(),
            )
            .on_drop(cx.listener(|t, paths: &ExternalPaths, w, cx| t.handle_drop(paths.paths(), w, cx)))
            .drag_over::<ExternalPaths>(|s, _, _, _| s.border_2().border_color(theme::accent()))
            .relative()
            .size_full()
            .flex()
            .flex_col()
            .bg(theme::bg())
            .text_color(theme::text())
            .font_family(".SystemUIFont")
            .child(self.render_toolbar(cx))
            .when(self.access_missing && cfg!(target_os = "linux"), |d| {
                d.child(
                    div()
                        .flex()
                        .flex_row()
                        .items_center()
                        .gap_2()
                        .px_5()
                        .py_2()
                        .flex_none()
                        .bg(theme::warning_soft())
                        .border_b_1()
                        .border_color(theme::warning_edge())
                        .text_body()
                        .child(div().flex_1().child(
                            "Badge found, but this account isn't allowed to write to it yet. \
                             This needs a one-time setup with your password.",
                        ))
                        .child(
                            button(
                                "setup-access",
                                if self.setting_up_access {
                                    "Waiting…"
                                } else {
                                    "Enable Badge Access"
                                },
                            )
                            .primary()
                            .compact()
                            .disabled(self.setting_up_access)
                            .on_click(cx.listener(|t, _: &ClickEvent, _, cx| t.set_up_access(cx))),
                        )
                        .child(button("access-help", "Details").compact().on_click(cx.listener(
                            |t, _: &ClickEvent, w, cx| {
                                t.open_device_access();
                                t.focus_dialog(w, cx);
                                cx.notify();
                            },
                        ))),
                )
            })
            .when_some(
                (!self.font_notice_dismissed)
                    .then(|| self.unavailable_fonts())
                    .filter(|f| !f.is_empty())
                    .map(|fonts| font_notice_text(&fonts)),
                |d, text| {
                    d.child(
                        div()
                            .flex()
                            .flex_row()
                            .items_center()
                            .gap_2()
                            .px_5()
                            .py_2()
                            .flex_none()
                            .bg(theme::warning_soft())
                            .border_b_1()
                            .border_color(theme::warning_edge())
                            .text_body()
                            .child(icon(Icon::Warning, 14., theme::warning()))
                            .child(div().flex_1().child(text))
                            .child(
                                button("use-default-font", "Use Default Font")
                                    .primary()
                                    .compact()
                                    .tooltip("Switch these messages to the built-in default font")
                                    .on_click(cx.listener(|t, _: &ClickEvent, _, cx| t.use_default_font(cx))),
                            )
                            .child(button("dismiss-font-notice", "Dismiss").compact().on_click(cx.listener(
                                |t, _: &ClickEvent, _, cx| {
                                    t.font_notice_dismissed = true;
                                    cx.notify();
                                },
                            ))),
                    )
                },
            )
            .children(self.render_recovery_banner(cx))
            .when_some(
                self.last_sent_offer
                    .as_ref()
                    .map(badge_formats::last_sent::LastSent::summary),
                |d, text| {
                    d.child(
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
                            // One line, cut to the window's width; the whole
                            // text on hover.
                            .child({
                                let full = SharedString::from(text);
                                div()
                                    .id("last-sent-summary")
                                    .flex_1()
                                    .min_w_0()
                                    .truncate()
                                    .child(full.clone())
                                    .tooltip(move |_, cx| crate::widgets::tooltip(full.clone(), cx))
                            })
                            .child(
                                button("load-last", "Load it")
                                    .primary()
                                    .compact()
                                    .on_click(cx.listener(|t, _: &ClickEvent, w, cx| t.load_last_sent(w, cx))),
                            )
                            .child(button("dismiss-last", "Dismiss").compact().on_click(cx.listener(
                                |t, _: &ClickEvent, _, cx| {
                                    t.last_sent_offer = None;
                                    cx.notify();
                                },
                            ))),
                    )
                },
            )
            .child({
                // Wide windows get a right-hand panel for the effect and the
                // preview's display options; narrow ones show them in cards.
                let wide = f32::from(window.viewport_size().width) >= WIDE_LAYOUT;
                let mut center = div()
                    .id("main")
                    .flex_1()
                    .min_w_0()
                    .h_full()
                    .overflow_y_scroll()
                    .px_6()
                    .py_5()
                    .flex()
                    .flex_col()
                    .gap_4()
                    .children(self.render_getting_started(cx))
                    .child(self.render_text_section(cx));
                if !wide {
                    center = center.child(section("How it's shown").child(self.render_effect_controls(false, cx)));
                }
                center = center
                    .child(self.render_preview_section(window, !wide, cx))
                    .child(self.render_editor_section(cx));
                let mut body = div()
                    .flex()
                    .flex_row()
                    .flex_1()
                    .min_h_0()
                    .child(self.render_message_list(cx))
                    .child(center);
                if wide {
                    body = body.child(
                        div()
                            .id("inspector")
                            .w(px(INSPECTOR_W))
                            .flex_none()
                            .h_full()
                            .overflow_y_scroll()
                            .bg(theme::panel())
                            .border_l_1()
                            .border_color(theme::border())
                            .px_5()
                            .py_5()
                            .flex()
                            .flex_col()
                            .gap_4()
                            .child(
                                div()
                                    .flex()
                                    .flex_col()
                                    .gap_3()
                                    .child(div().text_title().child("How it's shown"))
                                    .child(self.render_effect_controls(true, cx)),
                            )
                            .child(div().h(px(1.)).bg(theme::border()))
                            .child(self.render_preview_options(true, cx)),
                    );
                }
                body
            })
            .child(self.render_status(cx))
            .when(menu_open, |d| {
                d.child(
                    deferred(
                        div()
                            .id("menu-backdrop")
                            .absolute()
                            .top_0()
                            .left_0()
                            .size_full()
                            .occlude()
                            .on_mouse_down(MouseButton::Left, cx.listener(|t, _, w, cx| t.close_menu(w, cx))),
                    )
                    .with_priority(1),
                )
            })
            .when_some(dialog, |d, dlg| {
                d.child(deferred(self.render_dialog(dlg, window, cx)).with_priority(3))
            })
    }
}

pub fn bindings() -> Vec<gpui::KeyBinding> {
    use gpui::KeyBinding;
    let m = if cfg!(target_os = "macos") { "cmd" } else { "ctrl" };
    vec![
        KeyBinding::new(&format!("{m}-n"), NewProject, None),
        KeyBinding::new(&format!("{m}-o"), OpenProject, None),
        KeyBinding::new(&format!("{m}-s"), SaveProject, None),
        KeyBinding::new(&format!("{m}-shift-s"), SaveProjectAs, None),
        KeyBinding::new(&format!("{m}-enter"), SendToBadge, None),
        KeyBinding::new(&format!("{m}-q"), Quit, None),
        KeyBinding::new(&format!("{m}-z"), Undo, None),
        KeyBinding::new(&format!("{m}-shift-z"), Redo, None),
        KeyBinding::new(&format!("{m}-y"), Redo, None),
        KeyBinding::new(&format!("{m}-,"), ShowSettings, None),
        KeyBinding::new(&format!("{m}-/"), ShowShortcuts, None),
        KeyBinding::new("f1", ShowShortcuts, None),
        KeyBinding::new("tab", FocusNext, Some("BadgeApp")),
        KeyBinding::new("shift-tab", FocusPrev, Some("BadgeApp")),
        KeyBinding::new("tab", FocusNext, Some("Modal")),
        KeyBinding::new("shift-tab", FocusPrev, Some("Modal")),
        KeyBinding::new("enter", ModalConfirm, Some("Modal")),
        KeyBinding::new("up", MenuUp, Some("Menu")),
        KeyBinding::new("down", MenuDown, Some("Menu")),
        KeyBinding::new("enter", MenuActivate, Some("Menu")),
        KeyBinding::new("space", MenuActivate, Some("Menu")),
        KeyBinding::new("right", MenuRight, Some("Menu")),
        KeyBinding::new("left", MenuLeft, Some("Menu")),
        KeyBinding::new("escape", MenuClose, Some("Menu")),
        KeyBinding::new("escape", ModalCancel, Some("Modal")),
    ]
}

#[cfg(test)]
mod tests {
    use super::editor::line_cells;

    #[test]
    fn recent_files_with_the_same_name_are_told_apart() {
        use super::files::recent_labels;
        use std::path::PathBuf;
        let home = std::env::var("HOME").unwrap_or_else(|_| "/home/u".into());
        let p = |s: &str| PathBuf::from(s.replace('~', &home));
        let labels = recent_labels(&[
            p("~/Downloads/hello.badge"),
            p("~/work/a/x/demo.badge"),
            p("~/work/b/x/demo.badge"),
            p("/media/usb/x/demo.badge"),
        ]);
        assert_eq!(labels[0], ("hello.badge".into(), "Downloads".into()));
        // Same name and same parent folder "x": more of the path is shown.
        assert_eq!(labels[1], ("demo.badge".into(), "\u{2026}/a/x".into()));
        assert_eq!(labels[2], ("demo.badge".into(), "\u{2026}/b/x".into()));
        assert_eq!(labels[3], ("demo.badge".into(), "\u{2026}/usb/x".into()));
        let all: std::collections::HashSet<_> = labels.iter().collect();
        assert_eq!(all.len(), labels.len());
    }

    #[test]
    fn long_recent_names_are_shortened_in_the_middle() {
        use super::files::{recent_labels, shorten, shorten_name};
        use std::path::PathBuf;
        assert_eq!(shorten("short", 24), "short");
        let s = shorten("my-extremely-long-project-folder-2025", 24);
        assert_eq!(s.chars().count(), 24);
        assert!(s.starts_with("my-extremely") && s.ends_with("2025") && s.contains('\u{2026}'));
        let n = shorten_name(&format!("{}.badge", "x".repeat(80)), 40);
        assert!(n.ends_with(".badge") && n.chars().count() == 40, "{n}");

        // Long folders are shortened; entries still differ.
        let long = "a-really-long-folder-name-that-goes-on-and-on";
        let labels = recent_labels(&[
            PathBuf::from(format!("/data/{long}/one/demo.badge")),
            PathBuf::from(format!("/data/{long}/two/demo.badge")),
            PathBuf::from(format!("/data/{long}/notes.badge")),
        ]);
        assert_eq!((labels[0].1.as_str(), labels[1].1.as_str()), ("one", "two"));
        assert!(labels[2].1.chars().count() <= 24, "{:?}", labels[2]);
        // Two files whose difference would be shortened away keep it.
        let labels = recent_labels(&[
            PathBuf::from("/d/project-version-one-alpha-final-draft/x.badge"),
            PathBuf::from("/d/project-version-two-alpha-final-draft/x.badge"),
        ]);
        assert_ne!(labels[0], labels[1]);
    }

    #[test]
    fn the_missing_font_notice_names_messages_and_fonts() {
        use super::message::{font_notice_text, messages_phrase};
        assert_eq!(messages_phrase([1].into_iter()), "message 2");
        assert_eq!(messages_phrase([0, 4].into_iter()), "messages 1 and 5");
        assert_eq!(messages_phrase([0, 2, 3].into_iter()), "messages 1, 3 and 4");
        assert_eq!(
            font_notice_text(&[(1, "Comic Sans".into())]),
            "Message 2 uses \u{201c}Comic Sans\u{201d}, which isn't available here, so it's shown in a stand-in font."
        );
        assert_eq!(
            font_notice_text(&[(0, "B".into()), (2, "A".into()), (3, "B".into())]),
            "Messages 1, 3 and 4 use fonts that aren't available here (\u{201c}A\u{201d}, \u{201c}B\u{201d}), so they're shown in a stand-in font."
        );
    }

    #[test]
    fn sentences_start_with_a_capital() {
        use super::dialogs::sentence_case;
        assert_eq!(
            sentence_case("the file ends too early", true),
            "The file ends too early."
        );
        assert_eq!(sentence_case("line 10, column 68", false), "Line 10, column 68");
        assert_eq!(sentence_case("already done.", true), "Already done.");
        assert_eq!(sentence_case("", true), "");
    }

    #[test]
    fn lines_are_continuous() {
        let cells = line_cells((0, 0), (5, 2));
        assert_eq!(cells.last(), Some(&(5, 2)));
        let mut prev = (0usize, 0usize);
        for c in cells {
            assert!(c.0.abs_diff(prev.0) <= 1 && c.1.abs_diff(prev.1) <= 1);
            prev = c;
        }
        assert!(line_cells((3, 3), (3, 3)).is_empty());
        assert_eq!(line_cells((4, 1), (1, 1)), vec![(3, 1), (2, 1), (1, 1)]);
    }
}
