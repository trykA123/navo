//! The F3 editor's soft-wrap toggle (Alt+Z, View menu): one choice per window for the session,
//! applied while the editor edits. Split from `view_menu.rs` so that file stays under the size limit.

use crate::prelude::*;
use crate::window_view::NavoWindow;
use crate::window_view::viewer::ToggleSoftWrap;
use crate::window_view::viewer::body::{MenuEntry, ViewerBody};

/// Puts the checked "Soft wrap" row after the part toggles of `entries` when `body` offers it.
pub(super) fn add_soft_wrap(entries: &mut Vec<MenuEntry>, body: &dyn ViewerBody, cx: &App) {
    let Some(is_on) = body.soft_wrap(cx) else {
        return;
    };
    let at = entries
        .iter()
        .position(|entry| matches!(entry, MenuEntry::Break))
        .unwrap_or(entries.len());
    let label = navo_l10n::tr("viewer-soft-wrap-menu").into();
    entries.insert(at, MenuEntry::Run(label, Box::new(ToggleSoftWrap), is_on));
}

impl NavoWindow {
    /// Flips this window's soft wrap while the editor edits; otherwise the key passes on.
    pub(super) fn toggle_soft_wrap(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let body = self.viewer.as_ref().and_then(|viewer| viewer.body.as_ref());
        if !body.is_some_and(|body| body.soft_wrap(cx).is_some()) {
            cx.propagate();
            return;
        }
        self.editor_wraps = !self.editor_wraps;
        if let Some(body) = self.viewer.as_ref().and_then(|viewer| viewer.body.as_ref()) {
            body.show_soft_wrap(self.editor_wraps, cx);
        }
        self.refresh_view_menu(window, cx);
        cx.notify();
    }
}
