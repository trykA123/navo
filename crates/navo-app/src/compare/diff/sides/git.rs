//! Git file sides for the file diff and the Stream: one side built per blob.
//! The parent `sides` keeps the local and unreadable sides; Navo's local files open here as
//! `LocalSide`.

use std::ops::ControlFlow;
use std::sync::Arc;

use navo_compare::model::{
    CancelToken, EntryKind, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_core::{Location, ProviderId, ProviderPath};
use navo_fs::Provider;
use navo_fs::git::GitFs;
use navo_git::{CliGit, EngineKind, GixGit};

use super::super::super::entry::reason_of;
use super::super::super::git_side::engine_for;
use super::super::super::local::LocalSide;
use super::super::super::settings::CompareSettings;
use super::super::load::Side;

/// A Git blob addressed by its full `Location`; the engine comes from the settings.
fn git_side(location: &Location, settings: &CompareSettings) -> Result<Side, Reason> {
    let git = git_fs(settings);
    let found = git
        .metadata(location)
        .map_err(|error| reason_of(error.kind))?;
    let rel = RelPath::new(location.path().clone());
    let entry = SideEntry::new(rel, EntryKind::File, found.size, found.modified_ms);
    let side: Arc<dyn SideSource> = Arc::new(GitFileSource {
        git,
        location: location.clone(),
        provider: location.provider().clone(),
    });
    Ok((side, entry))
}

/// A Git reader that opens exactly the stored blob and never lists.
struct GitFileSource {
    git: GitFs,
    location: Location,
    provider: ProviderId,
}

impl SideSource for GitFileSource {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        _folder: &RelPath,
        _sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        _cancel: &CancelToken,
    ) -> Result<(), Reason> {
        Err(Reason::Unsupported)
    }

    fn open(
        &self,
        _rel: &RelPath,
        _hint: ReadHint,
    ) -> Result<Box<dyn std::io::Read + Send>, Reason> {
        self.git
            .read(&self.location)
            .map_err(|error| reason_of(error.kind))
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        let found = self
            .git
            .metadata(&self.location)
            .map_err(|error| reason_of(error.kind))?;
        Ok(Stamp {
            size: found.size,
            modified: found.modified_ms,
            file_id: None,
        })
    }

    fn object_id(&self, _rel: &RelPath) -> Option<navo_compare::model::ObjectId> {
        None
    }

    fn file_id(&self, _rel: &RelPath) -> Option<navo_core::FileIdentity> {
        None
    }

    fn cache_path(&self, _rel: &RelPath) -> ProviderPath {
        self.location.path().clone()
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        true
    }
}

/// A Git provider whose engine is chosen per repository from the compare settings.
fn git_fs(settings: &CompareSettings) -> GitFs {
    let settings = settings.clone();
    GitFs::with_factory(move |repo| match engine_for(&settings, repo) {
        EngineKind::Gix => Ok(Box::new(GixGit::new(repo)?)),
        EngineKind::Cli => Ok(Box::new(CliGit::new(repo)?)),
    })
}

/// Navo's side builder: a Git blob for the `git` provider, else a local file through
/// `LocalSide`, so Navo's own reads stay on `LocalFs` (NC113).
pub(crate) fn navo_side_of(
    location: &Location,
    settings: &CompareSettings,
) -> Result<Side, Reason> {
    if location.provider().as_str() == "git" {
        git_side(location, settings)
    } else {
        super::file_side(location, navo_local_root)
    }
}

/// Navo's root opener for a local file side: `LocalSide` over `LocalFs`.
fn navo_local_root(root: Location) -> Result<Arc<dyn SideSource>, Reason> {
    Ok(Arc::new(LocalSide::new(root)?))
}
