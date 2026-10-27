use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Role,
    StatefulInteractiveElement, Styled, div, px,
};

use super::row::{check_column, columns};
use super::{FileListAction, ListContext, ListParts, SortBy, SortKey, resize};
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

struct Column {
    by: SortBy,
    index: usize,
    label: &'static str,
    keys: &'static str,
}

pub(super) fn label(by: SortBy) -> &'static str {
    match by {
        SortBy::Name => "Name",
        SortBy::Ext => "Ext",
        SortBy::Size => "Size",
        SortBy::Date => "Date",
    }
}

pub(super) fn index_of(by: SortBy) -> usize {
    match by {
        SortBy::Name => 0,
        SortBy::Ext => 1,
        SortBy::Size => 2,
        SortBy::Date => 3,
    }
}

fn columns_of() -> [Column; 4] {
    [
        (SortBy::Name, "Ctrl+F3"),
        (SortBy::Ext, "Ctrl+F4"),
        (SortBy::Size, "Ctrl+F6"),
        (SortBy::Date, "Ctrl+F5"),
    ]
    .map(|(by, keys)| Column {
        by,
        index: index_of(by),
        label: label(by),
        keys,
    })
}

struct Sorted {
    key: SortKey,
    hidden: Option<&'static str>,
}

impl Sorted {
    fn of(key: SortKey, columns: &[Column], parts: ListParts) -> Self {
        let hidden = columns
            .iter()
            .find(|column| column.by == key.by && !parts.shows(column.by))
            .map(|column| column.label);
        Self { key, hidden }
    }

    fn marks(&self, column: &Column) -> bool {
        self.key.by == column.by || (column.by == SortBy::Name && self.hidden.is_some())
    }

    fn describe(&self) -> String {
        let way = if self.key.descending {
            "descending"
        } else {
            "ascending"
        };
        match self.hidden {
            Some(hidden) => format!("Sorted {way} by {hidden}"),
            None => format!("Sorted {way}"),
        }
    }

    fn note(&self, t: &Theme, column: &Column) -> Option<impl IntoElement + use<>> {
        let by = self.hidden.filter(|_| column.by == SortBy::Name)?;
        Some(
            div()
                .id("sort-hidden")
                .aria_label(format!("Sorted by {by}"))
                .test_support()
                .font_weight(FontWeight::NORMAL)
                .text_color(t.colors.muted)
                .child(format!("by {by}")),
        )
    }
}

fn arrow_slot(t: &Theme, sorted: &Sorted, is_active: bool) -> Div {
    let arrow = if sorted.key.descending {
        IconName::ArrowDown
    } else {
        IconName::ArrowUp
    };
    div().flex_none().w(px(10.)).when(is_active, |slot| {
        slot.child(icon(arrow, px(10.), t.colors.accent_text))
    })
}

fn cell(t: &Theme, column: Column, sorted: &Sorted, ctx: &Rc<ListContext>) -> impl IntoElement {
    let is_active = sorted.marks(&column);
    let right = matches!(column.by, SortBy::Size | SortBy::Date);
    let by = column.by;
    let width = ctx.widths.of(by);
    let handle = width.map(|_| resize::handle(t, by, ctx));
    let ctx = ctx.clone();
    div()
        .id(("sort", column.index))
        .role(Role::Button)
        .aria_label(column.label)
        .aria_keyshortcuts(column.keys)
        .when(is_active, |cell| cell.aria_description(sorted.describe()))
        .test_support()
        .flex()
        .items_center()
        .gap(t.spacing.xs)
        .relative()
        .cursor_pointer()
        .map(|cell| match width {
            Some(width) => cell.w(width).flex_none(),
            None => cell.flex_1().min_w_0(),
        })
        .when(right, |cell| cell.justify_end())
        .when(is_active, |cell| {
            cell.text_color(t.colors.text)
                .font_weight(FontWeight::SEMIBOLD)
        })
        .on_click(move |_, window, cx| ctx.act(FileListAction::Sort(by), window, cx))
        .child(
            div()
                .id(("sort-label", column.index))
                .test_support()
                .min_w_0()
                .truncate()
                .child(column.label),
        )
        .children(sorted.note(t, &column))
        .child(arrow_slot(t, sorted, is_active))
        .children(handle)
}

pub(super) fn header(t: &Theme, sort: SortKey, ctx: &Rc<ListContext>) -> Div {
    let c = &t.colors;
    let all = columns_of();
    let sorted = Sorted::of(sort, &all, ctx.parts);
    columns(t)
        .flex_none()
        .h(t.spacing.row_height)
        .text_size(t.typography.small)
        .text_color(c.muted)
        .child(div().w(check_column(t)).flex_none())
        .children(
            all.into_iter()
                .filter(|column| ctx.parts.shows(column.by))
                .map(|column| cell(t, column, &sorted, ctx)),
        )
}
