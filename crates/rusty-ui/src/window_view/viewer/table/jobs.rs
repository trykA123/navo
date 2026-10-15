use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use rusty_structured::Watch;
use rusty_structured::table::{RecordIndex, column_widths, index_records, read_rows, sort_order};

use super::rows::{Fetched, Rows};
use super::{SortKey, TableView};
use crate::prelude::*;
use crate::window_view::viewer::background::{Throttle, ranged, spawn_job};
use crate::window_view::viewer::body::Host;

const INDEX_EVERY: Duration = Duration::from_millis(100);
const FETCH_MARGIN: u64 = 64;
const SORT_OFF: &str = "Sorting is off for files over 16 MB: their rows are read on demand.";

pub(super) enum IndexMessage {
    Progress(u64),
    Done(Result<Option<RecordIndex>, String>),
}

pub(super) struct Fetch {
    range: Range<u64>,
    result: Result<Vec<Vec<String>>, String>,
}

pub(super) fn start_index(
    host: &Host,
    len: u64,
    cx: &mut Context<TableView>,
) -> crate::window_view::viewer::background::Job {
    let mut read = ranged(&host.hooks, &host.location);
    spawn_job(
        cx,
        move |cancel, send| {
            let mut throttle = Throttle::new(INDEX_EVERY);
            let mut progress = |done: u64| {
                throttle.when_due(|| send(IndexMessage::Progress(done)));
            };
            let mut watch = Watch {
                cancel,
                progress: &mut progress,
            };
            send(IndexMessage::Done(index_records(
                &mut read, len, &mut watch,
            )));
        },
        TableView::index_message,
    )
}

impl TableView {
    fn index_message(&mut self, message: IndexMessage, cx: &mut Context<Self>) {
        let Rows::Indexed(indexed) = &mut self.rows else {
            return;
        };
        match message {
            IndexMessage::Progress(done) => indexed.scanned = done,
            IndexMessage::Done(Ok(index)) => {
                indexed.job = None;
                indexed.index = index.map(Arc::new);
            }
            IndexMessage::Done(Err(error)) => {
                indexed.job = None;
                self.note = Some(error.into());
            }
        }
        cx.notify();
    }

    pub(super) fn request_rows(&mut self, top: usize, cx: &mut Context<Self>) {
        let len = self.rows.len() as u64;
        let top = top as u64;
        let wanted = top.min(len)..(top + self.page as u64 + 1).min(len);
        if !self.rows.missing(&wanted) {
            return;
        }
        let Rows::Indexed(indexed) = &self.rows else {
            return;
        };
        let Some(index) = indexed.index.clone() else {
            return;
        };
        let skip = indexed.skip;
        let range = top.saturating_sub(FETCH_MARGIN / 2)..(wanted.end + FETCH_MARGIN).min(len);
        let dialect = self.dialect;
        let mut read = ranged(&self.host.hooks, &self.host.location);
        let records = (range.start + skip)..(range.end + skip);
        let asked = range.clone();
        let job = spawn_job(
            cx,
            move |_, send| {
                let result = read_rows(&mut read, &index, dialect, records);
                send(Fetch {
                    range: asked,
                    result,
                });
            },
            Self::rows_fetched,
        );
        if let Rows::Indexed(indexed) = &mut self.rows {
            indexed.fetching = Some((range, job));
        }
    }

    fn rows_fetched(&mut self, fetch: Fetch, cx: &mut Context<Self>) {
        let Rows::Indexed(indexed) = &mut self.rows else {
            return;
        };
        indexed.fetching = None;
        match fetch.result {
            Ok(rows) => {
                if indexed.fetched.is_none() {
                    let columns = self.header.len();
                    self.widths = column_widths(&self.header, &rows, columns);
                }
                indexed.fetched = Some(Fetched {
                    start: fetch.range.start,
                    rows,
                });
            }
            Err(error) => self.note = Some(error.into()),
        }
        cx.notify();
    }

    pub(super) fn sort_by(&mut self, column: usize, cx: &mut Context<Self>) {
        let Rows::Memory { table, .. } = &self.rows else {
            self.set_note(SORT_OFF, cx);
            return;
        };
        let descending = self
            .sort
            .is_some_and(|sort| sort.column == column && !sort.descending);
        let key = SortKey { column, descending };
        let table = table.clone();
        self.sort = Some(key);
        self.note = None;
        self.sorting = Some(spawn_job(
            cx,
            move |_, send| send(sort_order(&table.rows, column, descending)),
            |view: &mut Self, order: Vec<u32>, cx| {
                view.sorting = None;
                if let Rows::Memory { order: shown, .. } = &mut view.rows {
                    *shown = Some(Arc::new(order));
                }
                cx.notify();
            },
        ));
        cx.notify();
    }
}
