use std::collections::HashMap;

use rusty_core::retention::{
    MAX_NODES, ProvedOverwrite, RetainedState, RetainedWritePlan, RetentionEntry, RetentionError,
    RetentionId, RetentionRecord,
};
use rusty_core::{Resolution, Step};

use super::{RetentionStoreError, RetentionTransaction, transaction::encode};
use crate::{JobId, JobState};

mod coverage;

pub struct RetainedCompletion<'a> {
    pub job: JobId,
    pub state: JobState,
    pub records: &'a [RetentionRecord],
    pub returned: &'a [ProvedOverwrite],
}

struct Owner {
    plan: RetainedWritePlan,
    state: String,
    created_at: i64,
}

impl RetentionTransaction<'_> {
    pub fn complete_overwrites(
        self,
        request: RetainedCompletion<'_>,
    ) -> Result<Vec<RetentionEntry>, RetentionStoreError> {
        let owner = self.validate_request(&request)?;
        let entries = self.complete_entries(&request, &owner)?;
        self.inner.execute(
            "UPDATE job SET state = ?2 WHERE id = ?1",
            rusqlite::params![request.job.0, state_text(request.state)?],
        )?;
        if request.state != JobState::RestorePending {
            self.inner
                .execute("DELETE FROM undo WHERE job_id = ?1", [request.job.0])?;
        }
        let entries = entries
            .into_iter()
            .map(|entry| self.get(entry.id))
            .collect::<Result<Vec<_>, _>>()?;
        self.commit()?;
        Ok(entries)
    }

    pub fn validate_overwrites(
        &self,
        request: &RetainedCompletion<'_>,
    ) -> Result<(), RetentionStoreError> {
        let owner = self.validate_request(request)?;
        if !matches!(owner.state.as_str(), "running" | "restore_pending") {
            return Err(RetentionError::Invalid("terminal owner has no recovery authority").into());
        }
        Ok(())
    }

    fn validate_request(
        &self,
        request: &RetainedCompletion<'_>,
    ) -> Result<Owner, RetentionStoreError> {
        let owner = self.owner(request)?;
        coverage::verify_temporaries(&self.inner, request.job)?;
        validate_records(&owner.plan, owner.created_at, request.records)?;
        validate_returned(&owner, request.returned)?;
        if matches!(owner.state.as_str(), "running" | "restore_pending") {
            coverage::verify(&self.inner, request)?;
        }
        Ok(owner)
    }

    fn owner(&self, request: &RetainedCompletion<'_>) -> Result<Owner, RetentionStoreError> {
        let (kind, state, created_at, bytes): (String, String, i64, Vec<u8>) =
            self.inner.query_row(
                "SELECT kind, state, created_at, plan FROM job WHERE id = ?1",
                [request.job.0],
                |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?, row.get(3)?)),
            )?;
        let next = state_text(request.state)?;
        if state != "running" && state != "restore_pending" && state != next {
            return Err(RetentionError::Invalid("retention owner cannot be completed").into());
        }
        let plan: RetainedWritePlan = serde_json::from_slice(&bytes)?;
        if kind != plan.journal_kind()
            || request.records.len().saturating_add(request.returned.len()) > MAX_NODES
        {
            return Err(
                RetentionError::Invalid("retention owner kind or record count changed").into(),
            );
        }
        if request.state == JobState::Done {
            self.verify_steps(request.job, plan.plan().steps.len())?;
        }
        Ok(Owner {
            plan,
            state,
            created_at,
        })
    }

    fn verify_steps(&self, job: JobId, expected: usize) -> Result<(), RetentionStoreError> {
        let mut statement = self
            .inner
            .prepare("SELECT seq, state FROM job_step WHERE job_id = ?1 ORDER BY seq")?;
        let mut rows = statement.query([job.0])?;
        let mut count = 0;
        while let Some(row) = rows.next()? {
            let seq: i64 = row.get(0)?;
            let state: String = row.get(1)?;
            if usize::try_from(seq).ok() != Some(count)
                || !matches!(state.as_str(), "done" | "skipped")
            {
                return Err(RetentionError::Invalid("retention owner has incomplete steps").into());
            }
            count += 1;
        }
        if count != expected {
            return Err(RetentionError::Invalid("retention owner has missing steps").into());
        }
        Ok(())
    }

    fn complete_entries(
        &self,
        request: &RetainedCompletion<'_>,
        owner: &Owner,
    ) -> Result<Vec<RetentionEntry>, RetentionStoreError> {
        let mut entries =
            self.retire_returned(self.owned_entries(request.job)?, request.returned)?;
        if entries.is_empty() && matches!(owner.state.as_str(), "running" | "restore_pending") {
            for record in request.records {
                self.inner.execute(
                    "INSERT INTO retained_entry (job_id, state, record) VALUES (?1, 'ready', ?2)",
                    rusqlite::params![request.job.0, encode(record)?],
                )?;
                entries.push(self.get(RetentionId(self.inner.last_insert_rowid()))?);
            }
        }
        let expected = request
            .records
            .iter()
            .map(|record| Ok((record.held()?, record)))
            .collect::<Result<HashMap<_, _>, RetentionError>>()?;
        if entries.len() != expected.len() {
            return Err(RetentionError::Invalid("retained completion coverage changed").into());
        }
        for entry in &entries {
            if entry.state != RetainedState::Ready
                || expected.get(&entry.record.held()?) != Some(&&entry.record)
            {
                return Err(RetentionError::Invalid(
                    "existing retained versions do not match completion",
                )
                .into());
            }
        }
        Ok(entries)
    }

    fn retire_returned(
        &self,
        entries: Vec<RetentionEntry>,
        returned: &[ProvedOverwrite],
    ) -> Result<Vec<RetentionEntry>, RetentionStoreError> {
        let proofs = returned
            .iter()
            .map(|saved| (&saved.held, saved))
            .collect::<HashMap<_, _>>();
        let mut retained = Vec::new();
        for mut entry in entries {
            let held = entry.record.held()?;
            let Some(saved) = proofs.get(&held) else {
                retained.push(entry);
                continue;
            };
            if entry.state != RetainedState::Ready || !coverage::matches(&entry.record, saved) {
                return Err(RetentionError::Invalid(
                    "returned entry differs from its durable proof",
                )
                .into());
            }
            entry.state = RetainedState::Restored;
            entry.target = Some(rusty_core::retention::RestoreTarget {
                location: entry.record.source.clone(),
                parent_identity: entry.record.parent_identity,
            });
            entry.validate()?;
            self.inner.execute(
                "UPDATE retained_entry SET state = 'restored', target = ?2,
                 terminal_ms = CAST(unixepoch('subsec') * 1000 AS INTEGER) WHERE id = ?1",
                rusqlite::params![entry.id.0, encode(&entry.target)?],
            )?;
        }
        Ok(retained)
    }

    fn owned_entries(&self, job: JobId) -> Result<Vec<RetentionEntry>, RetentionStoreError> {
        let mut statement = self
            .inner
            .prepare("SELECT id FROM retained_entry WHERE job_id = ?1 ORDER BY id")?;
        let ids = statement
            .query_map([job.0], |row| Ok(RetentionId(row.get(0)?)))?
            .collect::<Result<Vec<_>, _>>()?;
        ids.into_iter().map(|id| self.get(id)).collect()
    }
}

