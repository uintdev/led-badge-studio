//! macOS: the main window as a Mac document. An `NSDocument` attached to
//! GPUI's window gives it the native title: the file's name and icon,
//! "— Edited" while there are unsaved changes, and the menu from clicking
//! the title (Rename, Move To).
//!
//! The document never saves: the app opens and saves projects as on Linux
//! and Windows, asks before unsaved changes are lost, and keeps a copy of
//! unsaved work for after a crash (recovery.rs). The document only shows
//! that (macOS draws "Edited" only for documents that autosave, so it says
//! it does, but never has anything to autosave). Each save gives the file
//! as it was to macOS's version store (`keep_version`), for File → Revert
//! To.
//!
//! The app tells the document (in [`sync`], on each render) where the file
//! is and whether it's edited. What the document does on its own comes back
//! as an [`Event`].
//!
//! Only built on macOS, and not for tests (they have no real window).

use std::ffi::{CStr, c_char};
use std::sync::atomic::{AtomicBool, Ordering};

use objc::declare::ClassDecl;
use objc::runtime::{BOOL, Class, NO, Object, Sel, YES};
use objc::{class, msg_send, sel, sel_impl};

use super::*;

type Id = *mut Object;

/// The `.badge` type's ID, as declared in the app's Info.plist.
const FILE_TYPE: &str = "dev.uint.led-badge";

/// `NSChangeDone`, `NSChangeCleared`
const CHANGE_DONE: usize = 0;
const CHANGE_CLEARED: usize = 2;

/// What the document did on its own.
enum Event {
    /// The file was renamed or moved (from the title's menu).
    Moved(PathBuf),
    /// Another program changed the file while there were no unsaved changes
    /// here, and macOS asks for it to be read again.
    ChangedElsewhere(PathBuf),
    /// macOS was asked to quit (from the Dock, or by logging out) while
    /// there was something to ask about first; the quit was put off.
    QuitRequested,
}

/// What the document knows; on the main thread only.
struct State {
    doc: Id,
    events: futures::channel::mpsc::UnboundedSender<Event>,
    path: Option<PathBuf>,
    edited: bool,
    /// A message is being sent.
    sending: bool,
}

thread_local! {
    static STATE: RefCell<Option<State>> = const { RefCell::new(None) };
}

/// Set once the app has decided to quit (its own Quit asked what it needed
/// to, or its window closed): macOS then quits without asking it again.
static QUIT_APPROVED: AtomicBool = AtomicBool::new(false);

pub(crate) fn approve_quit() {
    QUIT_APPROVED.store(true, Ordering::Relaxed);
}

/// Tells the document about the project; attaches it on the first call.
/// Cheap when nothing changed (it runs on every frame).
#[allow(unexpected_cfgs)] // from the objc macros
pub(super) fn sync(app: &mut BadgeApp, window: &mut Window, cx: &mut Context<BadgeApp>) {
    if STATE.with_borrow(Option::is_none) && !attach(window, cx) {
        return;
    }
    let edited = app.is_dirty();
    // What changed, worked out before calling AppKit: the document's methods
    // below read the state too.
    let Some((doc, moved, now_edited, cleared)) = STATE.with_borrow_mut(|state| {
        let state = state.as_mut()?;
        let moved = (state.path != app.path).then(|| app.path.clone());
        if moved.is_some() {
            state.path = app.path.clone();
        }
        let now_edited = edited && !state.edited;
        let cleared = !edited && state.edited;
        state.edited = edited;
        state.sending = app.is_sending();
        Some((state.doc, moved, now_edited, cleared))
    }) else {
        return;
    };
    // SAFETY: AppKit calls on the main thread, on the document made in
    // `attach`, which lives as long as the app.
    unsafe {
        if let Some(path) = &moved {
            let url = path.as_deref().map_or(std::ptr::null_mut(), |p| file_url(p));
            let _: () = msg_send![doc, setFileURL: url];
            set_modification_date(doc, path.as_deref());
            let controllers: Id = msg_send![doc, windowControllers];
            let _: () =
                msg_send![controllers, makeObjectsPerformSelector: sel!(synchronizeWindowTitleWithDocumentName)];
        }
        if now_edited {
            let _: () = msg_send![doc, updateChangeCount: CHANGE_DONE];
        } else if cleared {
            // Saved (or opened): the file is the project again.
            let _: () = msg_send![doc, updateChangeCount: CHANGE_CLEARED];
            set_modification_date(doc, app.path.as_deref());
        }
    }
}

