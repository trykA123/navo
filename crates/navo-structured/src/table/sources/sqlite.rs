//! Read-only SQLite table source opened strictly with `SQLITE_OPEN_READ_ONLY`.

use std::fmt::Write as _;
use std::fs::File;
use std::io::{ErrorKind, Read};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU32, Ordering};

use rusqlite::hooks::{AuthAction, AuthContext, Authorization};
use rusqlite::limits::Limit;
use rusqlite::types::ValueRef;
use rusqlite::{Connection, ErrorCode, OpenFlags, Rows};

use super::{QueryResult, SheetInfo, SheetKind, SourceError, TablePage};
use crate::table::{MAX_COLUMNS, ROWS_BYTES};

const SQLITE_MAGIC: &[u8; 16] = b"SQLite format 3\0";

/// Maximum number of raw bytes rendered as hex in a BLOB cell preview.
pub const BLOB_PREVIEW_BYTES: usize = 32;

/// Maximum UTF-8 byte length kept verbatim for a single TEXT cell.
const MAX_TEXT_CELL_BYTES: usize = 64 * 1024;

/// SQLite VM instruction interval between progress handler ticks.
const PROGRESS_STEP_INTERVAL: i32 = 10_000;

/// Maximum progress ticks per operation (5,000,000 VM steps).
const MAX_PROGRESS_TICKS: u32 = 500;

/// Quotes a SQLite identifier with double quotes, doubling embedded `"` characters.
#[must_use]
pub fn quote_identifier(name: &str) -> String {
    let mut out = String::with_capacity(name.len() + 2);
    out.push('"');
    for ch in name.chars() {
        if ch == '"' {
            out.push('"');
        }
        out.push(ch);
    }
    out.push('"');
    out
}

/// Read-only SQLite database exposing user tables and views as selectable sheets.
pub struct SqliteTableSource {
    conn: Connection,
    sheets: Vec<SheetInfo>,
    progress_ticks: Arc<AtomicU32>,
}

impl SqliteTableSource {
    /// Opens an existing SQLite database strictly read-only without mutating its bytes.
    ///
    /// Falls back to `immutable=1` URI mode when a WAL database lives in an unwritable directory;
    /// that fallback avoids `-shm` creation and reads checkpointed data only.
    pub fn open(path: impl AsRef<Path>) -> Result<Self, SourceError> {
        let path = path.as_ref();
        verify_sqlite_header(path)?;
        match open_and_initialize(path) {
            Ok(source) => Ok(source),
            Err(err) if should_retry_immutable(&err) => {
                if let Some(uri) = sqlite_immutable_uri(path) {
                    open_immutable_uri(&uri).map_err(classify_sqlite_error)
                } else {
                    Err(classify_sqlite_error(err))
                }
            }
            Err(err) => Err(classify_sqlite_error(err)),
        }
    }

    /// Returns the sorted list of user tables and views in the database.
    #[must_use]
    pub fn sheets(&self) -> &[SheetInfo] {
        &self.sheets
    }

    /// Returns the column names of `sheet`.
    pub fn sheet_columns(&self, sheet: &str) -> Result<Vec<String>, SourceError> {
        self.ensure_sheet(sheet)?;
        self.reset_progress_budget();
        let sql = format!("SELECT * FROM {} LIMIT 0", quote_identifier(sheet));
        let stmt = self.conn.prepare(&sql).map_err(classify_sqlite_error)?;
        Ok(statement_columns(&stmt))
    }

    /// Returns the total row count of `sheet`.
    pub fn sheet_row_count(&self, sheet: &str) -> Result<u64, SourceError> {
        self.ensure_sheet(sheet)?;
        self.reset_progress_budget();
        let sql = format!("SELECT COUNT(*) FROM {}", quote_identifier(sheet));
        let count: i64 = self
            .conn
            .query_row(&sql, [], |row| row.get(0))
            .map_err(classify_sqlite_error)?;
        Ok(u64::try_from(count).unwrap_or(0))
    }

