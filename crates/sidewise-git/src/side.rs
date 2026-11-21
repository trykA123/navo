//! The Git side the compare engine reads: one commit's tree, listed from an in-memory index
//! and read from the object database. Read-only, case-sensitive; never starts a program.

use std::io::{Cursor, Read};
use std::ops::ControlFlow;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::{Arc, Mutex, MutexGuard, PoisonError};

use gix::error::Class;
use sidewise_core::{FileIdentity, ProviderId, ProviderPath};
use sidewise_engine::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};

use crate::checkout::Checkout;
use crate::checkout_config;
use crate::index::{Child, TreeIndex, lookup};
use crate::location::Form;
use crate::repo::{Commit, GitProblem, MAX_BLOB, Repo};

/// How a side opens its repository: `Repo::open`, or `Repo::open_isolated` in tests.
type Opener = fn(&Path) -> Result<Repo, GitProblem>;
/// Where a checkout side gets the system config: the machine's file, or none in tests.
type SystemFn = fn() -> Option<gix::config::File>;

/// No system config, for sides opened with only the repository's own config.
#[cfg(any(test, feature = "test-support"))]
fn no_system() -> Option<gix::config::File> {
    None
}

/// One revision of a repository as a compare side: a snapshot of the commit pinned at open.
/// It never moves, so tabs sharing it agree; a new run re-resolves through the side cache.
/// The tree index is built on the first `list` and shared by every caller.
/// It holds one thread-safe repository handle; each call takes a thread-local view of it.
pub struct GitTreeSide {
    repo: Repo,
    rev: String,
    form: Form,
    provider: ProviderId,
    commit: Commit,
    index: Mutex<Option<Arc<TreeIndex>>>,
    /// The converter, built only for `Form::Checkout`. Lock it after `index`, never before.
    checkout: Option<Mutex<Checkout>>,
    #[cfg(test)]
    builds: std::sync::atomic::AtomicUsize,
}

impl GitTreeSide {
    /// Opens the repository at `repo` and pins `rev` to its commit. Fails with the problem
    /// (`GitProblem::reason` is its compare reason): not a repository, no such revision, a
    /// format or form it cannot read (`Unsupported`), damaged data. Blocking: call off the UI thread.
    pub fn open(repo: &Path, rev: &str, form: Form) -> Result<GitTreeSide, GitProblem> {
        Self::with_opener(repo, Repo::open, checkout_config::system_file, rev, form)
    }

    /// As `open`, with only the repository's own config, for tests.
    #[cfg(any(test, feature = "test-support"))]
    pub fn open_isolated(repo: &Path, rev: &str, form: Form) -> Result<GitTreeSide, GitProblem> {
        Self::with_opener(repo, Repo::open_isolated, no_system, rev, form)
    }