/// Puts a document on the window, and starts handling what it does.
#[allow(unexpected_cfgs)] // from the objc macros
fn attach(window: &mut Window, cx: &mut Context<BadgeApp>) -> bool {
    use raw_window_handle::{HasWindowHandle, RawWindowHandle};
    let Ok(handle) = HasWindowHandle::window_handle(window) else {
        return false;
    };
    let RawWindowHandle::AppKit(handle) = handle.as_raw() else {
        return false;
    };
    // SAFETY: AppKit calls on the main thread; the view is GPUI's, in its
    // window. The window controller is kept by the document, and the
    // document for as long as the app runs.
    let doc = unsafe {
        let view = handle.ns_view.as_ptr() as Id;
        let ns_window: Id = msg_send![view, window];
        if ns_window.is_null() {
            return false;
        }
        let doc: Id = msg_send![document_class(), alloc];
        let doc: Id = msg_send![doc, init];
        let _: () = msg_send![doc, setFileType: ns_string(FILE_TYPE)];
        let controller: Id = msg_send![class!(NSWindowController), alloc];
        let controller: Id = msg_send![controller, initWithWindow: ns_window];
        let _: () = msg_send![doc, addWindowController: controller];
        let _: () = msg_send![controller, release];
        // GPUI's app delegate doesn't answer applicationShouldTerminate:,
        // so a quit from the Dock (or logging out) would end the app at
        // once. Answering it lets the app ask first, as its own Quit does.
        // The types: an NSUInteger reply, self, _cmd and the sender.
        let added = objc::runtime::class_addMethod(
            class!(GPUIApplicationDelegate) as *const Class as *mut Class,
            sel!(applicationShouldTerminate:),
            std::mem::transmute::<extern "C" fn(&Object, Sel, Id) -> usize, objc::runtime::Imp>(should_terminate),
            c"Q@:@".as_ptr(),
        );
        if added == NO {
            log::warn!("couldn't answer macOS's quit requests; quitting from the Dock won't ask first");
        }
        doc
    };
    let (events, mut rx) = futures::channel::mpsc::unbounded();
    STATE.set(Some(State {
        doc,
        events,
        path: None,
        edited: false,
        sending: false,
    }));
    cx.spawn_in(window, async move |this, cx| {
        while let Some(event) = rx.next().await {
            if this
                .update_in(cx, |app, window, cx| app.on_document_event(event, window, cx))
                .is_err()
            {
                break;
            }
        }
    })
    .detach();
    true
}

/// `applicationShouldTerminate:`: quits now when there's nothing to ask
/// about or the app already decided to; otherwise puts the quit off and
/// has the app's own Quit ask.
extern "C" fn should_terminate(_: &Object, _: Sel, _: Id) -> usize {
    const TERMINATE_CANCEL: usize = 0;
    const TERMINATE_NOW: usize = 1;
    if quit_needs_asking() {
        TERMINATE_CANCEL
    } else {
        TERMINATE_NOW
    }
}

/// Whether a quit from macOS has to wait for the app to ask first (unsaved
/// changes, or a send), unless the app already decided to quit; when it
/// does, the app is told to ask.
fn quit_needs_asking() -> bool {
    if QUIT_APPROVED.load(Ordering::Relaxed) {
        return false;
    }
    STATE.with_borrow(|state| {
        state
            .as_ref()
            .filter(|s| s.edited || s.sending)
            .is_some_and(|s| s.events.unbounded_send(Event::QuitRequested).is_ok())
    })
}

impl BadgeApp {
    fn on_document_event(&mut self, event: Event, window: &mut Window, cx: &mut Context<Self>) {
        match event {
            Event::Moved(path) => {
                if self.path.as_ref() != Some(&path) {
                    if let Some(old) = self.path.replace(path.clone()) {
                        self.settings.recent_files.retain(|f| f != &old);
                    }
                    self.settings.last_dir = path.parent().map(Path::to_path_buf);
                    self.settings.add_recent(path.clone());
                    self.settings.save();
                    self.note_file_stamp();
                    self.refresh_file_versions();
                    self.set_status(format!("The project is now {}", path.display()), Severity::Info);
                }
            }
            // Not read again: the app says so, and saving asks whose
            // changes to keep (as on the other systems).
            Event::ChangedElsewhere(path) if self.path.as_ref() == Some(&path) => {
                self.check_file_changed_elsewhere(cx);
            }
            Event::ChangedElsewhere(_) => {}
            Event::QuitRequested => self.quit(&Quit, window, cx),
        }
        cx.notify();
    }
}

