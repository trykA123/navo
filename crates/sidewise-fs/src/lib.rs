//! Paged files, volume ids and native path helpers shared by the compare stack.
#![deny(unsafe_code)]

mod drive_class;
pub mod durable;
pub mod native;
mod network_walk;
mod paged;
mod volume;
pub mod walk;

pub use drive_class::{DriveClass, drive_class};
pub use native::{native_identity, os_path};
pub use navo_core::{FileIdentity, FileIdentityKind};
pub use network_walk::{
    NETWORK_WALK_MAX_ENTRIES, NETWORK_WALK_MAX_TIME, NetworkStop, NetworkWalkBudget,
    needs_network_bound,
};
pub use paged::{
    CACHE_PAGES, INDEX_PROGRESS_EVERY, IndexResult, PAGE_SIZE, PagedFile, index_lines,
};
pub use volume::{VolumeId, volume_of};
