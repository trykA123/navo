mod draw;
mod find;
mod jobs;
mod keys;
mod rows;

use std::any::Any;
use std::sync::Arc;

use gpui_kit::{AnyView, FocusHandle};
use rusty_structured::table::{Dialect, MAX_COLUMNS, Table, column_widths};

pub(super) use keys::bind_keys;

use super::background::Job;
use super::body::{BodyKeys, Host, MenuEntry, ViewerBody};
use super::search::view_find::ViewFind;
use crate::prelude::*;
use crate::primitives::ListScroll;
use rows::{Indexed, Rows};

const WIDTH_SAMPLE: usize = 200;

pub(in crate::window_view::viewer) enum TableLoad {
    Memory { table: Table, dialect: Dialect },
    Indexed { table: Table, dialect: Dialect },
}

pub(in crate::window_view::viewer) struct TableFile {
    pub len: u64,
    pub load: TableLoad,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct SortKey {
    column: usize,
    descending: bool,
}

pub(in crate::window_view::viewer) struct TableView {
    title: SharedString,
    len: u64,
    host: Host,
    focus: FocusHandle,
    scroll: ListScroll,
    header: Vec<String>,
    widths: Arc<Vec<usize>>,
    dialect: Dialect,
    rows: Rows,
    cursor: (usize, usize),
    find: Option<ViewFind<(usize, usize)>>,
    left: usize,
    sort: Option<SortKey>,
    sorting: Option<Job>,
    note: Option<SharedString>,
    page: usize,
    available: Option<Pixels>,
    padding: Pixels,
}

impl TableView {
    fn new(file: TableFile, host: Host, cx: &mut Context<Self>) -> Self {
        let (table, dialect, is_indexed) = match file.load {
            TableLoad::Memory { table, dialect } => (table, dialect, false),
            TableLoad::Indexed { table, dialect } => (table, dialect, true),
        };
        let sample = table.rows.get(..table.rows.len().min(WIDTH_SAMPLE));
        let widths = column_widths(&table.header, sample.unwrap_or_default(), table.columns);
        let header = table.header.clone();
        let note = (table.widest > table.columns).then(|| too_wide(table.widest));
        let rows = if is_indexed {
            Rows::Indexed(Box::new(Indexed {
                skip: u64::from(dialect.has_header),
                ..Indexed::default()
            }))
        } else {
            Rows::Memory {
                table: Arc::new(table),
                order: None,
            }
        };
        let mut view = Self {
            title: host.title.clone(),
            len: file.len,
            host,
            focus: cx.focus_handle(),
            scroll: ListScroll::new(),
            header,
            widths: Arc::new(widths),
            dialect,
            rows,
            cursor: (0, 0),
            find: None,
            left: 0,
            sort: None,
            sorting: None,
            note,
            page: 1,
            available: None,
            padding: px(0.),
        };
        if let Rows::Indexed(indexed) = &mut view.rows {
            indexed.job = Some(jobs::start_index(&view.host, view.len, view.dialect, cx));
        }
        view
    }

    fn columns(&self) -> usize {
        self.header.len()
    }

    fn cell(&self, row: usize, column: usize) -> Option<&str> {
        let (_, cells) = self.rows.get(row)?;
        cells.get(column).map(String::as_str)
    }

    fn set_note(&mut self, note: impl Into<SharedString>, cx: &mut Context<Self>) {
        self.note = Some(note.into());
        cx.notify();
    }
}

fn too_wide(widest: usize) -> SharedString {
    format!(
        "Showing {} of {}.",
        crate::format::bytes(MAX_COLUMNS as u64),
        crate::format::count(widest as u64, "column", "columns")
    )
    .into()
}

pub(in crate::window_view::viewer) struct TableBody(pub Entity<TableView>);

impl TableBody {
    pub(in crate::window_view::viewer) fn new(file: TableFile, host: Host, cx: &mut App) -> Self {
        Self(cx.new(|cx| TableView::new(file, host, cx)))
    }
}

impl ViewerBody for TableBody {
    fn view(&self) -> AnyView {
        self.0.clone().into()
    }

    fn focus_handle(&self, cx: &App) -> FocusHandle {
        self.0.read(cx).focus.clone()
    }

    fn keys(&self, _: &App) -> BodyKeys {
        BodyKeys::Normal(vec![
            ("Ctrl+C", "Copy cell"),
            ("Ctrl+Shift+C", "Copy row"),
            ("Ctrl+F3", "Sort"),
        ])
    }

    fn find_again(&self, backward: bool, window: &mut Window, cx: &mut App) {
        self.0
            .update(cx, |view, cx| view.find_next(backward, window, cx));
    }

    fn menu(&self, _: bool, _: &mut App) -> Vec<MenuEntry> {
        keys::menu_entries()
    }

    fn set_note(&self, note: SharedString, cx: &mut App) {
        self.0.update(cx, |view, cx| view.set_note(note, cx));
    }

    fn as_any(&self) -> &dyn Any {
        self
    }
}
