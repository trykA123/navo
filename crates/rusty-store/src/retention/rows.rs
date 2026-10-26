use rusqlite::{Connection, OptionalExtension, Row};
use rusty_core::retention::{
    MAX_RECORD_BYTES, RetainedState, RetentionEntry, RetentionError, RetentionId, RetentionSummary,
};

use super::RetentionStoreError;

const SELECT: &str = "SELECT e.id, e.job_id, e.state, e.record, e.target,
    e.job_id IS NOT NULL AND (j.state IS NULL OR j.state NOT IN ('done', 'failed', 'cancelled', 'rolled_back', 'undone'))
    FROM retained_entry e LEFT JOIN job j ON j.id = e.job_id";

struct Raw {
    id: i64,
    job_id: Option<i64>,
    state: String,
    record: Vec<u8>,
    target: Option<Vec<u8>>,
    pinned: bool,
}

pub(crate) fn get(
    conn: &Connection,
    id: RetentionId,
) -> Result<RetentionEntry, RetentionStoreError> {
    let raw = conn
        .query_row(&format!("{SELECT} WHERE e.id = ?1"), [id.0], read)
        .optional()?
        .ok_or(RetentionStoreError::Missing(id))?;
    decode(raw)
}

pub(super) fn list(
    conn: &Connection,
    after: i64,
    limit: u32,
) -> Result<Vec<RetentionEntry>, RetentionStoreError> {
    if after < 0 || !(1..=1000).contains(&limit) {
        return Err(RetentionError::Invalid("invalid catalog page bounds").into());
    }
    let mut statement =
        conn.prepare(&format!("{SELECT} WHERE e.id > ?1 ORDER BY e.id LIMIT ?2"))?;
    let rows = statement
        .query_map(rusqlite::params![after, limit], read)?
        .collect::<Result<Vec<_>, _>>()?;
    rows.into_iter().map(decode).collect()
}

fn read(row: &Row<'_>) -> Result<Raw, rusqlite::Error> {
    let record = blob(row, 3)?;
    let target = match row.get_ref(4)? {
        rusqlite::types::ValueRef::Null => None,
        _ => Some(blob(row, 4)?),
    };
    Ok(Raw {
        id: row.get(0)?,
        job_id: row.get(1)?,
        state: row.get(2)?,
        record,
        target,
        pinned: row.get(5)?,
    })
}

pub(super) fn summaries(
    conn: &Connection,
    after: i64,
    limit: u32,
) -> Result<Vec<RetentionSummary>, RetentionStoreError> {
    if after < 0 || !(1..=1000).contains(&limit) {
        return Err(RetentionError::Invalid("invalid catalog page bounds").into());
    }
    let mut statement = conn.prepare(&format!(
        "{SELECT} WHERE e.id > ?1 AND e.state NOT IN ('restored', 'purged', 'aborted') ORDER BY e.id LIMIT ?2"
    ))?;
    let mut rows = statement.query(rusqlite::params![after, limit])?;
    let mut inventory = Vec::new();
    while let Some(row) = rows.next()? {
        inventory.push(RetentionSummary::try_from(&decode(read(row)?)?)?);
    }
    Ok(inventory)
}

fn blob(row: &Row<'_>, index: usize) -> Result<Vec<u8>, rusqlite::Error> {
    let raw = row.get_ref(index)?;
    let bytes = raw.as_blob().map_err(|error| {
        rusqlite::Error::FromSqlConversionFailure(index, raw.data_type(), error.into())
    })?;
    if bytes.len() > MAX_RECORD_BYTES {
        return Err(rusqlite::Error::FromSqlConversionFailure(
            index,
            raw.data_type(),
            std::io::Error::other("retention record exceeds its size limit").into(),
        ));
    }
    Ok(bytes.to_vec())
}

fn decode(raw: Raw) -> Result<RetentionEntry, RetentionStoreError> {
    let entry = RetentionEntry {
        id: RetentionId(raw.id),
        job_id: raw.job_id,
        pinned: raw.pinned,
        state: RetainedState::parse(&raw.state)?,
        record: serde_json::from_slice(&raw.record)?,
        target: raw
            .target
            .map(|bytes| serde_json::from_slice(&bytes))
            .transpose()?,
    };
    entry.validate()?;
    Ok(entry)
}

pub(crate) fn validate(conn: &Connection) -> Result<(), RetentionStoreError> {
    let mut statement = conn
        .prepare("SELECT id, job_id, state, record, target, 1 FROM retained_entry ORDER BY id")?;
    let mut rows = statement.query([])?;
    while let Some(row) = rows.next()? {
        decode(read(row)?)?;
    }
    Ok(())
}
