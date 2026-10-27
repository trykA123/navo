use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::mpsc::{Receiver, Sender, SyncSender, sync_channel};
use std::sync::{Mutex, PoisonError};

use rusty_core::{FsError, FsErrorKind, Step};

use super::StepOutcome;
use super::copy::{Copied, Copier, CopyHooks, CopyStep, copy, failure, remove_if_present, temp_of};
use super::remove::remove;

pub(super) enum Task<'p> {
    Copy { seq: u32, step: CopyStep<'p> },
    Remove { seq: u32, step: &'p Step },
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
    Removed {
        seq: u32,
        result: Result<StepOutcome, FsError>,
    },
}

pub(super) fn work(copier: &Copier<'_>, queue: &Mutex<Receiver<Task<'_>>>, events: &Sender<Event>) {
    while let Some(task) = next(queue) {
        let event = match task {
            Task::Copy { seq, step } => copy_one(copier, (seq, &step), events),
            Task::Remove { seq, step } => remove_one(copier, seq, step),
        };
        let _ = events.send(event);
    }
}

fn remove_one(copier: &Copier<'_>, seq: u32, step: &Step) -> Event {
    let result = catch_unwind(AssertUnwindSafe(|| {
        remove(copier.ops, copier.removals, step)
    }))
    .unwrap_or_else(|_| Err(FsError::new(FsErrorKind::Unknown)));
    Event::Removed { seq, result }
}

fn copy_one(
    copier: &Copier<'_>,
    (seq, step): (u32, &CopyStep<'_>),
    events: &Sender<Event>,
) -> Event {
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
            step,
            CopyHooks {
                progress: &mut on_bytes,
                created: &mut created,
            },
        )
    }))
    .unwrap_or_else(|_| Err(clean_after_panic(copier, step)));
    Event::Finished { seq, result }
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
