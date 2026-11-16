//! File sides for the file diff and the Stream: a local file (`StdSide` by default) or an
//! unreadable side.
//!
//! Building a side touches the file system, so it runs only on worker threads.
//! Git blobs live in the child module `git`.

use std::ops::ControlFlow;
use std::sync::Arc;

use navo_compare::model::{
    CancelToken, EntryKind, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_core::{Location, ProviderId, ProviderPath};

use super::super::settings::CompareSettings;
use super::super::std_side::StdSide;
use super::load::Side;

mod git;

pub(super) use git::navo_side_of;

/// Opens a side root folder; `side_of` uses `StdSide`, Navo's `navo_side_of` uses `LocalSide`.
type RootOpener = fn(Location) -> Result<Arc<dyn SideSource>, Reason>;

/// A local file side from one `Location`, read through `StdSide`; any other provider is
/// `Reason::Unsupported`.
pub(super) fn side_of(location: &Location, _settings: &CompareSettings) -> Result<Side, Reason> {
    file_side(location, std_root)
}

/// The default root opener: plain `std::fs` (`StdSide`).
fn std_root(root: Location) -> Result<Arc<dyn SideSource>, Reason> {
    Ok(Arc::new(StdSide::new(root)?))
}

/// A local file: its parent folder, opened by `open_root`, is the side root and its name is the
/// relative path. Fails with `Reason::Unsupported` for a non-local location or one with no parent.
fn file_side(location: &Location, open_root: RootOpener) -> Result<Side, Reason> {
    if !location.is_local() {
        return Err(Reason::Unsupported);
    }
    let (Some(parent), Some(name)) = location.parent_and_name() else {
        return Err(Reason::Unsupported);
    };
    let side = open_root(parent)?;
    let rel = RelPath::new(name);
    let stamp = side.stamp(&rel)?;
    Ok((
        side,
        SideEntry::new(rel, EntryKind::File, stamp.size, stamp.modified),
    ))
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
