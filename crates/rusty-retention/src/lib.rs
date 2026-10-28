#![forbid(unsafe_code)]

mod adopt;
mod ancestry;
mod capture;
mod locking;
mod maintenance;
mod proof;
mod purge;
mod reconcile;
mod restore;
mod snapshot;

use std::fs::File;
use std::path::Path;

pub use adopt::AdoptJob;
pub use maintenance::{MaintenanceReport, MaintenanceRequest};
pub use proof::can_retain;
use rusty_core::retention::{RetentionEntry, RetentionError, RetentionId, RetentionPolicy};
use rusty_core::{FsError, Location};
use rusty_fs::{CancelToken, ProviderOps};
use rusty_store::{Db, JobId, RetentionCatalog, RetentionStoreError};

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Catalog(#[from] RetentionStoreError),
    #[error(transparent)]
    Invalid(#[from] RetentionError),
    #[error("retention filesystem operation: {0:?}")]
    Fs(FsError),
    #[error("retention profile lock: {0}")]
    Lock(#[from] std::io::Error),
    #[error("retention ownership or content changed at {}", .0.as_str())]
    Changed(Location),
    #[error("the provider cannot safely retain {}", .0.as_str())]
    Unsupported(Location),
    #[error("retention entry {0:?} is not ready for this operation")]
    State(RetentionId),
    #[error("retention entry {0:?} is pinned by an active job")]
    Pinned(RetentionId),
}

impl From<FsError> for Error {
    fn from(error: FsError) -> Self {
        Self::Fs(error)
    }
}

pub struct CaptureRequest<'a> {
    pub source: &'a Location,
    pub policy: RetentionPolicy,
    pub created_at: i64,
    pub job: Option<JobId>,
}

pub struct RestoreRequest<'a> {
    pub id: RetentionId,
    pub target: &'a Location,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RetentionProgress {
    pub visited: u64,
    pub current: Location,
}

pub struct RetentionRun<'a> {
    pub cancel: &'a CancelToken,
    pub progress: &'a mut dyn FnMut(RetentionProgress),
    visited: u64,
}

impl<'a> RetentionRun<'a> {
    pub fn new(cancel: &'a CancelToken, progress: &'a mut dyn FnMut(RetentionProgress)) -> Self {
        Self {
            cancel,
            progress,
            visited: 0,
        }
    }

    fn visit(&mut self, location: &Location) -> Result<(), Error> {
        if self.cancel.is_cancelled() {
            return Err(FsError {
                location: Some(location.clone()),
                ..FsError::new(rusty_core::FsErrorKind::Cancelled)
            }
            .into());
        }
        self.visited = self.visited.saturating_add(1);
        (self.progress)(RetentionProgress {
            visited: self.visited,
            current: location.clone(),
        });
        if self.cancel.is_cancelled() {
            return Err(FsError {
                location: Some(location.clone()),
                ..FsError::new(rusty_core::FsErrorKind::Cancelled)
            }
            .into());
        }
        Ok(())
    }
}

pub struct Vault<'a> {
    catalog: RetentionCatalog,
    ops: &'a dyn ProviderOps,
    _lock: File,
}

impl<'a> Vault<'a> {
    pub fn new(db: Db, profile: &Path, ops: &'a dyn ProviderOps) -> Result<Self, Error> {
        let guard = locking::claim(profile)?;
        Ok(Self {
            catalog: RetentionCatalog::new(db)?,
            ops,
            _lock: guard,
        })
    }

    pub fn get(&self, id: RetentionId) -> Result<RetentionEntry, Error> {
        Ok(self.catalog.get(id)?)
    }

    pub fn list_after(&self, after: i64, limit: u32) -> Result<Vec<RetentionEntry>, Error> {
        Ok(self.catalog.list_after(after, limit)?)
    }

    fn cleanup(&mut self, entry: &RetentionEntry) -> Result<(), Error> {
        if proof::cleanup(self.ops, entry)? {
            self.catalog.acknowledge_cleanup(entry.id)?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests;
