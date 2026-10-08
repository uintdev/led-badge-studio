//! The system's light or dark setting, and native window frames to match the
//! app's colors.
//!
//! Windows and macOS always report a setting, which GPUI passes on. On Linux
//! GPUI reports "light" when the desktop doesn't say, so the setting is read
//! from the desktop portal here instead; without one the app stays dark.
//! GNOME's Light setting reaches the portal as "no preference" (its own apps
//! read that as light), so on GNOME it means light.

#[cfg(all(target_os = "linux", not(test)))]
use gpui::App;
use gpui::{Window, WindowAppearance};

/// Whether `appearance` (as GPUI reports it on Windows and macOS) is dark.
pub fn is_dark(appearance: WindowAppearance) -> bool {
    matches!(appearance, WindowAppearance::Dark | WindowAppearance::VibrantDark)
}

/// Calls `on_change` with the desktop's color scheme now and whenever it
/// changes: `Some(true)` for dark, `Some(false)` for light, `None` when the
/// desktop states no preference or has no portal.
///
/// Not in tests: the portal is reached over D-Bus, which works on threads
/// of its own, and GPUI's test scheduler stops the test when a task is woken
/// from a thread it doesn't run.
#[cfg(all(target_os = "linux", not(test)))]
pub fn watch_linux(cx: &mut App, on_change: impl Fn(Option<bool>, &mut App) + 'static) {
    use ashpd::desktop::settings::{ColorScheme, Settings};
    use futures::StreamExt;

    let (tx, mut rx) = futures::channel::mpsc::unbounded::<Option<bool>>();
    let gnome = is_gnome(std::env::var("XDG_CURRENT_DESKTOP").ok().as_deref());
    let to_dark = move |c: ColorScheme| match c {
        ColorScheme::PreferDark => Some(true),
        ColorScheme::PreferLight => Some(false),
        // GNOME's Light setting ("default"); elsewhere, no preference.
        ColorScheme::NoPreference => gnome.then_some(false),
    };
    cx.background_executor()
        .spawn(async move {
            let Ok(settings) = Settings::new().await else {
                return;
            };
            if let Ok(scheme) = settings.color_scheme().await {
                let _ = tx.unbounded_send(to_dark(scheme));
            }
            if let Ok(changes) = settings.receive_color_scheme_changed().await {
                let mut changes = std::pin::pin!(changes);
                while let Some(scheme) = changes.next().await {
                    if tx.unbounded_send(to_dark(scheme)).is_err() {
                        break;
                    }
                }
            }
        })
        .detach();
    cx.spawn(async move |cx| {
        while let Some(dark) = rx.next().await {
            cx.update(|cx| on_change(dark, cx));
        }
    })
    .detach();
}

/// Whether `XDG_CURRENT_DESKTOP` (a `:`-separated list, such as
/// `ubuntu:GNOME`) names GNOME.
#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
fn is_gnome(current_desktop: Option<&str>) -> bool {
    current_desktop.is_some_and(|d| d.split(':').any(|name| name.eq_ignore_ascii_case("GNOME")))
}

/// Gives the window's native frame (title bar, borders) the app's colors.
/// On Linux the app draws its own title bar, so there's nothing to do.
pub fn set_native_frame(window: &Window, dark: bool) {
    #[cfg(windows)]
    {
        use raw_window_handle::{HasWindowHandle, RawWindowHandle};
        use windows_sys::Win32::Graphics::Dwm::{DWMWA_USE_IMMERSIVE_DARK_MODE, DwmSetWindowAttribute};
        // (Not `window.window_handle()`: that's GPUI's own handle.)
        if let Ok(handle) = HasWindowHandle::window_handle(window)
            && let RawWindowHandle::Win32(h) = handle.as_raw()
        {
            let value: i32 = dark.into();
            // SAFETY: a valid window handle, and a BOOL-sized value.
            unsafe {
                DwmSetWindowAttribute(
                    h.hwnd.get() as _,
                    DWMWA_USE_IMMERSIVE_DARK_MODE as u32,
                    (&raw const value).cast(),
                    size_of::<i32>() as u32,
                );
            }
        }
    }
    #[cfg(not(windows))]
    let _ = (window, dark);
}

/// macOS: every window's frame follows the app's appearance; `None` follows
/// the system's again.
#[allow(unexpected_cfgs)] // from the objc macros
pub fn set_app_appearance(dark: Option<bool>) {
    #[cfg(target_os = "macos")]
    {
        use objc::runtime::Object;
        use objc::{class, msg_send, sel, sel_impl};
        // SAFETY: AppKit calls on the main thread, where the UI runs.
        unsafe {
            let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
            let appearance: *mut Object = match dark {
                None => std::ptr::null_mut(),
                Some(dark) => {
                    let name = if dark {
                        "NSAppearanceNameDarkAqua\0"
                    } else {
                        "NSAppearanceNameAqua\0"
                    };
                    let name: *mut Object = msg_send![class!(NSString), stringWithUTF8String: name.as_ptr()];
                    msg_send![class!(NSAppearance), appearanceNamed: name]
                }
            };
            let _: () = msg_send![app, setAppearance: appearance];
        }
    }
    #[cfg(not(target_os = "macos"))]
    let _ = dark;
}

#[cfg(test)]
mod tests {
    use super::is_gnome;

    #[test]
    fn gnome_is_recognized_in_current_desktop() {
        assert!(is_gnome(Some("GNOME")));
        assert!(is_gnome(Some("ubuntu:GNOME")));
        assert!(is_gnome(Some("Pop:GNOME")));
        assert!(!is_gnome(Some("KDE")));
        assert!(is_gnome(Some("GNOME-Flashback:GNOME")));
        assert!(!is_gnome(Some("XFCE")));
        assert!(!is_gnome(None));
    }
}
