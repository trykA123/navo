//! The sidebar's Recent entries (B5): a right click asks the shell for the start row menu.
//! A child of `sidebar_actions`, so it reads the sidebar's private parts.

use gpui_kit::base::TestSupportExt as _;
use gpui_kit::{AnyElement, ClickEvent, MouseButton};
use sidewise_kit::prelude::*;

use super::super::{Entry, Sidebar};
use super::SidebarEvent;
use crate::theme_data::palettes::ModeTokens;

impl Sidebar {
    /// Recent entry `index`: the static row, whose right click emits `RecentMenu(index, at)`.
    /// The right press keeps the focus where it is, so the menu can give it back on close.
    pub(in super::super) fn recent_row(
        &self,
        (index, entry): (usize, &Entry),
        m: &ModeTokens,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        self.row_view(entry, m, cx)
            .id(("Recent", index))
            .aria_label(entry.label.clone())
            .test_support()
            // Stops the sidebar root's `track_focus` from taking the focus on this press.
            .on_mouse_down(MouseButton::Right, |_, window, _| window.prevent_default())
            .on_aux_click(cx.listener(move |_, click: &ClickEvent, _, cx| {
                if click.is_right_click() {
                    cx.stop_propagation();
                    cx.emit(SidebarEvent::RecentMenu(index, click.position()));
                }
            }))
            .into_any_element()
    }
}
