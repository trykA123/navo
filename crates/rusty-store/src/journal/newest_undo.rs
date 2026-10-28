use rusqlite::OptionalExtension;
use rusqlite::types::Value;
use rusty_core::retention::RetentionId;

use super::{FinishedJob, JobId, JobJournal, JournalError};

const LIVE_ENTRY: &str = "e.state NOT IN ('restored', 'purged', 'aborted')
    AND CASE WHEN json_valid(e.record) THEN json_extract(e.record, '$.expires_at') END > ?2";

#[derive(Debug, Clone, Copy)]
pub struct UndoKinds<'a> {
    pub rename: &'a str,
    pub retained: &'a [&'a str],
    pub now_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Undoable {
    Rename(FinishedJob),
    Retained {
        id: JobId,
        created_at: i64,
        entries: Vec<RetentionId>,
    },
}

type Newest = (JobId, Option<Vec<u8>>, i64, bool);

impl JobJournal {
    pub fn newest_undoable(&self, kinds: UndoKinds<'_>) -> Result<Option<Undoable>, JournalError> {
        let Some((id, plan, created_at, restore_pending)) = self.newest_job(kinds)? else {
            return Ok(None);
        };
        Ok(Some(match plan {
            Some(plan) => Undoable::Rename(FinishedJob {
                id,
                plan,
                steps: self.steps(id)?,
                created_at,
                restore_pending,
            }),
            None => Undoable::Retained {
                id,
                created_at,
                entries: self.live_entries(id, kinds.now_ms)?,
            },
        }))
    }

    fn newest_job(&self, kinds: UndoKinds<'_>) -> Result<Option<Newest>, JournalError> {
        let marks = (3..kinds.retained.len() + 3)
            .map(|at| format!("?{at}"))
            .collect::<Vec<_>>()
            .join(", ");
        let sql = format!(
            "SELECT j.id, CASE WHEN j.kind = ?1 THEN j.plan END, j.created_at, j.state = 'restore_pending'
             FROM job j
             WHERE (j.kind = ?1 AND j.state IN ('done', 'restore_pending'))
                OR (j.state = 'done' AND j.kind IN ({marks}) AND EXISTS
                    (SELECT 1 FROM retained_entry e WHERE e.job_id = j.id AND {LIVE_ENTRY}))
             ORDER BY j.created_at DESC, j.id DESC LIMIT 1"
        );
        let mut values = vec![
            Value::from(kinds.rename.to_owned()),
            Value::from(kinds.now_ms),
        ];
        values.extend(
            kinds
                .retained
                .iter()
                .map(|kind| Value::from((*kind).to_owned())),
        );
        Ok(self
            .db
            .connection()
            .query_row(&sql, rusqlite::params_from_iter(values), |row| {
                Ok((JobId(row.get(0)?), row.get(1)?, row.get(2)?, row.get(3)?))
            })
            .optional()?)
    }

    fn live_entries(&self, job: JobId, now_ms: i64) -> Result<Vec<RetentionId>, JournalError> {
        let mut statement = self.db.connection().prepare(&format!(
            "SELECT e.id FROM retained_entry e WHERE e.job_id = ?1 AND {LIVE_ENTRY} ORDER BY e.id"
        ))?;
        let ids = statement
            .query_map(rusqlite::params![job.0, now_ms], |row| {
                row.get(0).map(RetentionId)
            })?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(ids)
    }
}
