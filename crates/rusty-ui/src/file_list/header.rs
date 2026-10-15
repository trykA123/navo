use std::rc::Rc;

use gpui_kit::base::TestSupportExt;
use gpui_kit::prelude::FluentBuilder;
use gpui_kit::{
    Div, FontWeight, InteractiveElement, IntoElement, ParentElement, Pixels, Role,
    StatefulInteractiveElement, Styled, div, px,
};

use super::row::{columns, date_column, ext_column};
use super::{FileListAction, ListContext, SortBy, SortKey};
use crate::primitives::{IconName, icon};
use crate::tokens::Theme;

struct Column {
    by: SortBy,
    index: usize,
    label: &'static str,
    width: Option<Pixels>,
}

fn columns_of(t: &Theme) -> [Column; 4] {
    [
        Column {
            by: SortBy::Name,
            index: 0,
            label: "Name",
            width: None,
        },
        Column {
            by: SortBy::Ext,
            index: 1,
            label: "Ext",
            width: Some(ext_column(t)),
        },
        Column {
            by: SortBy::Size,
            index: 2,
            label: "Size",
            width: Some(t.spacing.size_column),
        },
        Column {
            by: SortBy::Date,
            index: 3,
            label: "Date",
            width: Some(date_column(t)),
        },
    ]
}

fn cell(t: &Theme, column: Column, sort: SortKey, ctx: &Rc<ListContext>) -> impl IntoElement {
    let is_active = sort.by == column.by;
    let arrow = if sort.descending {
        IconName::ArrowDown
    } else {
        IconName::ArrowUp
    };
    let right = matches!(column.by, SortBy::Size | SortBy::Date);
    let by = column.by;
    let ctx = ctx.clone();
    div()
        .id(("sort", column.index))
        .role(Role::Button)
        .aria_label(column.label)
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
        .when(is_active, |cell| cell.text_color(t.colors.text))
        .on_click(move |_, window, cx| ctx.act(FileListAction::Sort(by), window, cx))
        .child(column.label)
        .when(is_active, |cell| {
            cell.child(icon(arrow, px(10.), t.colors.accent_text))
        })
}

pub(super) fn header(t: &Theme, sort: SortKey, ctx: &Rc<ListContext>) -> Div {
    let c = &t.colors;
    columns(t)
        .flex_none()
        .h(t.spacing.row_height)
        .text_size(t.typography.tiny)
        .font_weight(FontWeight::SEMIBOLD)
        .text_color(c.muted)
        .bg(c.rail)
        .border_t_1()
        .border_b_1()
        .border_color(c.line)
        .children(
            columns_of(t)
                .into_iter()
                .map(|column| cell(t, column, sort, ctx)),
        )
}
