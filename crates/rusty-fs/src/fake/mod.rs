mod ops;
mod tree;
mod watch;

use std::collections::HashMap;
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusty_core::{FsError, FsErrorKind, Location, ProviderId, ProviderPath};

use crate::watch::Depth;
use crate::{
    CancelToken, Capabilities, EntryBatch, FsEntry, ListSink, Provider, Watch, WatchChange,
    WatchSink,
};
use tree::Tree;
use watch::Watchers;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum FakeOp {
    List,
    Metadata,
    Create,
    Copy,
    Rename,
    Replace,
    Remove,
    Recycle,
    Watch,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Fault {
    Fail(FsErrorKind),
    FailAfterEntries { kind: FsErrorKind, entries: usize },
    FailAfterBytes { kind: FsErrorKind, bytes: u64 },
    Collide,
    LinkThenFail(FsErrorKind),
}

type CallKey = (String, FakeOp);

#[derive(Debug)]
pub struct FakeFs {
    tree: Mutex<Tree>,
    faults: Mutex<HashMap<CallKey, Vec<(u32, Fault)>>>,
    calls: Mutex<HashMap<CallKey, u32>>,
    recycled: Mutex<Vec<String>>,
    volumes: Mutex<HashMap<String, Option<u64>>>,
    batch_size: usize,
    watchable: bool,
    watchers: Watchers,
}

const DEFAULT_BATCH: usize = 512;
const ID: &str = "fake";

fn normalize(path: &str) -> &str {
    path.trim_start_matches('/')
}

fn error(kind: FsErrorKind, location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(kind)
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
            recycled: Mutex::new(Vec::new()),
            volumes: Mutex::new(HashMap::new()),
            batch_size: DEFAULT_BATCH,
            watchable: false,
            watchers: Watchers::default(),
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
    pub fn location(&self, path: &str) -> Location {
        let path = match normalize(path) {
            "" => "/".to_owned(),
            rest => rest.replace('\0', "\u{fffd}"),
        };
        Location::with_provider(provider_id(), ProviderPath::from_text(&path))
            .unwrap_or_else(|_| Location::provider_root(provider_id()))
    }

    pub fn add_folder(&self, path: &str) {
        let (existed, missing) = (self.exists(path), self.missing_folders(path));
        self.tree().add_folder(path);
        self.announce(&missing);
        self.added(path, existed);
    }

    pub fn add_file(&self, path: &str, size: u64) {
        let (existed, missing) = (self.exists(path), self.missing_folders(path));
        self.tree().add_file(path, size);
        self.announce(&missing);
        self.added(path, existed);
    }

    pub fn add_entry(&self, folder: &str, entry: FsEntry) {
        let path = format!("{}/{}", normalize(folder), entry.name.to_text());
        let (existed, missing) = (self.exists(&path), self.missing_folders(&path));
        self.tree().insert(folder, entry);
        self.announce(&missing);
        self.added(&path, existed);
    }

    pub fn remove(&self, path: &str) {
        self.tree().remove(path);
        self.changed(path, WatchChange::Removed);
    }

    pub fn rename_entry(&self, from: &str, to: &str) {
        self.tree().move_node(normalize(from), normalize(to));
        self.moved(from, to);
    }

    fn exists(&self, path: &str) -> bool {
        self.tree().metadata(normalize(path)).is_some()
    }

    fn added(&self, path: &str, existed: bool) {
        let change = if existed {
            WatchChange::Modified
        } else {
            WatchChange::Created
        };
        self.changed(path, change);
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

    pub fn set_volume(&self, path: &str, serial: u64) {
        self.set_volume_serial(path, Some(serial));
    }

    pub fn set_volume_serial(&self, path: &str, serial: Option<u64>) {
        lock(&self.volumes).insert(normalize(path).trim_end_matches('/').to_owned(), serial);
    }

    #[must_use]
    pub fn recycled(&self) -> Vec<String> {
        lock(&self.recycled).clone()
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
        if cancel.is_cancelled() {
            return Some(FsErrorKind::Cancelled);
        }
        if sink.batch(batch).is_break() {
            return None;
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
            watch: self.watchable,
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
            Some(Fault::Fail(kind) | Fault::FailAfterBytes { kind, .. }) => {
                return Err(error(kind, folder));
            }
            Some(Fault::FailAfterEntries { kind, entries }) => (entries, Some(kind)),
            Some(Fault::Collide | Fault::LinkThenFail(_)) | None => (usize::MAX, None),
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
            Some(
                Fault::Fail(kind)
                | Fault::FailAfterEntries { kind, .. }
                | Fault::FailAfterBytes { kind, .. },
            ) => Err(error(kind, location)),
            Some(Fault::Collide | Fault::LinkThenFail(_)) | None => self
                .tree()
                .metadata(&path)
                .ok_or_else(|| error(FsErrorKind::NotFound, location)),
        }
    }

    fn watch(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        self.start_watch(folder, sink, Depth::Folder)
    }

    fn watch_tree(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        self.start_watch(folder, sink, Depth::Subtree)
    }
}

#[cfg(test)]
mod tests;
