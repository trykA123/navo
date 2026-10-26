use std::collections::BTreeMap;

use rusqlite::{Connection, OptionalExtension};
use rusty_core::OperationPlan;
use rusty_core::retention::{
    RETAINED_DELETE_KIND, RetainedDeletePlan, RetainedWritePlan, is_retained_write,
};

use super::SalvageReport;
use super::copy::{CopyError, read_error};
use crate::recovery::UnrecoveredJob;

pub(super) fn validate(source: &Connection) -> Result<(), CopyError> {
    let mut statement = source
        .prepare("SELECT state, plan, id, kind, created_at FROM job")
        .map_err(read_error)?;
    let mut rows = statement.query([]).map_err(read_error)?;
    while let Some(row) = rows.next().map_err(read_error)? {
        row.get::<_, i64>(2).map_err(read_error)?;
        let kind: String = row.get(3).map_err(read_error)?;
        row.get::<_, i64>(4).map_err(read_error)?;
        let state: String = row.get(0).map_err(read_error)?;
        if !matches!(
            state.as_str(),
            "running"
                | "done"
                | "failed"
                | "cancelled"
                | "rolled_back"
                | "undone"
                | "restore_pending"
        ) {
            return Err(CopyError::Untrusted(format!("unknown job state {state:?}")));
        }
        let bytes: Vec<u8> = row.get(1).map_err(read_error)?;
        validate_plan(&kind, &state, &bytes)?;
    }
    validate_steps(source)?;
    validate_held(source)
}

fn validate_plan(kind: &str, state: &str, bytes: &[u8]) -> Result<(), CopyError> {
    if kind == RETAINED_DELETE_KIND {
        serde_json::from_slice::<RetainedDeletePlan>(bytes).map_err(|error| {
            CopyError::Untrusted(format!("invalid retained delete plan: {error}"))
        })?;
    } else if is_retained_write(kind) {
        let plan: RetainedWritePlan = serde_json::from_slice(bytes).map_err(|error| {
            CopyError::Untrusted(format!("invalid overwrite retention plan: {error}"))
        })?;
        if plan.journal_kind() != kind {
            return Err(CopyError::Untrusted(
                "overwrite retention kind changed".to_owned(),
            ));
        }
    } else if matches!(state, "running" | "restore_pending") {
        let retained = serde_json::from_slice::<serde_json::Value>(bytes)
            .ok()
            .is_some_and(|value| {
                value.get("retention_version").is_some()
                    || value.get("retention").is_some()
                    || value.get("overwrite_retention_version").is_some()
                    || value.get("temporary_namespace").is_some()
            });
        if retained {
            return Err(CopyError::Untrusted(
                "retention plan has an unexpected job kind".to_owned(),
            ));
        }
        serde_json::from_slice::<OperationPlan>(bytes)
            .map_err(|error| CopyError::Untrusted(format!("invalid recovery plan: {error}")))?;
    }
    Ok(())
}

fn validate_steps(source: &Connection) -> Result<(), CopyError> {
    let mut statement = source
        .prepare("SELECT seq, state, detail FROM job_step")
        .map_err(read_error)?;
    let mut rows = statement.query([]).map_err(read_error)?;
    while let Some(row) = rows.next().map_err(read_error)? {
        row.get::<_, u32>(0).map_err(read_error)?;
        row.get::<_, Option<Vec<u8>>>(2).map_err(read_error)?;
        let state: String = row.get(1).map_err(read_error)?;
        if !matches!(state.as_str(), "pending" | "done" | "failed" | "skipped") {
            return Err(CopyError::Untrusted(format!(
                "unknown step state {state:?}"
            )));
        }
    }
    Ok(())
}

fn validate_held(source: &Connection) -> Result<(), CopyError> {
    let mut statement = source
        .prepare("SELECT job_id, created_at, inverse FROM undo")
        .map_err(read_error)?;
    let mut rows = statement.query([]).map_err(read_error)?;
    while let Some(row) = rows.next().map_err(read_error)? {
        let owner: Option<i64> = row.get(0).map_err(read_error)?;
        row.get::<_, i64>(1).map_err(read_error)?;
        let bytes: Vec<u8> = row.get(2).map_err(read_error)?;
        if serde_json::from_slice::<serde_json::Value>(&bytes)
            .ok()
            .is_some_and(|value| value.get("proof").is_some())
        {
            let kind: Option<String> = source
                .query_row("SELECT kind FROM job WHERE id = ?1", [owner], |row| {
                    row.get(0)
                })
                .optional()
                .map_err(read_error)?;
            if !kind.as_deref().is_some_and(is_retained_write) {
                return Err(CopyError::Untrusted(
                    "overwrite proof has no retained owner".to_owned(),
                ));
            }
        }
    }
    Ok(())
}

pub(super) fn summarize(source: &Connection, report: &mut SalvageReport) {
    let mut jobs = BTreeMap::new();
    if let Err(error) = summarize_jobs(source, &mut jobs) {
        report
            .issues
            .push(format!("reading interrupted jobs: {error}"));
    }
    if let Err(error) = summarize_held(source, &mut jobs) {
        report
            .issues
            .push(format!("reading held-file records: {error}"));
    }
    report.jobs = jobs.into_values().collect();
}

fn summarize_jobs(
    source: &Connection,
    jobs: &mut BTreeMap<Option<i64>, UnrecoveredJob>,
) -> Result<(), rusqlite::Error> {
    let mut statement = source.prepare(
        "SELECT id, kind, created_at, plan FROM job
         WHERE state NOT IN ('done', 'failed', 'cancelled', 'rolled_back', 'undone')
         OR state IS NULL ORDER BY id",
    )?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id = Some(row.get(0)?);
        jobs.insert(
            id,
            UnrecoveredJob {
                id,
                kind: row.get(1).ok(),
                created_at: row.get(2).ok(),
                plan: row.get(3).ok(),
                held: Vec::new(),
            },
        );
    }
    Ok(())
}

fn summarize_held(
    source: &Connection,
    jobs: &mut BTreeMap<Option<i64>, UnrecoveredJob>,
) -> Result<(), rusqlite::Error> {
    let mut statement = source.prepare("SELECT job_id, inverse FROM undo ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let id = row.get(0)?;
        let held = row.get(1)?;
        jobs.entry(id)
            .or_insert_with(|| UnrecoveredJob {
                id,
                kind: None,
                created_at: None,
                plan: None,
                held: Vec::new(),
            })
            .held
            .push(held);
    }
    Ok(())
}

#[cfg(test)]
mod tests;
