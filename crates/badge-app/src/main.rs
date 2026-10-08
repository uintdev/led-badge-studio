//! LED Badge Studio: a cross-platform editor and uploader for 0416:5020 LED name badges.

// Windows: a GUI program, so opening it doesn't open a console window too
// (see `attach_to_terminal` for its log).
#![cfg_attr(windows, windows_subsystem = "windows")]

mod app;
mod chrome;
mod gpu;
mod icons;
mod led_view;
mod preview_window;
mod settings;
mod system_theme;
mod text_input;
mod theme;
mod widgets;

use std::path::PathBuf;

use gpui::{
    App, AppContext, Bounds, TitlebarOptions, WindowBackgroundAppearance, WindowBounds, WindowDecorations,
    WindowOptions, px, size,
};

/// Application ID: names the desktop entry, icons, Flatpak and macOS bundle.
pub const APP_ID: &str = "dev.uint.LedBadgeStudio";

/// The files built into the program that GPUI loads by path (the icons).
struct Assets;

impl gpui::AssetSource for Assets {
    fn load(&self, path: &str) -> gpui::Result<Option<std::borrow::Cow<'static, [u8]>>> {
        Ok(icons::load(path))
    }

    fn list(&self, path: &str) -> gpui::Result<Vec<gpui::SharedString>> {
        Ok(icons::paths().into_iter().filter(|p| p.starts_with(path)).collect())
    }
}

/// Windows: started from a terminal, the log goes to that terminal. (As a
/// GUI program, the app gets no console of its own; its output goes nowhere
/// unless it's redirected, which is left as it is.)
#[cfg(windows)]
fn attach_to_terminal() {
    use windows_sys::Win32::Foundation::{GENERIC_READ, GENERIC_WRITE, HANDLE, INVALID_HANDLE_VALUE};
    use windows_sys::Win32::Storage::FileSystem::{CreateFileW, FILE_SHARE_READ, FILE_SHARE_WRITE, OPEN_EXISTING};
    use windows_sys::Win32::System::Console::{
        ATTACH_PARENT_PROCESS, AttachConsole, GetStdHandle, STD_ERROR_HANDLE, STD_HANDLE, STD_OUTPUT_HANDLE,
        SetStdHandle,
    };
    let valid = |h: HANDLE| !h.is_null() && h != INVALID_HANDLE_VALUE;
    // SAFETY: plain console calls; the handle set is one just opened, kept
    // for as long as the app runs.
    unsafe {
        let has = |which: STD_HANDLE| valid(GetStdHandle(which));
        if has(STD_ERROR_HANDLE) || AttachConsole(ATTACH_PARENT_PROCESS) == 0 {
            return;
        }
        let console = CreateFileW(
            windows_sys::w!("CONOUT$"),
            GENERIC_READ | GENERIC_WRITE,
            FILE_SHARE_READ | FILE_SHARE_WRITE,
            std::ptr::null(),
            OPEN_EXISTING,
            0,
            std::ptr::null_mut(),
        );
        if !valid(console) {
            return;
        }
        SetStdHandle(STD_ERROR_HANDLE, console);
        if !has(STD_OUTPUT_HANDLE) {
            SetStdHandle(STD_OUTPUT_HANDLE, console);
        }
    }
}

