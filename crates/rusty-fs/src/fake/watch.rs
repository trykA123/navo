use std::sync::{Arc, Mutex};

use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};

use super::tree::split_last;
use super::{FakeFs, FakeOp, Fault, error, lock, normalize};
use crate::watch::RawSender;
use crate::{Watch, WatchChange, WatchSink};

#[derive(Debug, Default)]
struct List {
    next: u64,
    folders: Vec<(u64, String, RawSender)>,
}

#[derive(Debug, Default)]
pub(super) struct Watchers(Arc<Mutex<List>>);

struct Registration {
    list: Arc<Mutex<List>>,
    id: u64,
}

impl Drop for Registration {
    fn drop(&mut self) {
        lock(&self.list).folders.retain(|(id, _, _)| *id != self.id);
    }
}

impl Watchers {
    fn register(&self, folder: String, raw: RawSender) -> Registration {
        let mut list = lock(&self.0);
        list.next += 1;
        let id = list.next;
        list.folders.push((id, folder, raw));
        Registration {
            list: Arc::clone(&self.0),
            id,
        }
    }

    fn each(&self, folder: &str, send: impl Fn(&RawSender)) {
        lock(&self.0)
            .folders
            .iter()
            .filter(|(_, watched, _)| watched == folder)
            .for_each(|(_, _, raw)| send(raw));
    }
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
            .filter(|(_, watched, _)| watched == folder)
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

    pub(super) fn start_watch(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        let path = self.own_path(folder)?;
        if !self.watchable || !self.tree().is_folder(&path) {
            return Err(error(FsErrorKind::NotFound, folder));
        }
        if let Some(
            Fault::Fail(kind)
            | Fault::FailAfterEntries { kind, .. }
            | Fault::FailAfterBytes { kind, .. },
        ) = self.record_call(&path, FakeOp::Watch)
        {
            return Err(error(kind, folder));
        }
        Watch::start(folder, sink, |raw| Ok(self.watchers.register(path, raw)))
    }

    pub(super) fn changed(&self, path: &str, change: fn(ProviderPath) -> WatchChange) {
        if let Some((parent, name)) = split_last(normalize(path)) {
            self.watchers.each(parent, |raw| {
                raw.change(change(name.clone()));
            });
        }
    }

    pub(super) fn moved(&self, from: &str, to: &str) {
        let (Some((from_parent, from_name)), Some((to_parent, to_name))) =
            (split_last(normalize(from)), split_last(normalize(to)))
        else {
            return;
        };
        if from_parent == to_parent {
            let rename = WatchChange::Renamed {
                from: from_name,
                to: to_name,
            };
            self.watchers.each(from_parent, |raw| {
                raw.change(rename.clone());
            });
            return;
        }
        self.changed(from, WatchChange::Removed);
        self.changed(to, WatchChange::Created);
    }
}
