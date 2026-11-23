//! Inbox for the folder preview's background work. Scan snapshots share one latest-wins
//! slot, so a busy window holds at most one; reads, counts and totals queue on a channel.

use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError, Weak};

use navo_core::Location;
use navo_fs::{FsEntry, LiveSnapshot};
use navo_structured::NodeId;

use crate::browser::{FolderTotals, Waker};

/// The newest scan snapshot and the run it belongs to, or nothing since the last take.
pub(super) type Latest = Mutex<Option<(u64, LiveSnapshot)>>;

pub(super) struct ReadDone {
    pub load: u64,
    pub read: u64,
    pub node: NodeId,
    pub listed: Result<(Vec<FsEntry>, usize), String>,
}

pub(super) enum Arrival {
    Read(ReadDone),
    Snapshot { run: u64, snapshot: LiveSnapshot },
    Counted { location: Location, bytes: u64 },
    Totals { load: u64, totals: FolderTotals },
}

pub(super) struct Inbox {
    pub send: Sender<Arrival>,
    pub wake: Waker,
    latest: Arc<Latest>,
    receive: Receiver<Arrival>,
}

impl Inbox {
    /// Builds an empty inbox; `wake` is called after each arrival.
    pub(super) fn new(wake: Waker) -> Self {
        let (send, receive) = mpsc::channel();
        Self {
            send,
            wake,
            latest: Arc::default(),
            receive,
        }
    }

    /// A weak handle to the snapshot slot for a scan sink; it goes dead with the inbox.
    pub(super) fn latest(&self) -> Weak<Latest> {
        Arc::downgrade(&self.latest)
    }

    /// Drains the inbox: the newest snapshot first, then every channel arrival in order.
    pub(super) fn take(&self) -> Vec<Arrival> {
        let newest = lock(&self.latest).take();
        let snapshot = newest.map(|(run, snapshot)| Arrival::Snapshot { run, snapshot });
        snapshot
            .into_iter()
            .chain(self.receive.try_iter())
            .collect()
    }
}

/// Stores `snapshot` for `run` unless the slot holds a newer run; returns whether it stored.
pub(super) fn put_latest(slot: &Latest, run: u64, snapshot: LiveSnapshot) -> bool {
    let mut held = lock(slot);
    // Run ids only grow, so a late snapshot from a cancelled scan must not hide the current one.
    if held.as_ref().is_some_and(|(newer, _)| *newer > run) {
        return false;
    }
    let old = held.replace((run, snapshot));
    drop(held);
    drop(old);
    true
}

/// Locks the slot; a panic elsewhere leaves only a whole old value, so poisoning is ignored.
fn lock(slot: &Latest) -> MutexGuard<'_, Option<(u64, LiveSnapshot)>> {
    slot.lock().unwrap_or_else(PoisonError::into_inner)
}
