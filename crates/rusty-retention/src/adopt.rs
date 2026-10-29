use crate::{Error, RetentionRun, Vault, proof};
use rusty_core::retention::{
    OverwriteProof, ProvedOverwrite, RetainedKind, RetentionEntry, RetentionPolicy, RetentionRecord,
};
use rusty_fs::HeldFile;
use rusty_store::{JobId, JobState, RetainedCompletion};

pub struct AdoptJob<'a> {
    pub job: JobId,
    pub state: JobState,
    pub files: &'a [(HeldFile, OverwriteProof)],
    pub policy: RetentionPolicy,
    pub created_at: i64,
}

struct Batch {
    retained: Vec<(RetentionRecord, u64)>,
    returned: Vec<(RetentionRecord, ProvedOverwrite)>,
}

impl Vault<'_> {
    pub fn adopt_job(
        &mut self,
        request: &AdoptJob<'_>,
        run: &mut RetentionRun<'_>,
    ) -> Result<Vec<RetentionEntry>, Error> {
        let records = self.prepare_adoptions(request, run)?;
        let batch = self.collect_adoptions(request, (records, run.cancel))?;
        for (record, mount) in &batch.retained {
            verify_record(self.ops, record, *mount)?;
        }
        for (record, saved) in &batch.returned {
            verify_parent(self.ops, record, saved.proof.mount_identity)?;
            if !proof::node(self.ops, &record.source, &record.nodes[0])? {
                return Err(Error::Changed(record.source.clone()));
            }
        }
        let records: Vec<_> = batch
            .retained
            .into_iter()
            .map(|(record, _)| record)
            .collect();
        let returned: Vec<_> = batch.returned.into_iter().map(|(_, proof)| proof).collect();
        Ok(self
            .catalog
            .transaction()?
            .complete_overwrites(RetainedCompletion {
                job: request.job,
                state: request.state,
                records: &records,
                returned: &returned,
            })?)
    }

    fn prepare_adoptions(
        &mut self,
        request: &AdoptJob<'_>,
        run: &mut RetentionRun<'_>,
    ) -> Result<Vec<RetentionRecord>, Error> {
        if request.files.len() > rusty_core::retention::MAX_NODES {
            return Err(rusty_core::retention::RetentionError::Invalid(
                "too many overwrite proofs",
            )
            .into());
        }
        let mut records = Vec::new();
        for (held, evidence) in request.files {
            run.visit(&held.original)?;
            records.push(adopted_record(held, evidence, request)?);
        }
        self.catalog
            .transaction()?
            .validate_overwrites(&RetainedCompletion {
                job: request.job,
                state: request.state,
                records: &records,
                returned: &[],
            })?;
        Ok(records)
    }

    fn collect_adoptions(
        &self,
        request: &AdoptJob<'_>,
        (records, cancel): (Vec<RetentionRecord>, &rusty_fs::CancelToken),
    ) -> Result<Batch, Error> {
        let mut batch = Batch {
            retained: vec![],
            returned: vec![],
        };
        for ((held, evidence), record) in request.files.iter().zip(records) {
            if cancel.is_cancelled() {
                return Err(rusty_core::FsError::new(rusty_core::FsErrorKind::Cancelled).into());
            }
            verify_parent(self.ops, &record, evidence.mount_identity)?;
            return_missing(self.ops, &record, (request.state, evidence.mount_identity))?;
            if request.state != JobState::Done
                && proof::present(self.ops, &record.held()?)?.is_none()
                && proof::node(self.ops, &record.source, &record.nodes[0])?
            {
                proof::cleanup_record(self.ops, &record)?;
                batch.returned.push((
                    record,
                    ProvedOverwrite {
                        original: held.original.clone(),
                        held: held.held.clone(),
                        volume: held.volume,
                        placed_size: held.placed.map(|stamp| stamp.size),
                        placed_modified_ms: held.placed.and_then(|stamp| stamp.modified_ms),
                        proof: evidence.clone(),
                    },
                ));
            } else {
                verify_record(self.ops, &record, evidence.mount_identity)?;
                batch.retained.push((record, evidence.mount_identity));
            }
        }
        Ok(batch)
    }
}

fn return_missing(
    ops: &dyn rusty_fs::ProviderOps,
    record: &RetentionRecord,
    (state, mount): (JobState, u64),
) -> Result<(), Error> {
    if state == JobState::Done
        || proof::present(ops, &record.source)?.is_some()
        || proof::present(ops, &record.held()?)?.is_none()
    {
        return Ok(());
    }
    verify_record(ops, record, mount)?;
    ops.rename_atomic(&record.held()?, &record.source)?;
    Ok(())
}

fn verify_parent(
    ops: &dyn rusty_fs::ProviderOps,
    record: &RetentionRecord,
    mount: u64,
) -> Result<(), Error> {
    proof::parent(ops, record)?;
    let parent = record
        .source
        .parent()
        .ok_or_else(|| Error::Changed(record.source.clone()))?;
    if proof::scope(ops, &parent)?.mount != mount {
        return Err(Error::Changed(parent));
    }
    Ok(())
}

fn verify_record(
    ops: &dyn rusty_fs::ProviderOps,
    record: &RetentionRecord,
    mount: u64,
) -> Result<(), Error> {
    let scope = proof::owned(ops, record)?;
    if scope.mount != mount || !proof::node(ops, &record.held()?, &record.nodes[0])? {
        return Err(Error::Changed(record.held()?));
    }
    proof::on_scope(ops, &record.held()?, scope)
}

fn adopted_record(
    held: &HeldFile,
    evidence: &OverwriteProof,
    request: &AdoptJob<'_>,
) -> Result<RetentionRecord, Error> {
    let container = held
        .held
        .parent()
        .ok_or_else(|| Error::Changed(held.held.clone()))?;
    if evidence.node.kind == RetainedKind::Folder
        || !evidence.node.path.is_empty()
        || evidence.replacement_identity.volume != evidence.parent_identity.volume
    {
        return Err(Error::Changed(held.held.clone()));
    }
    let record = RetentionRecord {
        version: 2,
        payload_name: held.held.name().map(|name| name.to_text()),
        source: held.original.clone(),
        container,
        parent_identity: evidence.parent_identity,
        container_identity: Some(evidence.container_identity),
        nodes: vec![evidence.node.clone()],
        created_at: request.created_at,
        expires_at: request.policy.expires_at(request.created_at)?,
    };
    record.validate()?;
    Ok(record)
}
