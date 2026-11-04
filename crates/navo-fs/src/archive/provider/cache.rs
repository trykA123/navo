use std::collections::{HashMap, VecDeque};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};
use std::time::SystemTime;

use crate::CancelToken;

use super::super::list_with_handle;
use super::Clock;
use super::index::ArchiveIndex;

const MAX_ARCHIVES: usize = 8;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) struct State {
    pub size: u64,
    pub modified: Option<SystemTime>,
}

pub(super) enum LoadError {
    Io(std::io::Error),
    List(String),
    Changed,
    Cancelled,
}

pub(super) struct Cache {
    inner: Mutex<Inner>,
    clock: Clock,
}

#[derive(Default)]
struct Inner {
    archives: HashMap<PathBuf, Entry>,
    order: VecDeque<PathBuf>,
}

struct Entry {
    state: State,
    index: Arc<ArchiveIndex>,
}

impl Cache {
    pub(super) fn new(clock: Clock) -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
            clock,
        }
    }

    pub(super) fn load(
        &self,
        path: &Path,
        cancel: &CancelToken,
    ) -> Result<Arc<ArchiveIndex>, LoadError> {
        self.expire_idle();
        let before = state(path).map_err(LoadError::Io)?;
        if let Some(index) = self.get(path, before) {
            return Ok(index);
        }
        let (listing, zip) =
            list_with_handle(path, super::super::Limits::new(cancel.as_atomic_bool()))
                .map_err(LoadError::List)?;
        if cancel.is_cancelled() {
            return Err(LoadError::Cancelled);
        }
        let after = state(path).map_err(LoadError::Io)?;
        if after != before {
            return Err(LoadError::Changed);
        }
        let index = Arc::new(ArchiveIndex::build(
            listing,
            after,
            super::index::Source {
                archive: path.to_path_buf(),
                zip,
                clock: Arc::clone(&self.clock),
            },
        ));
        self.insert(path.to_path_buf(), Arc::clone(&index));
        Ok(index)
    }

    pub(super) fn refresh(&self) {
        let mut inner = self.lock();
        inner.archives.clear();
        inner.order.clear();
    }

    fn expire_idle(&self) {
        let now = (self.clock)();
        let inner = self.lock();
        for entry in inner.archives.values() {
            entry.index.expire_zip(now);
        }
    }

    fn get(&self, path: &Path, state: State) -> Option<Arc<ArchiveIndex>> {
        let mut inner = self.lock();
        match inner.archives.get(path) {
            Some(entry) if entry.state == state => Some(Arc::clone(&entry.index)),
            Some(_) => {
                inner.archives.remove(path);
                inner.order.retain(|old| old != path);
                None
            }
            None => None,
        }
    }

    fn insert(&self, path: PathBuf, index: Arc<ArchiveIndex>) {
        let mut inner = self.lock();
        let state = index.state;
        inner.archives.insert(path.clone(), Entry { state, index });
        inner.order.retain(|old| old != &path);
        inner.order.push_back(path);
        while inner.archives.len() > MAX_ARCHIVES {
            if let Some(old) = inner.order.pop_front() {
                inner.archives.remove(&old);
            }
        }
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

fn state(path: &Path) -> std::io::Result<State> {
    let metadata = std::fs::metadata(path)?;
    if !metadata.is_file() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "not a file",
        ));
    }
    Ok(State {
        size: metadata.len(),
        modified: metadata.modified().ok(),
    })
}
