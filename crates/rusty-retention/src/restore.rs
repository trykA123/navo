use rusty_core::retention::{RestoreTarget, RetainedState, RetentionEntry};
use rusty_store::JobId;

use crate::{Error, RestoreRequest, RetentionRun, Vault, proof, snapshot};

impl Vault<'_> {
    pub fn restore(
        &mut self,
        request: RestoreRequest<'_>,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        self.restore_authorized(request, None, run)
    }

    pub fn rollback(
        &mut self,
        id: rusty_core::retention::RetentionId,
        owner: JobId,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        self.catalog.transaction()?.authorize_rollback(id, owner)?;
        let before = self.get(id)?;
        if before
            .target
            .as_ref()
            .is_some_and(|target| target.location != before.record.source)
            || !matches!(
                before.state,
                RetainedState::Preparing
                    | RetainedState::Ready
                    | RetainedState::Restoring
                    | RetainedState::Restored
                    | RetainedState::Aborted
            )
        {
            return Err(Error::State(id));
        }
        let entry = self.reconcile(id, run)?;
        match entry.state {
            RetainedState::Restored | RetainedState::Aborted => Ok(entry),
            RetainedState::Ready => self.restore_authorized(
                RestoreRequest {
                    id,
                    target: &entry.record.source,
                },
                Some(owner),
                run,
            ),
            _ => Err(Error::State(id)),
        }
    }

    fn restore_authorized(
        &mut self,
        request: RestoreRequest<'_>,
        owner: Option<JobId>,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        run.visit(request.target)?;
        let parent = request
            .target
            .parent()
            .ok_or_else(|| Error::Unsupported(request.target.clone()))?;
        let parent_identity = proof::identity(self.ops, &parent)?;
        proof::folder(self.ops, &parent, parent_identity)?;
        let mut tx = self.catalog.transaction()?;
        let mut entry = tx.get(request.id)?;
        if entry.state != RetainedState::Ready {
            return Err(Error::State(entry.id));
        }
        if entry.pinned && owner.is_none() {
            return Err(Error::Pinned(entry.id));
        }
        if proof::present(self.ops, request.target)?.is_some() {
            return Err(Error::Changed(request.target.clone()));
        }
        entry.target = Some(RestoreTarget {
            location: request.target.clone(),
            parent_identity,
        });
        entry.state = RetainedState::Restoring;
        match owner {
            Some(owner) => tx.save_rollback(&entry, owner)?,
            None => tx.save(&entry, RetainedState::Ready)?,
        }
        tx.commit()?;
        self.complete_restore(request, owner, run)
    }

    fn complete_restore(
        &mut self,
        request: RestoreRequest<'_>,
        owner: Option<JobId>,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        let mut entry = self.catalog.get(request.id)?;
        if entry.state != RetainedState::Restoring {
            return Err(Error::State(entry.id));
        }
        if owner.is_none() && entry.pinned {
            return Err(Error::Pinned(entry.id));
        }
        proof::owned(self.ops, &entry.record)?;
        run.visit(request.target)?;
        snapshot::verify(self.ops, &entry.record, (&entry.record.held()?, false), run)?;
        let target = entry.target.as_ref().ok_or(Error::State(entry.id))?;
        let parent = target
            .location
            .parent()
            .ok_or_else(|| Error::Changed(target.location.clone()))?;
        let mut tx = self.catalog.checked_transaction(&entry)?;
        if let Some(owner) = owner {
            tx.authorize_rollback(entry.id, owner)?;
        }
        proof::owned(self.ops, &entry.record)?;
        proof::folder(self.ops, &parent, target.parent_identity)?;
        if !proof::node(self.ops, &entry.record.held()?, &entry.record.nodes[0])? {
            return Err(Error::Changed(entry.record.held()?));
        }
        self.ops
            .rename_atomic(&entry.record.held()?, &target.location)?;
        if !proof::same_object(self.ops, &target.location, &entry.record.nodes[0])? {
            return Err(Error::Changed(target.location.clone()));
        }
        entry.state = RetainedState::Restored;
        tx.save(&entry, RetainedState::Restoring)?;
        tx.commit()?;
        self.cleanup(&entry)?;
        Ok(entry)
    }
}
