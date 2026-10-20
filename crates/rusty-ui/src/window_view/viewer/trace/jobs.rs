use std::io::BufReader;
use std::sync::atomic::AtomicBool;
use std::time::Duration;

use rusty_automotive::{Entry, IndexError, Trace, Watch, index_blf};

use super::{Fetched, Inspected, TraceView};
use crate::prelude::*;
use crate::window_view::viewer::background::{Job, Ranged, Throttle, ranged, spawn_job};
use crate::window_view::viewer::body::Host;

const PROGRESS_EVERY: Duration = Duration::from_millis(100);
const BLF_BUFFER: usize = 256 * 1024;
const CHUNK: usize = 1024 * 1024;
const FIRST_ROWS: u64 = 2_000;
const CHUNK_ROWS: u64 = 250_000;
const FETCH_MARGIN: u64 = 64;

pub(super) enum ReadMessage {
    Progress(u64),
    Chunk(Box<Trace>),
    Done(Result<(), IndexError>),
}

pub(super) struct Fetch {
    shown: std::ops::Range<u64>,
    generation: u64,
    result: Result<Vec<Entry>, String>,
}

pub(super) struct Inspect {
    row: u64,
    result: Result<Vec<Entry>, String>,
}

fn index_whole(
    read: impl FnMut(u64, usize) -> Result<Vec<u8>, String>,
    len: u64,
    (cancel, send): (&AtomicBool, &dyn Fn(ReadMessage)),
) {
    let mut throttle = Throttle::new(PROGRESS_EVERY);
    let mut progress = |done: u64| {
        throttle.when_due(|| send(ReadMessage::Progress(done)));
    };
    let watch = Watch {
        cancel,
        progress: &mut progress,
    };
    let source = BufReader::with_capacity(BLF_BUFFER, Ranged { read, at: 0, len });
    match index_blf(source, watch) {
        Ok(trace) => {
            send(ReadMessage::Chunk(Box::new(trace)));
            send(ReadMessage::Done(Ok(())));
        }
        Err(error) => send(ReadMessage::Done(Err(error))),
    }
}

pub(super) fn start_index(
    host: &Host,
    (len, is_blf): (u64, bool),
    cx: &mut Context<TraceView>,
) -> Job {
    let read = ranged(&host.hooks, &host.location);
    if is_blf {
        return spawn_job(
            cx,
            move |cancel, send| index_whole(read, len, (cancel, send)),
            TraceView::read_message,
        );
    }
    spawn_job(
        cx,
        move |cancel, send| {
            let mut throttle = Throttle::new(PROGRESS_EVERY);
            let mut progress = |done: u64| {
                throttle.when_due(|| send(ReadMessage::Progress(done)));
            };
            let mut watch = Watch {
                cancel,
                progress: &mut progress,
            };
            let mut source = BufReader::with_capacity(CHUNK, Ranged { read, at: 0, len });
            let (mut tail, mut budget) = (Trace::default(), FIRST_ROWS);
            loop {
                let done = match tail.read_chunk(&mut source, &mut watch, budget) {
                    Ok(done) => done,
                    Err(error) => return send(ReadMessage::Done(Err(error))),
                };
                if done {
                    send(ReadMessage::Chunk(Box::new(tail)));
                    return send(ReadMessage::Done(Ok(())));
                }
                let next = tail.resume();
                send(ReadMessage::Chunk(Box::new(std::mem::replace(
                    &mut tail, next,
                ))));
                budget = CHUNK_ROWS;
            }
        },
        TraceView::read_message,
    )
}

impl TraceView {
    fn read_message(&mut self, message: ReadMessage, cx: &mut Context<Self>) {
        match message {
            ReadMessage::Progress(done) => self.scanned = done,
            ReadMessage::Chunk(tail) => self.append(*tail, cx),
            ReadMessage::Done(result) => {
                self.reading = None;
                self.is_indexed = true;
                if let Err(error) = result {
                    self.index_error = Some(error.to_string().into());
                }
            }
        }
        cx.notify();
    }

