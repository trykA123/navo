use std::collections::{HashMap, HashSet};
use std::sync::{Mutex, PoisonError};

use navo_core::{FsError, FsErrorKind, Location, OpKind, OperationPlan, Step};

use super::{KeptReason, KeptSource, Progress, StepOutcome};
use crate::{
    CancelToken, CopiedSource, CopyControl, FileIdentity, FileIdentityKind, MoveCopy, ProviderOps,
};

/// What a move's copy learned about its source.
#[derive(Clone, Copy)]
enum Read {
    /// The source as the copy read it: its removal checks it still matches.
    Proof(CopiedSource),
    /// The source changed while it was copied, or its read could not be checked.
    Changed,
    /// The source's volume keeps nothing that tells it from a replacement.
    Unconfirmed,
}

pub(super) struct Removals<'p> {
    kind: OpKind,
    folders: HashSet<&'p Location>,
    /// What each move source's copy read in this run, so its removal can tell whether it changed.
    copied: Mutex<HashMap<Location, Read>>,
    /// Each move source folder's file ID at the start of the job, so an empty folder put in its
    /// place meanwhile is kept; a folder without one is removed when empty, as before.
    folder_ids: HashMap<&'p Location, FileIdentity>,
}

impl<'p> Removals<'p> {
    /// Reads what a job's removals need before its first step; a move reads its folders' IDs
    /// and reports each folder as current. `None` when the job was cancelled meanwhile.
    pub(super) fn of(
        plan: &'p OperationPlan,
        ops: &dyn ProviderOps,
        (cancel, progress): (&CancelToken, &mut dyn FnMut(&Progress)),
    ) -> Option<Self> {
        let folders = match plan.kind {
            OpKind::Delete => plan
                .steps
                .iter()
                .filter_map(|step| match step {
                    Step::RemoveFolder { location } => Some(location),
                    _ => None,
                })
                .collect(),
            _ => HashSet::new(),
        };
        Some(Self {
            kind: plan.kind,
            folders,
            copied: Mutex::new(HashMap::new()),
            folder_ids: folder_ids(plan, ops, (cancel, progress))?,
        })
    }

    /// Removes an empty folder; a move first checks it is still the folder it started with.
    /// The check and the removal go by path: a swap between the two escapes it (a known limit).
    fn remove_folder(&self, ops: &dyn ProviderOps, location: &Location) -> Result<(), FsError> {
        if let Some(&expected) = self.folder_ids.get(location)
            && !ops.matches_identity(location, Some(expected))?
        {
            return Err(FsError {
                location: Some(location.clone()),
                ..FsError::new(FsErrorKind::ChangedOnDisk)
            });
        }
        ops.remove_empty_folder(location)
    }

    /// True for a move: its copies must prove their sources before the sources are removed.
    pub(super) fn moves(&self) -> bool {
        self.kind == OpKind::Move
    }

    /// Copies a move's source and keeps what the copy read as the proof its removal needs.
    pub(super) fn copy_source(
        &self,
        ops: &dyn ProviderOps,
        request: MoveCopy<'_>,
        control: CopyControl<'_>,
    ) -> Result<u64, FsError> {
        let source = request.source.clone();
        self.copied().remove(&source);
        let (written, read) = ops.copy_moved_file(request, control)?;
        let read = match read {
            Some(read) if read.proves() => Read::Proof(read),
            Some(_) => Read::Unconfirmed,
            None => Read::Changed,
        };
        self.copied().insert(source, read);
        Ok(written)
    }

    /// The proof the last copy of `source` left in this run, for the journal.
    pub(super) fn proof_of(&self, source: &Location) -> Option<CopiedSource> {
        match self.copied().get(source) {
            Some(Read::Proof(proof)) => Some(*proof),
            Some(Read::Changed | Read::Unconfirmed) | None => None,
        }
    }

    /// Removes a file. A move removes a source only through the proof its copy left in this run;
    /// without one (a resumed job, a changed source, a volume that proves nothing) it keeps it.
    fn remove_file(&self, ops: &dyn ProviderOps, location: &Location) -> Result<(), FsError> {
        if !self.moves() {
            return ops.remove_file(location);
        }
        let seen = self.copied().get(location).copied();
        let kind = match seen {
            Some(Read::Proof(expected)) => {
                ops.remove_copied_source(location, expected)?;
                self.copied().remove(location);
                return Ok(());
            }
            Some(Read::Changed) => FsErrorKind::ChangedOnDisk,
            Some(Read::Unconfirmed) | None => FsErrorKind::IdentityUnavailable,
        };
        Err(FsError {
            location: Some(location.clone()),
            ..FsError::new(kind)
        })
    }

