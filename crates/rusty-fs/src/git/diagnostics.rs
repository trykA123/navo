use std::collections::{HashMap, VecDeque};
use std::sync::{Mutex, MutexGuard, PoisonError};

use rusty_core::Location;
use rusty_git::{FilterNotice, GitError};

const MAX_PROBLEMS: usize = 64;

#[derive(Debug, Clone)]
pub enum GitProblem {
    Git(GitError),
    Filter(FilterNotice),
    ReadOnly,
    RevNotFound { rev: String },
    TreeTooLarge { found: usize, limit: usize },
    LocationTooLong { limit: usize },
    TooManyComponents { limit: usize },
    BadLocation,
}

pub(super) struct Problems {
    inner: Mutex<Inner>,
}

#[derive(Default)]
struct Inner {
    entries: HashMap<Location, GitProblem>,
    order: VecDeque<Location>,
}

impl Default for Problems {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner::default()),
        }
    }
}

impl Problems {
    pub(super) fn record(&self, location: &Location, problem: GitProblem) {
        let mut inner = self.lock();
        inner.entries.insert(location.clone(), problem);
        inner.order.retain(|old| old != location);
        inner.order.push_back(location.clone());
        while inner.entries.len() > MAX_PROBLEMS {
            if let Some(old) = inner.order.pop_front() {
                inner.entries.remove(&old);
            }
        }
    }

    pub(super) fn last(&self, location: &Location) -> Option<GitProblem> {
        self.lock().entries.get(location).cloned()
    }

    pub(super) fn clear(&self, location: &Location) {
        let mut inner = self.lock();
        inner.entries.remove(location);
        inner.order.retain(|old| old != location);
    }

    pub(super) fn clear_all(&self) {
        let mut inner = self.lock();
        inner.entries.clear();
        inner.order.clear();
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}
