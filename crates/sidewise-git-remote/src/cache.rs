//! The remote cache: one bare repository per address under a Sidewise-owned root (NC216). A
//! fetch holds the key's lock, creates through a staging folder and trashes a broken repository
//! (NC223); `local_at` finds an earlier fetch without any network.

use std::collections::HashMap;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::SystemTime;

use gix_transport::client::blocking_io::http::Http;
use sidewise_git::location::{Form, GitAt};

// A child of `cache` so it shares the `names` helpers; the file sits beside `cache.rs`.
#[path = "evict.rs"]
mod evict;
mod names;

pub use self::evict::{EvictReport, evict};
use self::names::Names;
use crate::fetch::job::{local_ref, open_options};
use crate::{Limits, RemoteAt, RemoteProblem};

/// The cache under one root folder. The app keeps one per process: it owns the per-key mutexes.
pub struct Cache {
    root: PathBuf,
    keys: Mutex<HashMap<String, Arc<Mutex<()>>>>,
}

/// What `<key>.git` is right now.
enum Live {
    Missing,
    Opens,
    /// Not a real folder, unreadable, or `gix::open_opts` fails (rule 5).
    Broken,
}

impl Cache {
    /// A cache rooted at `root`; nothing is created until the first fetch.
    #[must_use]
    pub fn new(root: PathBuf) -> Self {
        Self {
            root,
            keys: Mutex::new(HashMap::new()),
        }
    }

    /// The process-wide mutex of `key`, created on first use (rule 1).
    fn key_mutex(&self, key: &str) -> Arc<Mutex<()>> {
        // The guarded data is `()`; a panic elsewhere leaves nothing half-written here.
        let mut keys = self.keys.lock().unwrap_or_else(PoisonError::into_inner);
        Arc::clone(keys.entry(key.to_owned()).or_default())
    }

    /// Fetches `at.rev` of `at.url` through `http` within `limits` into this address's cache
    /// repository and returns its stored Git location. Disk failures in the cache are `Broken`.
    pub fn fetch_with<H: Http>(
        &self,
        at: &RemoteAt,
        http: H,
        limits: Limits,
    ) -> Result<GitAt, RemoteProblem> {
        let names = Names::new(&self.root, at.url.cache_key());
        let mutex = self.key_mutex(names.key());
        let _in_process = mutex.lock().unwrap_or_else(PoisonError::into_inner);
        let _across_processes = lock_file(&self.root, &names.lock())?;
        names.remove_staging();
        // NC223 rule 6: an eviction error leaves the cache as it is and never fails the fetch.
        evict(&self.root, names.key(), limits.max_bytes).ok();
        let live = names.live();
        let local = match live_state(&live) {
            Live::Opens => crate::fetch_with(&live, &at.url, &at.rev, http, limits)?,
            Live::Broken => {
                names.trash(&live).map_err(|_| RemoteProblem::Broken)?;
                stage(&names, at, http, limits)?
            }
            Live::Missing => stage(&names, at, http, limits)?,
        };
        touch(&names.used());
        Ok(git_at(live, local, at))
    }

    /// The stored Git location of an earlier fetch of `at`, or `None` when the repository or
    /// its ref is missing or will not open. Never uses the network; a hit only sets the mtime of
    /// `<key>.used`, so a restored tab's repository counts as recently used (it takes no lock).
    #[must_use]
    pub fn local_at(&self, at: &RemoteAt) -> Option<GitAt> {
        let names = Names::new(&self.root, at.url.cache_key());
        let live = names.live();
        if !matches!(live_state(&live), Live::Opens) {
            return None;
        }
        let repo = gix::open_opts(&live, open_options()).ok()?;
        let local = local_ref(&at.rev);
        repo.try_find_reference(local.as_str()).ok()??;
        touch(&names.used());
        Some(git_at(live, local, at))
    }
}

/// The stored location of ref `local` in the cache repository `live`, with `at`'s inner path.
fn git_at(live: PathBuf, local: String, at: &RemoteAt) -> GitAt {
    GitAt {
        repo: live,
        rev: local,
        inner: at.inner.clone(),
        form: Form::Stored,
    }
}

/// Creates `root`, opens `lock` (create, never truncate) and blocks until this process holds it
/// (rule 1); the lock ends when the file is dropped. Any failure is `Broken`.
fn lock_file(root: &Path, lock: &Path) -> Result<File, RemoteProblem> {
    std::fs::create_dir_all(root).map_err(|_| RemoteProblem::Broken)?;
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(lock)
        .map_err(|_| RemoteProblem::Broken)?;
    file.lock().map_err(|_| RemoteProblem::Broken)?;
    Ok(file)
}

/// Looks at `<key>.git` without following a link: missing, a repository that opens, or broken.
fn live_state(live: &Path) -> Live {
    match std::fs::symlink_metadata(live) {
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Live::Missing,
        Ok(meta) if meta.is_dir() && gix::open_opts(live, open_options()).is_ok() => Live::Opens,
        _ => Live::Broken,
    }
}

/// Fetches into a fresh `<key>.tmp-<hex>` and renames it to `<key>.git` (rule 2); returns the
/// local ref. On any failure the staging folder is removed.
fn stage<H: Http>(
    names: &Names<'_>,
    at: &RemoteAt,
    http: H,
    limits: Limits,
) -> Result<String, RemoteProblem> {
    let staging = names.staging();
    std::fs::create_dir(&staging).map_err(|_| RemoteProblem::Broken)?;
    let result = crate::fetch_with(&staging, &at.url, &at.rev, http, limits).and_then(|local| {
        std::fs::rename(&staging, names.live()).map_err(|_| RemoteProblem::Broken)?;
        Ok(local)
    });
    if result.is_err() {
        names::remove_any(&staging).ok();
    }
    result
}

/// Sets the mtime of `<key>.used` to now (rule 7). Best effort: a missing mark only makes the
/// key look oldest to eviction, so it never fails a fetch that succeeded.
fn touch(used: &Path) {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(used);
    if let Ok(file) = file {
        file.set_modified(SystemTime::now()).ok();
    }
}
