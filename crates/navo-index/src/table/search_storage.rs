//! Releases table columns that the old generation no longer needs for search.

use super::NameTable;

impl NameTable {
    /// Releases children and totals without moving search columns or changing entry IDs.
    /// Callers must expose only shared access afterward; mutation is unsupported.
    pub fn retain_search_columns(&mut self) {
        self.children = super::Children::default();
        self.bytes = Vec::new();
        self.counts = Vec::new();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::{SearchOptions, TableHit, Term, search, search_filtered};
    use crate::table::{Flags, NO_PARENT};

    /// Builds a sparse table with Unicode paths, filterable flags and allocated totals.
    fn table() -> NameTable {
        let mut table = NameTable::with_capacity(32, 512);
        let root = table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
        let folder = table
            .push(root, "données", Flags::FOLDER | Flags::HIDDEN)
            .unwrap();
        let file = table.push(folder, "résumé.rs", Flags::REPARSE).unwrap();
        table.set_size(file, 42).unwrap();
        table.push(folder, "résumé.rs", Flags::SYSTEM).unwrap();
        let deleted = table.push(folder, "obsolete.rs", Flags::NONE).unwrap();
        table.remove(deleted);
        table
    }

    /// Returns each retained allocation's address and capacity in bytes for comparison.
    fn retained_storage(table: &NameTable) -> [(*const u8, usize); 4] {
        [
            (table.names.as_ptr(), table.names.capacity()),
            (
                table.offsets.as_ptr().cast(),
                table.offsets.capacity() * size_of::<u32>(),
            ),
            (
                table.parents.as_ptr().cast(),
                table.parents.capacity() * size_of::<u32>(),
            ),
            (table.flags.as_ptr(), table.flags.capacity()),
        ]
    }

    /// Searches names, paths and extensions, returning all hits and reparse-only hits.
    fn search_queries(table: &NameTable) -> (Vec<TableHit>, Vec<TableHit>) {
        let extensions = [String::from("rs")];
        let query = [
            Term::Name("résumé"),
            Term::Path("données"),
            Term::Ext(&extensions),
        ];
        let hits = search(table, &query, 10, &|| false);
        let selected = search_filtered(
            table,
            &query,
            &SearchOptions {
                limit: 10,
                cancel: &|| false,
                include: &|id| {
                    table
                        .flags(id)
                        .is_some_and(|flags| flags.contains(Flags::REPARSE))
                },
            },
        );
        (hits, selected)
    }

    #[test]
    fn search_storage_releases_auxiliary_columns_and_preserves_queries() {
        let mut table = table();
        let storage = retained_storage(&table);
        let retained_bytes: usize = storage.iter().map(|(_, bytes)| bytes).sum();
        let counts = (table.len(), table.slots());
        let entries: Vec<_> = (0..table.slots())
            .map(|id| (table.path_with(id, '/'), table.parent(id), table.flags(id)))
            .collect();
        let (hits, selected) = search_queries(&table);
        assert_eq!(hits.iter().map(|hit| hit.id).collect::<Vec<_>>(), [2, 3]);
        assert_eq!(selected.iter().map(|hit| hit.id).collect::<Vec<_>>(), [2]);
        assert_eq!(entries[2].0.as_deref(), Some("C:/données/résumé.rs"));
        assert!(table.children.memory_bytes() > 0 && table.memory_parts().totals > 0);
        assert_eq!(table.size(2), Some(42));
        assert_eq!(table.totals(0).unwrap().files, 2);

        table.retain_search_columns();

        assert_eq!(
            table.memory_bytes(),
            retained_bytes,
            "search storage must release all non-search allocated bytes"
        );
        assert_eq!(table.children.memory_bytes(), 0);
        assert_eq!((table.bytes.capacity(), table.counts.capacity()), (0, 0));
        assert_eq!(retained_storage(&table), storage);
        assert_eq!((table.len(), table.slots()), counts);
        for (id, expected) in (0..table.slots()).zip(entries) {
            assert_eq!(
                (table.path_with(id, '/'), table.parent(id), table.flags(id)),
                expected
            );
            assert_eq!(table.size(id), None);
            assert_eq!(table.totals(id), None);
        }
        assert_eq!(table.size(u32::MAX), None);
        assert_eq!(table.totals(u32::MAX), None);
        assert_eq!(search_queries(&table), (hits, selected));
        table.retain_search_columns();
        assert_eq!(retained_storage(&table), storage);
        assert_eq!(table.memory_bytes(), retained_bytes);
    }
}
