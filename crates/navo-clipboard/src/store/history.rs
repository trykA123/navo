//! Reading and writing history rows.

use std::path::PathBuf;

use rusqlite::{Row, Transaction, TransactionBehavior, params};

use crate::content_key;
use crate::model::{Content, Entry, Transfer};

use super::{ClipError, ClipStore};

const KIND_TEXT: i64 = 0;
const KIND_PATHS: i64 = 1;

const RECORD: &str =
    "INSERT INTO clips (key, kind, body, paths, cut, source_app, first_ms, last_ms, uses)
     VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7,
             MAX(?8, COALESCE((SELECT MAX(last_ms) FROM clips), ?8)), 1)
     ON CONFLICT(key) DO UPDATE SET
         last_ms = MAX(
             clips.last_ms,
             excluded.last_ms,
             COALESCE(
                 (SELECT MAX(other.last_ms) + 1 FROM clips AS other
                  WHERE other.id <> clips.id),
                 excluded.last_ms
             )
         ),
         uses = clips.uses + 1,
         source_app = excluded.source_app";

/// One history row with the id that addresses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Stored {
    pub id: i64,
    pub entry: Entry,
}

struct Encoded<'a> {
    kind: i64,
    body: Option<&'a str>,
    paths: Option<String>,
    cut: i64,
}

impl ClipStore {
    /// Records one entry and trims the history inside one transaction.
    /// The recorded row always ends newest, even when `last_ms` is older than the stored
    /// times (a clock stepped back). A single body over the text cap stays; older rows go.
    /// A repeat or a trim empties the log; [`ClipError::CheckpointBusy`] then means the
    /// entry is recorded but the log still holds the old rows.
    pub fn record(&mut self, entry: &Entry) -> Result<(), ClipError> {
        let key = content_key(&entry.content);
        let encoded = encode(&entry.content);
        let transaction = self
            .conn
            .transaction_with_behavior(TransactionBehavior::Immediate)?;
        let repeat: bool = transaction.query_row(
            "SELECT EXISTS (SELECT 1 FROM clips WHERE key = ?1)",
            [key.as_slice()],
            |row| row.get(0),
        )?;
        transaction.execute(
            RECORD,
            params![
                key.as_slice(),
                encoded.kind,
                encoded.body,
                encoded.paths,
                encoded.cut,
                entry.source_app.as_deref(),
                entry.first_ms,
                entry.last_ms
            ],
        )?;
        let trimmed = trim_entries(&transaction, self.limits.max_entries)?
            + trim_text_bytes(&transaction, self.limits.max_history_text_bytes)?;
        transaction.commit()?;
        if repeat || trimmed > 0 {
            self.checkpoint()?;
        }
        Ok(())
    }

    /// Lists rows newest first, with the ids used by [`ClipStore::delete`].
    pub fn history(&self, limit: usize) -> Result<Vec<Stored>, ClipError> {
        let limit = i64::try_from(limit).unwrap_or(i64::MAX);
        let mut statement = self.conn.prepare(
            "SELECT id, kind, body, paths, cut, source_app, first_ms, last_ms, uses
             FROM clips
             ORDER BY last_ms DESC, id DESC
             LIMIT ?1",
        )?;
        let mut rows = statement.query([limit])?;
        let mut stored = Vec::new();
        while let Some(row) = rows.next()? {
            stored.push(decode(row)?);
        }
        Ok(stored)
    }

    /// Reads one row by the id [`ClipStore::history`] gave; `None` when it is gone.
    pub fn get(&self, id: i64) -> Result<Option<Stored>, ClipError> {
        let mut statement = self.conn.prepare(
            "SELECT id, kind, body, paths, cut, source_app, first_ms, last_ms, uses
             FROM clips
             WHERE id = ?1",
        )?;
        let mut rows = statement.query([id])?;
        rows.next()?.map(decode).transpose()
    }

