//! Status-bar Find results chip wired to `FocusFindResults`.

use std::rc::Rc;

use navo_core::PaneId;
use navo_l10n::t;

use super::super::{NavoWindow, keys};
use crate::find_results::book::FindTab;
use crate::prelude::*;
use crate::status_bar::FindChip;

impl NavoWindow {
    /// Builds the clickable status-bar chip for the active pane when find tabs exist.
    pub(in crate::window_view) fn find_chip_for(&self, pane: PaneId, cx: &App) -> Option<FindChip> {
        if self.browser.state().focused_pane() != pane {
            return None;
        }
        let book = self.find.book().read(cx);
        let count = book.len();
        if count == 0 {
            return None;
        }
        let running = book.tabs().iter().any(FindTab::is_running);
        let label = if count == 1 {
            t!("find-status-chip").into()
        } else {
            t!("find-status-chip-count", count = count).into()
        };
        Some(FindChip {
            label,
            running,
            on_click: Rc::new(|_, window, cx| {
                window.dispatch_action(Box::new(keys::FocusFindResults), cx);
            }),
        })
    }
}
