use navo_core::retention::{RestoreTarget, RetainedState, RetentionEntry, RetentionId};

use crate::{Error, RetentionRun, Vault, proof, snapshot};

impl Vault<'_> {
    pub fn reconcile(
        &mut self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        let entry = self.get(id)?;
        match entry.state {
            RetainedState::Preparing => self.settle_preparing(id, run),
            RetainedState::Restoring => self.settle_restoring(id, RetainedState::Restoring, run),
            // A Conflict with a target came from an interrupted restore and is checked again.
            RetainedState::Conflict if entry.target.is_some() => {
                self.settle_restoring(id, RetainedState::Conflict, run)
            }
            RetainedState::Purging => self.purge(id, run),
            state if state.is_terminal() => {
                self.cleanup(&entry)?;
                Ok(entry)
            }
            _ => Ok(entry),
        }
    }

    fn settle_preparing(
        &mut self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        let mut entry = self.catalog.get(id)?;
        proof::parent(self.ops, &entry.record)?;
        let state = if entry.record.container_identity.is_none() {
            unrecorded_state(self.ops, &entry, run)?
        } else {
            prepared_state(self.ops, &entry, run)?
        };
        let mut tx = self.catalog.checked_transaction(&entry)?;
        entry.state = state;
        tx.save(&entry, RetainedState::Preparing)?;
        tx.commit()?;
        if state.is_terminal() {
            self.cleanup(&entry)?;
        }
        Ok(entry)
    }

    /// Settles an unpinned restore left in `Restoring`; `None` when there was nothing to settle.
    pub(crate) fn settle_interrupted_restore(
        &mut self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<Option<RetentionEntry>, Error> {
        let entry = self.get(id)?;
        if entry.state != RetainedState::Restoring || entry.pinned {
            return Ok(None);
        }
        self.settle_restoring(id, RetainedState::Restoring, run)
            .map(Some)
    }

    /// Settles an entry in `from` (Restoring, or a Conflict it left) from the files on disk.
    fn settle_restoring(
        &mut self,
        id: RetentionId,
        from: RetainedState,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        let before = self.catalog.get(id)?;
        if before.state != from {
            return Err(Error::State(id));
        }
        let mut entry = before.clone();
        match self.restoring_outcome(&mut entry, run) {
            Ok(state) => entry.state = state,
            Err(error) if needs_attention(&error, run) => {
                tracing::warn!(
                    id = id.0,
                    error_kind = error.kind(),
                    "interrupted restore cannot settle"
                );
                entry = before.clone();
                entry.state = RetainedState::Conflict;
            }
            Err(error) => return Err(error),
        }
        if entry.state == RetainedState::Conflict && from == RetainedState::Conflict {
            return Ok(before);
        }
        let mut tx = self.catalog.checked_transaction(&before)?;
        tx.save(&entry, from)?;
        tx.commit()?;
        tracing::info!(
            id = id.0,
            state = entry.state.as_str(),
            "interrupted restore settled"
        );
        if entry.state.is_terminal() {
            self.cleanup(&entry)?;
        }
        Ok(entry)
    }

    fn restoring_outcome(
        &self,
        entry: &mut RetentionEntry,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetainedState, Error> {
        let target = entry.target.as_ref().ok_or(Error::State(entry.id))?;
        let held = entry.record.held()?;
        run.visit(&held)?;
        proof::owned(self.ops, &entry.record)?;
        let held_exists = proof::present(self.ops, &held)?.is_some();
        // A missing destination folder means the destination does not exist either.
        let target_exists = proof::present(self.ops, &target.location)?.is_some();
        Ok(match (held_exists, target_exists) {
            (true, false) => {
                snapshot::verify(self.ops, &entry.record, (&held, false), run)?;
                entry.target = None;
                RetainedState::Ready
            }
            (false, true)
                if same_folder(self.ops, target)?
                    && proof::same_object(self.ops, &target.location, &entry.record.nodes[0])? =>
            {
                RetainedState::Restored
            }
            _ => RetainedState::Conflict,
        })
    }
}

/// A definite proof failure, or a vanished holding or anchor folder; cancel, other I/O, lock and
/// catalog errors may pass and are retried instead.
fn needs_attention(error: &Error, run: &RetentionRun<'_>) -> bool {
    !run.cancel.is_cancelled()
        && match error {
            Error::Changed(_) | Error::Unsupported(_) | Error::Invalid(_) => true,
            Error::Fs(error) => error.kind == navo_core::FsErrorKind::NotFound,
            _ => false,
        }
}

fn same_folder(ops: &dyn navo_fs::ProviderOps, target: &RestoreTarget) -> Result<bool, Error> {
    let Some(parent) = target.location.parent() else {
        return Ok(false);
    };
    match proof::folder(ops, &parent, target.parent_identity) {
        Ok(()) => Ok(true),
        Err(Error::Changed(_)) => Ok(false),
        Err(Error::Fs(error)) if error.kind == navo_core::FsErrorKind::NotFound => Ok(false),
        Err(error) => Err(error),
    }
}

fn unrecorded_state(
    ops: &dyn navo_fs::ProviderOps,
    entry: &RetentionEntry,
    run: &mut RetentionRun<'_>,
) -> Result<RetainedState, Error> {
    let record = &entry.record;
    if !proof::node(ops, &record.source, &record.nodes[0])? {
        return Ok(RetainedState::Conflict);
    }
    let Some(found) = proof::present(ops, &record.container)? else {
        return Ok(RetainedState::Aborted);
    };
    if found.kind != navo_fs::EntryKind::Folder {
        return Ok(RetainedState::Conflict);
    }
    run.visit(&record.container)?;
    let identity = proof::identity(ops, &record.container)?;
    let scope = proof::scope(ops, &proof::anchor(record)?)?;
    proof::on_scope(ops, &record.container, scope)?;
    let mut occupied = false;
    ops.list(
        &record.container,
        &mut |batch: navo_fs::EntryBatch| {
            occupied |= !batch.entries.is_empty();
            if occupied {
                std::ops::ControlFlow::Break(())
            } else {
                std::ops::ControlFlow::Continue(())
            }
        },
        run.cancel,
    )?;
    if occupied || !proof::node(ops, &record.source, &record.nodes[0])? {
        return Ok(RetainedState::Conflict);
    }
    proof::parent(ops, record)?;
    proof::folder(ops, &record.container, identity)?;
    proof::on_scope(ops, &record.container, scope)?;
    ops.remove_empty_folder(&record.container)?;
    Ok(RetainedState::Aborted)
}

fn prepared_state(
    ops: &dyn navo_fs::ProviderOps,
    entry: &RetentionEntry,
    run: &mut RetentionRun<'_>,
) -> Result<RetainedState, Error> {
    proof::owned(ops, &entry.record)?;
    let root = &entry.record.nodes[0];
    let held = entry.record.held()?;
    let source_matches = proof::node(ops, &entry.record.source, root)?;
    if proof::node(ops, &held, root)? && !source_matches {
        snapshot::verify(ops, &entry.record, (&held, false), run)?;
        return Ok(RetainedState::Ready);
    }
    if source_matches && proof::present(ops, &held)?.is_none() {
        return Ok(RetainedState::Aborted);
    }
    Ok(RetainedState::Conflict)
}