    /// Deletes one row and empties the log, so its text leaves the disk at once.
    /// On [`ClipError::CheckpointBusy`] the row is already deleted; the next checkpoint
    /// empties the log.
    pub fn delete(&self, id: i64) -> Result<(), ClipError> {
        self.conn.execute("DELETE FROM clips WHERE id = ?1", [id])?;
        self.checkpoint()
    }

    /// Deletes every history row and truncates the write-ahead log.
    pub fn clear_history(&self) -> Result<(), ClipError> {
        self.conn.execute("DELETE FROM clips", [])?;
        self.checkpoint()?;
        Ok(())
    }
}

fn encode(content: &Content) -> Encoded<'_> {
    match content {
        Content::Text(text) => Encoded {
            kind: KIND_TEXT,
            body: Some(text),
            paths: None,
            cut: 0,
        },
        Content::Paths { paths, transfer } => Encoded {
            kind: KIND_PATHS,
            body: None,
            paths: Some(
                paths
                    .iter()
                    .map(|path| path.to_string_lossy().into_owned())
                    .collect::<Vec<_>>()
                    .join("\0"),
            ),
            cut: if *transfer == Transfer::Cut { 1 } else { 0 },
        },
    }
}

fn decode(row: &Row<'_>) -> Result<Stored, ClipError> {
    let id = row.get(0)?;
    let kind: i64 = row.get(1)?;
    let body: Option<String> = row.get(2)?;
    let joined: Option<String> = row.get(3)?;
    let cut: i64 = row.get(4)?;
    let content = match kind {
        KIND_TEXT => Content::Text(body.ok_or(ClipError::Malformed("text row without a body"))?),
        KIND_PATHS => Content::Paths {
            paths: split_paths(joined.ok_or(ClipError::Malformed("path row without paths"))?),
            transfer: decode_transfer(cut)?,
        },
        _ => return Err(ClipError::Malformed("unknown clipboard kind")),
    };
    Ok(Stored {
        id,
        entry: Entry {
            content,
            source_app: row.get(5)?,
            first_ms: row.get(6)?,
            last_ms: row.get(7)?,
            uses: row.get(8)?,
        },
    })
}

fn split_paths(joined: String) -> Vec<PathBuf> {
    if joined.is_empty() {
        return Vec::new();
    }
    joined.split('\0').map(PathBuf::from).collect()
}

fn decode_transfer(cut: i64) -> Result<Transfer, ClipError> {
    match cut {
        0 => Ok(Transfer::Copy),
        1 => Ok(Transfer::Cut),
        _ => Err(ClipError::Malformed(
            "clipboard transfer flag is not 0 or 1",
        )),
    }
}

fn trim_entries(transaction: &Transaction<'_>, max_entries: usize) -> Result<usize, ClipError> {
    let keep = i64::try_from(max_entries).unwrap_or(i64::MAX);
    Ok(transaction.execute(
        "DELETE FROM clips
         WHERE id IN (
             SELECT id FROM clips ORDER BY last_ms DESC, id DESC LIMIT -1 OFFSET ?1
         )",
        [keep],
    )?)
}

fn trim_text_bytes(transaction: &Transaction<'_>, max_bytes: usize) -> Result<usize, ClipError> {
    let cap = i64::try_from(max_bytes).unwrap_or(i64::MAX);
    let total: i64 = transaction.query_row(
        "SELECT COALESCE(SUM(length(CAST(body AS BLOB))), 0) FROM clips",
        [],
        |row| row.get(0),
    )?;
    if total <= cap {
        return Ok(0);
    }
    Ok(transaction.execute(
        "DELETE FROM clips
         WHERE id <> (SELECT id FROM clips ORDER BY last_ms DESC, id DESC LIMIT 1)
           AND id IN (
             SELECT id FROM (
                 SELECT id,
                        COALESCE(SUM(COALESCE(length(CAST(body AS BLOB)), 0)) OVER (
                            ORDER BY last_ms, id
                            ROWS BETWEEN UNBOUNDED PRECEDING AND 1 PRECEDING
                        ), 0) AS older_bytes
                 FROM clips
             )
             WHERE older_bytes < ?1
         )",
        [total - cap],
    )?)
}