    /// Reads a page of rows starting at `offset` with at most `limit` rows.
    pub fn read_page(
        &self,
        sheet: &str,
        offset: u64,
        limit: usize,
    ) -> Result<TablePage, SourceError> {
        self.ensure_sheet(sheet)?;
        let total_rows = self.sheet_row_count(sheet)?;
        self.reset_progress_budget();
        let sql = format!(
            "SELECT * FROM {} LIMIT ?1 OFFSET ?2",
            quote_identifier(sheet)
        );
        let mut stmt = self.conn.prepare(&sql).map_err(classify_sqlite_error)?;
        let columns = statement_columns(&stmt);
        let limit_i64 = i64::try_from(limit).unwrap_or(i64::MAX);
        let offset_i64 = i64::try_from(offset).unwrap_or(i64::MAX);
        let mut rows = stmt
            .query([limit_i64, offset_i64])
            .map_err(classify_sqlite_error)?;
        let (collected, is_cut) = collect_rows(&mut rows, columns.len(), limit)?;
        Ok(TablePage {
            columns,
            rows: collected,
            total_rows,
            offset,
            is_cut,
        })
    }

    /// Executes a single read-only SQL query and returns at most `max_rows` rows.
    pub fn query_read_only(&self, sql: &str, max_rows: usize) -> Result<QueryResult, SourceError> {
        self.reset_progress_budget();
        let mut stmt = self.conn.prepare(sql).map_err(classify_sqlite_error)?;
        if !stmt.readonly() {
            return Err(SourceError::ReadOnly {
                message: "statement is not read-only".to_owned(),
            });
        }
        let columns = statement_columns(&stmt);
        let mut rows = stmt.query([]).map_err(classify_sqlite_error)?;
        let (collected, budget_cut) = collect_rows(&mut rows, columns.len(), max_rows)?;
        let has_more = if budget_cut {
            true
        } else {
            rows.next().map_err(classify_sqlite_error)?.is_some()
        };
        Ok(QueryResult {
            columns,
            rows: collected,
            truncated: has_more,
        })
    }

    fn reset_progress_budget(&self) {
        self.progress_ticks.store(0, Ordering::Relaxed);
    }

    fn ensure_sheet(&self, sheet: &str) -> Result<(), SourceError> {
        if self.sheets.iter().any(|info| info.name == sheet) {
            Ok(())
        } else {
            Err(SourceError::SheetNotFound {
                name: sheet.to_owned(),
            })
        }
    }

    #[cfg(test)]
    pub(super) fn is_query_only_enabled(&mut self) -> bool {
        let _ = self
            .conn
            .authorizer(None::<fn(AuthContext<'_>) -> Authorization>);
        let enabled = self
            .conn
            .query_row("PRAGMA query_only", [], |row| row.get::<_, i64>(0))
            .map(|value| value != 0)
            .unwrap_or(false);
        let _ = install_read_only_authorizer(&mut self.conn);
        enabled
    }
}

fn open_and_initialize(path: &Path) -> Result<SqliteTableSource, rusqlite::Error> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY | OpenFlags::SQLITE_OPEN_NO_MUTEX;
    let conn = Connection::open_with_flags(path, flags)?;
    configure_and_load(conn)
}

fn open_immutable_uri(uri: &str) -> Result<SqliteTableSource, rusqlite::Error> {
    let flags = OpenFlags::SQLITE_OPEN_READ_ONLY
        | OpenFlags::SQLITE_OPEN_NO_MUTEX
        | OpenFlags::SQLITE_OPEN_URI;
    let conn = Connection::open_with_flags(uri, flags)?;
    configure_and_load(conn)
}

/// Applies read-only limits, `PRAGMA query_only`, progress budget, authorizer, and loads sheets.
fn configure_and_load(mut conn: Connection) -> Result<SqliteTableSource, rusqlite::Error> {
    conn.set_limit(Limit::SQLITE_LIMIT_ATTACHED, 0)?;
    conn.execute_batch("PRAGMA query_only = ON;")?;
    let progress_ticks = install_progress_handler(&mut conn)?;
    install_read_only_authorizer(&mut conn)?;
    progress_ticks.store(0, Ordering::Relaxed);
    let sheets = load_sheets(&conn)?;
    Ok(SqliteTableSource {
        conn,
        sheets,
        progress_ticks,
    })
}

fn install_progress_handler(conn: &mut Connection) -> Result<Arc<AtomicU32>, rusqlite::Error> {
    let ticks = Arc::new(AtomicU32::new(0));
    let counter = Arc::clone(&ticks);
    conn.progress_handler(
        PROGRESS_STEP_INTERVAL,
        Some(move || counter.fetch_add(1, Ordering::Relaxed) >= MAX_PROGRESS_TICKS),
    )?;
    Ok(ticks)
}

/// Restricts SQLite statement compilation to strictly read-only query actions.
fn install_read_only_authorizer(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    conn.authorizer(Some(|ctx: AuthContext<'_>| match ctx.action {
        AuthAction::Select
        | AuthAction::Read { .. }
        | AuthAction::Function { .. }
        | AuthAction::Recursive => Authorization::Allow,
        _ => Authorization::Deny,
    }))
}