/// The earlier versions macOS keeps of `path`, newest first: when each was
/// saved, worded as Finder does ("Today at 12:31"), and where its contents
/// can be read. They're kept when macOS autosaves a document.
#[allow(unexpected_cfgs)] // from the objc macros
pub(super) fn file_versions(path: &Path) -> Vec<(String, PathBuf)> {
    let mut found: Vec<(f64, String, PathBuf)> = Vec::new();
    // SAFETY: Foundation calls on the main thread; every object is either
    // autoreleased or owned by the formatter, which is released at the end.
    unsafe {
        let versions: Id = msg_send![class!(NSFileVersion), otherVersionsOfItemAtURL: file_url(path)];
        if versions.is_null() {
            return Vec::new();
        }
        let formatter: Id = msg_send![class!(NSDateFormatter), new];
        let _: () = msg_send![formatter, setDateStyle: 2usize]; // NSDateFormatterMediumStyle
        let _: () = msg_send![formatter, setTimeStyle: 1usize]; // NSDateFormatterShortStyle
        let _: () = msg_send![formatter, setDoesRelativeDateFormatting: YES];
        let count: usize = msg_send![versions, count];
        for i in 0..count {
            let version: Id = msg_send![versions, objectAtIndex: i];
            let date: Id = msg_send![version, modificationDate];
            let url: Id = msg_send![version, URL];
            let Some(contents) = url_path(url) else { continue };
            if date.is_null() {
                continue;
            }
            let seconds: f64 = msg_send![date, timeIntervalSince1970];
            let label: Id = msg_send![formatter, stringFromDate: date];
            let c: *const c_char = msg_send![label, UTF8String];
            if c.is_null() {
                continue;
            }
            found.push((seconds, CStr::from_ptr(c).to_string_lossy().into_owned(), contents));
        }
        let _: () = msg_send![formatter, release];
    }
    found.sort_by(|a, b| b.0.total_cmp(&a.0));
    found.into_iter().map(|(_, label, path)| (label, path)).collect()
}

/// `NSDocument` that says it autosaves (for "Edited") but never saves (see
/// the module's notes).
#[allow(unexpected_cfgs)] // from the objc macros
fn document_class() -> &'static Class {
    const NAME: &str = "LedBadgeStudioDocument";
    static REGISTER: std::sync::Once = std::sync::Once::new();
    REGISTER.call_once(|| {
        let mut decl = ClassDecl::new(NAME, class!(NSDocument)).expect("document class registered twice");
        // SAFETY: each method's signature matches its selector.
        unsafe {
            decl.add_class_method(
                sel!(autosavesInPlace),
                autosaves_in_place as extern "C" fn(&Class, Sel) -> BOOL,
            );
            decl.add_method(
                sel!(hasUnautosavedChanges),
                nothing_to_autosave as extern "C" fn(&Object, Sel) -> BOOL,
            );
            decl.add_method(
                sel!(autosavingFileType),
                no_autosaving_type as extern "C" fn(&Object, Sel) -> Id,
            );
            decl.add_method(
                sel!(writeToURL:ofType:error:),
                refuse_to_write as extern "C" fn(&Object, Sel, Id, Id, *mut std::ffi::c_void) -> BOOL,
            );
            decl.add_method(
                sel!(readFromURL:ofType:error:),
                read_from_url as extern "C" fn(&Object, Sel, Id, Id, *mut std::ffi::c_void) -> BOOL,
            );
            decl.add_method(
                sel!(shouldCloseWindowController:delegate:shouldCloseSelector:contextInfo:),
                should_close_window as extern "C" fn(&Object, Sel, Id, Id, Sel, *mut std::ffi::c_void),
            );
            decl.add_method(
                sel!(canCloseDocumentWithDelegate:shouldCloseSelector:contextInfo:),
                can_close as extern "C" fn(&Object, Sel, Id, Sel, *mut std::ffi::c_void),
            );
            decl.add_method(sel!(setFileURL:), set_file_url as extern "C" fn(&Object, Sel, Id));
            decl.add_method(
                sel!(validateUserInterfaceItem:),
                validate_item as extern "C" fn(&Object, Sel, Id) -> BOOL,
            );
        }
        decl.register();
    });
    Class::get(NAME).expect("document class registered")
}

fn send(event: Event) {
    STATE.with_borrow(|s| {
        if let Some(s) = s {
            let _ = s.events.unbounded_send(event);
        }
    });
}

/// What makes macOS show "Edited" in the title.
extern "C" fn autosaves_in_place(_: &Class, _: Sel) -> BOOL {
    YES
}

