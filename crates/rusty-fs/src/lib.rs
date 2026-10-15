#![forbid(unsafe_code)]

mod fake;
mod io_queue;
mod provider;

pub use fake::{FakeFs, FakeOp, Fault};
pub use io_queue::{
    Cancelled, IoQueue, Job, JobId, OwnerGuard, OwnerId, Priority, Scheduler, VolumeId, VolumeKind,
    volume_of,
};
pub use provider::{
    Attributes, CancelToken, Capabilities, EntryBatch, EntryKind, FsEntry, ListSink, Provider,
};
