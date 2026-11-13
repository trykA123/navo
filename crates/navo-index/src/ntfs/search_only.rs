//! Converts old NTFS generations to search storage and checks their read-only contract.

use super::{FrnMap, NtfsIndex, PendingRenames};

impl NtfsIndex {
    /// Reports whether this generation retains only read-only search storage.
    pub fn is_search_only(&self) -> bool {
        self.search_only
    }

    /// Releases update and totals storage while preserving search; repeated calls are harmless.
    pub fn make_search_only(&mut self) {
        self.table.retain_search_columns();
        self.ids = FrnMap::default();
        self.renames = PendingRenames::default();
        self.search_only = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ntfs::NtfsError::InvalidResponse;
    use crate::search::{Term, search};
    use crate::table::Flags;
    use crate::usn::{JournalPosition, UsnBatch, UsnRecord};

    /// Builds a valid rename batch ending at the supplied journal cursor.
    fn batch(next_usn: i64) -> UsnBatch {
        UsnBatch {
            next_usn,
            records: vec![UsnRecord {
                file_id: 9,
                parent_id: 5,
                usn: next_usn - 1,
                reason: 0x1000,
                flags: Flags::HIDDEN,
                name: "needle.rs".into(),
                size: None,
            }],
        }
    }

    /// Builds a searchable file with allocated identity and pending-rename storage.
    fn index() -> NtfsIndex {
        let position = JournalPosition {
            journal_id: 7,
            next_usn: 100,
        };
        let mut index = NtfsIndex::new('C', position).unwrap();
        let file = index.table.push(0, "needle.rs", Flags::HIDDEN).unwrap();
        index.table.set_size(file, 42).unwrap();
        index.ids.insert(9, file);
        index.apply_batch(batch(110)).unwrap();
        assert!(!index.is_search_only());
        assert!(index.ids.memory_bytes() > 0 && index.renames.memory_bytes() > 0);
        index
    }

    #[test]
    fn search_only_releases_allocations_and_preserves_search_repeatedly() {
        let mut index = index();
        let query = [Term::Name("needle"), Term::Path("C:")];
        let hits = search(&index.table, &query, 10, &|| false);
        assert_eq!(hits.len(), 1);
        let file = hits[0].id;
        let path = index.table.path_with(file, '\\');
        let flags = index.table.flags(file);
        let counts = (index.table.len(), index.table.slots());
        let position = index.position;
        for _ in 0..2 {
            index.make_search_only();
            assert_eq!(
                index.ids.memory_bytes(),
                0,
                "search-only must release the FRN map"
            );
            assert_eq!(index.renames.memory_bytes(), 0);
            assert!(index.is_search_only());
            assert_eq!(search(&index.table, &query, 10, &|| false), hits);
            assert_eq!(index.table.path_with(file, '\\'), path);
            assert_eq!(index.table.flags(file), flags);
            assert_eq!((index.table.len(), index.table.slots()), counts);
            assert_eq!(index.position, position);
            assert_eq!(index.table.size(file), None);
            assert_eq!(index.table.totals(0), None);
        }
    }

    #[test]
    fn search_only_rejects_snapshots_and_new_batches() {
        let mut index = index();
        index.restart_at(index.position);
        assert!(index.snapshot().is_some());
        index.make_search_only();
        assert!(
            index.snapshot().is_none(),
            "search-only snapshots must be rejected"
        );
        assert!(matches!(
            index.begin_batch(batch(120)),
            Err(InvalidResponse(_))
        ));
        assert!(matches!(
            index.apply_batch(batch(120)),
            Err(InvalidResponse(_))
        ));
    }

    #[test]
    fn search_only_rejects_batches_created_before_conversion() {
        let mut index = index();
        let position = index.position;
        let mut stepping = index.begin_batch(batch(120)).unwrap();
        let finishing = index.begin_batch(batch(120)).unwrap();
        index.make_search_only();
        assert!(matches!(
            stepping.step(&mut index, 1),
            Err(InvalidResponse(_))
        ));
        assert!(matches!(
            finishing.finish(&mut index),
            Err(InvalidResponse(_))
        ));
        assert_eq!(index.position, position);
        assert_eq!(index.renames.memory_bytes(), 0);
    }
}
