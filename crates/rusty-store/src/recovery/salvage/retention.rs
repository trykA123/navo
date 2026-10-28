use rusqlite::Connection;
use rusty_core::retention::MAX_RECORD_BYTES;

use super::copy::{CopyError, read_error, write_error};
use crate::{RetentionStoreError, SalvageReport, UnrecoveredRetained};

pub(super) fn is_legacy_empty(source: &Connection, version: u32) -> Result<bool, rusqlite::Error> {
    if version >= 6 {
        return Ok(false);
    }
    let present: bool = source.query_row(
        "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE type='table' AND name='retained_entry')",
        [],
        |row| row.get(0),
    )?;
    Ok(!present)
}

pub(super) fn validate(source: &Connection) -> Result<(), CopyError> {
    crate::retention::validate(source).map_err(|error| match error {
        RetentionStoreError::Sqlite(error) => read_error(error),
        error => CopyError::Untrusted(error.to_string()),
    })
}

pub(super) fn stamp_terminal(target: &Connection) -> Result<(), CopyError> {
    target
        .execute(
            "UPDATE retained_entry SET terminal_ms = CAST(unixepoch('subsec') * 1000 AS INTEGER)
             WHERE terminal_ms IS NULL AND state IN ('restored', 'purged', 'aborted')",
            [],
        )
        .map_err(write_error)?;
    Ok(())
}

pub(super) fn summarize(source: &Connection, report: &mut SalvageReport) {
    if let Err(error) = read(source, report) {
        report
            .issues
            .push(format!("reading retained-file records: {error}"));
    }
}

fn read(source: &Connection, report: &mut SalvageReport) -> Result<(), rusqlite::Error> {
    let mut statement =
        source.prepare("SELECT id, record, target FROM retained_entry ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        let record = row
            .get_ref(1)?
            .as_blob()
            .ok()
            .filter(|bytes| bytes.len() <= MAX_RECORD_BYTES)
            .map(<[u8]>::to_vec);
        let target = row
            .get_ref(2)?
            .as_blob()
            .ok()
            .filter(|bytes| bytes.len() <= MAX_RECORD_BYTES)
            .map(<[u8]>::to_vec);
        report.retained.push(UnrecoveredRetained {
            id: row.get(0).ok(),
            record,
            target,
        });
    }
    Ok(())
}
