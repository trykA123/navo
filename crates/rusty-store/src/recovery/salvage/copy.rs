use rusqlite::{Connection, ErrorCode};

use super::schema;

pub(super) enum CopyError {
    Untrusted(String),
    Destination(rusqlite::Error),
    Read(rusqlite::Error),
}

pub(super) fn read_error(error: rusqlite::Error) -> CopyError {
    match error.sqlite_error_code() {
        None
        | Some(
            ErrorCode::DatabaseCorrupt
            | ErrorCode::NotADatabase
            | ErrorCode::Unknown
            | ErrorCode::TypeMismatch
            | ErrorCode::ConstraintViolation,
        ) => CopyError::Untrusted(error.to_string()),
        _ => CopyError::Read(error),
    }
}

pub(super) fn write_error(error: rusqlite::Error) -> CopyError {
    if error.sqlite_error_code() == Some(ErrorCode::ConstraintViolation) {
        CopyError::Untrusted(error.to_string())
    } else {
        CopyError::Destination(error)
    }
}

pub(super) fn table(
    source: &Connection,
    target: &Connection,
    table: &str,
) -> Result<u64, CopyError> {
    let shared = schema::shared_columns(source, target, table)?;
    if shared.is_empty() {
        return Err(CopyError::Untrusted(format!(
            "no compatible columns in {table}"
        )));
    }
    let column_names = shared
        .iter()
        .map(|column| format!("\"{}\"", column.name.replace('"', "\"\"")))
        .collect::<Vec<_>>()
        .join(",");
    let parameters = vec!["?"; shared.len()].join(",");
    let mut select = source
        .prepare(&format!("SELECT {column_names} FROM {table}"))
        .map_err(read_error)?;
    let mut insert = target
        .prepare(&format!(
            "INSERT INTO {table} ({column_names}) VALUES ({parameters})"
        ))
        .map_err(CopyError::Destination)?;
    let mut rows = select.query([]).map_err(read_error)?;
    let mut count = 0;
    while let Some(row) = rows.next().map_err(read_error)? {
        let values = schema::read_row(row, &shared)?;
        insert
            .execute(rusqlite::params_from_iter(values))
            .map_err(write_error)?;
        count += 1;
    }
    Ok(count)
}
