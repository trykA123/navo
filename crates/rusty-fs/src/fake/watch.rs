use std::sync::{Arc, Mutex};

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};

use super::tree::split_last;
use super::{FakeFs, FakeOp, Fault, error, lock, normalize};
use crate::watch::{Depth, RawSender};
use crate::{Watch, WatchChange, WatchSink};

#[derive(Debug, Default)]
struct List {
    next: u64,
    folders: Vec<(u64, String, Depth, RawSender)>,
}

#[derive(Debug, Default)]
pub(super) struct Watchers(Arc<Mutex<List>>);

struct Registration {
    list: Arc<Mutex<List>>,
    id: u64,
}

impl Drop for Registration {
    fn drop(&mut self) {
        lock(&self.list).folders.retain(|(id, ..)| *id != self.id);
    }
}

impl Watchers {
    fn register(&self, folder: String, depth: Depth, raw: RawSender) -> Registration {
        let mut list = lock(&self.0);
        list.next += 1;
        let id = list.next;
        list.folders.push((id, folder, depth, raw));
        Registration {
            list: Arc::clone(&self.0),
            id,
        }
    }

    fn each(&self, folder: &str, send: impl Fn(&RawSender)) {
        lock(&self.0)
            .folders
            .iter()
            .filter(|(_, watched, ..)| watched == folder)
            .for_each(|(.., raw)| send(raw));
    }

    fn notify(&self, path: &str, change: impl Fn(ProviderPath) -> WatchChange) {
        for (_, watched, depth, raw) in &lock(&self.0).folders {
            if let Some(name) = below(path, watched, *depth) {
                raw.change(change(name));
            }
        }
    }

    fn rename(&self, from: &str, to: &str) {
        for (_, watched, depth, raw) in &lock(&self.0).folders {
            if let (Some(from), Some(to)) =
                (below(from, watched, *depth), below(to, watched, *depth))
            {
                raw.change(WatchChange::Renamed { from, to });
            }
        }
    }
}

fn below(path: &str, folder: &str, depth: Depth) -> Option<ProviderPath> {
    let name = match depth {
        Depth::Folder => {
            let (parent, name) = split_last(path)?;
            return (parent == folder).then_some(name);
        }
        Depth::Subtree if folder.is_empty() => path,
        Depth::Subtree => path.strip_prefix(folder)?.strip_prefix('/')?,
    };
    (!name.is_empty()).then(|| ProviderPath::from_text(name))
}

impl FakeFs {
    #[must_use]
    pub fn with_watch(self) -> Self {
        Self {
            watchable: true,
            ..self
        }
    }

    #[must_use]
    pub fn watchers(&self, folder: &str) -> usize {
        let folder = normalize(folder);
        lock(&self.watchers.0)
            .folders
            .iter()
            .filter(|(_, watched, ..)| watched == folder)
            .count()
    }

    pub fn overflow_watchers(&self, folder: &str) {
        self.watchers.each(normalize(folder), |raw| {
            raw.overflow();
        });
    }

    pub fn fail_watchers(&self, folder: &str, kind: FsErrorKind) {
        let location = self.location(folder);
        self.watchers
            .each(normalize(folder), |raw| raw.failed(error(kind, &location)));
    }

    pub(super) fn start_watch(
        &self,
        folder: &Location,
        sink: WatchSink,
        depth: Depth,
    ) -> Result<Watch, FsError> {
        let path = self.own_path(folder)?;
        if !self.watchable || !self.tree().is_folder(&path) {
            return Err(error(FsErrorKind::NotFound, folder));
        }
        if let Some(
            Fault::Fail(kind)
            | Fault::FailOs { kind, .. }
            | Fault::FailAfterEntries { kind, .. }
            | Fault::FailAfterBytes { kind, .. },
        ) = self.record_call(&path, FakeOp::Watch)
        {
            return Err(error(kind, folder));
        }
        let backend = |raw: RawSender| {
            raw.mark_live();
            Ok(self.watchers.register(path, depth, raw))
        };
        match depth {
            Depth::Folder => Watch::start(folder, sink, backend),
            Depth::Subtree => Watch::start_tree(folder, sink, backend),
        }
    }

    pub(super) fn changed(&self, path: &str, change: fn(ProviderPath) -> WatchChange) {
        self.watchers.notify(normalize(path), change);
    }

    pub(super) fn missing_folders(&self, path: &str) -> Vec<String> {
        let mut missing = Vec::new();
        let mut at = normalize(path);
        while let Some((parent, _)) = split_last(at) {
            if parent.is_empty() || self.exists(parent) {
                break;
            }
            missing.push(parent.to_owned());
            at = parent;
        }
        missing.reverse();
        missing
    }

    pub(super) fn announce(&self, folders: &[String]) {
        for folder in folders {
            self.changed(folder, WatchChange::Created);
        }
    }

    pub(super) fn moved(&self, from: &str, to: &str) {
        let (from, to) = (normalize(from), normalize(to));
        let parent = |path| split_last(path).map(|(parent, _)| parent);
        if parent(from) == parent(to) {
            self.watchers.rename(from, to);
            return;
        }
        self.changed(from, WatchChange::Removed);
        self.changed(to, WatchChange::Created);
    }
}