    /// Opens `path` with `opener`, resolves `rev` and builds the side around it. A checkout
    /// side also builds its converter, reading the system config through `system` (only then).
    #[expect(
        clippy::too_many_arguments,
        reason = "the G17 plan adds `system` to this private constructor"
    )]
    fn with_opener(
        path: &Path,
        opener: Opener,
        system: SystemFn,
        rev: &str,
        form: Form,
    ) -> Result<GitTreeSide, GitProblem> {
        let provider = form.provider().map_err(|_| GitProblem::Unsupported)?;
        let repo = opener(path)?;
        let commit = repo.resolve(rev)?;
        let checkout = match form {
            Form::Checkout => Some(Mutex::new(Checkout::open(
                &repo.local(),
                commit.tree,
                system().as_ref(),
            )?)),
            Form::Stored => None,
        };
        Ok(GitTreeSide {
            repo,
            rev: rev.to_owned(),
            form,
            provider,
            commit,
            index: Mutex::new(None),
            checkout,
            #[cfg(test)]
            builds: std::sync::atomic::AtomicUsize::new(0),
        })
    }

    /// The commit this side shows: the one pinned at open.
    #[must_use]
    pub fn commit(&self) -> Commit {
        self.commit.clone()
    }

    /// The revision text this side was opened with.
    #[must_use]
    pub fn rev(&self) -> &str {
        &self.rev
    }

    /// The location form this side was opened with.
    #[must_use]
    pub fn form(&self) -> Form {
        self.form
    }

    /// The commit the revision names now, read from the repository; this side is unchanged.
    /// Fails as `open` (`NotFound` for a deleted branch, `ReadError` for damaged data).
    pub fn resolve_now(&self) -> Result<Commit, Reason> {
        self.repo.resolve(&self.rev).map_err(GitProblem::reason)
    }

    /// The kind of the entry at `rel` in the pinned commit. Fails with `NotFound` for an
    /// unknown path, or as `lookup` for a damaged tree.
    pub fn kind(&self, rel: &RelPath) -> Result<EntryKind, Reason> {
        self.stored_child(rel).map(|(child, _)| child.kind)
    }

    /// The tree index of the pinned commit, built under the lock on first use so concurrent
    /// callers share one build; a checkout side stores checkout sizes. Fails as
    /// `TreeIndex::build` or `Stopped`; a failed or stopped build stores nothing.
    fn index(&self, cancel: &CancelToken) -> Result<Arc<TreeIndex>, Reason> {
        let mut slot = lock(&self.index);
        if let Some(index) = slot.as_ref() {
            return Ok(Arc::clone(index));
        }
        let commit = self.commit();
        #[cfg(test)]
        self.builds
            .fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        let mut index = TreeIndex::build(&self.repo, commit, cancel)?;
        if let Some(checkout) = &self.checkout {
            let local = self.repo.local();
            let mut read = |id| read_blob(&self.repo, id);
            index.convert_sizes(&mut lock(checkout), &local, &mut read, cancel)?;
        }
        let index = Arc::new(index);
        *slot = Some(Arc::clone(&index));
        Ok(index)
    }

    /// The entry at `rel` and whether it came from the built index: from the index when
    /// `list` built one, else looked up alone, since a build here could not be cancelled.
    /// Fails with `NotFound` for an unknown or non-text path (a lossy name is found only
    /// through the index), or as `lookup`. A looked-up entry keeps its stored size.
    fn stored_child(&self, rel: &RelPath) -> Result<(Child, bool), Reason> {
        let text = rel.as_path().as_str().ok_or(Reason::NotFound)?;
        let built = lock(&self.index).clone();
        match built {
            Some(index) => index
                .child(text)
                .cloned()
                .map(|child| (child, true))
                .ok_or(Reason::NotFound),
            None => lookup(&self.repo, self.commit().tree, text)?
                .map(|child| (child, false))
                .ok_or(Reason::NotFound),
        }
    }

    /// The entry at `rel` in this side's form: a looked-up entry of a checkout side gets its
    /// checkout size (reading the blob when a conversion applies). Fails as `stored_child`.
    fn child(&self, rel: &RelPath) -> Result<Child, Reason> {
        let (mut child, indexed) = self.stored_child(rel)?;
        if let (Some(checkout), false) = (&self.checkout, indexed) {
            let text = rel.as_path().as_str().ok_or(Reason::NotFound)?;
            let mut read = |id| read_blob(&self.repo, id);
            lock(checkout).fix_child(&self.repo.local(), text, &mut child, &mut read);
        }
        Ok(child)
    }

    /// The object id the index holds for `rel`, for tests that damage the object store.
    #[cfg(test)]
    fn index_child_id(&self, rel: &str) -> Option<gix::ObjectId> {
        self.index(&CancelToken::new())
            .ok()?
            .child(rel)
            .map(|child| child.id)
    }
}

/// Locks `mutex`, taking the value back from a poisoned lock: every holder stores whole
/// values, so the data is never half-written.
fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex.lock().unwrap_or_else(PoisonError::into_inner)
}

/// `name` joined under `folder` (`""` is the root) as a row path, kept verbatim.
fn child_rel(folder: &str, name: &str) -> RelPath {
    let text = if folder.is_empty() {
        name.to_owned()
    } else {
        format!("{folder}/{name}")
    };
    RelPath::new(ProviderPath::from_bytes(text.as_bytes()))
}