/// The document never autosaves; the app saves when asked to.
extern "C" fn nothing_to_autosave(_: &Object, _: Sel) -> BOOL {
    NO
}

extern "C" fn no_autosaving_type(_: &Object, _: Sel) -> Id {
    std::ptr::null_mut()
}

/// Saving is the app's; should macOS still ask the document to write, it
/// refuses (`error` is an `NSError **`).
#[allow(unexpected_cfgs)] // from the objc macros
extern "C" fn refuse_to_write(_: &Object, _: Sel, _: Id, _: Id, error: *mut std::ffi::c_void) -> BOOL {
    let error = error.cast::<Id>();
    if !error.is_null() {
        // SAFETY: `error` points to where an error goes. NSFileWriteUnknownError.
        unsafe {
            *error = msg_send![class!(NSError), errorWithDomain: ns_string("NSCocoaErrorDomain") code: 512isize userInfo: std::ptr::null_mut::<Object>()];
        }
    }
    NO
}

/// The window's close button, for a project that has no file yet: macOS
/// would ask whether to keep the new document, and then the app asked
/// again. The window may close as far as the document goes; the window's
/// close then asks the app (`install_close_guard`). A project with a file
/// is left to NSDocument, which doesn't ask.
#[allow(unexpected_cfgs)] // from the objc macros
extern "C" fn should_close_window(
    this: &Object,
    _: Sel,
    controller: Id,
    delegate: Id,
    selector: Sel,
    context: *mut std::ffi::c_void,
) {
    // SAFETY: NSDocument's own method, on the same object, with what AppKit
    // passed; `answer` as below.
    unsafe {
        if has_file(this) {
            let _: () = msg_send![super(this, class!(NSDocument)), shouldCloseWindowController: controller delegate: delegate shouldCloseSelector: selector contextInfo: context];
        } else {
            answer(this, delegate, selector, YES, context);
        }
    }
}

/// macOS asking whether the document may close other than from its window:
/// when the app is quitting (from the Dock, or by logging out), before
/// `applicationShouldTerminate:`. A yes would have macOS close the window
/// without the app's question, so for a project with no file yet it's the
/// same answer as `should_terminate`: yes once there's nothing to ask about,
/// otherwise no (which calls the quit off) and the app's own Quit asks. A
/// project with a file is left to NSDocument.
#[allow(unexpected_cfgs)] // from the objc macros
extern "C" fn can_close(this: &Object, _: Sel, delegate: Id, selector: Sel, context: *mut std::ffi::c_void) {
    // SAFETY: as in `should_close_window`.
    unsafe {
        if has_file(this) {
            let _: () = msg_send![super(this, class!(NSDocument)), canCloseDocumentWithDelegate: delegate shouldCloseSelector: selector contextInfo: context];
        } else {
            let yes = if quit_needs_asking() { NO } else { YES };
            answer(this, delegate, selector, yes, context);
        }
    }
}

#[allow(unexpected_cfgs)] // from the objc macros
unsafe fn has_file(doc: &Object) -> bool {
    // SAFETY: NSDocument's `fileURL`, nil until the project has a file.
    let url: Id = unsafe { msg_send![doc, fileURL] };
    !url.is_null()
}

/// Gives `delegate` the document's answer, by the `selector` AppKit passed
/// (`document:shouldClose:contextInfo:`).
unsafe fn answer(doc: &Object, delegate: Id, selector: Sel, yes: BOOL, context: *mut std::ffi::c_void) {
    if delegate.is_null() || selector.as_ptr().is_null() {
        return;
    }
    // SAFETY: AppKit's delegate answers `selector`, which takes the
    // document, a BOOL and the context it was given.
    unsafe {
        let doc = doc as *const Object as Id;
        let _: Result<(), _> = objc::Message::send_message(&*delegate, selector, (doc, yes, context));
    }
}

/// macOS asks for the file to be read again: another program changed it
/// while the document had no unsaved changes. The app only tells you (see
/// `Event::ChangedElsewhere`); what it shows stays as it is.
extern "C" fn read_from_url(_: &Object, _: Sel, url: Id, _: Id, _: *mut std::ffi::c_void) -> BOOL {
    // SAFETY: a file URL from AppKit.
    match unsafe { url_path(url) } {
        Some(path) => {
            send(Event::ChangedElsewhere(path));
            YES
        }
        None => NO,
    }
}

