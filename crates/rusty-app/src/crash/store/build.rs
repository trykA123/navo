use std::fs::OpenOptions;
use std::path::Path;

use rusty_store::{
    Db, QuarantineReport, SalvageOutcome, SalvageReport, quarantine_database, salvage_database,
};

use super::state::{Notice, Progress, State, candidate};
use super::{Error, details, files, promote};

pub(super) fn preserved(data: &Path, slot: u64) -> Result<QuarantineReport, Error> {
    let path = rusty_store::StoreDirs::journal_in(data);
    if !files::regular(&files::suffix(&path, ".quarantine-receipt"))? {
        return Err(Error::Invalid("the quarantine receipt is missing".into()));
    }
    let report = quarantine_database(&path)?
        .ok_or_else(|| Error::Invalid("quarantine files are missing".into()))?;
    if report.slot != slot {
        return Err(Error::Invalid("the quarantine slot changed".into()));
    }
    Ok(report)
}

pub(super) fn rebuild(
    data: &Path,
    state: &mut State,
    (case, slot, database): (u64, u64, bool),
) -> Result<(), Error> {
    let source = preserved(data, slot)?;
    if source.database.is_some() != database {
        return Err(Error::Invalid("the quarantine file set changed".into()));
    }
    let attempt = reserve(data, case)?;
    let path = candidate(data, case, attempt);
    let (fresh, _) = Db::open(&path, false)?;
    let mode: String = fresh
        .connection()
        .pragma_update_and_check(None, "journal_mode", "DELETE", |row| row.get(0))
        .map_err(rusty_store::DbError::from)?;
    if mode != "delete" {
        return Err(Error::Invalid(
            "the recovery candidate is still using a WAL".into(),
        ));
    }
    fresh
        .connection()
        .pragma_update(None, "synchronous", "EXTRA")
        .map_err(rusty_store::DbError::from)?;
    let report = match &source.database {
        Some(path) => salvage_database(path, &fresh)?,
        None => SalvageReport::default(),
    };
    details::create(data, (case, attempt), &source, &report)?;
    let notice = summary(&report, (case, attempt, slot, database));
    tracing::error!(slot, copied_rows = notice.copied_rows, reset = ?notice.reset, journal_unverified = notice.journal_unverified, "database files preserved and readable data salvaged");
    drop(fresh);
    OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)?
        .sync_all()?;
    for suffix in ["-wal", "-shm", "-journal"] {
        if files::regular(&files::suffix(&path, suffix))? {
            return Err(Error::Invalid(
                "the recovery candidate has live sidecars".into(),
            ));
        }
    }
    let identity = promote::identity(&path)?;
    state.pending = Some(Progress::Promote { notice, identity });
    state.save(data)
}

fn reserve(data: &Path, case: u64) -> Result<u64, Error> {
    for attempt in 1..=u64::MAX {
        let path = candidate(data, case, attempt);
        let siblings = [
            path.clone(),
            files::suffix(&path, "-wal"),
            files::suffix(&path, "-shm"),
            files::suffix(&path, "-journal"),
            super::state::details(data, case, attempt),
        ];
        if siblings.iter().try_fold(false, |found, path| {
            files::regular(path).map(|next| found || next)
        })? {
            continue;
        }
        match OpenOptions::new().write(true).create_new(true).open(path) {
            Ok(file) => {
                file.sync_all()?;
                return Ok(attempt);
            }
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => continue,
            Err(error) => return Err(error.into()),
        }
    }
    Err(Error::Invalid("recovery candidate limit reached".into()))
}

fn summary(
    report: &SalvageReport,
    (case, attempt, slot, database): (u64, u64, u64, bool),
) -> Notice {
    let mut copied_rows = 0_u64;
    let mut reset = Vec::new();
    for table in &report.tables {
        match &table.outcome {
            SalvageOutcome::Copied { rows } => copied_rows = copied_rows.saturating_add(*rows),
            SalvageOutcome::Reset { .. } => reset.push(table.name.into()),
        }
    }
    Notice {
        case,
        slot,
        attempt,
        database,
        copied_rows,
        reset,
        unreadable_jobs: report.jobs.len(),
        journal_unverified: !database
            || report.tables.iter().any(|table| {
                matches!(table.name, "job" | "job_step" | "undo")
                    && matches!(table.outcome, SalvageOutcome::Reset { .. })
            }),
        recovered_jobs: 0,
        pending_paths: details::manual_paths(report).into_iter().take(4).collect(),
        retry_jobs: Vec::new(),
    }
}
