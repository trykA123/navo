//! Multi-sheet table sources (read-only SQLite in E11a; extensible for spreadsheets in E11b).

pub mod sqlite;

#[cfg(test)]
mod tests;

use std::path::PathBuf;

use thiserror::Error;

pub use sqlite::{BLOB_PREVIEW_BYTES, SqliteTableSource, quote_identifier};

/// Kind of sheet exposed by a structured table source.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SheetKind {
    Table,
    View,
}

/// Metadata for a single selectable sheet (table or view).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SheetInfo {
    pub name: String,
    pub kind: SheetKind,
}

/// One paged window of rows from a table source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct TablePage {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub total_rows: u64,
    pub offset: u64,
    pub is_cut: bool,
}

/// Result of a read-only SQL query against a database table source.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct QueryResult {
    pub columns: Vec<String>,
    pub rows: Vec<Vec<String>>,
    pub truncated: bool,
}

/// Typed errors returned by table sources.
#[derive(Debug, Error)]
pub enum SourceError {
    #[error("failed to read {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("not a valid SQLite database: {path}")]
    NotSqlite { path: PathBuf },
    #[error("corrupt SQLite database: {message}")]
    Corrupt { message: String },
    #[error("sheet not found: {name}")]
    SheetNotFound { name: String },
    #[error("read-only connection rejected write statement: {message}")]
    ReadOnly { message: String },
    #[error("SQLite error: {0}")]
    Sqlite(#[from] rusqlite::Error),
}
