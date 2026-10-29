use rusty_core::retention::{
    RetainedKind, RetainedNode, RetainedState, RetentionEntry, RetentionId,
};
use rusty_store::RetentionCatalog;

use crate::{Error, RetentionRun, Vault, ancestry::Ancestors, proof, snapshot};

impl Vault<'_> {
    pub fn purge(
        &mut self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        let mut entry = self.get(id)?;
        if entry.pinned {
            return Err(Error::Pinned(id));
        }
        if entry.state == RetainedState::Ready {
            proof::owned(self.ops, &entry.record)?;
            snapshot::verify(self.ops, &entry.record, (&entry.record.held()?, false), run)?;
            let mut tx = self.catalog.checked_transaction(&entry)?;
            proof::owned(self.ops, &entry.record)?;
            if !proof::node(self.ops, &entry.record.held()?, &entry.record.nodes[0])? {
                return Err(Error::Changed(entry.record.held()?));
            }
            entry.state = RetainedState::Purging;
            tx.save(&entry, RetainedState::Ready)?;
            tx.commit()?;
        } else if entry.state != RetainedState::Purging {
            return Err(Error::State(id));
        }
        self.complete_purge(id, run)
    }

    fn complete_purge(
        &mut self,
        id: RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        let mut entry = self.catalog.get(id)?;
        if entry.state != RetainedState::Purging {
            return Err(Error::State(id));
        }
        if entry.pinned {
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
    ops: &'a dyn rusty_fs::ProviderOps,
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
