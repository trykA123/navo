//! F5 and F6 from the Find results window: plans a copy or move of hit files from many folders
//! through the same refusals and planning job as a drop, keeping folders below the search root.
//! The UI only queues the request; the planning job checks names and disk (`check`), so a large
//! selection never holds a frame.

use std::sync::Arc;

use navo_core::{ConflictPolicy, EntryKind, FileIdentity, FsError, FsErrorKind, Location, OpKind};
use navo_fs::{FsEntry, ProviderOps};

use super::{Browser, OpRequest, PasteRefusal, Structure, drop_refusal};
use crate::drag::Refusal;

mod alias;
mod check;
mod identity;

pub(super) use check::{plan_rows, recheck};

/// A copy or move of result rows into `target`; `base` is the root the search ran in.
/// The caller leaves archive entries out before building it.
#[derive(Debug, Clone)]
pub struct ResultsRequest {
    pub kind: OpKind,
    pub rows: Vec<ResultRow>,
    pub base: Location,
    pub target: Location,
}

/// One selected hit and what the search saw there.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResultRow {
    pub location: Location,
    pub stamp: RowStamp,
}

/// What the search saw at a row: kind, size, modified time and the object's identity (its file
/// ID, or else its creation time). A row whose entry no longer matches is refused as changed
/// since the search.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RowStamp {
    pub kind: EntryKind,
    pub size: u64,
    pub modified_ms: Option<i64>,
    pub identity: Option<FileIdentity>,
    pub created_ms: Option<i64>,
}

impl RowStamp {
    /// The stamp of a listed entry, without its identity; `read` adds it.
    #[must_use]
    pub fn of(entry: &FsEntry) -> Self {
        Self {
            kind: entry.kind,
            size: entry.size,
            modified_ms: entry.modified_ms,
            identity: None,
            created_ms: None,
        }
    }

    /// Reads the stamp of `location` when the search lists it: its file ID where the provider
    /// has one, else its creation time.
    pub fn read(provider: &dyn ProviderOps, location: &Location) -> Result<Self, FsError> {
        let identity = provider.file_identity(location).ok().flatten();
        let created_ms = match identity {
            Some(_) => None,
            None => provider.details(location).ok().and_then(|it| it.created_ms),
        };
        Ok(Self {
            identity,
            created_ms,
            ..Self::of(&provider.metadata(location)?)
        })
    }
}

impl Browser {
    /// Queues a plan of `request` like a drop that always asks before writing; returns the
    /// planning job's id, or None for a kind other than copy and move or when nothing is queued.
    /// Every refusal arrives later as that failed job, by name: the drop refusals, archive
    /// writes, changed rows and writes that would land on a source. The job plans repeated rows,
    /// aliases and rows inside another row's folder once.
    pub fn prepare_from_results(&mut self, request: ResultsRequest) -> Option<u64> {
        let ResultsRequest {
            kind,
            rows,
            base,
            target,
        } = request;
        if !matches!(kind, OpKind::Copy | OpKind::Move) {
            return None;
        }
        let (sources, stamps): (Vec<Location>, Vec<RowStamp>) = rows
            .into_iter()
            .map(|row| (row.location, row.stamp))
            .unzip();
        // The planning job refuses by name, then decides the structure choice and
        // `same_volume` from every row (`screen`).
        let structure = Some(Structure {
            base,
            keep: true,
            planning: false,
            same_volume: false,
            deciding: true,
        });
        self.next_op += 1;
        let request = OpRequest {
            id: self.next_op,
            kind,
            target: Some(target),
            same_volume: false,
            protected: self.operations.protected.clone(),
            source_tab: self.drop_source_tab(&sources),
            asks: true,
            structure,
            stream: self.archive_stream(&sources),
            sources,
            policy: ConflictPolicy::Ask,
            results: Some(Arc::from(stamps)),
        };
        let id = request.id;
        self.submit_plan(request).then_some(id)
    }
}

/// The name-only checks of a first results plan, on the planning job: the drop refusals and
/// archive writes, refused with the drop's text, then whether any row sits below the search
/// root, which keeps the structure choice. A replan keeps the choice its first plan made.
pub(super) fn screen(request: &mut OpRequest, target: &Location) -> Result<(), String> {
    if let Some(refused) = drop_refusal(target, &request.sources) {
        return Err(refusal_text(refused));
    }
    let (kind, sources) = (request.kind, &request.sources);
    if crate::browser::archive::writes_archive(kind, sources, Some(target)) {
        return Err(refusal_text(PasteRefusal::ReadOnlyArchive));
    }
    let Some(structure) = request.structure.as_mut().filter(|s| s.deciding) else {
        return Ok(());
    };
    let base = &structure.base;
    let nested = sources.iter().any(|source| {
        source
            .parent()
            .is_none_or(|parent| !parent.same_place(base))
    });
    structure.deciding = false;
    if !nested {
        request.structure = None;
    }
    Ok(())
}

/// The text a failed results plan shows for `refused`, as a drop would say it.
fn refusal_text(refused: PasteRefusal) -> String {
    let refusal = match refused {
        PasteRefusal::Nothing => return FsError::new(FsErrorKind::NotFound).message(),
        PasteRefusal::AlreadyHere => Refusal::SameFolder,
        PasteRefusal::IntoItself => Refusal::IntoItself,
        PasteRefusal::IntoResults => Refusal::Results,
        PasteRefusal::ReadOnlyArchive => Refusal::ReadOnlyArchive,
    };
    refusal.text().to_string()
}
