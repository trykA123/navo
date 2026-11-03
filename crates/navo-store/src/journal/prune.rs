use super::{JobJournal, JournalError};

const RENAME_KIND: &str = "rename";
const MOVE_KIND: &str = "move";
const KEEP_RENAME_UNDO: u32 = 20;
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// How many finished jobs the journal keeps. Protected jobs never count against the age and
/// byte limits: running, restore-pending, live retained entries, undo rows, the newest 20
/// renames, undoable renames and moves, and the newest job (so job ids are never reused).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PruneLimits {
    pub keep_finished: u32,
    pub max_age_ms: Option<i64>,
    pub max_bytes: Option<u64>,
    pub now_ms: i64,
}

impl PruneLimits {
    pub const KEEP_FINISHED: u32 = 50;
    pub const MAX_AGE_MS: i64 = 30 * DAY_MS;
    pub const MAX_BYTES: u64 = 32 * 1024 * 1024;

    /// The app's limits: the newest 50, none older than 30 days, at most 32 MiB of plans and steps.
    #[must_use]
    pub fn standard(now_ms: i64) -> Self {
        Self {
            keep_finished: Self::KEEP_FINISHED,
            max_age_ms: Some(Self::MAX_AGE_MS),
            max_bytes: Some(Self::MAX_BYTES),
            now_ms,
        }
    }

    /// Keeps the newest `keep_finished` jobs, with no age or byte limit.
    #[must_use]
    pub fn keep(keep_finished: u32) -> Self {
        Self {
            keep_finished,
            max_age_ms: None,
            max_bytes: None,
            now_ms: 0,
        }
    }

    fn cutoff(&self) -> i64 {
        self.max_age_ms
            .map_or(i64::MIN, |age| self.now_ms.saturating_sub(age))
    }

    fn byte_cap(&self) -> i64 {
        self.max_bytes
            .map_or(i64::MAX, |bytes| i64::try_from(bytes).unwrap_or(i64::MAX))
    }
}

/// Result of the one VACUUM that turns on incremental auto_vacuum for an older database.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VacuumOnce {
    AlreadyIncremental,
    Converted,
    JobsPending,
    LowSpace { needed: u64 },
}

// Rank counts every non-running job, as before; the byte total counts only unprotected ones.
const PRUNE: &str = "
WITH ranked AS (
    SELECT id, row_number() OVER (ORDER BY created_at DESC, id DESC) AS rank
    FROM job WHERE state != 'running'
), candidate AS (
    SELECT j.id, j.created_at, r.rank,
        (j.kind IN (?2, ?3) AND j.state IN ('done', 'failed', 'cancelled'))
            OR j.id = (SELECT max(id) FROM job) AS exempt,
        length(j.plan) + coalesce(
            (SELECT sum(length(s.detail)) FROM job_step s WHERE s.job_id = j.id), 0) AS bytes
    FROM job j JOIN ranked r ON r.id = j.id
    WHERE j.state NOT IN ('running', 'restore_pending')
    AND j.id NOT IN (SELECT job_id FROM retained_entry WHERE job_id IS NOT NULL
        AND state NOT IN ('restored', 'purged', 'aborted'))
    AND j.id NOT IN (SELECT job_id FROM undo WHERE job_id IS NOT NULL)
    AND j.id NOT IN (
        SELECT id FROM job WHERE state = 'done' AND kind = ?2
        ORDER BY created_at DESC, id DESC LIMIT ?4
    )
), totals AS (
    SELECT id, created_at, rank, exempt,
        sum(bytes) OVER (ORDER BY created_at DESC, id DESC ROWS UNBOUNDED PRECEDING) AS total
    FROM candidate
)
DELETE FROM job WHERE id IN (
    SELECT id FROM totals
    WHERE rank > ?1 OR (NOT exempt AND (created_at < ?5 OR total > ?6))
)";

impl JobJournal {
    /// Deletes finished jobs past `limits`, oldest first; returns how many went.
    pub fn prune(&mut self, limits: PruneLimits) -> Result<u32, JournalError> {
        self.delete_past(limits)
    }

    fn delete_past(&self, limits: PruneLimits) -> Result<u32, JournalError> {
        let deleted = self.db.connection().execute(
            PRUNE,
            rusqlite::params![
                limits.keep_finished,
                RENAME_KIND,
                MOVE_KIND,
                KEEP_RENAME_UNDO,
                limits.cutoff(),
                limits.byte_cap(),
            ],
        )?;
        Ok(u32::try_from(deleted).unwrap_or(u32::MAX))
    }

    /// Runs the one VACUUM that enables incremental auto_vacuum, only with no running or
    /// restore-pending job and twice the database size free. Safe to call on every start.
    pub fn vacuum_once(&mut self, free_bytes: u64) -> Result<VacuumOnce, JournalError> {
        if self.db.is_incremental_vacuum()? {
            return Ok(VacuumOnce::AlreadyIncremental);
        }
        let pending: bool = self.db.connection().query_row(
            "SELECT EXISTS (SELECT 1 FROM job WHERE state IN ('running', 'restore_pending'))",
            [],
            |row| row.get(0),
        )?;
        if pending {
            return Ok(VacuumOnce::JobsPending);
        }
        let needed = self.db.size_bytes()?.saturating_mul(2);
        if free_bytes < needed {
            return Ok(VacuumOnce::LowSpace { needed });
        }
        self.db.vacuum_incremental()?;
        Ok(VacuumOnce::Converted)
    }

    /// Returns up to `pages` free pages to the file system; returns the free pages left.
    pub fn reclaim(&self, pages: u32) -> Result<u64, JournalError> {
        Ok(self.db.reclaim_free_pages(pages)?)
    }

    /// Truncates the WAL when no other connection is using it; returns false when busy.
    pub fn try_truncate_wal(&self) -> Result<bool, JournalError> {
        Ok(self.db.try_truncate_wal()?)
    }

    /// Like `prune`, but gives up at once (`None`) when another connection holds the write lock.
    pub fn prune_if_idle(&self, limits: PruneLimits) -> Result<Option<u32>, JournalError> {
        let _no_wait = self.db.no_wait()?;
        busy_is_none(self.delete_past(limits))
    }

    /// Like `reclaim`, but gives up at once (`None`) when another connection holds the write lock.
    pub fn reclaim_if_idle(&self, pages: u32) -> Result<Option<u64>, JournalError> {
        let _no_wait = self.db.no_wait()?;
        busy_is_none(self.reclaim(pages))
    }
}

fn busy_is_none<T>(result: Result<T, JournalError>) -> Result<Option<T>, JournalError> {
    match result {
        Ok(value) => Ok(Some(value)),
        Err(error) if error.is_busy() => Ok(None),
        Err(error) => Err(error),
    }
}
