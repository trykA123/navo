use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels, Role,
    StatefulInteractiveElement, Styled, div, px,
};

use super::row::{check_column, columns, date_column, ext_column};
use super::{FileListAction, ListContext, ListParts, SortBy, SortKey};
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

struct Column {
    by: SortBy,
    index: usize,
    label: &'static str,
    keys: &'static str,
    width: Option<Pixels>,
}

fn columns_of(t: &Theme) -> [Column; 4] {
    [
        Column {
            by: SortBy::Name,
            index: 0,
            label: "Name",
            keys: "Ctrl+F3",
            width: None,
        },
        Column {
            by: SortBy::Ext,
            index: 1,
            label: "Ext",
            keys: "Ctrl+F4",
            width: Some(ext_column(t)),
        },
        Column {
            by: SortBy::Size,
            index: 2,
            label: "Size",
            keys: "Ctrl+F6",
            width: Some(t.spacing.size_column),
        },
        Column {
            by: SortBy::Date,
            index: 3,
            label: "Date",
            keys: "Ctrl+F5",
            width: Some(date_column(t)),
        },
    ]
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
        .cursor_pointer()
        .map(|cell| match column.width {
            Some(width) => cell.w(width).flex_none(),
            None => cell.flex_1(),
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
                .child(column.label),
        )
        .children(sorted.note(t, &column))
        .child(arrow_slot(t, sorted, is_active))
}

pub(super) fn header(t: &Theme, sort: SortKey, ctx: &Rc<ListContext>) -> Div {
    let c = &t.colors;
    let all = columns_of(t);
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
