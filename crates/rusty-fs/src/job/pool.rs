use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, Sender};
use std::sync::{Mutex, PoisonError};

use rusty_core::{FsError, FsErrorKind};

use super::copy::{Copied, Copier, CopyStep, copy, failure, remove_if_present, temp_of};

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
        result: Result<Copied, FsError>,
    },
}

pub(super) fn work(copier: &Copier<'_>, queue: &Mutex<Receiver<Task<'_>>>, events: &Sender<Event>) {
    while let Some(Task { seq, step }) = next(queue) {
        let mut on_bytes = |bytes: u64| {
            let _ = events.send(Event::Bytes { seq, bytes });
            copier.cancel.wait_while_paused();
        };
        let result = catch_unwind(AssertUnwindSafe(|| copy(copier, &step, &mut on_bytes)))
            .unwrap_or_else(|_| Err(clean_after_panic(copier, &step)));
        let _ = events.send(Event::Finished { seq, result });
    }
}

fn clean_after_panic(copier: &Copier<'_>, step: &CopyStep<'_>) -> FsError {
    if let Ok(temp) = temp_of(step.target) {
        let _ = remove_if_present(copier.ops, &temp);
    }
    failure(FsErrorKind::Unknown, step.target)
}

fn next<'p>(queue: &Mutex<Receiver<Task<'p>>>) -> Option<Task<'p>> {
    queue
        .lock()
        .unwrap_or_else(PoisonError::into_inner)
        .recv()
        .ok()
}