/// Renaming or moving the file (from the title's menu) tells the app; the
/// app's own changes (see `sync`) are already known.
#[allow(unexpected_cfgs)] // from the objc macros
extern "C" fn set_file_url(this: &Object, _: Sel, url: Id) {
    // SAFETY: NSDocument's own method, on the same object; `url` is a file
    // URL or nil.
    unsafe {
        let _: () = msg_send![super(this, class!(NSDocument)), setFileURL: url];
        let Some(path) = url_path(url) else {
            return;
        };
        let known = STATE.with_borrow(|s| s.as_ref().is_some_and(|s| s.path.as_ref() == Some(&path)));
        if !known {
            STATE.with_borrow_mut(|s| {
                if let Some(s) = s {
                    s.path = Some(path.clone());
                }
            });
            send(Event::Moved(path));
        }
    }
}

/// Leaves out what the document can't do: anything that saves (Duplicate,
/// Revert To, which rewrites the file, and Rename or Move To for a project
/// that has no file yet), Browse All Versions (which needs a second window
/// for the old version) and Locked. File → Revert To is the app's.
#[allow(unexpected_cfgs)] // from the objc macros
extern "C" fn validate_item(this: &Object, _: Sel, item: Id) -> BOOL {
    // SAFETY: `item` answers `action` (NSValidatedUserInterfaceItem), and
    // NSDocument's own methods are on the same object.
    unsafe {
        let action: *const std::ffi::c_void = msg_send![item, action];
        let left_out = [
            sel!(duplicateDocument:),
            sel!(revertDocumentToSaved:),
            sel!(browseDocumentVersions:),
            sel!(lockDocument:),
        ];
        if left_out.iter().any(|s| s.as_ptr() == action) {
            return NO;
        }
        let url: Id = msg_send![this, fileURL];
        if url.is_null()
            && [sel!(renameDocument:), sel!(moveDocument:)]
                .iter()
                .any(|s| s.as_ptr() == action)
        {
            return NO;
        }
        msg_send![super(this, class!(NSDocument)), validateUserInterfaceItem: item]
    }
}

/// Gives the file as it is now to macOS's version store, as an earlier
/// version (File → Revert To); called just before saving over it.
#[allow(unexpected_cfgs)] // from the objc macros
pub(super) fn keep_version(path: &Path) {
    if !path.is_file() {
        return;
    }
    // SAFETY: Foundation calls on the main thread with a file URL.
    unsafe {
        let url = file_url(path);
        let mut error: Id = std::ptr::null_mut();
        let version: Id = msg_send![class!(NSFileVersion), addVersionOfItemAtURL: url withContentsOfURL: url options: 0usize error: &mut error];
        if version.is_null() {
            log::warn!("couldn't keep the earlier version of {}", path.display());
        }
    }
}

/// Lets the document know the file on disk is as the app left it, so it
/// doesn't take the app's own saves for another program's changes.
#[allow(unexpected_cfgs)] // from the objc macros
unsafe fn set_modification_date(doc: Id, path: Option<&Path>) {
    let Some(path) = path else {
        return;
    };
    // SAFETY: Foundation calls on the main thread.
    unsafe {
        let manager: Id = msg_send![class!(NSFileManager), defaultManager];
        let attributes: Id = msg_send![manager, attributesOfItemAtPath: ns_string(&path.to_string_lossy()) error: std::ptr::null_mut::<Id>()];
        if attributes.is_null() {
            return;
        }
        let date: Id = msg_send![attributes, objectForKey: ns_string("NSFileModificationDate")];
        let _: () = msg_send![doc, setFileModificationDate: date];
    }
}

#[allow(unexpected_cfgs)] // from the objc macros
unsafe fn ns_string(s: &str) -> Id {
    let c = std::ffi::CString::new(s).unwrap_or_default();
    // SAFETY: a valid C string.
    unsafe { msg_send![class!(NSString), stringWithUTF8String: c.as_ptr()] }
}

#[allow(unexpected_cfgs)] // from the objc macros
unsafe fn file_url(path: &Path) -> Id {
    // SAFETY: Foundation calls on a valid string.
    unsafe { msg_send![class!(NSURL), fileURLWithPath: ns_string(&path.to_string_lossy())] }
}

/// The path of a file URL; `None` for nil or other URLs.
#[allow(unexpected_cfgs)] // from the objc macros
unsafe fn url_path(url: Id) -> Option<PathBuf> {
    if url.is_null() {
        return None;
    }
    // SAFETY: an NSURL; `path` is nil for URLs that aren't files.
    unsafe {
        let path: Id = msg_send![url, path];
        if path.is_null() {
            return None;
        }
        let c: *const c_char = msg_send![path, UTF8String];
        (!c.is_null()).then(|| PathBuf::from(CStr::from_ptr(c).to_string_lossy().into_owned()))
    }
}
