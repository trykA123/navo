mod tree;

use std::collections::HashMap;
use std::sync::Mutex;

use rusty_core::{FsError, FsErrorKind, Location, ProviderId};

use crate::{CancelToken, Capabilities, FsEntry, ListSink, Provider};
use tree::Tree;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FakeOp {
    List,
    Metadata,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    Fail(FsErrorKind),
    FailAfterEntries { kind: FsErrorKind, entries: usize },
}

type CallKey = (String, FakeOp);

#[derive(Debug)]
pub struct FakeFs {
    tree: Mutex<Tree>,
    faults: Mutex<HashMap<CallKey, Vec<(u32, Fault)>>>,
    calls: Mutex<HashMap<CallKey, u32>>,
    batch_size: usize,
}

impl FakeFs {
    #[must_use]
    pub fn new() -> Self {
        todo!()
    }

    #[must_use]
    pub fn with_batch_size(self, _size: usize) -> Self {
        todo!()
    }

    #[must_use]
    pub fn location(&self, _path: &str) -> Location {
        todo!()
    }

    pub fn add_folder(&self, _path: &str) {
        todo!()
    }

    pub fn add_file(&self, _path: &str, _size: u64) {
        todo!()
    }

    pub fn add_entry(&self, _folder: &str, _entry: FsEntry) {
        todo!()
    }

    pub fn remove(&self, _path: &str) {
        todo!()
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "signature fixed by the P6 F3 contract"
    )]
    pub fn fault(&self, _path: &str, _op: FakeOp, _on_call: u32, _fault: Fault) {
        todo!()
    }

    #[must_use]
    pub fn calls(&self, _path: &str, _op: FakeOp) -> u32 {
        todo!()
    }
}

impl Default for FakeFs {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for FakeFs {
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

#[cfg(test)]
mod tests;
