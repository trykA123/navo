#![deny(unsafe_code)]

pub mod archive;
mod code_page;
mod drive_class;
mod exists;
mod fake;
pub mod hostile_names;
mod io_queue;
mod job;
mod local;
mod measure;
mod paged;
mod provider;
mod scan;
mod snapshot;
mod watch;

pub use code_page::{ansi_code_page, encoding_for_code_page};
pub use drive_class::{DriveClass, drive_class};
pub(crate) use exists::{ALREADY_EXISTS_CODES, already_exists};
pub use fake::{FakeFs, FakeOp, Fault};
pub use io_queue::{
    Cancelled, IoQueue, Job, JobId, JobSlot, OwnerGuard, OwnerId, Priority, Scheduler, VolumeId,
    VolumeKind, volume_of,
};
pub use job::{
    FileStamp, HELD_PREFIX, HeldFile, JobContext, JobLog, JobOutcome, Journaled, PlacedFile,
    Progress, StepOutcome, TEMP_SUFFIX, WorkerCounts, clean_up, copy_workers, release_held,
    roll_back, run_job, run_job_parallel,
};
pub use local::{AutoTuning, LocalFs, os_path};
pub use measure::{
    FolderProfile, FolderSize, KIND_LIMIT, LARGEST_LIMIT, LIVE_EVERY, LargeFile, LiveHandle,
    LiveRequest, LiveSink, LiveSnapshot, PROGRESS_EVERY, ProfileHandle, ProfileSink, ProfileState,
    RANKING_LIMIT, Ranking, Tally, folder_profile, folder_size, spawn_live, spawn_profile,
};
pub use paged::{
    CACHE_PAGES, INDEX_PROGRESS_EVERY, IndexResult, PAGE_SIZE, PagedFile, index_lines,
};
pub use provider::{
    Attributes, CancelToken, Capabilities, EntryBatch, EntryKind, FsEntry, ListSink, Provider,
    ProviderOps,
};
pub use scan::{existing_at, scan};
pub use snapshot::{FolderSnapshot, load_folder, load_folder_reporting};
pub use watch::{WATCH_MAX_DELAY, WATCH_QUIET, Watch, WatchChange, WatchEvent, WatchSink};
