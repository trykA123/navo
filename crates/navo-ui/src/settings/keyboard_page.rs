//! The Keyboard page: the modifier hint rows, then the shortcut list (or the button that
//! opens it), so Shortcuts and Modifier hints share one page.

use gpui_kit::ScrollHandle;

use super::fields::{Field, Section};
use super::find::Filter;
use super::help_page::group_title;
use super::steps::moved;
use super::view::{Look, row, section_heading};
use super::{Handler, SheetEvent};
use crate::prelude::*;
use crate::primitives::{Button, Kbd};
use crate::tokens::Theme;

const LAST_ROW: Field = Field::ModifierHintsDelay;

/// The ↓ key in the row list: the next row, or the shortcut list from the last Keyboard row.
pub(super) fn down_event(filter: &Filter, row: usize) -> SheetEvent {
    if filter.is_empty() && row == LAST_ROW.index() {
        return SheetEvent::Shortcuts;
    }
    SheetEvent::Select(moved(filter, row, true))
}

/// The row the page marks as current: none while the shortcut list has focus.
pub(super) fn shown_row(row: usize, shows_shortcuts: bool) -> usize {
    if shows_shortcuts { usize::MAX } else { row }
}

/// The page body: the shortcut list under the hint rows, or the page's rows.
pub(super) fn content(
    look: &Look,
    section: Section,
    shortcuts: Option<AnyElement>,
    scroll: Option<&ScrollHandle>,
) -> AnyElement {
    match shortcuts {
        Some(list) => with_shortcuts(look, list),
        None => {
            let rows = super::view::content(look, section, scroll);
            page(look, section, rows.into_any_element())
        }
    }
}

/// The shown page: `content` alone, or on the Keyboard page with no find, followed by the
/// way into the shortcut list.
fn page(look: &Look, section: Section, content: AnyElement) -> AnyElement {
    if section != Section::Keyboard || !look.filter.is_empty() {
        return content;
    }
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .child(content)
        .child(shortcuts_entry(look.theme, look.send.as_ref()))
        .into_any_element()
}

/// The Shortcuts group below the hint rows: a button, and ↓ from the last row.
fn shortcuts_entry(t: &Theme, send: Option<&Handler>) -> AnyElement {
    let send = send.cloned();
    let open = Button::new(
        "settings-keyboard-shortcuts",
        navo_l10n::tr("settings-keyboard-show-shortcuts"),
    )
    .on_click(move |_, window, cx| {
        if let Some(send) = &send {
            send(&SheetEvent::Shortcuts, window, cx);
        }
    });
    div()
        .flex()
        .flex_col()
        .flex_none()
        .px(t.spacing.md)
        .child(group_title(t, "settings-shortcuts"))
        .child(
            div()
                .flex()
                .items_center()
                .gap(t.spacing.md)
                .px(t.spacing.xl)
                .py(t.spacing.md)
                .child(open)
                .children(Kbd::new("settings-keyboard-shortcuts-key", "down")),
        )
        .into_any_element()
}

/// The page while the shortcut list has focus: the hint rows stay on top, unselected.
fn with_shortcuts(look: &Look, shortcuts: AnyElement) -> AnyElement {
    let t = look.theme;
    let hints =
        [Field::ModifierHints, LAST_ROW].map(|field| row(look, field.index(), field.spec()));
    div()
        .flex()
        .flex_col()
        .flex_1()
        .min_h_0()
        .min_w_0()
        .child(
            div()
                .flex()
                .flex_col()
                .flex_none()
                .gap(t.spacing.xxs)
                .px(t.spacing.md)
                .child(section_heading(t, Section::Keyboard))
                .child(group_title(t, "settings-group-modifier-hints"))
                .children(hints)
                .child(group_title(t, "settings-shortcuts")),
        )
        .child(shortcuts)
        .into_any_element()
}
