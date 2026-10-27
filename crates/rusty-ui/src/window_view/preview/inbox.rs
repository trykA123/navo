use std::sync::mpsc::{self, Receiver, Sender};

use rusty_core::Location;
use rusty_fs::{FsEntry, LiveSnapshot};
use rusty_structured::NodeId;

use crate::browser::Waker;

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
}

pub(super) struct Inbox {
    pub send: Sender<Arrival>,
    pub wake: Waker,
    receive: Receiver<Arrival>,
}

impl Inbox {
    pub(super) fn new(wake: Waker) -> Self {
        let (send, receive) = mpsc::channel();
        Self {
            send,
            wake,
            receive,
        }
    }

    pub(super) fn take(&self) -> Vec<Arrival> {
        self.receive.try_iter().collect()
    }
}