fn main() {
    #[cfg(windows)]
    attach_to_terminal();
    gpu::run_probe_if_requested();
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("warn")).init();
    gpu::configure();

    // A project to open: an argument, or an "open URLs" event on macOS.
    let open: Option<PathBuf> = std::env::args_os().nth(1).map(PathBuf::from);
    #[cfg(target_os = "macos")]
    app_menu::name_unbundled_app();
    let (opened_tx, mut opened_rx) = futures::channel::mpsc::unbounded::<PathBuf>();
    let application = gpui_platform::application().with_assets(Assets);
    application.on_open_urls(move |urls| {
        for path in urls.iter().filter_map(|u| file_url_to_path(u)) {
            let _ = opened_tx.unbounded_send(path);
        }
    });

    application.run(move |cx: &mut App| {
        cx.bind_keys(text_input::bindings());
        cx.bind_keys(app::bindings());
        cx.bind_keys(preview_window::bindings());
        // The window handles Quit itself (asking about unsaved changes);
        // this only runs when no window does.
        cx.on_action(|_: &app::Quit, cx| cx.quit());

        // As large as is comfortable, within 90% of the screen.
        let screen = cx.primary_display().map(|d| d.bounds().size);
        let fit = |want: f32, have: Option<gpui::Pixels>, least: f32| {
            have.map_or(want, |h| (f32::from(h) * 0.9).min(want)).max(least)
        };
        let bounds = Bounds::centered(
            None,
            size(
                px(fit(1480., screen.map(|s| s.width), 960.)),
                px(fit(940., screen.map(|s| s.height), 640.)),
            ),
            cx,
        );
        let window = cx
            .open_window(
                WindowOptions {
                    window_bounds: Some(WindowBounds::Windowed(bounds)),
                    titlebar: Some(TitlebarOptions {
                        title: Some(env!("BRAND_NAME").into()),
                        ..Default::default()
                    }),
                    window_min_size: Some(size(px(900.), px(600.))),
                    // Matches the desktop file's name, so docks show the right icon.
                    app_id: Some(APP_ID.into()),
                    // Linux gets our own title bar; GNOME doesn't draw one.
                    window_decorations: cfg!(target_os = "linux").then_some(WindowDecorations::Client),
                    window_background: if cfg!(target_os = "linux") {
                        WindowBackgroundAppearance::Transparent
                    } else {
                        WindowBackgroundAppearance::Opaque
                    },
                    ..Default::default()
                },
                move |window, cx| {
                    cx.new(|cx| {
                        app::BadgeApp::install_close_guard(window, cx);
                        app::BadgeApp::new(open, window, cx)
                    })
                },
            )
            .expect("failed to open window");
        window
            .update(cx, |_, window, cx| {
                window.activate_window();
                cx.activate(true);
            })
            .ok();
        #[cfg(target_os = "macos")]
        app_menu::install(window, cx);
        let main_id = window.window_id();
        cx.on_window_closed(move |cx, _| {
            if !cx.windows().iter().any(|w| w.window_id() == main_id) {
                // The window asked what it needed to before closing.
                #[cfg(all(target_os = "macos", not(test)))]
                app::approve_quit();
                cx.quit();
            }
        })
        .detach();
        cx.spawn(async move |cx| {
            use futures::StreamExt;
            while let Some(path) = opened_rx.next().await {
                let opened = window.update(cx, |app, window, cx| app.open_requested(path, window, cx));
                if opened.is_err() {
                    break;
                }
            }
        })
        .detach();
    });
}

/// The local path in a `file://` URL, with `%XX` escapes decoded.
fn file_url_to_path(url: &str) -> Option<PathBuf> {
    let rest = url.strip_prefix("file://")?;
    // Skip an optional host ("file://localhost/...").
    let path = &rest[rest.find('/')?..];
    let bytes = path.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        let hex = bytes.get(i + 1..i + 3).and_then(|h| std::str::from_utf8(h).ok());
        match (bytes[i], hex.and_then(|h| u8::from_str_radix(h, 16).ok())) {
            (b'%', Some(b)) => {
                out.push(b);
                i += 3;
            }
            (b, _) => {
                out.push(b);
                i += 1;
            }
        }
    }
    Some(PathBuf::from(String::from_utf8(out).ok()?))
}

#[cfg(test)]
mod tests {
    use super::file_url_to_path;
    use std::path::PathBuf;

    #[test]
    fn file_urls_become_paths() {
        assert_eq!(
            file_url_to_path("file:///Users/me/My%20Badges/hello.badge"),
            Some(PathBuf::from("/Users/me/My Badges/hello.badge"))
        );
        assert_eq!(
            file_url_to_path("file://localhost/tmp/a.badge"),
            Some(PathBuf::from("/tmp/a.badge"))
        );
        assert_eq!(file_url_to_path("https://example.com/a.badge"), None);
    }
}

/// The macOS menu bar: the application menu (under the app's name), File,
/// Edit, Window and Help.
#[cfg(target_os = "macos")]
mod app_menu {
    use std::path::PathBuf;

    use gpui::{Action, App, KeyBinding, Menu, MenuItem, SystemMenuType, WindowHandle, actions};

    use crate::app::{self, BadgeApp};
    use crate::text_input;

    actions!(app_menu, [Hide, HideOthers, ShowAll, Minimize, Zoom, BringAllToFront]);

