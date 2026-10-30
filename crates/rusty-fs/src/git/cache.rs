use std::collections::{HashMap, VecDeque};
use std::hash::Hash;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use rusty_git::{GitError, GitSource, ObjectId, TreeEntry};

pub(super) type Open = dyn Fn(&Path) -> Result<Box<dyn GitSource>, GitError> + Send + Sync;

type Source = Arc<dyn GitSource>;
type RevKey = (PathBuf, String);
type TreeKey = (PathBuf, ObjectId);

pub(super) const MAX_SOURCES: usize = 8;
pub(super) const MAX_TREES: usize = 8;
pub(super) const MAX_TREE_ENTRIES: usize = 200_000;

pub(super) enum TreeLoad {
    Loaded(Arc<Vec<TreeEntry>>),
    TooLarge(usize),
}

pub(super) struct Cache {
    inner: Mutex<Inner>,
}

struct Inner {
    sources: Lru<PathBuf, Source>,
    revisions: HashMap<RevKey, ObjectId>,
    trees: Lru<TreeKey, Arc<Vec<TreeEntry>>>,
}

impl Default for Cache {
    fn default() -> Self {
        Self {
            inner: Mutex::new(Inner {
                sources: Lru::new(MAX_SOURCES),
                revisions: HashMap::new(),
                trees: Lru::new(MAX_TREES),
            }),
        }
    }
}

impl Cache {
    pub(super) fn source(&self, repo: &Path, open: &Open) -> Result<Source, GitError> {
        if let Some(source) = self.lock().sources.get(&repo.to_path_buf()) {
            return Ok(source);
        }
        let source: Source = Arc::from(open(repo)?);
        self.lock()
            .sources
            .insert(repo.to_path_buf(), Arc::clone(&source));
        Ok(source)
    }

    pub(super) fn resolve(
        &self,
        repo: &Path,
        rev: &str,
        source: &dyn GitSource,
    ) -> Result<Option<ObjectId>, GitError> {
        let key = (repo.to_path_buf(), rev.to_owned());
        if let Some(id) = self.lock().revisions.get(&key).copied() {
            return Ok(Some(id));
        }
        let Some(id) = source.resolve(rev)? else {
            return Ok(None);
        };
        self.lock().revisions.insert(key, id);
        Ok(Some(id))
    }

    pub(super) fn tree(
        &self,
        repo: &Path,
        commit: ObjectId,
        source: &dyn GitSource,
    ) -> Result<TreeLoad, GitError> {
        let key = (repo.to_path_buf(), commit);
        if let Some(tree) = self.lock().trees.get(&key) {
            return Ok(TreeLoad::Loaded(tree));
        }
        let entries = source.list_tree(&hex(commit))?;
        if entries.len() > MAX_TREE_ENTRIES {
            return Ok(TreeLoad::TooLarge(entries.len()));
        }
        let tree = Arc::new(entries);
        self.lock().trees.insert(key, Arc::clone(&tree));
        Ok(TreeLoad::Loaded(tree))
    }

    pub(super) fn refresh(&self) {
        self.lock().revisions.clear();
    }

    fn lock(&self) -> MutexGuard<'_, Inner> {
        self.inner.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

struct Lru<K, V> {
    values: HashMap<K, V>,
    order: VecDeque<K>,
    limit: usize,
}

impl<K: Eq + Hash + Clone, V: Clone> Lru<K, V> {
    fn new(limit: usize) -> Self {
        Self {
            values: HashMap::new(),
            order: VecDeque::new(),
            limit,
        }
    }

    fn get(&mut self, key: &K) -> Option<V> {
        let value = self.values.get(key)?.clone();
        self.order.retain(|old| old != key);
        self.order.push_back(key.clone());
        Some(value)
    }

    fn insert(&mut self, key: K, value: V) {
        self.values.insert(key.clone(), value);
        self.order.retain(|old| old != &key);
        self.order.push_back(key);
        while self.values.len() > self.limit {
            if let Some(old) = self.order.pop_front() {
                self.values.remove(&old);
            }
        }
    }
}

fn hex(id: ObjectId) -> String {
    const DIGITS: &[u8; 16] = b"0123456789abcdef";
    let mut text = String::with_capacity(id.byte_len() * 2);
    for byte in id.as_bytes() {
        text.push(char::from(DIGITS[usize::from(byte >> 4)]));
        text.push(char::from(DIGITS[usize::from(byte & 0x0F)]));
    }
    text
}
