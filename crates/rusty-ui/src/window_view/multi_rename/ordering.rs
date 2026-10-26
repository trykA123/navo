use std::sync::Arc;
use std::time::Duration;

use super::FerriteWindow;
use crate::prelude::*;

impl FerriteWindow {
    pub(super) fn move_rename_row(
        &mut self,
        forward: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_rename_names(window, cx);
        let Some(open) = self.multi_rename.open.as_ref() else {
            return;
        };
        let Some(row) = open.view.read(cx).cursor() else {
            return;
        };
        if row >= open.source.items.len() {
            return;
        }
        let next = if forward {
            row.checked_add(1)
        } else {
            row.checked_sub(1)
        };
        let Some(next) = next.filter(|&next| next < open.source.items.len()) else {
            return;
        };
        let mut order: Vec<_> = (0..open.source.items.len()).collect();
        order.swap(row, next);
        self.reorder_rename_rows((&order, Some(next)), window, cx);
        if let Some(open) = self.multi_rename.open.as_mut() {
            open.sorted = None;
        }
    }

    pub(super) fn sort_rename_rows(
        &mut self,
        column: usize,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_rename_names(window, cx);
        let Some(open) = self
            .multi_rename
            .open
            .as_mut()
            .filter(|open| open.is_current())
        else {
            return;
        };
        if column > 2 {
            return;
        }
        let descending = open.sorted.is_some_and(|sorted| sorted == (column, false));
        let Some(preview) = &open.preview else { return };
        let mut order: Vec<_> = (0..open.source.items.len()).collect();
        let key = |row: usize| match column {
            1 => preview.rows[row].new.to_lowercase(),
            2 => format!("{:?}", preview.rows[row].state),
            _ => open.source.items[row].name.to_lowercase(),
        };
        order.sort_by_cached_key(|&row| key(row));
        if descending {
            order.reverse();
        }
        let cursor = open
            .view
            .read(cx)
            .cursor()
            .and_then(|row| order.iter().position(|&old| old == row));
        open.sorted = Some((column, descending));
        self.reorder_rename_rows((&order, cursor), window, cx);
    }

    fn reorder_rename_rows(
        &mut self,
        (order, cursor): (&[usize], Option<usize>),
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.sync_rename_names(window, cx);
        let Some(open) = self.multi_rename.open.as_mut() else {
            return;
        };
        if !open.form.reorder(order) {
            return;
        }
        open.source.items = Arc::new(
            order
                .iter()
                .map(|&row| open.source.items[row].clone())
                .collect(),
        );
        open.source.fields = Arc::default();
        open.run_waiting_for = None;
        if let Some(preview) = &mut open.preview {
            reorder_preview(preview, order);
        }
        open.push_preview(cx);
        open.view.update(cx, |view, cx| view.set_cursor(cursor, cx));
        if let (Some(editor), Some(names)) = (&open.names, open.form.edited_names()) {
            let text = names.join("\n");
            editor
                .input
                .update(cx, |input, cx| input.set_value(text, window, cx));
        }
        self.schedule_rename_preview(Duration::ZERO, window, cx);
        cx.notify();
    }
}

fn reorder_preview(preview: &mut crate::multi_rename::Preview, order: &[usize]) {
    use rusty_core::rename::check::{Problem, RowState};
    if preview.rows.len() != order.len() {
        return;
    }
    let mut inverse = vec![0; order.len()];
    for (new, &old) in order.iter().enumerate() {
        inverse[old] = new;
    }
    preview.rows = order
        .iter()
        .map(|&old| {
            let mut row = preview.rows[old].clone();
            if let RowState::Problem(Problem::Duplicate(other)) = row.state {
                row.state = RowState::Problem(Problem::Duplicate(inverse[other]));
            }
            row
        })
        .collect();
    preview.plan = None;
}
