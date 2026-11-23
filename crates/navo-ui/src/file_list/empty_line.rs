//! The file list's one muted line for a finished listing with no rows: an empty folder or a filter.

use gpui_kit::base::TestSupportExt;
use gpui_kit::{InteractiveElement, ParentElement, Role, StatefulInteractiveElement, Styled, div};
use navo_l10n::t;

use crate::tokens::Theme;
use crate::{AnyElement, Div, IntoElement};

/// Builds the centred status line: the filter text when a filter hides every row, else the empty-folder text.
pub(super) fn empty_line(t: &Theme, is_filtered: bool) -> impl IntoElement + Styled {
    let text = if is_filtered {
        t!("pane-empty-filter")
    } else {
        t!("pane-empty-folder")
    };
    div()
        .id("list-empty")
        .role(Role::Status)
        .aria_label(text.to_string())
        .test_support()
        .flex()
        .flex_1()
        .items_center()
        .justify_center()
        .p(t.spacing.lg)
        .text_size(t.typography.body)
        .text_color(t.colors.muted)
        .child(text.to_string())
}

/// Lays the optional `note` over the list `area` so the area keeps its drop target and its space.
pub(super) fn over(area: Div, note: Option<impl IntoElement + Styled>) -> AnyElement {
    let Some(note) = note else {
        return area.into_any_element();
    };
    div()
        .relative()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(area)
        .child(note.absolute().inset_0())
        .into_any_element()
}
