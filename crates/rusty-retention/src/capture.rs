use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusty_core::retention::{
    CONTAINER_PREFIX, RetainedNode, RetainedState, RetentionEntry, RetentionRecord,
    VAULT_RECORD_VERSION,
};
use rusty_core::{FsError, FsErrorKind, Location, ProviderPath};

use crate::{CaptureRequest, Error, RetentionRun, Vault, place, proof, snapshot};

static CAPTURES: AtomicU64 = AtomicU64::new(0);

#[cfg(windows)]
pub(super) const CROSS_DEVICE: i32 = 17;
#[cfg(not(windows))]
pub(super) const CROSS_DEVICE: i32 = 18;

struct Source {
    parent: Location,
    scope: proof::Scope,
    nodes: Vec<RetainedNode>,
}

impl Vault<'_> {
    pub fn capture(
        &mut self,
        request: &CaptureRequest<'_>,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        let source = self.inspect(request, run)?;
        for chain in place::chains(self.ops, (&source.parent, source.scope), &self.places)? {
            if let Some(entry) = self.capture_in(&chain, (request, &source), run)? {
                return Ok(entry);
            }
        }
        Err(Error::Unsupported(request.source.clone()))
    }

    fn inspect(
        &self,
        request: &CaptureRequest<'_>,
        run: &mut RetentionRun<'_>,
    ) -> Result<Source, Error> {
        if !self.ops.supports_atomic_rename() {
            return Err(Error::Unsupported(request.source.clone()));
        }
        let parent = request
            .source
            .parent()
            .ok_or_else(|| Error::Unsupported(request.source.clone()))?;
        proof::folder(self.ops, &parent, proof::identity(self.ops, &parent)?)?;
        let scope = proof::scope(self.ops, &parent)?;
        let nodes = snapshot::read(self.ops, (request.source, scope), run)?;
        Ok(Source {
            parent,
            scope,
            nodes,
        })
    }

    fn capture_in(
        &mut self,
        chain: &[Location],
        (request, source): (&CaptureRequest<'_>, &Source),
        run: &mut RetentionRun<'_>,
    ) -> Result<Option<RetentionEntry>, Error> {
        let mut created = place::Created::new();
        let registered = place::ensure_chain(self.ops, (chain, source.scope), &mut created)
            .map(|placed| self.register(request, source, &placed))
            .transpose();
        let entry = match registered {
            Ok(Some(entry)) => entry,
            outcome => {
                place::discard(self.ops, &created);
                return outcome.map(|_| None);
            }
        };
        let id = entry.id;
        match self.fill(&entry, run)? {
            Some(entry) => Ok(Some(entry)),
            None => self.withdraw(id, run),
        }
    }

    fn register(
        &mut self,
        request: &CaptureRequest<'_>,
        source: &Source,
        placed: &place::Placed,
    ) -> Result<RetentionEntry, Error> {
        let record = RetentionRecord {
            version: VAULT_RECORD_VERSION,
            payload_name: None,
            source: request.source.clone(),
            container: new_container(&placed.vault)?,
            parent_identity: placed.identity,
            container_identity: None,
            nodes: source.nodes.clone(),
            created_at: request.created_at,
            expires_at: request.policy.expires_at(request.created_at)?,
        };
        record.validate()?;
        let mut tx = self.catalog.transaction()?;
        let entry = tx.insert(&record, request.job)?;
        tx.commit()?;
        Ok(entry)
    }

    fn fill(
        &mut self,
        entry: &RetentionEntry,
        run: &mut RetentionRun<'_>,
    ) -> Result<Option<RetentionEntry>, Error> {
        match self.ops.create_private_folder(&entry.record.container) {
            Err(error) if error.kind == FsErrorKind::AccessDenied => return Ok(None),
            created => created?,
        }
        let mut tx = self.catalog.transaction()?;
        let mut entry = tx.get(entry.id)?;
        let identity = proof::identity(self.ops, &entry.record.container)?;
        entry.record.container_identity = Some(identity);
        tx.save(&entry, RetainedState::Preparing)?;
        tx.commit()?;
        self.ops.hide_retained(&entry.record.container, identity)?;
        self.complete_capture(&entry, run)
    }

    fn withdraw(
        &mut self,
        id: rusty_core::retention::RetentionId,
        run: &mut RetentionRun<'_>,
    ) -> Result<Option<RetentionEntry>, Error> {
        let settled = self.reconcile(id, run)?;
        if settled.state != RetainedState::Aborted {
            return Err(Error::State(id));
        }
        tracing::info!(
            id = id.0,
            "vault refused the capture; trying the next vault"
        );
        Ok(None)
    }

    fn complete_capture(
        &mut self,
        entry: &RetentionEntry,
        run: &mut RetentionRun<'_>,
    ) -> Result<Option<RetentionEntry>, Error> {
        let mut entry = self.catalog.get(entry.id)?;
        if entry.state != RetainedState::Preparing {
            return Err(Error::State(entry.id));
        }
        proof::owned(self.ops, &entry.record)?;
        snapshot::verify(self.ops, &entry.record, (&entry.record.source, false), run)?;
        let tx = self.catalog.checked_transaction(&entry)?;
        proof::owned(self.ops, &entry.record)?;
        if !proof::node(self.ops, &entry.record.source, &entry.record.nodes[0])? {
            return Err(Error::Changed(entry.record.source.clone()));
        }
        match self
            .ops
            .rename_atomic(&entry.record.source, &entry.record.held()?)
        {
            Err(error) if crosses_devices(&error) => return Ok(None),
            moved => moved?,
        }
        tx.commit()?;
        snapshot::verify(self.ops, &entry.record, (&entry.record.held()?, false), run)?;
        let mut tx = self.catalog.checked_transaction(&entry)?;
        proof::owned(self.ops, &entry.record)?;
        if !proof::node(self.ops, &entry.record.held()?, &entry.record.nodes[0])? {
            return Err(Error::Changed(entry.record.held()?));
        }
        entry.state = RetainedState::Ready;
        tx.save(&entry, RetainedState::Preparing)?;
        tx.commit()?;
        Ok(Some(entry))
    }
}

fn crosses_devices(error: &FsError) -> bool {
    error.os_code == Some(CROSS_DEVICE)
}

fn new_container(parent: &Location) -> Result<Location, Error> {
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |value| value.as_nanos());
    let serial = CAPTURES.fetch_add(1, Ordering::Relaxed);
    let name = ProviderPath::from_text(&format!(
        "{CONTAINER_PREFIX}{:x}-{nanos:x}-{serial:x}",
        std::process::id()
    ));
    parent
        .join(&name)
        .map_err(|_| Error::Unsupported(parent.clone()))
}
