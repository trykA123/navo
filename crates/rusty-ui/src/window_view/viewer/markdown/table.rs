use std::cell::RefCell;
use std::collections::HashMap;

use gpui_kit::base::{Scrollbar, ScrollbarMode, TestSupportExt};
use gpui_kit::{ScrollHandle, TextRun, font};
use rusty_structured::markdown::{Align, Block, BlockKind, MAX_COLUMNS};

use super::block::{Page, text_element};
use super::runs::{RunLook, runs};
use crate::prelude::*;

const SLACK: f32 = 1.;

#[derive(Default)]
pub(super) struct TableScrolls(RefCell<HashMap<u32, ScrollHandle>>);

impl TableScrolls {
    fn handle(&self, table: u32) -> ScrollHandle {
        self.0.borrow_mut().entry(table).or_default().clone()
    }
}

fn aligned<E: Styled>(cell: E, align: Align) -> E {
    match align {
        Align::Center => cell.text_center(),
        Align::Right => cell.text_right(),
        Align::None | Align::Left => cell.text_left(),
    }
}

fn row_look(page: &Page, is_head: bool) -> RunLook<'_> {
    RunLook {
        t: &page.t,
        is_mono: false,
        weight: if is_head {
            FontWeight::SEMIBOLD
        } else {
            FontWeight::NORMAL
        },
        color: page.t.colors.text,
        picked: page.picked,
    }
}

fn word_width(page: &Page, (table, column): (u32, usize), window: &Window) -> Pixels {
    let t = &page.t;
    let Some(word) = page.doc.column_word(table, column) else {
        return px(0.);
    };
    let mut face = font(if word.is_mono {
        t.typography.mono_font.clone()
    } else {
        t.typography.ui_font.clone()
    });
    face.weight = FontWeight::BOLD;
    let run = TextRun {
        len: word.text.len(),
        font: face,
        color: t.colors.text,
        background_color: None,
        underline: None,
        strikethrough: None,
    };
    let line = window
        .text_system()
        .layout_line(word.text, t.typography.small, &[run], None);
    line.width.ceil() + px(SLACK)
}

fn is_last_row(page: &Page, index: usize, table: u32) -> bool {
    !page.doc.blocks().get(index + 1).is_some_and(
        |next| matches!(next.kind, BlockKind::Row { table: next_table, .. } if next_table == table),
    )
}

fn scrollbar(page: &Page, table: u32, handle: &ScrollHandle) -> Option<impl IntoElement> {
    if handle.max_offset().x <= px(0.) {
        return None;
    }
    Some(
        div()
            .id(("md-table-bar", table as usize))
            .test_support()
            .relative()
            .w_full()
            .h(page.t.spacing.xxl)
            .child(
                Scrollbar::horizontal(handle)
                    .id(("md-table-scrollbar", table as usize))
                    .viewport_from_layout()
                    .mode(ScrollbarMode::Always),
            ),
    )
}

fn cells(page: &Page, block: &Block, table_id: u32, window: &Window) -> (Vec<AnyElement>, Pixels) {
    let t = &page.t;
    let Some(table) = page.doc.table(table_id) else {
        return (Vec::new(), px(0.));
    };
    let is_head = matches!(block.kind, BlockKind::Row { is_head: true, .. });
    let look = row_look(page, is_head);
    let mut total = px(2.);
    let cells = table
        .widths()
        .iter()
        .zip(table.alignments())
        .take(MAX_COLUMNS)
        .enumerate()
        .map(|(column, (width, align))| {
            let basis = word_width(page, (table_id, column), window) + t.spacing.md * 2.;
            total += basis;
            let shown = runs(&page.doc, block, u8::try_from(column).ok(), &look);
            aligned(
                div()
                    .id(("md-col", column))
                    .test_support()
                    .flex_basis(basis)
                    .flex_grow(f32::from(*width))
                    .flex_shrink_0()
                    .px(t.spacing.md)
                    .py(t.spacing.sm),
                *align,
            )
            .child(text_element(page, ("md-cell", column), shown))
            .into_any_element()
        })
        .collect();
    (cells, total)
}

pub(super) fn table_row(
    page: &Page,
    block: &Block,
    (table_id, is_head): (u32, bool),
    (index, window): (usize, &Window),
) -> AnyElement {
    let t = &page.t;
    let (cells, total) = cells(page, block, table_id, window);
    let row = div()
        .id(("md-row", index))
        .test_support()
        .flex()
        .w_full()
        .min_w(total)
        .text_size(t.typography.small)
        .border_x_1()
        .border_b_1()
        .border_color(t.colors.line)
        .when(is_head, |head| {
            head.border_t_1().bg(t.colors.rail).rounded_t(t.radii.sm)
        })
        .when(!is_head && index.is_multiple_of(2), |row| {
            row.bg(t.colors.zebra)
        })
        .children(cells);
    let handle = page.tables.handle(table_id);
    let scroller = div()
        .id(("md-table", index))
        .test_support()
        .w_full()
        .overflow_x_scroll()
        .restrict_scroll_to_axis()
        .track_scroll(&handle)
        .child(row);
    let bar = is_last_row(page, index, table_id)
        .then(|| scrollbar(page, table_id, &handle))
        .flatten();
    div()
        .w_full()
        .flex()
        .flex_col()
        .child(scroller)
        .children(bar)
        .into_any_element()
}
