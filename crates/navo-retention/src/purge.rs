use navo_core::retention::{
    RetainedKind, RetainedNode, RetainedState, RetentionEntry, RetentionId,
};
use navo_store::RetentionCatalog;

use crate::{Error, RetentionRun, Vault, ancestry::Ancestors, proof, snapshot};

impl Vault<'_> {
    /// Purges `id` for maintenance; refuses any pinned entry.
    pub fn purge(
        &mut self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        self.purge_as(id, false, run)
    }

    /// Deletes `id` for the user: unlike `purge`, also a Conflict and an entry kept by a failed delete.
    pub fn delete(
        &mut self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        self.purge_as(id, true, run)
    }

    fn purge_as(
        &mut self,
        id: RetentionId,
        user: bool,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        // A restore that had landed already ends here; the caller sees the Restored state.
        if let Some(settled) = self.settle_interrupted_restore(id, run)?
            && settled.state == RetainedState::Restored
        {
            return Ok(settled);
        }
        let mut entry = self.get(id)?;
        if entry.pinned && !(user && entry.kept) && entry.state != RetainedState::Purging {
            return Err(Error::Pinned(id));
        }
        if user && entry.state == RetainedState::Conflict && entry.target.is_some() {
            entry = self.reconcile(id, run)?;
            if entry.state == RetainedState::Restored {
                return Ok(entry);
            }
        }
        match entry.state {
            RetainedState::Ready => self.begin_purge(entry, RetainedState::Ready, run)?,
            RetainedState::Conflict if user => {
                self.begin_purge(entry, RetainedState::Conflict, run)?;
            }
            RetainedState::Purging => {}
            _ => return Err(Error::State(id)),
        }
        self.complete_purge(id, run)
    }

    /// Moves `entry` from `from` to Purging once its whole held copy is proven unchanged.
    fn begin_purge(
        &mut self,
        mut entry: RetentionEntry,
        from: RetainedState,
        run: &mut RetentionRun<'_>,
    ) -> Result<(), Error> {
        proof::owned(self.ops, &entry.record)?;
        snapshot::verify(self.ops, &entry.record, (&entry.record.held()?, false), run)?;
        let mut tx = self.catalog.checked_transaction(&entry)?;
        proof::owned(self.ops, &entry.record)?;
        if !proof::node(self.ops, &entry.record.held()?, &entry.record.nodes[0])? {
            return Err(Error::Changed(entry.record.held()?));
        }
        entry.state = RetainedState::Purging;
        entry.target = None;
        tx.save(&entry, from)?;
        tx.commit()?;
        Ok(())
    }

    /// A Purging entry was chosen for deletion already, so only an active job stops it.
    fn complete_purge(
        &mut self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        let mut entry = self.catalog.get(id)?;
        if entry.state != RetainedState::Purging {
            return Err(Error::State(id));
        }
        if entry.pinned && !entry.kept {
            return Err(Error::Pinned(id));
        }
        Eraser {
            ops: self.ops,
            catalog: &mut self.catalog,
            entry: &entry,
        }
        .erase(run)?;
        let mut tx = self.catalog.checked_transaction(&entry)?;
        entry.state = RetainedState::Purged;
        tx.save(&entry, RetainedState::Purging)?;
        tx.commit()?;
        self.cleanup(&entry)?;
        Ok(entry)
    }
}

struct Eraser<'a> {
    ops: &'a dyn navo_fs::ProviderOps,
    catalog: &'a mut RetentionCatalog,
    entry: &'a RetentionEntry,
}

impl Eraser<'_> {
    fn erase(&mut self, run: &mut RetentionRun<'_>) -> Result<(), Error> {
        let record = &self.entry.record;
        let scope = proof::owned(self.ops, record)?;
        let ancestors = Ancestors::recorded(record, scope)?;
        let root = record.held()?;
        snapshot::verify(self.ops, record, (&root, true), run)?;
        for node in record.nodes.iter().rev() {
            run.visit(&node.location(&root)?)?;
            self.erase_node((&ancestors, scope), node)?;
        }
        Ok(())
    }

    fn erase_node(
        &mut self,
        (ancestors, scope): (&Ancestors, proof::Scope),
        node: &RetainedNode,
    ) -> Result<(), Error> {
        let record = &self.entry.record;
        let location = node.location(&record.held()?)?;
        let tx = self.catalog.transaction()?;
        tx.check_purge_owner(self.entry)?;
        proof::owned(self.ops, record)?;
        if !ancestors.verify(self.ops, &node.path)?
            || proof::present(self.ops, &location)?.is_none()
        {
            return Ok(());
        }
        if !proof::node(self.ops, &location, node)? {
            return Err(Error::Changed(location));
        }
        proof::on_scope(self.ops, &location, scope)?;
        match node.kind {
            RetainedKind::File => self.ops.remove_file(&location)?,
            RetainedKind::Link => self.ops.remove_link(&location)?,
            RetainedKind::Folder => self.ops.remove_empty_folder(&location)?,
        }
        tx.commit()?;
        Ok(())
    }
}
