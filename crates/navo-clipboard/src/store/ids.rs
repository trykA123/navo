//! Id high-water marks, so ids keep rising when the database file is recreated.

use super::{ClipError, ClipStore};

const TABLES: [&str; 2] = ["clips", "saved"];

/// The highest id each table has issued, in `TABLES` order. Numbers only, never text.
pub(super) struct IdMarks([i64; 2]);

impl ClipStore {
    pub(super) fn id_marks(&self) -> Result<IdMarks, ClipError> {
        let mut marks = [0; 2];
        for (mark, table) in marks.iter_mut().zip(TABLES) {
            // The table name comes from `TABLES`, never from outside.
            *mark = self.conn.query_row(
                &format!(
                    "SELECT max(
                         COALESCE((SELECT seq FROM sqlite_sequence WHERE name = ?1), 0),
                         COALESCE((SELECT max(id) FROM {table}), 0))"
                ),
                [table],
                |row| row.get(0),
            )?;
        }
        Ok(IdMarks(marks))
    }

    /// Makes the next ids start above `marks`; a mark at or below the current one is kept.
    pub(super) fn seed_ids(&self, marks: IdMarks) -> Result<(), ClipError> {
        let transaction = self.conn.unchecked_transaction()?;
        for (mark, table) in marks.0.into_iter().zip(TABLES) {
            transaction.execute(
                "DELETE FROM sqlite_sequence WHERE name = ?1 AND seq < ?2",
                (table, mark),
            )?;
            transaction.execute(
                "INSERT INTO sqlite_sequence (name, seq)
                 SELECT ?1, ?2 WHERE NOT EXISTS (SELECT 1 FROM sqlite_sequence WHERE name = ?1)",
                (table, mark),
            )?;
        }
        transaction.commit()?;
        Ok(())
    }
}
