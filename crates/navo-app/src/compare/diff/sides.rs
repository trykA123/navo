//! File sides for the file diff and the Stream: a local file, a Git blob, or an unreadable side.
//!
//! Building a side touches the file system or a repository, so it runs only on worker threads.

use std::ops::ControlFlow;
use std::sync::Arc;

use navo_compare::model::{
    CancelToken, EntryKind, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_core::{Location, ProviderId, ProviderPath};
use navo_fs::Provider;
use navo_fs::git::GitFs;
use navo_git::{CliGit, EngineKind, GixGit};

use super::super::entry::reason_of;
use super::super::local::LocalSide;
use super::super::settings::{CompareSettings, engine_for};
use super::load::Side;

/// A local or Git file side from one `Location`.
pub(super) fn side_of(location: &Location, settings: &CompareSettings) -> Result<Side, Reason> {
    if location.is_local() {
        local_side(location)
    } else if location.provider().as_str() == "git" {
        git_side(location, settings)
    } else {
        Err(Reason::Unsupported)
    }
}

/// A local file: its parent folder is the side root and its name is the relative path.
fn local_side(location: &Location) -> Result<Side, Reason> {
    let (Some(parent), Some(name)) = location.parent_and_name() else {
        return Err(Reason::Unsupported);
    };
    let side: Arc<dyn SideSource> = Arc::new(LocalSide::new(parent)?);
    let rel = RelPath::new(name);
    let stamp = side.stamp(&rel)?;
    Ok((
        side,
        SideEntry::new(rel, EntryKind::File, stamp.size, stamp.modified),
    ))
}

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

/// A present side that could not open: every read fails with `reason`, so the loader reports it
/// Unavailable and never gives it a Viewed key.
pub(super) fn unreadable_side(location: &Location, reason: Reason) -> Side {
    let rel = RelPath::new(location.path().clone());
    let entry = SideEntry::new(rel, EntryKind::File, 0, None);
    let side: Arc<dyn SideSource> = Arc::new(UnreadableSource {
        provider: location.provider().clone(),
        reason,
    });
    (side, entry)
}

/// A source whose every call fails with the reason the side could not open.
struct UnreadableSource {
    provider: ProviderId,
    reason: Reason,
}

impl SideSource for UnreadableSource {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        _folder: &RelPath,
        _sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        _cancel: &CancelToken,
    ) -> Result<(), Reason> {
        Err(self.reason)
    }

    fn open(
        &self,
        _rel: &RelPath,
        _hint: ReadHint,
    ) -> Result<Box<dyn std::io::Read + Send>, Reason> {
        Err(self.reason)
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        Err(self.reason)
    }

    fn object_id(&self, _rel: &RelPath) -> Option<navo_compare::model::ObjectId> {
        None
    }

    fn file_id(&self, _rel: &RelPath) -> Option<navo_core::FileIdentity> {
        None
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        rel.as_path().clone()
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        true
    }
}