    /// macOS names the application menu after the bundle's `CFBundleName`.
    /// The .app's Info.plist has it (from branding.toml); a bare executable
    /// (`cargo run`) has none, so macOS would show the file name. Fill it in
    /// from the same branding before the menu bar is made.
    #[allow(unexpected_cfgs)] // from the objc macros
    pub fn name_unbundled_app() {
        use objc::runtime::{NO, Object};
        use objc::{class, msg_send, sel, sel_impl};
        let name = concat!(env!("BRAND_NAME"), "\0");
        // SAFETY: plain Foundation calls on the main thread, before AppKit
        // starts; the dictionary is only changed if it is a mutable one.
        unsafe {
            let bundle: *mut Object = msg_send![class!(NSBundle), mainBundle];
            let info: *mut Object = msg_send![bundle, infoDictionary];
            if info.is_null() {
                return;
            }
            let key: *mut Object = msg_send![class!(NSString), stringWithUTF8String: c"CFBundleName".as_ptr()];
            let current: *mut Object = msg_send![info, objectForKey: key];
            let mutable: objc::runtime::BOOL = msg_send![info, isKindOfClass: class!(NSMutableDictionary)];
            if current.is_null() && mutable != NO {
                let value: *mut Object = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
                let _: () = msg_send![info, setObject: value forKey: key];
            }
        }
    }

    pub fn install(main: WindowHandle<BadgeApp>, cx: &mut App) {
        cx.bind_keys([
            KeyBinding::new("cmd-w", app::CloseWindow, None),
            KeyBinding::new("cmd-h", Hide, None),
            KeyBinding::new("cmd-alt-h", HideOthers, None),
            KeyBinding::new("cmd-m", Minimize, None),
        ]);
        cx.on_action(|_: &Hide, cx| cx.hide());
        cx.on_action(|_: &HideOthers, cx| cx.hide_other_apps());
        cx.on_action(|_: &ShowAll, cx| cx.unhide_other_apps());
        cx.on_action(|_: &Minimize, cx| {
            if let Some(w) = cx.active_window() {
                w.update(cx, |_, window, _| window.minimize_window()).ok();
            }
        });
        cx.on_action(|_: &Zoom, cx| {
            if let Some(w) = cx.active_window() {
                w.update(cx, |_, window, _| window.zoom_window()).ok();
            }
        });
        cx.on_action(|_: &BringAllToFront, cx| cx.activate(true));

        // The main window handles these itself; when another window (the
        // pop-out preview) is in front, they go to the main window.
        forward::<app::NewProject>(main, cx);
        forward::<app::OpenProject>(main, cx);
        forward::<app::OpenRecent>(main, cx);
        forward::<app::ClearRecentFiles>(main, cx);
        forward::<app::SaveProject>(main, cx);
        forward::<app::SaveProjectAs>(main, cx);
        forward::<app::LoadLastSent>(main, cx);
        forward::<app::CheckProjectFile>(main, cx);
        forward::<app::ShowAbout>(main, cx);
        forward::<app::ShowSettings>(main, cx);
        forward::<app::ShowShortcuts>(main, cx);
        forward::<app::ShowLicenses>(main, cx);
        forward::<app::RevertToSaved>(main, cx);
        forward::<app::RevertToVersion>(main, cx);

        // Open Recent and macOS's own recent documents (which the Dock icon's
        // menu shows) list the recent files, and Revert To the file's
        // versions, so they're made again when those change (not on every
        // redraw of the main window).
        let Ok(entity) = main.entity(cx) else {
            return;
        };
        let mut shown = entity.read(cx).file_menu();
        cx.set_menus(menus(&shown));
        mirror_system_recents(&shown.recent, cx);
        cx.observe(&entity, move |entity, cx| {
            let file = entity.read(cx).file_menu();
            if file != shown {
                cx.set_menus(menus(&file));
                if file.recent != shown.recent {
                    mirror_system_recents(&file.recent, cx);
                }
                shown = file;
            }
        })
        .detach();
    }

