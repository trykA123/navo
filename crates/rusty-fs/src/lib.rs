#![forbid(unsafe_code)]

mod fake;
pub mod hostile_names;
mod io_queue;
mod local;
mod provider;

pub use fake::{FakeFs, FakeOp, Fault};
pub use io_queue::{
    Cancelled, IoQueue, Job, JobId, JobSlot, OwnerGuard, OwnerId, Priority, Scheduler, VolumeId,
    VolumeKind, volume_of,
};
pub use local::LocalFs;
pub use provider::{
    Attributes, CancelToken, Capabilities, EntryBatch, EntryKind, FsEntry, ListSink, Provider,
};
