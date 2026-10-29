use std::ops::Range;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};

use crate::model::{CancelToken, Reason, SideEntry, SideSource};

mod offset;

const EVENT_BUFFER: usize = 64;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Align {
    Offset,
    Content,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ByteRegion {
    pub left: Range<u64>,
    pub right: Range<u64>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ByteEvent {
    Region(ByteRegion),
    Progress { done: u64, total: u64 },
    Done,
    Failed(Reason),
}

#[derive(Clone)]
pub struct ByteSide {
    pub source: Arc<dyn SideSource>,
    pub entry: SideEntry,
}

impl ByteSide {
    #[must_use]
    pub fn new(source: Arc<dyn SideSource>, entry: SideEntry) -> Self {
        Self { source, entry }
    }
}

pub struct ByteDiff;

impl ByteDiff {
    #[must_use]
    pub fn start(
        left: ByteSide,
        right: ByteSide,
        align: Align,
        cancel: &CancelToken,
    ) -> Receiver<ByteEvent> {
        let (events, receive) = mpsc::sync_channel(EVENT_BUFFER);
        let fallback = events.clone();
        let cancel = cancel.clone();
        let spawned = std::thread::Builder::new()
            .name("byte-diff".to_owned())
            .spawn(move || {
                offset::run(offset::Job {
                    left,
                    right,
                    align,
                    cancel: &cancel,
                    events: &events,
                });
            });
        if spawned.is_err() {
            let failed = fallback.send(ByteEvent::Failed(Reason::ReadError)).is_err();
            debug_assert!(!failed, "the receiver is local until start returns");
        }
        receive
    }
}
