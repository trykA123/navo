use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, Sender, SyncSender, sync_channel};
use std::sync::{Mutex, PoisonError};

use rusty_core::{FsError, FsErrorKind};

use super::copy::{Copied, Copier, CopyHooks, CopyStep, copy, failure, remove_if_present, temp_of};

pub(super) struct Task<'p> {
    pub seq: u32,
    pub step: CopyStep<'p>,
}

pub(super) enum Event {
    Created {
        seq: u32,
        identity: crate::FileIdentity,
        ack: SyncSender<Result<(), FsError>>,
    },
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
        let mut created = |identity| {
            let (ack, registered) = sync_channel(1);
            events
                .send(Event::Created { seq, identity, ack })
                .map_err(|_| failure(FsErrorKind::Unknown, step.target))?;
            registered
                .recv()
                .map_err(|_| failure(FsErrorKind::Unknown, step.target))?
        };
        let result = catch_unwind(AssertUnwindSafe(|| {
            copy(
                copier,
                &step,
                CopyHooks {
                    progress: &mut on_bytes,
                    created: &mut created,
                },
            )
        }))
        .unwrap_or_else(|_| Err(clean_after_panic(copier, &step)));
        let _ = events.send(Event::Finished { seq, result });
    }
}

fn clean_after_panic(copier: &Copier<'_>, step: &CopyStep<'_>) -> FsError {
    if !copier.strict
        && let Ok(temp) = temp_of(step.target)
    {
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
