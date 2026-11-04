//! Content hashing on a worker with a per-file deadline; a stuck read is abandoned.

use std::sync::mpsc::{self, RecvTimeoutError};
use std::time::{Duration, Instant};

use crate::source::{ReadOnlySource, RecordId};

use super::entry::{self, Hash};

pub(super) const GRACE: Duration = Duration::from_secs(5);

pub(super) type HashFn = fn(&ReadOnlySource, &RecordId, u64) -> Option<(Hash, u64)>;

pub(super) struct Job {
    pub(super) index: usize,
    pub(super) size: u64,
    pub(super) id: RecordId,
}

#[derive(Clone, Copy)]
pub(super) struct Limits {
    pub(super) small_file_bytes: u64,
    pub(super) budget: u64,
    pub(super) max_time: Duration,
    pub(super) grace: Duration,
}

pub(super) struct Hashed {
    pub(super) payloads: Vec<(usize, Hash)>,
    pub(super) spent: u64,
    /// Files whose content could not be read; any makes the capture incomplete.
    pub(super) unreadable: u64,
    pub(super) complete: bool,
}

struct Plan {
    limits: Limits,
    soft: Option<Instant>,
    hash: HashFn,
}

/// How a hashing pass ended.
#[derive(Clone, Copy)]
struct Finish {
    complete: bool,
    unreadable: u64,
}

enum Message {
    Payload(usize, Hash, u64),
    Done(Finish),
}

pub(super) fn production() -> HashFn {
    entry::hash_file
}

/// Hashes `jobs` in order within the budget; the result is incomplete past `max_time` and
/// whenever a file could not be read.
pub(super) fn run(source: &ReadOnlySource, jobs: Vec<Job>, limits: Limits, hash: HashFn) -> Hashed {
    let mut hashed = Hashed {
        payloads: Vec::new(),
        spent: 0,
        unreadable: 0,
        complete: false,
    };
    let Ok(worker) = source.share() else {
        return hashed;
    };
    let start = Instant::now();
    let deadline = start.checked_add(limits.max_time.saturating_add(limits.grace));
    let (sender, receiver) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name("navo-bench-hash".into())
        .spawn(move || {
            let plan = Plan {
                limits,
                soft: start.checked_add(limits.max_time),
                hash,
            };
            let finish = work(&worker, jobs, &plan, |message| sender.send(message).is_ok());
            let _ = sender.send(Message::Done(finish));
        });
    if spawned.is_err() {
        return hashed;
    }
    loop {
        let message = match deadline {
            Some(at) => receiver.recv_timeout(at.saturating_duration_since(Instant::now())),
            None => receiver.recv().map_err(|_| RecvTimeoutError::Disconnected),
        };
        match message {
            Ok(Message::Payload(index, digest, bytes)) => {
                hashed.payloads.push((index, digest));
                hashed.spent = hashed.spent.saturating_add(bytes);
            }
            Ok(Message::Done(finish)) => {
                hashed.complete = finish.complete && finish.unreadable == 0;
                hashed.unreadable = finish.unreadable;
                return hashed;
            }
            Err(_) => return hashed,
        }
    }
}

fn work(
    source: &ReadOnlySource,
    jobs: Vec<Job>,
    plan: &Plan,
    mut send: impl FnMut(Message) -> bool,
) -> Finish {
    let budget = plan.limits.budget;
    let mut spent = 0_u64;
    let mut unreadable = 0_u64;
    for job in jobs {
        if plan.soft.is_some_and(|at| Instant::now() >= at) {
            return Finish {
                complete: false,
                unreadable,
            };
        }
        if spent.saturating_add(job.size) > budget {
            continue;
        }
        let Some((digest, bytes)) = (plan.hash)(source, &job.id, plan.limits.small_file_bytes)
        else {
            unreadable += 1;
            continue;
        };
        if spent.saturating_add(bytes) > budget {
            continue;
        }
        spent += bytes;
        if !send(Message::Payload(job.index, digest, bytes)) {
            return Finish {
                complete: false,
                unreadable,
            };
        }
    }
    Finish {
        complete: true,
        unreadable,
    }
}
