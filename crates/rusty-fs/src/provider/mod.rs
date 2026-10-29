mod cancel;
mod ops;
mod owned;

use std::ops::ControlFlow;

use rusty_core::{FsError, FsErrorKind, Location, ProviderId, ProviderPath};

pub use cancel::CancelToken;
pub use ops::{FileIdentity, FileIdentityKind, ProviderOps, UnixAccess};
pub use owned::{CopyControl, OwnedCopy};

use crate::{Watch, WatchSink};

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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct DetailAttributes {
    pub readonly: bool,
    pub hidden: bool,
    pub system: bool,
    pub archive: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AclRow {
    pub who: String,
    pub rights: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileDetails {
    pub kind: EntryKind,
    pub size: u64,
    pub on_disk: Option<u64>,
    pub created_ms: Option<i64>,
    pub modified_ms: Option<i64>,
    pub accessed_ms: Option<i64>,
    pub attributes: DetailAttributes,
    pub owner: Option<String>,
    pub acl: Vec<AclRow>,
    pub inherited_from: Option<String>,
    pub security_note: Option<String>,
}

impl From<FsEntry> for FileDetails {
    fn from(entry: FsEntry) -> Self {
        Self {
            kind: entry.kind,
            size: entry.size,
            on_disk: None,
            created_ms: None,
            modified_ms: entry.modified_ms,
            accessed_ms: None,
            attributes: DetailAttributes {
                readonly: entry.attributes.readonly,
                hidden: entry.attributes.hidden,
                system: entry.attributes.system,
                archive: false,
            },
            owner: None,
            acl: Vec::new(),
            inherited_from: None,
            security_note: None,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct LinkKey(pub u128, pub u64);

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SizeEntry {
    pub name: ProviderPath,
    pub kind: EntryKind,
    pub size: u64,
    pub on_disk: Option<u64>,
    pub link: Option<LinkKey>,
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
    fn list_sizes(
        &self,
        folder: &Location,
        sink: &mut dyn FnMut(SizeEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        self.list(
            folder,
            &mut |batch: EntryBatch| {
                batch.entries.into_iter().try_for_each(|entry| {
                    sink(SizeEntry {
                        name: entry.name,
                        kind: entry.kind,
                        size: entry.size,
                        on_disk: None,
                        link: None,
                    })
                })
            },
            cancel,
        )
    }
    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError>;
    fn details(&self, location: &Location) -> Result<FileDetails, FsError> {
        self.metadata(location).map(FileDetails::from)
    }
    fn watch(&self, folder: &Location, _sink: WatchSink) -> Result<Watch, FsError> {
        Err(FsError {
            location: Some(folder.clone()),
            ..FsError::new(FsErrorKind::Unknown)
        })
    }
    fn watch_tree(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        self.watch(folder, sink)
    }
}
