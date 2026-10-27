use std::{io, path::Path};

use rusqlite::{Connection, OpenFlags, config::DbConfig};

use super::{RecoveryError, SalvageOutcome, SalvageReport, SalvagedTable, io_error};
use crate::Db;

mod copy;
mod journal;
mod retention;
mod schema;

use copy::{CopyError, read_error};

const JOURNAL: &[&str] = &["job", "job_step", "undo"];
const RETAINED: &[&str] = &["retained_entry"];
const GROUPS: &[&[&str]] = &[
    JOURNAL,
    &["tags"],
    &["module_kv"],
    &["settings_defaults"],
    &["history"],
    &["frecency"],
    &["search_history"],
    &["viewer_memory"],
    &["command_use"],
    &["rename_preset"],
    &["collections"],
    &["collection_items"],
    RETAINED,
];

pub fn salvage_database(path: &Path, fresh: &Db) -> Result<SalvageReport, RecoveryError> {
    ensure_empty(fresh.connection())?;
    if !super::quarantine::regular(path)? {
        return Err(io_error(
            "finding salvage source",
            path,
            io::ErrorKind::NotFound.into(),
        ));
    }
    let source = match open_source(path) {
        Ok(source) => source,
        Err(error) => return rejected_source(path, error),
    };
    let version: u32 = match source.pragma_query_value(None, "user_version", |row| row.get(0)) {
        Ok(version) => version,
        Err(error) => return rejected_source(path, error),
    };
    if version > Db::VERSION {
        return Err(RecoveryError::UnsupportedVersion {
            found: version,
            supported: Db::VERSION,
        });
    }
    let mut report = SalvageReport::default();
    for &tables in GROUPS {
        if tables == RETAINED
            && retention::is_legacy_empty(&source, version)
                .map_err(|error| source_error(path, error))?
        {
            report.tables.push(SalvagedTable {
                name: "retained_entry",
                outcome: SalvageOutcome::Copied { rows: 0 },
            });
            continue;
        }
        match recover_group(&source, fresh.connection(), tables) {
            Ok(copied) => report.tables.extend(copied),
            Err(CopyError::Untrusted(reason)) => {
                reset(&mut report, tables, &reason);
                if tables == JOURNAL {
                    journal::summarize(&source, &mut report);
                }
                if tables == RETAINED {
                    retention::summarize(&source, &mut report);
                }
            }
            Err(CopyError::Destination(error)) => return Err(RecoveryError::Destination(error)),
            Err(CopyError::Read(error)) => return Err(source_error(path, error)),
        }
    }
    Ok(report)
}

fn ensure_empty(target: &Connection) -> Result<(), RecoveryError> {
    target.pragma_update(None, "foreign_keys", true)?;
    for &table in GROUPS.iter().flat_map(|tables| tables.iter()) {
        let has_rows: bool = target.query_row(
            &format!("SELECT EXISTS(SELECT 1 FROM {table})"),
            [],
            |row| row.get(0),
        )?;
        if has_rows {
            return Err(RecoveryError::DestinationNotEmpty(table));
        }
    }
    Ok(())
}

fn open_source(path: &Path) -> Result<Connection, rusqlite::Error> {
    let source = Connection::open_with_flags(path, OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    source.set_db_config(DbConfig::SQLITE_DBCONFIG_NO_CKPT_ON_CLOSE, true)?;
    source.execute_batch("PRAGMA trusted_schema=OFF; BEGIN;")?;
    Ok(source)
}

fn rejected_source(path: &Path, error: rusqlite::Error) -> Result<SalvageReport, RecoveryError> {
    match read_error(error) {
        CopyError::Untrusted(reason) => {
            let mut report = SalvageReport::default();
            for &tables in GROUPS {
                reset(&mut report, tables, &reason);
            }
            report.issues.push(reason);
            Ok(report)
        }
        CopyError::Read(error) | CopyError::Destination(error) => Err(source_error(path, error)),
    }
}

fn source_error(path: &Path, source: rusqlite::Error) -> RecoveryError {
    RecoveryError::Source {
        path: path.to_owned(),
        source,
    }
}

fn recover_group(
    source: &Connection,
    target: &Connection,
    tables: &'static [&'static str],
) -> Result<Vec<SalvagedTable>, CopyError> {
    for table in tables {
        check_table(source, table)?;
    }
    if tables == JOURNAL {
        journal::validate(source)?;
    }
    if tables == RETAINED {
        retention::validate(source)?;
    }
    let transaction = target
        .unchecked_transaction()
        .map_err(CopyError::Destination)?;
    let copied = tables
        .iter()
        .map(|&table| {
            let rows = copy::table(source, &transaction, table)?;
            Ok(SalvagedTable {
                name: table,
                outcome: SalvageOutcome::Copied { rows },
            })
        })
        .collect::<Result<Vec<_>, CopyError>>()?;
    transaction.commit().map_err(copy::write_error)?;
    Ok(copied)
}

fn check_table(source: &Connection, table: &str) -> Result<(), CopyError> {
    let mut statement = source
        .prepare(&format!("PRAGMA integrity_check({table})"))
        .map_err(read_error)?;
    let messages = statement
        .query_map([], |row| row.get::<_, String>(0))
        .map_err(read_error)?
        .collect::<Result<Vec<_>, _>>()
        .map_err(read_error)?;
    if messages.as_slice() == ["ok"] {
        return Ok(());
    }
    Err(CopyError::Untrusted(messages.join("\n")))
}

fn reset(report: &mut SalvageReport, tables: &'static [&'static str], reason: &str) {
    report
        .tables
        .extend(tables.iter().map(|&name| SalvagedTable {
            name,
            outcome: SalvageOutcome::Reset {
                reason: reason.to_owned(),
            },
        }));
}

#[cfg(test)]
mod tests;