fn state_text(state: JobState) -> Result<&'static str, RetentionError> {
    match state {
        JobState::Done => Ok("done"),
        JobState::Failed => Ok("failed"),
        JobState::Cancelled => Ok("cancelled"),
        JobState::RestorePending => Ok("restore_pending"),
        _ => Err(RetentionError::Invalid(
            "unsupported retained completion state",
        )),
    }
}

fn validate_returned(
    owner: &Owner,
    returned: &[ProvedOverwrite],
) -> Result<(), RetentionStoreError> {
    let records = returned
        .iter()
        .map(|saved| {
            Ok(RetentionRecord {
                version: 2,
                payload_name: saved.held.name().map(|name| name.to_text()),
                source: saved.original.clone(),
                container: saved
                    .held
                    .parent()
                    .ok_or(RetentionError::Invalid("returned proof has no holder"))?,
                parent_identity: saved.proof.parent_identity,
                container_identity: Some(saved.proof.container_identity),
                nodes: vec![saved.proof.node.clone()],
                created_at: owner.created_at,
                expires_at: owner.plan.policy().expires_at(owner.created_at)?,
            })
        })
        .collect::<Result<Vec<_>, RetentionError>>()?;
    validate_records(&owner.plan, owner.created_at, &records)
}

fn validate_records(
    plan: &RetainedWritePlan,
    created_at: i64,
    records: &[RetentionRecord],
) -> Result<(), RetentionStoreError> {
    let mut held = std::collections::HashSet::new();
    for record in records {
        record.validate()?;
        let allowed = plan.plan().steps.iter().any(|step| matches!(step,
            Step::CopyFile { target, .. } | Step::Rename { target, .. } if target.same_place(&record.source)))
            && plan.plan().conflicts.iter().any(|conflict| conflict.resolution == Resolution::Overwrite && conflict.target.same_place(&record.source));
        if record.version != 2
            || !allowed
            || record.created_at != created_at
            || record.expires_at != plan.policy().expires_at(created_at)?
            || !held.insert(record.held()?)
        {
            return Err(
                RetentionError::Invalid("retained version does not match its owner plan").into(),
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests;
