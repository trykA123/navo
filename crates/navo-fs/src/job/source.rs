use std::io::Read;
use std::sync::Arc;

use navo_core::{FsError, Location, ProviderId};
use navo_git::ObjectId;

use super::FileStamp;
use crate::git::GitFs;
use crate::{EntryKind, FsEntry, Provider};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SourceIdentity {
    /// Live content must keep its size and modification time throughout the copy.
    /// Time checks depend on the provider's clock resolution; equal stamps can hide changes.
    Stamp(FileStamp),
    /// Immutable content may stream a different size from the plan's estimate.
    Object(ObjectId),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SourceMetadata {
    pub kind: EntryKind,
    pub identity: SourceIdentity,
}

impl From<FsEntry> for SourceMetadata {
    fn from(entry: FsEntry) -> Self {
        Self {
            kind: entry.kind,
            identity: SourceIdentity::Stamp(FileStamp {
                size: entry.size,
                modified_ms: entry.modified_ms,
            }),
        }
    }
}

/// Read side of a provider a job copies from when it differs from the target's provider.
pub trait SourceOps: Send + Sync {
    fn id(&self) -> ProviderId;
    fn metadata(&self, location: &Location) -> Result<SourceMetadata, FsError>;
    fn open_read(&self, location: &Location) -> Result<Box<dyn Read + Send>, FsError>;
}

/// The source readers a job may use, found by provider id.
#[derive(Clone, Default)]
pub struct Sources {
    readers: Vec<Arc<dyn SourceOps>>,
}

impl Sources {
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    #[must_use]
    pub fn with(mut self, reader: Arc<dyn SourceOps>) -> Self {
        self.readers.push(reader);
        self
    }

    #[must_use]
    pub fn find(&self, provider: &ProviderId) -> Option<&dyn SourceOps> {
        self.readers
            .iter()
            .find(|reader| &reader.id() == provider)
            .map(|reader| &**reader as &dyn SourceOps)
    }
}

impl SourceOps for GitFs {
    fn id(&self) -> ProviderId {
        Provider::id(self)
    }

    fn metadata(&self, location: &Location) -> Result<SourceMetadata, FsError> {
        Ok(SourceMetadata {
            kind: Provider::metadata(self, location)?.kind,
            identity: SourceIdentity::Object(self.object_id(location)?),
        })
    }

    fn open_read(&self, location: &Location) -> Result<Box<dyn Read + Send>, FsError> {
        self.read_for_copy(location)
    }
}
