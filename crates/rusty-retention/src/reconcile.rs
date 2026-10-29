use rusty_core::retention::{RetainedState, RetentionEntry, RetentionId};

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
            RetainedState::Restoring => self.settle_restoring(id, run),
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

    fn settle_restoring(
        &mut self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        let mut entry = self.catalog.get(id)?;
        let target = entry.target.as_ref().ok_or(Error::State(id))?;
        let parent = target
            .location
            .parent()
            .ok_or_else(|| Error::Changed(target.location.clone()))?;
        proof::folder(self.ops, &parent, target.parent_identity)?;
        let held = entry.record.held()?;
        run.visit(&held)?;
        proof::owned(self.ops, &entry.record)?;
        let held_exists = proof::present(self.ops, &held)?.is_some();
        let target_exists = proof::present(self.ops, &target.location)?.is_some();
        let before = entry.clone();
        entry.state = match (held_exists, target_exists) {
            (true, false) => {
                snapshot::verify(self.ops, &entry.record, (&held, false), run)?;
                entry.target = None;
                RetainedState::Ready
            }
            (false, true)
                if proof::same_object(self.ops, &target.location, &entry.record.nodes[0])? =>
            {
                RetainedState::Restored
            }
            _ => RetainedState::Conflict,
        };
        let mut tx = self.catalog.checked_transaction(&before)?;
        tx.save(&entry, RetainedState::Restoring)?;
        tx.commit()?;
        if entry.state.is_terminal() {
            self.cleanup(&entry)?;
        }
        Ok(entry)
    }
}

fn unrecorded_state(
    ops: &dyn rusty_fs::ProviderOps,
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
    if found.kind != rusty_fs::EntryKind::Folder {
        return Ok(RetainedState::Conflict);
    }
    run.visit(&record.container)?;
    let identity = proof::identity(ops, &record.container)?;
    let scope = proof::scope(ops, &proof::anchor(record)?)?;
    proof::on_scope(ops, &record.container, scope)?;
    let mut occupied = false;
    ops.list(
        &record.container,
        &mut |batch: rusty_fs::EntryBatch| {
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
    ops: &dyn rusty_fs::ProviderOps,
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
