use rusty_core::retention::{
    MAX_RECORD_BYTES, RetainedState, RetentionEntry, RetentionError, RetentionId, RetentionRecord,
};

use super::{RetentionStoreError, rows};
use crate::JobId;

pub struct RetentionTransaction<'a> {
    pub(super) inner: rusqlite::Transaction<'a>,
}

impl RetentionTransaction<'_> {
    pub fn check_purge_owner(&self, entry: &RetentionEntry) -> Result<(), RetentionStoreError> {
        if entry.state != RetainedState::Purging {
            return Err(RetentionStoreError::Stale(entry.id));
        }
        let allowed: bool = self.inner.query_row(
            "SELECT EXISTS(SELECT 1 FROM retained_entry e WHERE e.id = ?1 AND e.state = 'purging'
             AND e.job_id IS ?2 AND (e.job_id IS NULL OR EXISTS(SELECT 1 FROM job j WHERE j.id = e.job_id
             AND j.state IN ('done', 'failed', 'cancelled', 'rolled_back', 'undone'))))",
            rusqlite::params![entry.id.0, entry.job_id], |row| row.get(0),
        )?;
        if !allowed {
            return Err(RetentionStoreError::Pinned(entry.id));
        }
        Ok(())
    }
    pub fn get(&self, id: RetentionId) -> Result<RetentionEntry, RetentionStoreError> {
        rows::get(&self.inner, id)
    }

    pub fn insert(
        &mut self,
        record: &RetentionRecord,
        job: Option<JobId>,
    ) -> Result<RetentionEntry, RetentionStoreError> {
        record.validate()?;
        if record.container_identity.is_some() {
            return Err(RetentionError::Invalid("new capture already owns a container").into());
        }
        let bytes = encode(record)?;
        self.inner.execute(
            "INSERT INTO retained_entry (job_id, state, record) VALUES (?1, 'preparing', ?2)",
            rusqlite::params![job.map(|job| job.0), bytes],
        )?;
        self.get(RetentionId(self.inner.last_insert_rowid()))
    }

    pub fn save(
        &mut self,
        entry: &RetentionEntry,
        expected: RetainedState,
    ) -> Result<(), RetentionStoreError> {
        self.save_authorized(entry, expected, None)
    }

    pub fn authorize_rollback(
        &self,
        id: RetentionId,
        owner: JobId,
    ) -> Result<(), RetentionStoreError> {
        let entry = self.get(id)?;
        let active: bool = self.inner.query_row(
            "SELECT EXISTS(SELECT 1 FROM job WHERE id = ?1 AND state IN ('running', 'restore_pending'))",
            [owner.0], |row| row.get(0),
        )?;
        if entry.job_id != Some(owner.0) || !entry.pinned || !active {
            return Err(RetentionStoreError::Pinned(id));
        }
        Ok(())
    }

    pub fn save_rollback(
        &mut self,
        entry: &RetentionEntry,
        owner: JobId,
    ) -> Result<(), RetentionStoreError> {
        self.authorize_rollback(entry.id, owner)?;
        if entry.state != RetainedState::Restoring
            || !entry
                .target
                .as_ref()
                .is_some_and(|target| target.location == entry.record.source)
        {
            return Err(
                RetentionError::Invalid("owner rollback must restore its original source").into(),
            );
        }
        self.save_authorized(entry, RetainedState::Ready, Some(owner))
    }

    fn save_authorized(
        &mut self,
        entry: &RetentionEntry,
        expected: RetainedState,
        owner: Option<JobId>,
    ) -> Result<(), RetentionStoreError> {
        entry.validate()?;
        let before = self.get(entry.id)?;
        if before.state != expected {
            return Err(RetentionStoreError::Stale(entry.id));
        }
        validate_change(&before, entry)?;
        if before.pinned
            && owner.is_none()
            && matches!(
                entry.state,
                RetainedState::Restoring | RetainedState::Purging
            )
        {
            return Err(RetentionStoreError::Pinned(entry.id));
        }
        let record = encode(&entry.record)?;
        let target = entry.target.as_ref().map(encode).transpose()?;
        self.inner.execute(
            "UPDATE retained_entry SET state = ?2, record = ?3, target = ?4,
             terminal_ms = CASE WHEN ?2 IN ('restored', 'purged', 'aborted')
                 THEN CAST(unixepoch('subsec') * 1000 AS INTEGER) END
             WHERE id = ?1",
            rusqlite::params![entry.id.0, entry.state.as_str(), record, target],
        )?;
        Ok(())
    }

    pub fn commit(self) -> Result<(), RetentionStoreError> {
        self.inner.commit()?;
        Ok(())
    }
}

pub(super) fn encode(value: &impl serde::Serialize) -> Result<Vec<u8>, RetentionStoreError> {
    let bytes = serde_json::to_vec(value)?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(RetentionError::Invalid("retention record exceeds its size limit").into());
    }
    Ok(bytes)
}

fn validate_change(
    before: &RetentionEntry,
    after: &RetentionEntry,
) -> Result<(), RetentionStoreError> {
    let mut record = before.record.clone();
    record.container_identity = after.record.container_identity;
    if record != after.record
        || before.job_id != after.job_id
        || !allowed(before.state, after.state)
        || (before.record.container_identity.is_some()
            && before.record.container_identity != after.record.container_identity)
        || (before.target.is_some()
            && before.target != after.target
            && after.state != RetainedState::Ready)
    {
        return Err(RetentionError::Invalid("invalid retention state transition").into());
    }
    Ok(())
}

fn allowed(from: RetainedState, to: RetainedState) -> bool {
    use RetainedState::{
        Aborted, Conflict, Preparing, Purged, Purging, Ready, Restored, Restoring,
    };
    matches!(
        (from, to),
        (Preparing, Preparing | Ready | Aborted | Conflict)
            | (Ready, Restoring | Purging | Conflict)
            | (Restoring, Ready | Restored | Conflict)
            | (Purging, Purged | Conflict)
    )
}
