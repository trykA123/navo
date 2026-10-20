use std::io::BufReader;
use std::ops::Range;
use std::sync::Arc;
use std::time::Duration;

use rusty_automotive::{Format, Image, IndexError, Layout, LayoutOptions, Watch, index};

use super::entries::entries;
use super::text::is_wide;
use super::{Fetched, Index, MemoryView, Ready, Windows};
use crate::prelude::*;
use crate::window_view::viewer::background::{Job, Ranged, Throttle, ranged, spawn_job};
use crate::window_view::viewer::body::Host;

const INDEX_EVERY: Duration = Duration::from_millis(100);
const CHUNK: usize = 1024 * 1024;
const FETCH_MARGIN: u64 = 64;

pub(super) enum IndexMessage {
    Progress(u64),
    Done(Result<Image, IndexError>),
}

pub(super) struct Fetch {
    rows: Range<u64>,
    result: Result<Windows, String>,
}

pub(super) fn start_index(
    host: &Host,
    len: u64,
    format: Format,
    cx: &mut Context<MemoryView>,
) -> Job {
    let read = ranged(&host.hooks, &host.location);
    spawn_job(
        cx,
        move |cancel, send| {
            let mut throttle = Throttle::new(INDEX_EVERY);
            let mut progress = |done: u64| {
                throttle.when_due(|| send(IndexMessage::Progress(done)));
            };
            let watch = Watch {
                cancel,
                progress: &mut progress,
            };
            let source = BufReader::with_capacity(CHUNK, Ranged { read, at: 0, len });
            send(IndexMessage::Done(index(format, source, watch)));
        },
        MemoryView::index_message,
    )
}

pub(super) fn options(row_bytes: u64) -> LayoutOptions {
    LayoutOptions {
        row_bytes,
        ..LayoutOptions::default()
    }
}

impl MemoryView {
    fn index_message(&mut self, message: IndexMessage, cx: &mut Context<Self>) {
        match message {
            IndexMessage::Progress(done) => {
                if let Index::Running { scanned, .. } = &mut self.index {
                    *scanned = done;
                }
            }
            IndexMessage::Done(Ok(image)) => {
                let ready = Ready {
                    layout: Layout::new(&image.ranges, options(self.row_bytes)),
                    items: entries(&image),
                    wide: is_wide(&image),
                    image: Arc::new(image),
                };
                self.index = Index::Ready(Box::new(ready));
                self.move_cursor(0, cx);
            }
            IndexMessage::Done(Err(error)) => {
                self.index = Index::Failed;
                self.note = Some(error.to_string().into());
            }
        }
        cx.notify();
    }

    fn is_covered(&self, wanted: &Range<u64>) -> bool {
        let covers = |rows: &Range<u64>| rows.start <= wanted.start && wanted.end <= rows.end;
        self.fetched
            .as_ref()
            .is_some_and(|fetched| covers(&fetched.rows))
            || self.fetching.as_ref().is_some_and(|(rows, _)| covers(rows))
    }

    pub(super) fn request_rows(&mut self, top: u64, cx: &mut Context<Self>) {
        let Some(ready) = self.ready() else {
            return;
        };
        let len = ready.layout.len();
        let wanted = top.min(len)..(top + self.page + 1).min(len);
        if wanted.is_empty() || self.is_covered(&wanted) {
            return;
        }
        let rows = top.saturating_sub(FETCH_MARGIN)..(wanted.end + FETCH_MARGIN).min(len);
        let windows = ready.layout.windows(rows.clone());
        let image = ready.image.clone();
        let mut read = ranged(&self.host.hooks, &self.host.location);
        let asked = rows.clone();
        let job = spawn_job(
            cx,
            move |_, send| {
                let result = windows
                    .into_iter()
                    .map(|window| {
                        let bytes = image.read(window.clone(), &mut read)?;
                        Ok((window, bytes))
                    })
                    .collect();
                send(Fetch {
                    rows: asked,
                    result,
                });
            },
            Self::rows_fetched,
        );
        self.fetching = Some((rows, job));
    }

    fn rows_fetched(&mut self, fetch: Fetch, cx: &mut Context<Self>) {
        self.fetching = None;
        let windows = match fetch.result {
            Ok(windows) => {
                self.read_error = None;
                windows
            }
            Err(error) => {
                self.read_error = Some(error.into());
                Vec::new()
            }
        };
        self.fetched = Some(Fetched {
            rows: fetch.rows,
            windows,
        });
        cx.notify();
    }
}