fn should_retry_immutable(err: &rusqlite::Error) -> bool {
    matches!(
        err,
        rusqlite::Error::SqliteFailure(ffi, _)
            if matches!(
                ffi.code,
                ErrorCode::ReadOnly | ErrorCode::CannotOpen | ErrorCode::SystemIoFailure
            )
    )
}

/// Builds a percent-encoded `file://` URI with `mode=ro&immutable=1` for read-only WAL fallback.
///
/// `immutable=1` avoids creating `-shm` sidecar files in unwritable directories and sees
/// checkpointed data only.
pub(super) fn sqlite_immutable_uri(path: &Path) -> Option<String> {
    let raw = path.to_str()?;
    let normalized = raw.replace('\\', "/");
    let (prefix, body) = if let Some(rest) = strip_verbatim_unc(&normalized) {
        ("file://", rest)
    } else if let Some(rest) = normalized.strip_prefix("//?/") {
        if rest.as_bytes().get(1) == Some(&b':') {
            ("file:///", rest)
        } else {
            ("file://", rest)
        }
    } else if let Some(rest) = normalized.strip_prefix("//") {
        ("file://", rest)
    } else if normalized.starts_with('/') {
        ("file://", normalized.as_str())
    } else if normalized.as_bytes().get(1) == Some(&b':') {
        ("file:///", normalized.as_str())
    } else {
        ("file:", normalized.as_str())
    };
    let mut uri = String::with_capacity(prefix.len() + body.len() + 32);
    uri.push_str(prefix);
    for byte in body.bytes() {
        if byte.is_ascii_alphanumeric() || matches!(byte, b'/' | b'-' | b'_' | b'.' | b'~' | b':') {
            uri.push(char::from(byte));
        } else {
            let _ = write!(uri, "%{byte:02X}");
        }
    }
    uri.push_str("?mode=ro&immutable=1");
    Some(uri)
}

/// Strips a Windows verbatim UNC prefix (`//?/UNC/` case-insensitively).
fn strip_verbatim_unc(path: &str) -> Option<&str> {
    let rest = path.strip_prefix("//?/")?;
    let head = rest.get(..4)?;
    head.eq_ignore_ascii_case("unc/").then(|| &rest[4..])
}

/// Collects column names from `stmt`, capped at `MAX_COLUMNS`.
fn statement_columns(stmt: &rusqlite::Statement<'_>) -> Vec<String> {
    stmt.column_names()
        .take(MAX_COLUMNS)
        .map(str::to_owned)
        .collect()
}

/// Checks that `path` exists, is readable, and begins with the 16-byte SQLite 3 magic header.
fn verify_sqlite_header(path: &Path) -> Result<(), SourceError> {
    let mut file = File::open(path).map_err(|source| SourceError::Io {
        path: path.to_path_buf(),
        source,
    })?;
    let mut header = [0_u8; 16];
    match file.read_exact(&mut header) {
        Ok(()) => {}
        Err(err) if err.kind() == ErrorKind::UnexpectedEof => {
            return Err(SourceError::NotSqlite {
                path: path.to_path_buf(),
            });
        }
        Err(source) => {
            return Err(SourceError::Io {
                path: path.to_path_buf(),
                source,
            });
        }
    }
    if &header != SQLITE_MAGIC {
        return Err(SourceError::NotSqlite {
            path: path.to_path_buf(),
        });
    }
    Ok(())
}

