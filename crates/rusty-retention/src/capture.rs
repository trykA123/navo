use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

use rusty_core::ProviderPath;
use rusty_core::retention::{CONTAINER_PREFIX, RetainedState, RetentionEntry, RetentionRecord};

use crate::{CaptureRequest, Error, RetentionRun, Vault, proof, snapshot};

static CAPTURES: AtomicU64 = AtomicU64::new(0);

impl Vault<'_> {
    pub fn capture(
        &mut self,
        request: CaptureRequest<'_>,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
        let record = self.prepare_record(&request, run)?;
        let mut tx = self.catalog.transaction()?;
        let entry = tx.insert(&record, request.job)?;
        tx.commit()?;
        self.ops.create_folder(&record.container)?;
        let mut tx = self.catalog.transaction()?;
        let mut entry = tx.get(entry.id)?;
        entry.record.container_identity = Some(proof::identity(self.ops, &record.container)?);
        tx.save(&entry, RetainedState::Preparing)?;
        tx.commit()?;
        self.ops.hide_retained(
            &entry.record.container,
            entry
                .record
                .container_identity
                .ok_or(Error::State(entry.id))?,
        )?;
        self.complete_capture(entry, run)
    }

    fn prepare_record(
        &self,
        request: &CaptureRequest<'_>,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionRecord, Error> {
        if !self.ops.supports_atomic_rename() {
            return Err(Error::Unsupported(request.source.clone()));
        }
        let parent = request
            .source
            .parent()
            .ok_or_else(|| Error::Unsupported(request.source.clone()))?;
        let parent_identity = proof::identity(self.ops, &parent)?;
        proof::folder(self.ops, &parent, parent_identity)?;
        let container = new_container(&parent)?;
        let record = RetentionRecord {
            version: 1,
            payload_name: None,
            source: request.source.clone(),
            container,
            parent_identity,
            container_identity: None,
            nodes: snapshot::read(
                self.ops,
                (request.source, proof::scope(self.ops, &parent)?),
                run,
            )?,
            created_at: request.created_at,
            expires_at: request.policy.expires_at(request.created_at)?,
        };
        record.validate()?;
        Ok(record)
    }

    fn complete_capture(
        &mut self,
        entry: RetentionEntry,
        run: &mut RetentionRun<'_>,
    ) -> Result<RetentionEntry, Error> {
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
        self.ops
            .rename_atomic(&entry.record.source, &entry.record.held()?)?;
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
        Ok(entry)
    }
}

fn new_container(parent: &rusty_core::Location) -> Result<rusty_core::Location, Error> {
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