    /// Makes macOS's recent documents for the app (which its Dock icon's
    /// menu lists, running or not, as do Finder's recent items) the app's
    /// recent files: cleared, then filled oldest first so the newest is first.
    #[allow(unexpected_cfgs)] // from the objc macros
    fn mirror_system_recents(recent: &[(String, PathBuf)], cx: &mut App) {
        use objc::runtime::Object;
        use objc::{class, msg_send, sel, sel_impl};
        // SAFETY: AppKit calls on the main thread, where the UI runs.
        unsafe {
            let controller: *mut Object = msg_send![class!(NSDocumentController), sharedDocumentController];
            let _: () = msg_send![controller, clearRecentDocuments: std::ptr::null_mut::<Object>()];
        }
        for (_, path) in recent.iter().rev() {
            cx.add_recent_document(path);
        }
    }

    /// Runs `A` in the main window, bringing it to the front.
    fn forward<A: Action>(main: WindowHandle<BadgeApp>, cx: &mut App) {
        cx.on_action(move |action: &A, cx| {
            main.update(cx, |app, window, cx| {
                window.activate_window();
                app.run_menu_action(action, window, cx);
            })
            .ok();
        });
    }

    fn menus(file: &app::FileMenu) -> Vec<Menu> {
        let name = env!("BRAND_NAME");
        let recent = &file.recent;
        let mut revert_items = vec![MenuItem::action(file.revert_label, app::RevertToSaved).disabled(!file.can_revert)];
        if !file.versions.is_empty() {
            revert_items.push(MenuItem::separator());
            revert_items.extend(
                file.versions
                    .iter()
                    .map(|(label, path)| MenuItem::action(label.clone(), app::RevertToVersion { path: path.clone() })),
            );
        }
        let mut recent_items: Vec<MenuItem> = recent
            .iter()
            .map(|(label, path)| MenuItem::action(label.clone(), app::OpenRecent { path: path.clone() }))
            .collect();
        if !recent_items.is_empty() {
            recent_items.push(MenuItem::separator());
        }
        recent_items.push(MenuItem::action("Clear Menu", app::ClearRecentFiles).disabled(recent.is_empty()));
        vec![
            Menu::new(name).items([
                MenuItem::action(format!("About {name}"), app::ShowAbout),
                MenuItem::separator(),
                MenuItem::action("Settings\u{2026}", app::ShowSettings),
                MenuItem::separator(),
                MenuItem::os_submenu("Services", SystemMenuType::Services),
                MenuItem::separator(),
                MenuItem::action(format!("Hide {name}"), Hide),
                MenuItem::action("Hide Others", HideOthers),
                MenuItem::action("Show All", ShowAll),
                MenuItem::separator(),
                MenuItem::action(format!("Quit {name}"), app::Quit),
            ]),
            Menu::new("File").items([
                MenuItem::action("New", app::NewProject),
                MenuItem::action("Open\u{2026}", app::OpenProject),
                MenuItem::submenu(Menu::new("Open Recent").items(recent_items)),
                MenuItem::separator(),
                MenuItem::action("Close Window", app::CloseWindow),
                MenuItem::action("Save", app::SaveProject),
                MenuItem::action("Save As\u{2026}", app::SaveProjectAs),
                MenuItem::submenu(Menu::new("Revert To").items(revert_items)),
                MenuItem::separator(),
                MenuItem::action("Load What Was Last Sent to the Badge", app::LoadLastSent),
                MenuItem::action("Check Project File\u{2026}", app::CheckProjectFile),
            ]),
            // Plain actions rather than macOS's cut:/copy:/paste: commands,
            // which GPUI's windows don't answer (the items would always be
            // disabled).
            Menu::new("Edit").items([
                MenuItem::action("Undo", app::Undo),
                MenuItem::action("Redo", app::Redo),
                MenuItem::separator(),
                MenuItem::action("Cut", text_input::Cut),
                MenuItem::action("Copy", text_input::Copy),
                MenuItem::action("Paste", text_input::Paste),
                MenuItem::action("Select All", text_input::SelectAll),
            ]),
            // Named "Window", macOS lists the open windows under these.
            Menu::new("Window").items([
                MenuItem::action("Minimize", Minimize),
                MenuItem::action("Zoom", Zoom),
                MenuItem::separator(),
                MenuItem::action("Bring All to Front", BringAllToFront),
            ]),
            Menu::new("Help").items([
                MenuItem::action("Keyboard Shortcuts", app::ShowShortcuts),
                MenuItem::separator(),
                MenuItem::action("Licenses", app::ShowLicenses),
            ]),
        ]
    }
}
