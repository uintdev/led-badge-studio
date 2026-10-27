//! How the app answers macOS quitting it, or closing its document. The
//! answers are given in native_document.rs (only built on macOS, and not for
//! tests); the decisions are here, so tests check them on every system.

use std::sync::atomic::{AtomicBool, Ordering};

/// Where the app stands when macOS asks to quit it.
#[derive(Clone, Copy)]
pub(super) struct QuitState {
    /// The app already decided to quit (its own Quit asked what it needed to).
    pub(super) approved: bool,
    /// There's something to ask about: unsaved changes, or a send.
    pub(super) unsaved: bool,
}

/// Quits from macOS (from the Dock, or by logging out) that wait for the
/// app to ask first, and whether the app has been told to ask and hasn't
/// yet: macOS can ask twice in one quit (the document, then the app), and
/// the app asks once.
pub(super) struct QuitRequests {
    told: AtomicBool,
}

impl QuitRequests {
    pub(super) const fn new() -> Self {
        QuitRequests {
            told: AtomicBool::new(false),
        }
    }

    /// A quit from macOS: whether it waits for the app to ask first. It
    /// doesn't once the app decided to quit, or with nothing to ask about.
    /// When it waits, the app is told to ask (`tell`, which says whether the
    /// app could be told) unless it was already; an app that can't be told
    /// can't ask, so the quit goes ahead.
    pub(super) fn quit(&self, state: QuitState, tell: impl FnOnce() -> bool) -> bool {
        if state.approved || !state.unsaved {
            return false;
        }
        if !self.told.load(Ordering::Relaxed) {
            if !tell() {
                return false;
            }
            self.told.store(true, Ordering::Relaxed);
        }
        true
    }

    /// macOS asking whether the document may close other than from its
    /// window, which it does only when quitting. `None` for a project with a
    /// file: NSDocument answers, which doesn't ask, and the app isn't told.
    /// Otherwise yes unless the quit waits for the app (see `quit`).
    pub(super) fn document_may_close(
        &self,
        has_file: bool,
        state: QuitState,
        tell: impl FnOnce() -> bool,
    ) -> Option<bool> {
        (!has_file).then(|| !self.quit(state, tell))
    }

    /// The app asked, or can no longer: the next quit tells it again.
    pub(super) fn asked(&self) {
        self.told.store(false, Ordering::Relaxed);
    }
}

/// The window's close button, as far as the document goes: `None` for a
/// project with a file (NSDocument answers, which doesn't ask); otherwise
/// yes, and the window's close then asks the app.
pub(super) fn window_may_close(has_file: bool) -> Option<bool> {
    (!has_file).then_some(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    const UNSAVED: QuitState = QuitState {
        approved: false,
        unsaved: true,
    };
    const SAVED: QuitState = QuitState {
        approved: false,
        unsaved: false,
    };
    const APPROVED: QuitState = QuitState {
        approved: true,
        unsaved: true,
    };

    #[test]
    fn a_quit_waits_only_for_unsaved_work() {
        let q = QuitRequests::new();
        assert!(!q.quit(SAVED, || panic!("nothing to ask")));
        assert!(!q.quit(APPROVED, || panic!("the app decided")));
    }

    #[test]
    fn the_app_is_told_once_until_it_asks() {
        let q = QuitRequests::new();
        let mut told = 0;
        assert!(q.quit(UNSAVED, || {
            told += 1;
            true
        }));
        // macOS asks again in the same quit.
        assert!(q.quit(UNSAVED, || panic!("told already")));
        assert_eq!(told, 1);
        q.asked();
        assert!(q.quit(UNSAVED, || true), "a later quit tells it again");
    }

    #[test]
    fn an_app_that_cant_be_told_lets_the_quit_go_ahead() {
        let q = QuitRequests::new();
        assert!(!q.quit(UNSAVED, || false));
        assert!(q.quit(UNSAVED, || true), "not counted as told");
    }

    #[test]
    fn only_a_project_without_a_file_gets_the_apps_answer() {
        let q = QuitRequests::new();
        assert_eq!(q.document_may_close(true, UNSAVED, || panic!("not told")), None);
        assert_eq!(
            q.document_may_close(false, SAVED, || panic!("nothing to ask")),
            Some(true)
        );
        assert_eq!(q.document_may_close(false, UNSAVED, || true), Some(false));
        assert_eq!(window_may_close(true), None);
        assert_eq!(window_may_close(false), Some(true));
    }
}