/// Reads the blob `id` whole. Fails with `MissingBlob` when the object is absent,
/// `Unsupported` past `MAX_BLOB` and `ReadError` for any other error or a parser panic.
fn read_blob(repo: &Repo, id: gix::ObjectId) -> Result<Vec<u8>, Reason> {
    let local = repo.local();
    let found = catch_unwind(AssertUnwindSafe(|| {
        let object = local.try_find_object(id).map_err(|error| {
            if error.dominant_class() == Some(Class::NotFound) {
                Reason::MissingBlob
            } else {
                Reason::ReadError
            }
        })?;
        let object = object.ok_or(Reason::MissingBlob)?.detach();
        if object.kind != gix::objs::Kind::Blob {
            return Err(Reason::ReadError);
        }
        Ok(object.data)
    }));
    let data = found.map_err(|_| Reason::ReadError)??;
    if data.len() as u64 > MAX_BLOB {
        return Err(Reason::Unsupported);
    }
    Ok(data)
}

impl SideSource for GitTreeSide {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    /// Feeds the children of `folder` from the index (built on the first call, honouring
    /// `cancel`). Fails with `NotFound` for a path that is not a folder.
    fn list(
        &self,
        folder: &RelPath,
        sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), Reason> {
        let index = self.index(cancel)?;
        let folder = folder.as_path().as_str().ok_or(Reason::NotFound)?;
        let children = index.children(folder).ok_or(Reason::NotFound)?;
        for child in children {
            let rel = child_rel(folder, &child.name);
            let entry = SideEntry::new(rel, child.kind, child.size, None);
            if cancel.is_cancelled() || sink(entry).is_break() {
                break;
            }
        }
        Ok(())
    }

    /// Reads a file's bytes in this side's form into memory. Links, folders, submodules and
    /// non-UTF-8 paths are `Unsupported`, as is a blob over `MAX_BLOB` (refused before any
    /// read) or checkout bytes over `MAX_BLOB`; a failed conversion fails as `convert`.
    fn open(&self, rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        let child = self.child(rel)?;
        if child.kind != EntryKind::File || !child.readable || child.size > MAX_BLOB {
            return Err(Reason::Unsupported);
        }
        let mut data = read_blob(&self.repo, child.id)?;
        if let Some(checkout) = &self.checkout {
            let text = rel.as_path().as_str().ok_or(Reason::NotFound)?;
            data = lock(checkout).convert(&self.repo.local(), text, &data)?;
            if data.len() as u64 > MAX_BLOB {
                return Err(Reason::Unsupported);
            }
        }
        Ok(Box::new(Cursor::new(data)))
    }

    /// The size in this side's form; a commit has no modified time and no file id.
    fn stamp(&self, rel: &RelPath) -> Result<Stamp, Reason> {
        let child = self.child(rel)?;
        Ok(Stamp {
            size: child.size,
            modified: None,
            file_id: None,
        })
    }

    /// The blob id of a file, so two revisions compare without reading; `None` otherwise and
    /// for a checkout side, whose bytes may differ from the blob.
    fn object_id(&self, rel: &RelPath) -> Option<ObjectId> {
        if self.form == Form::Checkout {
            return None;
        }
        let child = self.child(rel).ok()?;
        (child.kind == EntryKind::File)
            .then(|| ObjectId::from_bytes(child.id.as_slice()))
            .flatten()
    }

    fn file_id(&self, _rel: &RelPath) -> Option<FileIdentity> {
        None
    }

    /// `<commit hex>/<rel>`: stable for one commit, different after the revision moves.
    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        let hex: String = self
            .commit()
            .id
            .as_bytes()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        ProviderPath::from_text(&format!("{hex}/{}", rel.as_path().to_text()))
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        true
    }

    fn may_have_unknown_size(&self) -> bool {
        false
    }
}

#[cfg(test)]
#[path = "side_tests.rs"]
mod tests;

#[cfg(test)]
#[path = "checkout_side_tests.rs"]
mod checkout_tests;
