//! Undo and redo.

use super::*;

impl BadgeApp {
    /// Records the change since the last call (if any) as an undo step.
    pub(super) fn commit_history(&mut self) {
        if self.project == self.history.baseline.project {
            return;
        }
        let kind = self.edit_hint.take().unwrap_or(EditKind::Other);
        let h = &mut self.history;
        let recent = h.last_at.elapsed() < Duration::from_millis(1200);
        let group = match kind {
            EditKind::Stroke(_) => h.last_kind == Some(kind),
            EditKind::Text | EditKind::Slider => h.last_kind == Some(kind) && recent,
            EditKind::Other => false,
        };
        if !group || h.undo.is_empty() {
            h.undo.push(h.baseline.clone());
            if h.undo.len() > HISTORY_LIMIT {
                h.undo.remove(0);
            }
        }
        h.redo.clear();
        h.baseline = Snapshot {
            project: self.project.clone(),
            current: self.current,
        };
        h.last_kind = Some(kind);
        h.last_at = std::time::Instant::now();
    }

    /// Forgets all history (after New/Open).
    pub(super) fn reset_history(&mut self) {
        self.history.undo.clear();
        self.history.redo.clear();
        self.history.baseline = Snapshot {
            project: self.project.clone(),
            current: self.current,
        };
        self.history.last_kind = None;
        self.edit_hint = None;
        self.import_source = None;
    }

    pub(super) fn restore(&mut self, snap: Snapshot, cx: &mut Context<Self>) {
        self.project = snap.project.clone();
        self.current = snap.current.min(MESSAGE_COUNT - 1);
        self.history.baseline = snap;
        self.history.last_kind = None;
        self.preview_step = 0;
        self.sync_input(cx);
        cx.notify();
    }

    pub(super) fn undo(&mut self, _: &Undo, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_history();
        if let Some(prev) = self.history.undo.pop() {
            let now = self.history.baseline.clone();
            self.history.redo.push(now);
            self.restore(prev, cx);
            self.set_status("Undone.", Severity::Info);
        }
    }

    pub(super) fn redo(&mut self, _: &Redo, _: &mut Window, cx: &mut Context<Self>) {
        self.commit_history();
        if let Some(next) = self.history.redo.pop() {
            let now = self.history.baseline.clone();
            self.history.undo.push(now);
            self.restore(next, cx);
            self.set_status("Redone.", Severity::Info);
        }
    }
}
