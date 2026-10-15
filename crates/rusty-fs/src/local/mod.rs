use std::path::PathBuf;

use rusty_core::{FsError, Location, ProviderId};

use crate::{CancelToken, Capabilities, FsEntry, ListSink, Provider};

const DEFAULT_BATCH: usize = 512;

#[derive(Debug, Clone)]
pub struct LocalFs {
    batch_size: usize,
}

impl LocalFs {
    #[must_use]
    pub fn new() -> Self {
        Self {
            batch_size: DEFAULT_BATCH,
        }
    }

    #[must_use]
    pub fn with_batch_size(self, size: usize) -> Self {
        Self {
            batch_size: size.max(1),
        }
    }
}

impl Default for LocalFs {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for LocalFs {
    fn id(&self) -> ProviderId {
        todo!()
    }

    fn capabilities(&self) -> Capabilities {
        todo!()
    }

    fn list(
        &self,
        _folder: &Location,
        _sink: &mut dyn ListSink,
        _cancel: &CancelToken,
    ) -> Result<(), FsError> {
        todo!()
    }

    fn metadata(&self, _location: &Location) -> Result<FsEntry, FsError> {
        todo!()
    }
}

pub(crate) fn os_path(_location: &Location) -> Result<PathBuf, FsError> {
    todo!()
}

#[cfg(test)]
mod tests;
