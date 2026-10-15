use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use rusty_core::{FsError, Location, ProviderId, ProviderPath};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EntryKind {
    File,
    Folder,
    Link,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Attributes {
    pub hidden: bool,
    pub system: bool,
    pub readonly: bool,
    pub link_to_folder: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FsEntry {
    pub name: ProviderPath,
    pub kind: EntryKind,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub attributes: Attributes,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EntryBatch {
    pub entries: Vec<FsEntry>,
    pub last: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct Capabilities {
    pub write: bool,
    pub watch: bool,
    pub recycle: bool,
    pub rename: bool,
}

#[derive(Debug, Clone, Default)]
pub struct CancelToken {
    flag: Arc<AtomicBool>,
}

impl CancelToken {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    pub fn cancel(&self) {
        self.flag.store(true, Ordering::Release);
    }

    #[must_use]
    pub fn is_cancelled(&self) -> bool {
        self.flag.load(Ordering::Acquire)
    }
}

pub trait ListSink {
    fn batch(&mut self, batch: EntryBatch) -> ControlFlow<()>;
}

impl<F: FnMut(EntryBatch) -> ControlFlow<()>> ListSink for F {
    fn batch(&mut self, batch: EntryBatch) -> ControlFlow<()> {
        self(batch)
    }
}

pub trait Provider: Send + Sync {
    fn id(&self) -> ProviderId;
    fn capabilities(&self) -> Capabilities;
    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError>;
    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError>;
}
