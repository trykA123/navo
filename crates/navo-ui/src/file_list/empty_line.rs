//! The file list's calm centered card for a finished listing with no rows: empty folder or filter.

use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    App, InteractiveElement, ParentElement, Role, StatefulInteractiveElement, Styled, Window, div,
};
use navo_l10n::t;

use crate::pane_chrome::{StateCard, state_card};
use crate::primitives::{Button, ButtonVariant, IconName};
use crate::tokens::Theme;
use crate::{AnyElement, Div, IntoElement};

pub(super) type ClearHandler = Rc<dyn Fn(&mut Window, &mut App)>;

/// Builds the centered state card: filter-no-match with clear action, or empty-folder card.
pub(super) fn empty_line(
    t: &Theme,
    is_filtered: bool,
    on_clear: Option<ClearHandler>,
) -> impl IntoElement + Styled {
    let (status_label, glyph, title, subtitle, action) = if is_filtered {
        let button = Button::new("list-empty-clear-filter", t!("pane-empty-clear-filter"))
            .variant(ButtonVariant::Primary)
            .when_some(on_clear, |button, clear| {
                button.on_click(move |_, window, cx| clear(window, cx))
            })
            .into_any_element();
        (
            t!("pane-empty-filter"),
            IconName::Search,
            t!("pane-empty-filter-title"),
            t!("pane-empty-filter-hint"),
            Some(button),
        )
    } else {
        (
            t!("pane-empty-folder"),
            IconName::FolderOpen,
            t!("pane-empty-folder"),
            t!("pane-empty-folder-hint"),
            None,
        )
    };
    div()
        .id("list-empty")
        .role(Role::Status)
        .aria_label(status_label)
        .test_support()
        .flex()
        .flex_1()
        .size_full()
        .child(state_card(
            t,
            StateCard {
                card_id: "list-empty-card".into(),
                icon: glyph,
                is_error: false,
                title: title.into(),
                subtitle: subtitle.into(),
                action,
            },
        ))
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
