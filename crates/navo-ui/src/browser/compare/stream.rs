//! What the Stream reads from a folder compare: every row under the shown folder, depth first,
//! the file pair of one row, and a stamp that changes whenever rows arrive (D25).

use super::hooks::CompareRow;
use super::mode::{CompareMode, FileDiffRequest};

impl CompareMode {
    /// Every row under the shown folder at any depth: a folder row comes before the rows inside
    /// it, and each folder keeps the engine's order (folders first, then names).
    #[must_use]
    pub fn rows_under_folder(&self) -> Vec<CompareRow> {
        self.rows
            .under(self.folder())
            .into_iter()
            .cloned()
            .collect()
    }

    /// The file pair of the row with `id`: each side that is a file, by its location.
    /// `None` when the row has not arrived.
    #[must_use]
    pub fn pair_of(&self, id: u64) -> Option<FileDiffRequest> {
        self.rows.by_id(id).map(|row| self.file_diff(row))
    }

    /// The id of the nearest row after (`step` > 0) or before (`step` < 0) the row `id` that
    /// `keep` accepts, in the depth-first order under the shown folder. One walk, no copies;
    /// `None` when `id` is not under the folder or no such row exists.
    #[must_use]
    pub fn row_beside(
        &self,
        id: u64,
        step: i32,
        keep: impl Fn(&CompareRow) -> bool,
    ) -> Option<u64> {
        let rows = self.rows.under(self.folder());
        let here = rows.iter().position(|row| row.id == id)?;
        let found = match step > 0 {
            true => rows[here + 1..].iter().find(|row| keep(row)),
            false => rows[..here].iter().rev().find(|row| keep(row)),
        };
        found.map(|row| row.id)
    }

    /// The generation and the row batches received in it; it differs after any row update.
    #[must_use]
    pub fn rows_stamp(&self) -> (u64, u64) {
        (self.rows.generation(), self.rows.batches())
    }
}
