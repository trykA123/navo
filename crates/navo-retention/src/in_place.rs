//! In-place restore of an overwritten copy via journaled park-then-place.

use navo_core::retention::{
    Replacement, RestoreTarget, RetainedState, RetentionEntry, RetentionId,
};
use navo_core::{FsErrorKind, Location};
use navo_fs::ProviderOps;

use crate::{Error, RetentionRun, Vault, proof, snapshot};

/// Outcome of attempting an in-place restore of a retained overwrite.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum InPlace {
    /// Original file was restored to its recorded source path.
    Restored(Box<RetentionEntry>),
    /// Preconditions or step 2 did not allow in-place restore; no file moved and catalog is unchanged.
    NotInPlace,
}

struct InPlacePlan {
    entry: RetentionEntry,
    held: Location,
    parked: Location,
    replacement: Replacement,
}

impl Vault<'_> {
    /// Restores a retained overwritten copy in place at its original path.
    ///
    /// Returns [`InPlace::NotInPlace`] without moving any file or modifying the catalog when
    /// preconditions are not met or parking the copy cannot proceed.
    pub fn restore_in_place(
        &mut self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<InPlace, Error> {
        let Some(plan) = self.plan_in_place(id, run)? else {
            return Ok(InPlace::NotInPlace);
        };
        let Some(mut entry) = self.begin_in_place(&plan)? else {
            return Ok(InPlace::NotInPlace);
        };
        let _held_pause = match self.ops.pause_git(&plan.held) {
            Ok(guard) => guard,
            Err(error) => return self.abort_before_park(&mut entry, error.into()),
        };
        let _source_pause = match self.ops.pause_git(&entry.record.source) {
            Ok(guard) => guard,
            Err(error) => return self.abort_before_park(&mut entry, error.into()),
        };
        if !self.park_copy(&mut entry, &plan)? {
            return Ok(InPlace::NotInPlace);
        }
        self.place_original(&mut entry, &plan)?;
        let mut tx = self.catalog.checked_transaction(&entry)?;
        entry.state = RetainedState::Restored;
        tx.save(&entry, RetainedState::Restoring)?;
        tx.commit()?;
        if let Err(error) = self.finish_in_place(&entry, &plan) {
            tracing::warn!(
                id = entry.id.0,
                error_kind = error.kind(),
                "in-place restore cleanup deferred"
            );
        }
        Ok(InPlace::Restored(Box::new(entry)))
    }

    fn plan_in_place(
        &self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<Option<InPlacePlan>, Error> {
        let entry = self.get(id)?;
        run.visit(&entry.record.source)?;
        if entry.state != RetainedState::Ready || (entry.pinned && !entry.kept) {
            return Ok(None);
        }
        let Some(replacement) = entry.record.replaced_by.clone() else {
            return Ok(None);
        };
        if replacement.moved {
            return Ok(None);
        }
        let Some(parked) = entry.record.parked()? else {
            return Ok(None);
        };
        let held = entry.record.held()?;
        let plan = InPlacePlan {
            entry,
            held,
            parked,
            replacement,
        };
        if !self.check_in_place_files(&plan, run)? {
            return Ok(None);
        }
        Ok(Some(plan))
    }

    fn check_in_place_files(
        &self,
        plan: &InPlacePlan,
        run: &mut RetentionRun<'_>,
    ) -> Result<bool, Error> {
        let record = &plan.entry.record;
        let Some(root) = record.nodes.first() else {
            return Ok(false);
        };
        if !allow_miss(proof::parent(self.ops, record))? {
            return Ok(false);
        }
        if !allow_miss_bool(proof::node(
            self.ops,
            &record.source,
            &plan.replacement.node,
        ))? {
            return Ok(false);
        }
        if proof::present(self.ops, &plan.replacement.from)?.is_none() {
            return Ok(false);
        }
        if proof::present(self.ops, &plan.parked)?.is_some() {
            return Ok(false);
        }
        if !allow_miss(proof::owned(self.ops, record).map(|_| ()))? {
            return Ok(false);
        }
        if !allow_miss(snapshot::verify(self.ops, record, (&plan.held, false), run))? {
            return Ok(false);
        }
        allow_miss_bool(proof::node(self.ops, &plan.held, root))
    }

    fn begin_in_place(&mut self, plan: &InPlacePlan) -> Result<Option<RetentionEntry>, Error> {
        let mut tx = self.catalog.transaction()?;
        let mut entry = tx.get(plan.entry.id)?;
        if entry.state != RetainedState::Ready || (entry.pinned && !entry.kept) {
            return Ok(None);
        }
        entry.target = Some(RestoreTarget {
            location: entry.record.source.clone(),
            parent_identity: entry.record.parent_identity,
        });
        entry.state = RetainedState::Restoring;
        tx.save(&entry, RetainedState::Ready)?;
        tx.commit()?;
        Ok(Some(entry))
    }

    fn abort_before_park(
        &mut self,
        entry: &mut RetentionEntry,
        error: Error,
    ) -> Result<InPlace, Error> {
        self.revert_in_place(entry)?;
        if is_cancelled(&error) {
            Err(error)
        } else {
            Ok(InPlace::NotInPlace)
        }
    }

    /// Step (2): park the replacement copy; rolls back and returns `Ok(false)` (`NotInPlace`) on failure.
    fn park_copy(&mut self, entry: &mut RetentionEntry, plan: &InPlacePlan) -> Result<bool, Error> {
        let result = park_replacement(self.ops, &entry.record, &plan.parked, &plan.replacement);
        let Err(error) = result else {
            return Ok(true);
        };
        let source_gone = proof::present(self.ops, &entry.record.source)?.is_none();
        if source_gone {
            if !proof::same_object(self.ops, &plan.parked, &plan.replacement.node)? {
                return Err(error);
            }
            self.ops.rename_atomic(&plan.parked, &entry.record.source)?;
            self.ops.flush(&proof::anchor(&entry.record)?)?;
            if !proof::same_object(self.ops, &entry.record.source, &plan.replacement.node)? {
                return Err(Error::Changed(entry.record.source.clone()));
            }
        }
        self.revert_in_place(entry)?;
        if is_cancelled(&error) {
            Err(error)
        } else {
            Ok(false)
        }
    }

    /// Step (3): place the held original at `source`; rolls back `parked -> source` and `Ready` on failure.
    fn place_original(
        &mut self,
        entry: &mut RetentionEntry,
        plan: &InPlacePlan,
    ) -> Result<(), Error> {
        let Some(root) = entry.record.nodes.first() else {
            return Err(Error::State(entry.id));
        };
        let Err(error) = place_held(self.ops, &entry.record, &plan.held, root) else {
            return Ok(());
        };
        if let Ok(None) = proof::present(self.ops, &entry.record.source)
            && let Ok(true) = proof::same_object(self.ops, &plan.parked, &plan.replacement.node)
            && self
                .ops
                .rename_atomic(&plan.parked, &entry.record.source)
                .is_ok()
        {
            if let Ok(parent) = proof::anchor(&entry.record) {
                let _ = self.ops.flush(&parent);
            }
            if let Ok(true) =
                proof::same_object(self.ops, &entry.record.source, &plan.replacement.node)
            {
                let _ = self.revert_in_place(entry);
            }
        }
        Err(error)
    }

    fn revert_in_place(&mut self, entry: &mut RetentionEntry) -> Result<(), Error> {
        let mut tx = self.catalog.checked_transaction(entry)?;
        entry.state = RetainedState::Ready;
        entry.target = None;
        tx.save(entry, RetainedState::Restoring)?;
        tx.commit()?;
        Ok(())
    }

    fn finish_in_place(&mut self, entry: &RetentionEntry, plan: &InPlacePlan) -> Result<(), Error> {
        if proof::node(self.ops, &plan.parked, &plan.replacement.node)? {
            self.ops.remove_file(&plan.parked)?;
        }
        self.cleanup(entry)
    }
}

fn park_replacement(
    ops: &dyn ProviderOps,
    record: &navo_core::retention::RetentionRecord,
    parked: &Location,
    replacement: &Replacement,
) -> Result<(), Error> {
    ops.rename_atomic(&record.source, parked)?;
    ops.flush(&record.container)?;
    if !proof::same_object(ops, parked, &replacement.node)? {
        return Err(Error::Changed(parked.clone()));
    }
    Ok(())
}

fn place_held(
    ops: &dyn ProviderOps,
    record: &navo_core::retention::RetentionRecord,
    held: &Location,
    root: &navo_core::retention::RetainedNode,
) -> Result<(), Error> {
    ops.rename_atomic(held, &record.source)?;
    if !proof::same_object(ops, &record.source, root)? {
        return Err(Error::Changed(record.source.clone()));
    }
    ops.flush(&proof::anchor(record)?)?;
    Ok(())
}

fn allow_miss(result: Result<(), Error>) -> Result<bool, Error> {
    match result {
        Ok(()) => Ok(true),
        Err(error) if precondition_miss(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

fn allow_miss_bool(result: Result<bool, Error>) -> Result<bool, Error> {
    match result {
        Ok(matched) => Ok(matched),
        Err(error) if precondition_miss(&error) => Ok(false),
        Err(error) => Err(error),
    }
}

fn precondition_miss(error: &Error) -> bool {
    match error {
        Error::Changed(_) | Error::Unsupported(_) => true,
        Error::Fs(fs) => fs.kind == FsErrorKind::NotFound,
        _ => false,
    }
}

fn is_cancelled(error: &Error) -> bool {
    matches!(error, Error::Fs(fs) if fs.kind == FsErrorKind::Cancelled)
}
