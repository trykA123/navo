use crate::model::{CancelToken, ContentClass, HashCache, ReadHint, SideEntry, SideSource, Status};

mod checker;
mod hash;
mod read;

pub use checker::check;

pub const CONTENT_BLOCK: usize = 1024 * 1024;
const LARGE_CONTENT_THRESHOLD: u64 = 128 * 1024 * 1024;
/// Two 16 MiB buffers per uncached pair; large files run one per volume (L7).
const PARALLEL_CONTENT_BLOCK: usize = 16 * 1024 * 1024;

#[derive(Clone, Copy)]
pub struct Side<'a> {
    pub entry: &'a SideEntry,
    pub source: &'a dyn SideSource,
}

#[derive(Clone, Copy)]
pub struct CheckOptions<'a> {
    pub cache: &'a dyn HashCache,
    pub cancel: &'a CancelToken,
    pub read_hint: ReadHint,
    pub known_same: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentProgress {
    pub bytes_done: u64,
    pub bytes_total: u64,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ContentResult {
    pub status: Status,
    pub content: ContentClass,
}

#[cfg(test)]
mod tests;
