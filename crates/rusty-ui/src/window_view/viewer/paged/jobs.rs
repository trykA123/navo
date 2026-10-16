use std::time::Duration;

use rusty_core::paging::{Found, LineIndex, Search, find};

use super::PagedView;
use crate::prelude::*;
use crate::window_view::viewer::files::{IndexRun, Reopen, SharedSource};
use crate::window_view::viewer::search::jobs::{Job, Throttle, spawn_job};

const SEARCH_EVERY: Duration = Duration::from_millis(100);
const INDEX_EVERY: Duration = Duration::from_millis(250);

pub(super) struct IndexDone {
    pub result: Result<Option<LineIndex>, String>,
    pub line_count: Option<u64>,
}

pub(super) enum IndexMessage {
    Snapshot(LineIndex),
    Done(IndexDone),
}

pub(super) fn start_index(run: IndexRun, source: SharedSource, cx: &mut Context<PagedView>) -> Job {
    spawn_job(
        cx,
        move |cancel, send| {
            let mut throttle = Throttle::new(INDEX_EVERY);
            let result = run(cancel, &mut |snapshot| {
                throttle.when_due(|| send(IndexMessage::Snapshot(snapshot.clone())));
            });
            let line_count = match &result {
                Ok(Some(index)) => index.line_count(&*source).ok(),
                _ => None,
            };
            send(IndexMessage::Done(IndexDone { result, line_count }));
        },
        PagedView::index_message,
    )
}

pub(super) enum SearchMessage {
    Progress(u64),
    Done(Result<Found, String>),
}

pub(super) fn start_search(reopen: Reopen, search: Search, cx: &mut Context<PagedView>) -> Job {
    spawn_job(
        cx,
        move |cancel, send| {
            let mut throttle = Throttle::new(SEARCH_EVERY);
            let result = reopen().and_then(|source| {
                find(&*source, &search, cancel, &mut |done| {
                    throttle.when_due(|| send(SearchMessage::Progress(done)));
                })
            });
            send(SearchMessage::Done(result));
        },
        PagedView::search_message,
    )
}