    /// The move source a failed `step` keeps in place, and why; `None` for any other failure,
    /// which stops the job as before. A source folder that is gone still stops it.
    pub(super) fn kept(&self, (seq, step): (u32, &Step), error: &FsError) -> Option<KeptSource> {
        let location = match step {
            Step::RemoveFile { location } => location,
            Step::RemoveFolder { location } if error.kind != FsErrorKind::NotFound => location,
            _ => return None,
        };
        if !self.moves() {
            return None;
        }
        let kept = |reason| {
            Some(KeptSource {
                seq,
                location: location.clone(),
                reason,
            })
        };
        if matches!(step, Step::RemoveFolder { .. }) {
            return kept(match error.kind {
                FsErrorKind::ChangedOnDisk => KeptReason::Changed,
                _ => KeptReason::NotRemoved(error.clone()),
            });
        }
        let reason = match (self.copied().get(location), error.kind) {
            (None, _) => KeptReason::Resumed,
            (Some(Read::Unconfirmed), _) => KeptReason::Unconfirmed,
            (Some(Read::Changed), _) | (Some(Read::Proof(_)), FsErrorKind::ChangedOnDisk) => {
                KeptReason::Changed
            }
            (Some(Read::Proof(_)), _) => KeptReason::NotRemoved(error.clone()),
        };
        kept(reason)
    }

    fn copied(&self) -> std::sync::MutexGuard<'_, HashMap<Location, Read>> {
        self.copied.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

/// Refuses a move that streams a copy from another provider, before any step runs: a streamed
/// copy takes no proof of its source, so the move could never remove it safely.
pub(super) fn refuse_streamed_move(plan: &OperationPlan) -> Result<(), (u32, FsError)> {
    if plan.kind != OpKind::Move {
        return Ok(());
    }
    let streamed = (0_u32..)
        .zip(&plan.steps)
        .find_map(|(seq, step)| match step {
            Step::CopyFile { source, target, .. } if source.provider() != target.provider() => {
                Some((seq, source))
            }
            _ => None,
        });
    match streamed {
        Some((seq, source)) => Err((
            seq,
            FsError {
                location: Some(source.clone()),
                ..FsError::new(FsErrorKind::NotSupportedYet)
            },
        )),
        None => Ok(()),
    }
}

pub(super) fn remove(
    ops: &dyn ProviderOps,
    removals: &Removals<'_>,
    step: &Step,
) -> Result<StepOutcome, FsError> {
    let (result, location) = match step {
        Step::RemoveFile { location } => (removals.remove_file(ops, location), location),
        Step::RemoveLink { location } => (ops.remove_link(location), location),
        Step::RemoveFolder { location } => (removals.remove_folder(ops, location), location),
        _ => return Err(FsError::new(FsErrorKind::Unknown)),
    };
    match result {
        Ok(()) => Ok(StepOutcome::Done),
        Err(error)
            if removals.kind == OpKind::Delete
                && error.kind == FsErrorKind::NotFound
                && already_gone(ops, removals, location) =>
        {
            Ok(StepOutcome::Skipped)
        }
        Err(error) => Err(error),
    }
}

/// The native file ID of each folder a move removes, read once before any step runs. Each
/// folder is reported as current, and Cancel stops the scan. A folder whose ID cannot be read is
/// removed when empty, as before (it fails open, like a volume without IDs).
fn folder_ids<'p>(
    plan: &'p OperationPlan,
    ops: &dyn ProviderOps,
    (cancel, progress): (&CancelToken, &mut dyn FnMut(&Progress)),
) -> Option<HashMap<&'p Location, FileIdentity>> {
    let mut ids = HashMap::new();
    if plan.kind != OpKind::Move {
        return Some(ids);
    }
    let mut seen = Progress::start(plan);
    for step in &plan.steps {
        let Step::RemoveFolder { location } = step else {
            continue;
        };
        if cancel.is_cancelled() {
            return None;
        }
        seen.current = Some(location.clone());
        progress(&seen);
        let Some(identity) = ops.file_identity(location).ok().flatten() else {
            continue;
        };
        if !matches!(identity.kind, FileIdentityKind::Fingerprint { .. }) {
            ids.insert(location, identity);
        }
    }
    Some(ids)
}

fn already_gone(ops: &dyn ProviderOps, removals: &Removals<'_>, location: &Location) -> bool {
    if !matches!(ops.metadata(location), Err(error) if error.kind == FsErrorKind::NotFound) {
        return false;
    }
    for ancestor in std::iter::successors(location.parent(), Location::parent) {
        let planned = removals.folders.contains(&ancestor);
        match ops.metadata(&ancestor) {
            Ok(_) => return true,
            Err(error) if planned && error.kind == FsErrorKind::NotFound => {}
            Err(_) => return false,
        }
    }
    false
}
