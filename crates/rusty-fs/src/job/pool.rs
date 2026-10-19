use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, PoisonError};

use rusty_core::{FsError, FsErrorKind};

use super::StepOutcome;
use super::copy::{Copier, CopyStep, copy, failure};

pub(super) struct Task<'p> {
    pub seq: u32,
    pub step: CopyStep<'p>,
}

pub(super) enum Event {
    Bytes {
        seq: u32,
        bytes: u64,
    },
    Finished {
        seq: u32,
        result: Result<StepOutcome, FsError>,
    },
}

pub(super) fn work(copier: &Copier<'_>, queue: &Mutex<Receiver<Task<'_>>>, events: &Sender<Event>) {
    while let Some(Task { seq, step }) = next(queue) {
        let mut on_bytes = |bytes: u64| {
            let _ = events.send(Event::Bytes { seq, bytes });
            copier.cancel.wait_while_paused();
        };
        let result = catch_unwind(AssertUnwindSafe(|| copy(copier, &step, &mut on_bytes)))
            .unwrap_or_else(|_| Err(failure(FsErrorKind::Unknown, step.target)));
        let _ = events.send(Event::Finished { seq, result });
    }
}

fn next<'p>(queue: &Mutex<Receiver<Task<'p>>>) -> Option<Task<'p>> {
    queue
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .recv()
        .ok()
}
