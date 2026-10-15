use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant};

use futures::StreamExt;
use gpui_kit::Task;
use rusty_core::paging::{Found, LineIndex, Search, find};

use super::PagedView;
use crate::prelude::*;
use crate::window_view::viewer::files::{IndexRun, Reopen, SharedSource};

const SEARCH_EVERY: Duration = Duration::from_millis(100);
const INDEX_EVERY: Duration = Duration::from_millis(250);

struct Cancel(Arc<AtomicBool>);

impl Drop for Cancel {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Relaxed);
    }
}

pub(super) struct Job {
    _cancel: Cancel,
    _task: Task<()>,
}

struct Throttle {
    last: Instant,
    every: Duration,
}

impl Throttle {
    fn new(every: Duration) -> Self {
        Self {
            last: Instant::now(),
            every,
        }
    }

    fn when_due(&mut self, action: impl FnOnce()) {
        if self.last.elapsed() >= self.every {
            self.last = Instant::now();
            action();
        }
    }
}

fn spawn_job<M: Send + 'static>(
    cx: &mut Context<PagedView>,
    work: impl FnOnce(&AtomicBool, &dyn Fn(M)) + Send + 'static,
    apply: impl Fn(&mut PagedView, M, &mut Context<PagedView>) + 'static,
) -> Job {
    let flag = Arc::new(AtomicBool::new(false));
    let (sender, mut receiver) = futures::channel::mpsc::unbounded::<M>();
    let worker_flag = flag.clone();
    cx.background_spawn(async move {
        work(&worker_flag, &|message| {
            let _ = sender.unbounded_send(message);
        });
    })
    .detach();
    let task = cx.spawn(async move |this, cx| {
        while let Some(message) = receiver.next().await {
            if this
                .update(cx, |view, cx| apply(view, message, cx))
                .is_err()
            {
                break;
            }
        }
    });
    Job {
        _cancel: Cancel(flag),
        _task: task,
    }
}

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
