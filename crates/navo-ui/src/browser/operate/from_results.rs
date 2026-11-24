//! F5 and F6 from the Find results window: plans a copy or move of hit files from many folders
//! through the same refusals and planning job as a drop, keeping folders below the search root.

use std::collections::HashSet;

use navo_core::{ConflictPolicy, Location, OpKind, ProviderId, WindowsFoldKey};

use super::{Browser, OpRequest, PasteRefusal, Structure, drop_refusal, same_volume};

/// A copy or move of result rows into `target`; `base` is the root the search ran in.
/// The caller leaves archive entries out before building it.
#[derive(Debug, Clone)]
pub struct ResultsRequest {
    pub kind: OpKind,
    pub sources: Vec<Location>,
    pub base: Location,
    pub target: Location,
}

impl Browser {
    /// Plans `request` like a drop and always asks before writing; returns the pending op's id.
    /// Only copy and move are offered. Repeated rows and rows inside another row's folder are
    /// planned once; a row that vanished since the search fails the plan as a failed job.
    pub fn prepare_from_results(&mut self, request: ResultsRequest) -> Result<u64, PasteRefusal> {
        let ResultsRequest {
            kind,
            sources,
            base,
            target,
        } = request;
        if !matches!(kind, OpKind::Copy | OpKind::Move) {
            return Err(PasteRefusal::Nothing);
        }
        let sources = distinct_tops(sources);
        if let Some(refused) = drop_refusal(&target, &sources) {
            return Err(refused);
        }
        if crate::browser::archive::writes_archive(kind, &sources, Some(&target)) {
            return Err(PasteRefusal::ReadOnlyArchive);
        }
        let same_volume = sources
            .first()
            .is_some_and(|source| same_volume(source, &target));
        let nested = sources.iter().any(|source| {
            source
                .parent()
                .is_none_or(|parent| !parent.same_place(&base))
        });
        let structure = nested.then_some(Structure {
            base,
            keep: true,
            planning: false,
            same_volume,
        });
        self.next_op += 1;
        let request = OpRequest {
            id: self.next_op,
            kind,
            target: Some(target),
            same_volume,
            protected: self.operations.protected.clone(),
            source_tab: self.drop_source_tab(&sources),
            asks: true,
            structure,
            stream: self.archive_stream(&sources),
            sources,
            policy: ConflictPolicy::Ask,
        };
        let id = request.id;
        if self.submit_plan(request) {
            Ok(id)
        } else {
            Err(PasteRefusal::Nothing)
        }
    }
}

/// A location's identity for `same_place` checks in a hash set: case folded where the
/// location folds case.
#[derive(PartialEq, Eq, Hash)]
enum Place {
    Folded(ProviderId, WindowsFoldKey),
    Exact(Location),
}

fn place(location: &Location) -> Place {
    if location.folds_case() {
        Place::Folded(
            location.provider().clone(),
            location.path().windows_fold_key(),
        )
    } else {
        Place::Exact(location.clone())
    }
}

/// `sources` in first-seen order without repeats and without rows below another row: a folder
/// row already carries everything inside it.
fn distinct_tops(sources: Vec<Location>) -> Vec<Location> {
    let all: HashSet<Place> = sources.iter().map(place).collect();
    let mut seen = HashSet::new();
    sources
        .into_iter()
        .filter(|source| {
            let covered = std::iter::successors(source.parent(), Location::parent)
                .any(|ancestor| all.contains(&place(&ancestor)));
            !covered && seen.insert(place(source))
        })
        .collect()
}