/// Loads sorted user tables and views from `sqlite_master` (excluding internal `sqlite_*` objects).
fn load_sheets(conn: &Connection) -> Result<Vec<SheetInfo>, rusqlite::Error> {
    let mut stmt = conn.prepare(
        "SELECT name, type FROM sqlite_master \
         WHERE type IN ('table', 'view') AND name NOT LIKE 'sqlite_%' \
         ORDER BY name",
    )?;
    let mut rows = stmt.query([])?;
    let mut sheets = Vec::new();
    while let Some(row) = rows.next()? {
        let name: String = row.get(0)?;
        let kind_str: String = row.get(1)?;
        let kind = if kind_str == "view" {
            SheetKind::View
        } else {
            SheetKind::Table
        };
        sheets.push(SheetInfo { name, kind });
    }
    Ok(sheets)
}

/// Streams up to `max_rows` formatted rows from `rows`, stopping early if `ROWS_BYTES` is reached.
fn collect_rows(
    rows: &mut Rows<'_>,
    column_count: usize,
    max_rows: usize,
) -> Result<(Vec<Vec<String>>, bool), SourceError> {
    let mut out = Vec::new();
    let mut used_bytes = 0_usize;
    let mut is_cut = false;
    while out.len() < max_rows {
        let Some(row) = rows.next().map_err(classify_sqlite_error)? else {
            break;
        };
        let mut cells = Vec::with_capacity(column_count);
        for index in 0..column_count {
            let value = row.get_ref(index).map_err(classify_sqlite_error)?;
            let formatted = format_value(value);
            used_bytes = used_bytes.saturating_add(formatted.len());
            cells.push(formatted);
        }
        out.push(cells);
        if used_bytes >= ROWS_BYTES {
            is_cut = true;
            break;
        }
    }
    Ok((out, is_cut))
}

/// Formats one SQLite cell value into a bounded display string.
fn format_value(value: ValueRef<'_>) -> String {
    match value {
        ValueRef::Null => "NULL".to_owned(),
        ValueRef::Integer(number) => number.to_string(),
        ValueRef::Real(number) => number.to_string(),
        ValueRef::Text(bytes) => format_text(bytes),
        ValueRef::Blob(bytes) => format_blob(bytes),
    }
}

fn format_text(bytes: &[u8]) -> String {
    if bytes.len() <= MAX_TEXT_CELL_BYTES {
        return String::from_utf8_lossy(bytes).into_owned();
    }
    let preview = bytes.get(..MAX_TEXT_CELL_BYTES).unwrap_or_default();
    let text = String::from_utf8_lossy(preview);
    format!("{text}… ({} bytes)", bytes.len())
}

fn format_blob(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return "0x".to_owned();
    }
    let preview = bytes
        .get(..bytes.len().min(BLOB_PREVIEW_BYTES))
        .unwrap_or_default();
    let mut out = String::with_capacity(2 + preview.len() * 2 + 24);
    out.push_str("0x");
    for byte in preview {
        let _ = write!(out, "{byte:02x}");
    }
    if bytes.len() > BLOB_PREVIEW_BYTES {
        let _ = write!(out, "… ({} bytes)", bytes.len());
    }
    out
}

/// Maps `rusqlite::Error` into typed `SourceError` variants for corruption, limits, and denials.
fn classify_sqlite_error(err: rusqlite::Error) -> SourceError {
    if let rusqlite::Error::SqliteFailure(ref ffi, ref detail) = err {
        let message = detail.clone().unwrap_or_else(|| ffi.to_string());
        match ffi.code {
            ErrorCode::ReadOnly | ErrorCode::AuthorizationForStatementDenied => {
                return SourceError::ReadOnly { message };
            }
            ErrorCode::OperationInterrupted => {
                return SourceError::QueryLimitExceeded { message };
            }
            ErrorCode::NotADatabase | ErrorCode::DatabaseCorrupt => {
                return SourceError::Corrupt { message };
            }
            _ if message.contains("too many attached databases") => {
                return SourceError::ReadOnly { message };
            }
            _ => {}
        }
    }
    SourceError::Sqlite(err)
}
