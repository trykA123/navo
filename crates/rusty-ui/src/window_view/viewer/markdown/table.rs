use rusty_structured::markdown::{Align, Block, MAX_COLUMNS};

use super::block::{Page, text_element};
use super::runs::{RunLook, runs};
use crate::prelude::*;

fn aligned(cell: Div, align: Align) -> Div {
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

pub(super) fn table_row(
    page: &Page,
    block: &Block,
    (table, is_head): (u32, bool),
    index: usize,
) -> AnyElement {
    let t = &page.t;
    let Some(table) = page.doc.table(table) else {
        return div().into_any_element();
    };
    let total: f32 = table.widths().iter().map(|width| f32::from(*width)).sum();
    let look = row_look(page, is_head);
    let cells = table
        .widths()
        .iter()
        .zip(table.alignments())
        .take(MAX_COLUMNS)
        .enumerate()
        .map(|(column, (width, align))| {
            let cell = u8::try_from(column).ok();
            let shown = runs(&page.doc, block, cell, &look);
            aligned(
                div()
                    .flex_none()
                    .w(relative(f32::from(*width) / total.max(1.)))
                    .min_w_0()
                    .px(t.spacing.md)
                    .py(t.spacing.sm),
                *align,
            )
            .child(text_element(page, ("md-cell", column), shown))
        });
    div()
        .flex()
        .w_full()
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
        .children(cells)
        .into_any_element()
}
