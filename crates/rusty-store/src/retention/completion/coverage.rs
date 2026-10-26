use std::collections::{HashMap, HashSet};

use super::RetainedCompletion;
use crate::{JobState, RetentionStoreError};
use rusty_core::retention::{MAX_RECORD_BYTES, ProvedOverwrite, RetentionError, RetentionRecord};

pub(super) fn verify(
    conn: &rusqlite::Connection,
    request: &RetainedCompletion<'_>,
) -> Result<(), RetentionStoreError> {
    let records = request
        .records
        .iter()
        .map(|record| Ok((record.held()?, record)))
        .collect::<Result<HashMap<_, _>, RetentionError>>()?;
    let returned: HashMap<_, _> = request
        .returned
        .iter()
        .map(|proof| (&proof.held, proof))
        .collect();
    if returned.len() != request.returned.len()
        || (request.state == JobState::Done && !returned.is_empty())
    {
        return Err(RetentionError::Invalid("invalid returned overwrite coverage").into());
    }
    let mut known = HashSet::new();
    let mut retained_count = 0;
    let mut returned_count = 0;
    let mut statement = conn.prepare("SELECT inverse FROM undo WHERE job_id = ?1 ORDER BY id")?;
    let mut rows = statement.query([request.job.0])?;
    while let Some(row) = rows.next()? {
        let bytes: Vec<u8> = row.get(0)?;
        let proof = decode(&bytes)?;
        if !known.insert(proof.held.clone()) {
            return Err(RetentionError::Invalid("duplicate overwrite proof").into());
        }
        if let Some(record) = records.get(&proof.held) {
            if !matches(record, &proof) {
                return Err(RetentionError::Invalid(
                    "retained version differs from its durable proof",
                )
                .into());
            }
            retained_count += 1;
        } else if returned.get(&proof.held) == Some(&&proof) {
            returned_count += 1;
        } else {
            return Err(
                RetentionError::Invalid("an overwrite proof is missing from completion").into(),
            );
        }
    }
    if retained_count != records.len() || returned_count != returned.len() {
        return Err(
            RetentionError::Invalid("overwrite completion lacks durable proof coverage").into(),
        );
    }
    Ok(())
}

pub(super) fn matches(record: &RetentionRecord, saved: &ProvedOverwrite) -> bool {
    record.source == saved.original
        && record.parent_identity == saved.proof.parent_identity
        && record.container_identity == Some(saved.proof.container_identity)
        && record.nodes.len() == 1
        && record.nodes.first() == Some(&saved.proof.node)
}

fn decode(bytes: &[u8]) -> Result<ProvedOverwrite, RetentionStoreError> {
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(RetentionError::Invalid("oversized overwrite proof").into());
    }
    Ok(serde_json::from_slice(bytes)?)
}

pub(super) fn verify_temporaries(
    conn: &rusqlite::Connection,
    job: crate::JobId,
) -> Result<(), RetentionStoreError> {
    let pending: bool = conn.query_row(
        "SELECT EXISTS(SELECT 1 FROM job_step WHERE job_id = ?1 AND detail IS NOT NULL)",
        [job.0],
        |row| row.get(0),
    )?;
    if pending {
        return Err(RetentionError::Invalid("copy temporary evidence is not retired").into());
    }

    Ok(())
}
