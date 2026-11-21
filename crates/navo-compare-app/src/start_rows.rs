//! The Recent and Named sections of the start screen: the row order, the sections and their drawing.
//! The rows are plain data from `navo_compare_profile::recent` (NS13); this file reads no file.

use std::cell::Cell;
use std::rc::Rc;

use gpui_kit::{Bounds, FocusHandle, Pixels, TestSupportExt, canvas};
use navo_compare_profile::recent::{RowKind, StartRow, StartRows};
use sidewise_kit::prelude::*;
use sidewise_kit::tokens;

use crate::start::StartScreen;
use crate::start_menu::{CopyPaths, ForgetSelected, ShowRowMenu};

/// Where the selected row was last drawn; shared with the screen for the menu key.
pub(crate) type RowBounds = Rc<Cell<Option<Bounds<Pixels>>>>;

/// Key context of the row list; the Up, Down and Enter keys are bound inside it.
pub(crate) const LIST_CONTEXT: &str = "StartList";

/// Number of rows over both sections (recent first, then named).
pub(crate) fn row_count(rows: &StartRows) -> usize {
    rows.recent.len() + rows.named.len()
}

/// The row at `index` over both sections, or `None` past the end.
pub(crate) fn row_at(rows: &StartRows, index: usize) -> Option<&StartRow> {
    rows.recent.iter().chain(rows.named.iter()).nth(index)
}

/// The sections that have rows: title, rows and flat position of the first row.
fn sections(rows: &StartRows) -> Vec<(&'static str, &[StartRow], usize)> {
    let all = [
        ("Recent", rows.recent.as_slice(), 0),
        ("Named", rows.named.as_slice(), rows.recent.len()),
    ];
    all.into_iter()
        .filter(|(_, rows, _)| !rows.is_empty())
        .collect()
}

/// The titles of the sections that have rows, in drawing order.
#[cfg(test)]
pub(crate) fn section_titles(rows: &StartRows) -> Vec<&'static str> {
    sections(rows)
        .into_iter()
        .map(|(title, _, _)| title)
        .collect()
}

/// The label of a row's kind.
fn kind_label(kind: &RowKind) -> &'static str {
    match kind {
        RowKind::GitRefs => "Git refs",
        RowKind::Paths => "Paths",
        RowKind::Mixed => "Mixed",
    }
}

/// A section heading.
fn heading(title: &'static str, cx: &App) -> Div {
    let t = tokens::current(cx);
    div()
        .px(t.spacing.md)
        .pt(t.spacing.xxl)
        .pb(t.spacing.xs)
        .text_size(t.typography.tiny)
        .text_color(t.colors.faint)
        .child(title)
}

/// The cells of a row: name, both sides, kind and age.
fn cells(row: &StartRow, muted: Hsla, cx: &App) -> Vec<AnyElement> {
    let t = tokens::current(cx);
    let side = |text: &str| {
        div()
            .font_family(t.typography.mono_font.clone())
            .child(text.to_owned())
    };
    let age = div()
        .flex_1()
        .flex()
        .justify_end()
        .text_color(muted)
        .child(row.age.clone());
    let name = row.name.as_ref().map(|name| div().child(name.clone()));
    name.into_iter()
        .chain([
            side(&row.left),
            div().text_color(muted).child("\u{2194}"),
            side(&row.right),
            div().text_color(muted).child(kind_label(&row.kind)),
            age,
        ])
        .map(IntoElement::into_any_element)
        .collect()
}

/// One row at flat position `index`; a click selects it, a double click opens it, a right click
/// opens its menu. The selected row (`probe` is set) is highlighted and reports its bounds.
fn row_view(
    index: usize,
    row: &StartRow,
    probe: Option<RowBounds>,
    cx: &Context<StartScreen>,
) -> impl IntoElement {
    let t = tokens::current(cx);
    let (background, text, muted) = if probe.is_some() {
        (
            t.colors.selection,
            t.colors.on_selection,
            t.colors.on_selection,
        )
    } else {
        (gpui_kit::transparent_black(), t.colors.text, t.colors.muted)
    };
    div()
        .id(("start-row", index))
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.lg)
        .w_full()
        .h(t.spacing.row_height)
        .px(t.spacing.md)
        .rounded(t.radii.sm)
        .bg(background)
        .text_color(text)
        .text_size(t.typography.small)
        .cursor_pointer()
        .hover(|style| style.bg(t.colors.hover))
        .on_click(cx.listener(move |this, event: &ClickEvent, _, cx| {
            this.click_row(index, cx);
            if event.click_count() >= 2 {
                this.open_selected(cx);
            }
        }))
        .on_aux_click(cx.listener(move |this, event: &ClickEvent, window, cx| {
            if event.is_secondary() {
                this.click_row(index, cx);
                let at = window.mouse_position();
                this.show_row_menu(at, window, cx);
            }
        }))
        .children(probe.map(|cell| {
            canvas(move |bounds, _, _| cell.set(Some(bounds)), |_, (), _, _| {})
                .absolute()
                .size_full()
        }))
        .children(cells(row, muted, cx))
}

/// The scrollable row list: a heading and the rows of each section that has rows.
/// `selected` is a flat position; `focus` makes the list a keyboard target.
pub(crate) fn rows_view(
    rows: &StartRows,
    selected: Option<usize>,
    (focus, bounds): (&FocusHandle, &RowBounds),
    cx: &Context<StartScreen>,
) -> impl IntoElement {
    let mut list = div()
        .id("start-rows")
        .test_support()
        .key_context(LIST_CONTEXT)
        .track_focus(focus)
        .flex_1()
        .min_h_0()
        .w_full()
        .overflow_y_scroll()
        .on_action(cx.listener(|this, _: &CopyPaths, _, cx| this.copy_paths(cx)))
        .on_action(cx.listener(|this, _: &ForgetSelected, _, cx| this.forget_selected(cx)))
        .on_action(cx.listener(|this, _: &ShowRowMenu, window, cx| {
            this.show_row_menu_at_row(window, cx);
        }));
    for (title, section, first) in sections(rows) {
        list = list.child(heading(title, cx));
        for (offset, row) in section.iter().enumerate() {
            let index = first + offset;
            list = list.child(row_view(
                index,
                row,
                (selected == Some(index)).then(|| bounds.clone()),
                cx,
            ));
        }
    }
    list
}
