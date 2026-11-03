//! A Git revision as a read-only compare side.

use std::io::Read;
use std::ops::ControlFlow;
use std::path::PathBuf;

use navo_compare::model::{
    CancelToken, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_core::{FileIdentity, Location, ProviderId, ProviderPath};
use navo_fs::git::{GitFs, location, provider_id};
use navo_fs::{CancelToken as FsCancel, Provider};
use navo_git::{EngineKind, GixGit};

use super::entry::{feed, reason_of, segments};

/// The files of `repo@rev`. Submodules list as plain folders and cannot be entered.
pub(crate) struct GitSide {
    git: GitFs,
    repo: PathBuf,
    rev: String,
    provider: ProviderId,
}

impl GitSide {
    /// Opens a side with the given engine; fails with a reason when the repository or the
    /// revision is missing.
    pub(crate) fn new(repo: PathBuf, rev: String, engine: EngineKind) -> Result<Self, Reason> {
        let side = Self {
            git: git_fs(engine),
            repo,
            rev,
            provider: provider_id(),
        };
        let root = side.location(&RelPath::root())?;
        side.git
            .metadata(&root)
            .map_err(|error| reason_of(error.kind))?;
        Ok(side)
    }

    /// Forgets the resolved revision, so the next listing sees the current commit.
    pub(crate) fn refresh(&self) {
        self.git.refresh();
    }

    fn location(&self, rel: &RelPath) -> Result<Location, Reason> {
        let inner = segments(rel).join("/");
        location(&self.repo, &self.rev, &inner).map_err(|_| Reason::Unsupported)
    }
}

fn git_fs(engine: EngineKind) -> GitFs {
    match engine {
        EngineKind::Gix => GitFs::with_factory(|repo| Ok(Box::new(GixGit::new(repo)?))),
        EngineKind::Cli => GitFs::new(),
    }
}

impl SideSource for GitSide {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        folder: &RelPath,
        sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        cancel: &CancelToken,
    ) -> Result<(), Reason> {
        let at = self.location(folder)?;
        let mut batch = |batch| feed(folder, batch, sink, cancel);
        self.git
            .list(&at, &mut batch, &FsCancel::new())
            .map_err(|error| reason_of(error.kind))
    }

    fn open(&self, rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        let at = self.location(rel)?;
        self.git.read(&at).map_err(|error| reason_of(error.kind))
    }

    fn stamp(&self, rel: &RelPath) -> Result<Stamp, Reason> {
        let at = self.location(rel)?;
        let found = self
            .git
            .metadata(&at)
            .map_err(|error| reason_of(error.kind))?;
        Ok(Stamp {
            size: found.size,
            modified: None,
            file_id: None,
        })
    }

    fn object_id(&self, rel: &RelPath) -> Option<ObjectId> {
        self.git.object_id(&self.location(rel).ok()?).ok()
    }

    fn file_id(&self, _rel: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        self.location(rel)
            .map_or_else(|_| rel.as_path().clone(), |at| at.path().clone())
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        true
    }
}