    pub(super) fn append(&mut self, tail: Trace, cx: &mut Context<Self>) {
        let before = self.trace.rows();
        let ids = self.trace.stats.len();
        self.scanned = tail.end;
        self.trace.append(tail);
        self.len = self.len.max(self.trace.end);
        if let Some(filter) = &mut self.filter {
            let more = self.trace.rows_with_from(filter.key, before);
            filter.rows.extend(more);
        }
        if ids != self.trace.stats.len() && self.navigator.read(cx).is_open() {
            self.refresh_nav();
        }
        if before == 0 && self.trace.rows() > 0 {
            self.move_cursor(0, cx);
        }
        self.follow_grew(cx);
    }

    fn is_covered(&self, wanted: &std::ops::Range<u64>) -> bool {
        let covers = |shown: &std::ops::Range<u64>, generation: u64| {
            generation == self.generation && shown.start <= wanted.start && wanted.end <= shown.end
        };
        self.fetched
            .as_ref()
            .is_some_and(|fetched| covers(&fetched.shown, fetched.generation))
            || self
                .fetching
                .as_ref()
                .is_some_and(|(shown, generation, _)| covers(shown, *generation))
    }

    pub(super) fn request_rows(&mut self, top: u64, cx: &mut Context<Self>) {
        let len = self.shown_rows();
        let wanted = top.min(len)..(top + self.page + 1).min(len);
        if wanted.is_empty() || self.is_covered(&wanted) || self.has_failed(&wanted) {
            return;
        }
        let shown = top.saturating_sub(FETCH_MARGIN)..(wanted.end + FETCH_MARGIN).min(len);
        let rows: Vec<u64> = shown.clone().filter_map(|at| self.actual(at)).collect();
        let plan = self.trace.plan(&rows);
        let mut read = ranged(&self.host.hooks, &self.host.location);
        let (asked, generation) = (shown.clone(), self.generation);
        let job = spawn_job(
            cx,
            move |_, send| {
                send(Fetch {
                    shown: asked,
                    generation,
                    result: plan.decode(&mut read),
                });
            },
            Self::rows_fetched,
        );
        self.fetching = Some((shown, generation, job));
    }

    fn rows_fetched(&mut self, fetch: Fetch, cx: &mut Context<Self>) {
        if fetch.generation != self.generation {
            return;
        }
        self.fetching = None;
        match fetch.result {
            Ok(entries) => {
                self.failed_rows = None;
                self.read_error = None;
                self.fetched = Some(Fetched {
                    shown: fetch.shown,
                    generation: fetch.generation,
                    entries,
                });
            }
            Err(error) => {
                self.failed_rows = Some((fetch.shown, fetch.generation));
                self.read_error = Some(error.into());
            }
        }
        cx.notify();
    }

    fn has_failed(&self, wanted: &std::ops::Range<u64>) -> bool {
        self.failed_rows
            .as_ref()
            .is_some_and(|(shown, generation)| {
                *generation == self.generation
                    && shown.start <= wanted.start
                    && wanted.end <= shown.end
            })
    }

    pub(super) fn request_inspect(&mut self, cx: &mut Context<Self>) {
        let Some(row) = self.actual(self.cursor) else {
            return;
        };
        let known = self.inspected.as_ref().map(|inspected| inspected.row);
        let asked = self.inspecting.as_ref().map(|(row, _)| *row);
        let skip = known == Some(row) || asked == Some(row) || self.failed_inspect == Some(row);
        if skip || !self.inspector.read(cx).is_open() {
            return;
        }
        let rows: Vec<u64> = self
            .trace
            .previous_with(row)
            .into_iter()
            .chain([row])
            .collect();
        let plan = self.trace.plan(&rows);
        let mut read = ranged(&self.host.hooks, &self.host.location);
        let job = spawn_job(
            cx,
            move |_, send| {
                send(Inspect {
                    row,
                    result: plan.decode(&mut read),
                });
            },
            Self::inspected,
        );
        self.inspecting = Some((row, job));
    }

    fn inspected(&mut self, inspect: Inspect, cx: &mut Context<Self>) {
        self.inspecting = None;
        match inspect.result {
            Ok(entries) => {
                let (current, previous): (Vec<Entry>, Vec<Entry>) = entries
                    .into_iter()
                    .partition(|entry| entry.row == inspect.row);
                self.inspected = Some(Inspected {
                    row: inspect.row,
                    current: current.into_iter().next(),
                    previous: previous.into_iter().next(),
                });
            }
            Err(error) => {
                self.failed_inspect = Some(inspect.row);
                self.read_error = Some(error.into());
            }
        }
        cx.notify();
    }
}
