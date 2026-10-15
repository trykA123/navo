mod tree;

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusty_core::{FsError, FsErrorKind, Location, ProviderId, ProviderPath};

use crate::{CancelToken, Capabilities, EntryBatch, FsEntry, ListSink, Provider};
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

const DEFAULT_BATCH: usize = 512;
const ID: &str = "fake";

fn normalize(path: &str) -> &str {
    path.trim_start_matches('/')
}

fn error(kind: FsErrorKind, location: &Location) -> FsError {
    FsError {
        kind,
        location: Some(location.clone()),
        os_code: None,
    }
}

fn provider_id() -> ProviderId {
    ProviderId::new(ID).unwrap_or_else(|_| ProviderId::local())
}

impl FakeFs {
    #[must_use]
    pub fn new() -> Self {
        Self {
            tree: Mutex::new(Tree::new()),
            faults: Mutex::new(HashMap::new()),
            calls: Mutex::new(HashMap::new()),
            batch_size: DEFAULT_BATCH,
        }
    }

    #[must_use]
    pub fn with_batch_size(self, size: usize) -> Self {
        Self {
            batch_size: size.max(1),
            ..self
        }
    }

    #[must_use]
    #[expect(
        clippy::expect_used,
        reason = "the path is never empty and has no NUL, the only two errors"
    )]
    pub fn location(&self, path: &str) -> Location {
        let path = match normalize(path) {
            "" => "/".to_owned(),
            rest => rest.replace('\0', "\u{fffd}"),
        };
        Location::with_provider(provider_id(), ProviderPath::from_text(&path))
            .expect("a non-empty path without NUL is a valid location")
    }

    pub fn add_folder(&self, path: &str) {
        self.tree().add_folder(path);
    }

    pub fn add_file(&self, path: &str, size: u64) {
        self.tree().add_file(path, size);
    }

    pub fn add_entry(&self, folder: &str, entry: FsEntry) {
        self.tree().insert(folder, entry);
    }

    pub fn remove(&self, path: &str) {
        self.tree().remove(path);
    }

    #[expect(
        clippy::too_many_arguments,
        reason = "signature fixed by the P6 F3 contract"
    )]
    pub fn fault(&self, path: &str, op: FakeOp, on_call: u32, fault: Fault) {
        lock(&self.faults)
            .entry((normalize(path).to_owned(), op))
            .or_default()
            .push((on_call, fault));
    }

    #[must_use]
    pub fn calls(&self, path: &str, op: FakeOp) -> u32 {
        lock(&self.calls)
            .get(&(normalize(path).to_owned(), op))
            .copied()
            .unwrap_or(0)
    }

    fn tree(&self) -> MutexGuard<'_, Tree> {
        lock(&self.tree)
    }

    fn own_path(&self, location: &Location) -> Result<String, FsError> {
        if location.provider().as_str() != ID {
            return Err(error(FsErrorKind::NotFound, location));
        }
        Ok(normalize(&location.path().to_text()).to_owned())
    }

    fn record_call(&self, path: &str, op: FakeOp) -> Option<Fault> {
        let key = (path.to_owned(), op);
        let count = {
            let mut calls = lock(&self.calls);
            let count = calls.entry(key.clone()).or_insert(0);
            *count = count.saturating_add(1);
            *count
        };
        let mut faults = lock(&self.faults);
        let pending = faults.get_mut(&key)?;
        let index = pending.iter().position(|(on_call, _)| *on_call == count)?;
        Some(pending.remove(index).1)
    }
}

fn batches(entries: &[FsEntry], size: usize, failure: Option<FsErrorKind>) -> Vec<EntryBatch> {
    let mut chunks: Vec<&[FsEntry]> = entries.chunks(size).collect();
    if chunks.is_empty() && failure.is_none() {
        chunks.push(&[]);
    }
    let total = chunks.len();
    chunks
        .into_iter()
        .enumerate()
        .map(|(index, chunk)| EntryBatch {
            entries: chunk.to_vec(),
            last: failure.is_none() && index + 1 == total,
        })
        .collect()
}

fn stream(
    batches: Vec<EntryBatch>,
    failure: Option<FsErrorKind>,
    sink: &mut dyn ListSink,
    cancel: &CancelToken,
) -> Option<FsErrorKind> {
    for batch in batches {
        if sink.batch(batch).is_break() {
            return None;
        }
        if cancel.is_cancelled() {
            return Some(FsErrorKind::Cancelled);
        }
    }
    failure
}

fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

impl Default for FakeFs {
    fn default() -> Self {
        Self::new()
    }
}

impl Provider for FakeFs {
    fn id(&self) -> ProviderId {
        provider_id()
    }

    fn capabilities(&self) -> Capabilities {
        Capabilities {
            write: true,
            watch: true,
            recycle: true,
            rename: true,
        }
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        let path = self.own_path(folder)?;
        let fault = self.record_call(&path, FakeOp::List);
        if cancel.is_cancelled() {
            return Err(error(FsErrorKind::Cancelled, folder));
        }
        let (limit, failure) = match fault {
            Some(Fault::Fail(kind)) => return Err(error(kind, folder)),
            Some(Fault::FailAfterEntries { kind, entries }) => (entries, Some(kind)),
            None => (usize::MAX, None),
        };
        let mut entries = self
            .tree()
            .children(&path)
            .map_err(|kind| error(kind, folder))?;
        entries.truncate(limit);
        match stream(
            batches(&entries, self.batch_size, failure),
            failure,
            sink,
            cancel,
        ) {
            Some(kind) => Err(error(kind, folder)),
            None => Ok(()),
        }
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        let path = self.own_path(location)?;
        match self.record_call(&path, FakeOp::Metadata) {
            Some(Fault::Fail(kind) | Fault::FailAfterEntries { kind, .. }) => {
                Err(error(kind, location))
            }
            None => self
                .tree()
                .metadata(&path)
                .ok_or_else(|| error(FsErrorKind::NotFound, location)),
        }
    }
}

#[cfg(test)]
mod tests;
