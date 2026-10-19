use gpui_kit::{ClipboardItem, Stateful};

use super::TableView;
use crate::prelude::*;
use crate::window_view::viewer::body::MenuEntry;
use crate::window_view::viewer::{
    GoEnd, GoStart, LINE_BREAK, PageDown, PageUp, RowDown, RowUp, ScrollLeft, ScrollRight,
};

pub(super) const CONTEXT: &str = "Table";

gpui_kit::actions!(
    viewer_table,
    [
        CellFirst, CellLast, CopyCell, CopyRow, CopyColumn, SortColumn
    ]
);

pub(in crate::window_view::viewer) fn bind_keys(cx: &mut App) {
    crate::commands::bind_namespace(cx, "viewer_table");
}

pub(super) fn menu_entries() -> Vec<MenuEntry> {
    vec![
        MenuEntry::Run("Copy cell", Box::new(CopyCell), false),
        MenuEntry::Run("Copy row", Box::new(CopyRow), false),
        MenuEntry::Run("Copy column", Box::new(CopyColumn), false),
        MenuEntry::Break,
        MenuEntry::Run("Sort by this column", Box::new(SortColumn), false),
    ]
}

fn step(at: usize, by: i64, len: usize) -> usize {
    let moved = i64::try_from(at)
        .unwrap_or(i64::MAX)
        .saturating_add(by)
        .max(0);
    usize::try_from(moved)
        .unwrap_or(usize::MAX)
        .min(len.saturating_sub(1))
}

impl TableView {
    pub(super) fn move_to(&mut self, row: usize, column: usize, cx: &mut Context<Self>) {
        let row = row.min(self.rows.len().saturating_sub(1));
        let column = column.min(self.columns().saturating_sub(1));
        self.cursor = (row, column);
        self.scroll.scroll_to(row);
        if column < self.left {
            self.left = column;
        }
        while self.left < column && !self.shows_column(column) {
            self.left += 1;
        }
        self.note = None;
        cx.notify();
    }

    fn move_by(&mut self, rows: i64, columns: i64, cx: &mut Context<Self>) {
        let (row, column) = self.cursor;
        let row = step(row, rows, self.rows.len());
        let column = step(column, columns, self.columns());
        self.move_to(row, column, cx);
    }

    fn page_rows(&self) -> i64 {
        i64::try_from(self.page.max(1)).unwrap_or(1)
    }

    fn copy(&mut self, whole_row: bool, cx: &mut Context<Self>) {
        let (row, column) = self.cursor;
        let text = if whole_row {
            self.rows.get(row).map(|(_, cells)| cells.join("\t"))
        } else {
            self.cell(row, column).map(str::to_owned)
        };
        if let Some(text) = text {
            cx.write_to_clipboard(ClipboardItem::new_string(text));
            let note = if whole_row {
                "Copied the row"
            } else {
                "Copied the cell"
            };
            self.set_note(note, cx);
        }
    }

    fn copy_column(&mut self, cx: &mut Context<Self>) {
        let column = self.cursor.1;
        let Some(header) = self.header.get(column) else {
            return;
        };
        let total = self.rows.len();
        let mut lines = vec![header.as_str()];
        lines.extend((0..total).filter_map(|row| {
            let (_, cells) = self.rows.get(row)?;
            Some(cells.get(column).map_or("", String::as_str))
        }));
        let copied = lines.len() - 1;
        cx.write_to_clipboard(ClipboardItem::new_string(lines.join(LINE_BREAK)));
        let note = if copied < total {
            format!("Copied the {copied} loaded rows of {total}.")
        } else {
            format!("Copied the {header} column.")
        };
        self.set_note(note, cx);
    }

    pub(super) fn keys(root: Stateful<Div>, cx: &mut Context<Self>) -> Stateful<Div> {
        root.on_action(cx.listener(|view, _: &RowUp, _, cx| view.move_by(-1, 0, cx)))
            .on_action(cx.listener(|view, _: &RowDown, _, cx| view.move_by(1, 0, cx)))
            .on_action(cx.listener(|view, _: &PageUp, _, cx| {
                view.move_by(-view.page_rows(), 0, cx);
            }))
            .on_action(cx.listener(|view, _: &PageDown, _, cx| {
                view.move_by(view.page_rows(), 0, cx);
            }))
            .on_action(cx.listener(|view, _: &GoStart, _, cx| {
                view.move_to(0, view.cursor.1, cx);
            }))
            .on_action(cx.listener(|view, _: &GoEnd, _, cx| {
                view.move_to(usize::MAX, view.cursor.1, cx);
            }))
            .on_action(cx.listener(|view, _: &ScrollLeft, _, cx| view.move_by(0, -1, cx)))
            .on_action(cx.listener(|view, _: &ScrollRight, _, cx| view.move_by(0, 1, cx)))
            .on_action(cx.listener(|view, _: &CellFirst, _, cx| {
                view.move_to(view.cursor.0, 0, cx);
            }))
            .on_action(cx.listener(|view, _: &CellLast, _, cx| {
                view.move_to(view.cursor.0, usize::MAX, cx);
            }))
            .on_action(cx.listener(|view, _: &CopyCell, _, cx| view.copy(false, cx)))
            .on_action(cx.listener(|view, _: &CopyRow, _, cx| view.copy(true, cx)))
            .on_action(cx.listener(|view, _: &CopyColumn, _, cx| view.copy_column(cx)))
            .on_action(cx.listener(|view, _: &SortColumn, _, cx| {
                view.sort_by(view.cursor.1, cx);
            }))
    }
}
