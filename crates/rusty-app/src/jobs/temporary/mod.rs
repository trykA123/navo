mod cleanup;
mod scope;

use super::{JournalLog, SharedJournal, lock};
use rusty_core::retention::{CopyTemporary, RetainedWritePlan};
use rusty_core::{FsError, FsErrorKind, Location};
use rusty_fs::{FileIdentity, ProviderOps};
use rusty_store::{JobId, StepEvidence};

pub(super) use cleanup::{cleanup, paths};

pub(super) fn prepare(log: &JournalLog, proof: &CopyTemporary) -> Result<(), FsError> {
    let plan = &log
        .overwrites
        .as_ref()
        .ok_or_else(|| failure(&proof.temporary))?
        .plan;
    proof
        .validate(plan)
        .map_err(|_| failure(&proof.temporary))?;
    let bytes = serde_json::to_vec(proof).map_err(|_| failure(&proof.temporary))?;
    lock(&log.journal)
        .replace_step_detail(
            log.job,
            StepEvidence {
                seq: proof.seq,
                before: None,
                after: Some(&bytes),
            },
        )
        .map_err(|_| failure(&proof.temporary))
}

pub(super) fn created(
    log: &JournalLog,
    (seq, identity): (u32, FileIdentity),
    ops: &dyn ProviderOps,
) -> Result<(), FsError> {
    let plan = &log
        .overwrites
        .as_ref()
        .ok_or_else(|| FsError::new(FsErrorKind::Unknown))?
        .plan;
    let (bytes, mut proof) = read(&log.journal, log.job, (seq, plan))
        .map_err(|_| FsError::new(FsErrorKind::Unknown))?
        .ok_or_else(|| FsError::new(FsErrorKind::Unknown))?;
    if proof.identity.is_some_and(|expected| expected != identity) {
        return Err(failure(&proof.temporary));
    }
    proof.identity = Some(identity);
    proof
        .validate(plan)
        .map_err(|_| failure(&proof.temporary))?;
    scope::parent(ops, &proof).map_err(|_| failure(&proof.temporary))?;
    let created = serde_json::to_vec(&proof).map_err(|_| failure(&proof.temporary))?;
    lock(&log.journal)
        .replace_step_detail(
            log.job,
            StepEvidence {
                seq,
                before: Some(&bytes),
                after: Some(&created),
            },
        )
        .map_err(|_| failure(&proof.temporary))
}

pub(super) fn retire(log: &JournalLog, seq: u32, ops: &dyn ProviderOps) -> Result<(), FsError> {
    let plan = &log
        .overwrites
        .as_ref()
        .ok_or_else(|| FsError::new(FsErrorKind::Unknown))?
        .plan;
    let Some((bytes, proof)) =
        read(&log.journal, log.job, (seq, plan)).map_err(|_| FsError::new(FsErrorKind::Unknown))?
    else {
        return Ok(());
    };
    scope::absent(ops, &proof).map_err(|_| failure(&proof.temporary))?;
    clear(&log.journal, log.job, (seq, &bytes)).map_err(|_| failure(&proof.temporary))
}

fn read(
    journal: &SharedJournal,
    job: JobId,
    (seq, plan): (u32, &RetainedWritePlan),
) -> Result<Option<(Vec<u8>, CopyTemporary)>, String> {
    let Some(bytes) = lock(journal)
        .step_detail(job, seq)
        .map_err(|error| error.to_string())?
    else {
        return Ok(None);
    };
    let proof = decode(&bytes, (seq, plan))?;
    Ok(Some((bytes, proof)))
}

fn decode(bytes: &[u8], (seq, plan): (u32, &RetainedWritePlan)) -> Result<CopyTemporary, String> {
    if bytes.len() > rusty_core::retention::MAX_RECORD_BYTES {
        return Err("copy temporary evidence is oversized".into());
    }
    let proof: CopyTemporary = serde_json::from_slice(bytes).map_err(|error| error.to_string())?;
    if proof.seq != seq {
        return Err("copy temporary step changed".into());
    }
    proof.validate(plan).map_err(|error| error.to_string())?;
    Ok(proof)
}

fn clear(journal: &SharedJournal, job: JobId, (seq, before): (u32, &[u8])) -> Result<(), String> {
    lock(journal)
        .replace_step_detail(
            job,
            StepEvidence {
                seq,
                before: Some(before),
                after: None,
            },
        )
        .map_err(|error| error.to_string())
}

fn failure(location: &Location) -> FsError {
    FsError {
        location: Some(location.clone()),
        ..FsError::new(FsErrorKind::Locked)
    }
}
