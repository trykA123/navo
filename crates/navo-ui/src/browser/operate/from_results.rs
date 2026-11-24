//! F5 and F6 from the Find results window: plans a copy or move of hit files from many folders
//! through the same refusals and planning job as a drop, keeping folders below the search root.
//! The UI only refuses what it sees in the names; the planning job checks the disk (`check`).

use std::sync::Arc;

use navo_core::{ConflictPolicy, EntryKind, FileIdentity, FsError, Location, OpKind};
use navo_fs::{FsEntry, ProviderOps};

use super::{Browser, OpRequest, PasteRefusal, Structure, drop_refusal};

mod alias;
mod check;

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
    /// Plans `request` like a drop and always asks before writing; returns the pending op's id.
    /// Only copy and move are offered. The planning job checks each row against its stamp,
    /// plans repeated rows, aliases and rows inside another row's folder once, and fails the
    /// plan as a failed job, by name, when a row changed or a write would land on a source.
    pub fn prepare_from_results(&mut self, request: ResultsRequest) -> Result<u64, PasteRefusal> {
        let ResultsRequest {
            kind,
            rows,
            base,
            target,
        } = request;
        if !matches!(kind, OpKind::Copy | OpKind::Move) {
            return Err(PasteRefusal::Nothing);
        }
        let (sources, stamps): (Vec<Location>, Vec<RowStamp>) = rows
            .into_iter()
            .map(|row| (row.location, row.stamp))
            .unzip();
        if let Some(refused) = drop_refusal(&target, &sources) {
            return Err(refused);
        }
        if crate::browser::archive::writes_archive(kind, &sources, Some(&target)) {
            return Err(PasteRefusal::ReadOnlyArchive);
        }
        let nested = sources.iter().any(|source| {
            source
                .parent()
                .is_none_or(|parent| !parent.same_place(&base))
        });
        // The planning job decides `same_volume` from every row.
        let structure = nested.then_some(Structure {
            base,
            keep: true,
            planning: false,
            same_volume: false,
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
        if self.submit_plan(request) {
            Ok(id)
        } else {
            Err(PasteRefusal::Nothing)
        }
    }
}
