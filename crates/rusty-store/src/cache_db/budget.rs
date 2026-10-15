use rusqlite::Connection;

pub(super) fn used_bytes(conn: &Connection, table: &str) -> Result<u64, rusqlite::Error> {
    conn.query_row(
        "SELECT COALESCE(SUM(size), 0) FROM blob WHERE tbl = ?1",
        [table],
        read_size,
    )
}

pub(super) fn enforce(
    conn: &mut Connection,
    table: &str,
    bytes: u64,
) -> Result<(), rusqlite::Error> {
    let transaction = conn.transaction()?;
    evict(&transaction, table, bytes)?;
    transaction.commit()
}

pub(super) fn evict(conn: &Connection, table: &str, bytes: u64) -> Result<(), rusqlite::Error> {
    let mut used = used_bytes(conn, table)?;
    while used > bytes {
        let removed: u64 = conn.query_row(
            "DELETE FROM blob WHERE tbl = ?1 AND key = (
                 SELECT key FROM blob WHERE tbl = ?1 ORDER BY accessed, key LIMIT 1
             ) RETURNING size",
            [table],
            read_size,
        )?;
        used = used.saturating_sub(removed);
    }
    Ok(())
}

fn read_size(row: &rusqlite::Row<'_>) -> Result<u64, rusqlite::Error> {
    let size: i64 = row.get(0)?;
    u64::try_from(size).map_err(|_| rusqlite::Error::IntegralValueOutOfRange(0, size))
}
