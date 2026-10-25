//! Linux: the send's progress on the app's taskbar or dock button, through
//! the `com.canonical.Unity.LauncherEntry` D-Bus signal. KDE Plasma's task
//! manager and Ubuntu's dock show it; desktops that don't listen for it
//! (GNOME's own, for one) ignore it, so it's sent wherever the app runs.
//! Only built on Linux.
//!
//! The signal goes out from a thread of its own, so connecting to the
//! session bus never holds up the window. Without a session bus, or when
//! sending fails, the progress just isn't shown.

use std::collections::HashMap;

use zbus::zvariant::Value;

/// The app's desktop file, which the taskbar matches to the window.
fn app_uri() -> String {
    format!("application://{}.desktop", crate::APP_ID)
}

/// The signal's properties: the bar at `fraction` (0 to 1), or none.
fn properties(fraction: Option<f64>) -> HashMap<&'static str, Value<'static>> {
    let mut props = HashMap::from([("progress-visible", Value::from(fraction.is_some()))]);
    if let Some(f) = fraction {
        props.insert("progress", Value::from(f.clamp(0.0, 1.0)));
    }
    props
}

/// Shows `fraction` (0 to 1) on the app's taskbar button, or takes the bar
/// away with `None`. Doesn't wait for it to be sent.
#[cfg(not(test))]
pub(super) fn set(fraction: Option<f64>) {
    use std::sync::OnceLock;
    use std::sync::mpsc::{Receiver, Sender, channel};

    static UPDATES: OnceLock<Sender<Option<f64>>> = OnceLock::new();

    fn send_updates(updates: Receiver<Option<f64>>) {
        let connection = match futures::executor::block_on(zbus::Connection::session()) {
            Ok(c) => c,
            Err(e) => {
                log::info!("no session bus, so no taskbar progress: {e}");
                return;
            }
        };
        let uri = app_uri();
        while let Ok(mut fraction) = updates.recv() {
            // Only the newest matters when several came in meanwhile.
            while let Ok(newer) = updates.try_recv() {
                fraction = newer;
            }
            let sent = futures::executor::block_on(connection.emit_signal(
                None::<zbus::names::BusName<'_>>,
                "/dev/uint/LedBadgeStudio",
                "com.canonical.Unity.LauncherEntry",
                "Update",
                &(uri.as_str(), properties(fraction)),
            ));
            if let Err(e) = sent {
                log::debug!("couldn't update the taskbar progress: {e}");
            }
        }
    }

    let updates = UPDATES.get_or_init(|| {
        let (tx, rx) = channel();
        let started = std::thread::Builder::new()
            .name("taskbar-progress".into())
            .spawn(move || send_updates(rx));
        if let Err(e) = started {
            log::warn!("couldn't start the taskbar progress thread: {e}");
        }
        tx
    });
    // Fails only once the thread has given up (no session bus).
    let _ = updates.send(fraction);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_the_apps_desktop_file() {
        assert_eq!(app_uri(), "application://dev.uint.LedBadgeStudio.desktop");
    }

    #[test]
    fn progress_is_shown_clamped_or_taken_away() {
        let shown = properties(Some(0.25));
        assert_eq!(shown["progress-visible"], Value::from(true));
        assert_eq!(shown["progress"], Value::from(0.25));
        assert_eq!(properties(Some(1.5))["progress"], Value::from(1.0));
        let gone = properties(None);
        assert_eq!(gone["progress-visible"], Value::from(false));
        assert!(!gone.contains_key("progress"));
    }
}
