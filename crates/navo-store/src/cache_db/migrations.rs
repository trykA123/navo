use rusqlite::{Connection, TransactionBehavior};

pub(super) const VERSION: u32 = 1;

const MIGRATIONS: [&str; 1] = [V1];

// V1 is the first versioned cache schema; older files are at user_version 0.
const V1: &str = "
CREATE TABLE IF NOT EXISTS blob (
    tbl TEXT, key BLOB, value BLOB, size INTEGER, accessed INTEGER,
    PRIMARY KEY(tbl, key)
);
CREATE INDEX IF NOT EXISTS blob_age ON blob (tbl, accessed, key, size);
CREATE TABLE tbl_bytes (
    tbl TEXT PRIMARY KEY,
    bytes INTEGER NOT NULL
) STRICT;
CREATE TRIGGER blob_insert AFTER INSERT ON blob BEGIN
    INSERT INTO tbl_bytes (tbl, bytes) VALUES (NEW.tbl, COALESCE(NEW.size, 0))
    ON CONFLICT(tbl) DO UPDATE SET bytes = bytes + excluded.bytes;
END;
CREATE TRIGGER blob_delete AFTER DELETE ON blob BEGIN
    UPDATE tbl_bytes SET bytes = bytes - COALESCE(OLD.size, 0) WHERE tbl = OLD.tbl;
END;
CREATE TRIGGER blob_update_size AFTER UPDATE OF size ON blob BEGIN
    UPDATE tbl_bytes SET bytes = bytes - COALESCE(OLD.size, 0) WHERE tbl = OLD.tbl;
    INSERT INTO tbl_bytes (tbl, bytes) VALUES (NEW.tbl, COALESCE(NEW.size, 0))
    ON CONFLICT(tbl) DO UPDATE SET bytes = bytes + excluded.bytes;
END;
INSERT INTO tbl_bytes (tbl, bytes)
SELECT tbl, SUM(COALESCE(size, 0)) FROM blob GROUP BY tbl;
";

pub(super) fn migrate(conn: &mut Connection) -> Result<(), rusqlite::Error> {
    let from: u32 = conn.pragma_query_value(None, "user_version", |row| row.get(0))?;
    for (version, sql) in (1..=VERSION)
        .zip(MIGRATIONS)
        .skip(usize::try_from(from).unwrap_or(usize::MAX))
    {
        let transaction = conn.transaction_with_behavior(TransactionBehavior::Immediate)?;
        transaction.execute_batch(sql)?;
        transaction.pragma_update(None, "user_version", version)?;
        transaction.commit()?;
    }
    Ok(())
}
