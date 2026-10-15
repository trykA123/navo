#![forbid(unsafe_code)]

mod fake;
pub mod hostile_names;
mod io_queue;
mod job;
mod local;
mod measure;
mod provider;
mod scan;
mod snapshot;

pub use fake::{FakeFs, FakeOp, Fault};
pub use io_queue::{
    Cancelled, IoQueue, Job, JobId, JobSlot, OwnerGuard, OwnerId, Priority, Scheduler, VolumeId,
    VolumeKind, volume_of,
};
pub use job::{
    JobContext, JobLog, JobOutcome, Progress, StepOutcome, TEMP_SUFFIX, clean_up, roll_back,
    run_job,
};
pub use local::LocalFs;
pub use measure::{FolderSize, folder_size};
pub use provider::{
    Attributes, CancelToken, Capabilities, EntryBatch, EntryKind, FsEntry, ListSink, Provider,
    ProviderOps,
};
pub use scan::{existing_at, scan};
pub use snapshot::{FolderSnapshot, load_folder};
