//! How the app answers macOS quitting it, or closing its document. The
//! answers are given in native_document.rs (only built on macOS, and not for
//! tests); the decisions are here, so tests check them on every system.

/// A quit from macOS (from the Dock, or by logging out): whether it has to
/// wait for the app to ask first, and whether the app has to be told to ask.
/// `approved`: the app already decided to quit. `unsaved`: there are unsaved
/// changes, or a send. `told`: the app was told and hasn't asked yet (macOS
/// can ask twice in one quit: the document, then the app).
pub(super) fn quit_waits(approved: bool, unsaved: bool, told: bool) -> (bool, bool) {
    let waits = !approved && unsaved;
    (waits, waits && !told)
}

/// The document's answer when macOS asks whether it may close: `None` for a
/// project with a file (NSDocument answers, without asking), otherwise yes
/// unless a quit `waits` for the app to ask first.
pub(super) fn document_may_close(has_file: bool, waits: bool) -> Option<bool> {
    (!has_file).then_some(!waits)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_quit_waits_only_for_unsaved_work_and_asks_once() {
        assert_eq!(quit_waits(false, false, false), (false, false), "nothing to ask");
        assert_eq!(quit_waits(false, true, false), (true, true), "the app is told");
        assert_eq!(quit_waits(false, true, true), (true, false), "told already");
        assert_eq!(quit_waits(true, true, false), (false, false), "the app decided");
    }

    #[test]
    fn only_a_project_without_a_file_gets_the_apps_answer() {
        assert_eq!(document_may_close(true, true), None);
        assert_eq!(document_may_close(true, false), None);
        assert_eq!(document_may_close(false, false), Some(true));
        assert_eq!(document_may_close(false, true), Some(false));
    }
}
