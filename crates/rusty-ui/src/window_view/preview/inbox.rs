use std::sync::mpsc::{self, Receiver, Sender};

use rusty_fs::{FolderProfile, FsEntry};
use rusty_structured::NodeId;

use crate::browser::Waker;

pub(super) enum Arrival {
    Read {
        load: u64,
        node: NodeId,
        listed: Result<Vec<FsEntry>, String>,
    },
    Snapshot {
        load: u64,
        run: u64,
        profile: FolderProfile,
    },
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
