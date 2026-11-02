use super::{NtfsError, NtfsIndex};
use crate::file::{Header, validate_ids, validate_root};
use crate::table::NameTable;
use crate::usn::{FrnMap, JournalPosition, PendingRenames};

impl NtfsIndex {
    pub fn snapshot(&self) -> Option<(&NameTable, &FrnMap, JournalPosition)> {
        (self.renames == PendingRenames::default()).then_some((
            &self.table,
            &self.ids,
            self.position,
        ))
    }

    pub fn restore(
        snapshot: (Header, NameTable, FrnMap),
        volume_serial: u64,
        journal_id: u64,
    ) -> Result<Self, NtfsError> {
        let (header, table, ids) = snapshot;
        if header.volume_serial != volume_serial
            || header.journal_id != journal_id
            || header.next_usn < 0
            || ids.get(&super::ROOT_REFERENCE) != Some(&0)
            || !table.valid_structure()
        {
            return Err(NtfsError::Reenumerate);
        }
        validate_root(&header, &table)?;
        validate_ids(&table, &ids)?;
        Ok(Self {
            table,
            ids,
            position: JournalPosition {
                journal_id: header.journal_id,
                next_usn: header.next_usn,
            },
            renames: PendingRenames::default(),
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::file::{self, Header};
    use crate::ntfs::{NtfsError, NtfsIndex};
    use crate::table::Flags;
    use crate::usn::{JournalPosition, Resume, UsnBatch, UsnRecord};

    const SERIAL: u64 = 42;
    const WIDE_ID: u128 = (1u128 << 100) | 8;
    const POSITION: JournalPosition = JournalPosition {
        journal_id: 7,
        next_usn: 100,
    };

    fn record(file_id: u128, parent_id: u128, reason: u32, name: &str) -> UsnRecord {
        UsnRecord {
            size: None,
            file_id,
            parent_id,
            usn: 100,
            reason,
            flags: Flags::FOLDER,
            name: name.into(),
        }
    }

    fn index() -> NtfsIndex {
        let mut index = NtfsIndex::new('C', POSITION).unwrap();
        index
            .apply_batch(UsnBatch {
                next_usn: 110,
                records: vec![
                    record(WIDE_ID, 5, 0x100, "old"),
                    record(9, WIDE_ID, 0x100, "child"),
                    record(10, 5, 0x100, "remove"),
                ],
            })
            .unwrap();
        index
    }

    fn header(index: &NtfsIndex) -> Header {
        Header {
            volume_serial: SERIAL,
            journal_id: index.position.journal_id,
            next_usn: index.position.next_usn,
            root: "C:".into(),
            built_at: 0,
        }
    }

    #[test]
    fn restored_snapshot_applies_later_journal_changes_like_the_original() {
        let mut original = index();
        let folder = tempfile::tempdir().unwrap();
        let path = folder.path().join("volume.index");
        let (table, ids, position) = original.snapshot().unwrap();
        file::save_with_ids(&path, &header(&original), table, ids).unwrap();
        let saved = file::load_with_ids(&path).unwrap();
        let mut restored = NtfsIndex::restore(saved, SERIAL, POSITION.journal_id).unwrap();
        assert_eq!(restored.position, position);
        restored.restart_at(position);
        let batch = UsnBatch {
            next_usn: 120,
            records: vec![
                record(WIDE_ID, 5, 0x1000, "old"),
                record(WIDE_ID, 5, 0x2000, "new"),
                record(10, 5, 0x200, "remove"),
                record(11, WIDE_ID, 0x100, "added"),
                record(9, WIDE_ID, 0x8000, "child"),
            ],
        };
        assert_eq!(
            original.apply_batch(batch.clone()).unwrap(),
            restored.apply_batch(batch).unwrap()
        );
        assert_eq!(restored.position, original.position);
        assert_eq!(restored.ids, original.ids);
        assert_eq!(restored.table.slots(), original.table.slots());
        for id in 0..original.table.slots() {
            assert_eq!(restored.table.path(id), original.table.path(id));
            assert_eq!(restored.table.flags(id), original.table.flags(id));
        }
        assert_eq!(
            restored.table.path_with(restored.ids[&9], '\\').as_deref(),
            Some("C:\\new\\child")
        );
    }

    #[test]
    fn restore_refuses_a_different_volume() {
        let index = index();
        let saved = (header(&index), index.table, index.ids);
        assert!(NtfsIndex::restore(saved, SERIAL + 1, POSITION.journal_id).is_err());
    }

    #[test]
    fn restore_refuses_a_different_journal() {
        let index = index();
        let saved = (header(&index), index.table, index.ids);
        assert!(matches!(
            NtfsIndex::restore(saved, SERIAL, POSITION.journal_id + 1),
            Err(NtfsError::Reenumerate)
        ));
    }

    #[test]
    fn restore_refuses_an_out_of_range_entry_id() {
        let mut index = index();
        index.ids.insert(20, index.table.slots());
        let saved = (header(&index), index.table, index.ids);
        assert!(NtfsIndex::restore(saved, SERIAL, POSITION.journal_id).is_err());
    }

    #[test]
    fn restore_refuses_a_dead_frn_target() {
        let mut index = index();
        index.table.remove(index.ids[&10]);
        let saved = (header(&index), index.table, index.ids);
        assert!(NtfsIndex::restore(saved, SERIAL, POSITION.journal_id).is_err());
    }

    #[test]
    fn restore_refuses_a_root_that_differs_from_the_header() {
        let index = index();
        let mut header = header(&index);
        header.root = "D:".into();
        let saved = (header, index.table, index.ids);
        assert!(NtfsIndex::restore(saved, SERIAL, POSITION.journal_id).is_err());
    }

    #[test]
    fn restore_refuses_a_root_reference_that_points_to_another_entry() {
        let mut index = index();
        index.ids.insert(5, index.ids[&WIDE_ID]);
        let saved = (header(&index), index.table, index.ids);
        assert!(NtfsIndex::restore(saved, SERIAL, POSITION.journal_id).is_err());
    }

    #[test]
    fn restore_refuses_an_invalid_parent() {
        let mut index = index();
        index.table.push(999, "orphan", Flags::NONE).unwrap();
        let saved = (header(&index), index.table, index.ids);
        assert!(NtfsIndex::restore(saved, SERIAL, POSITION.journal_id).is_err());
    }

    #[test]
    fn restore_refuses_a_parent_cycle() {
        let mut index = index();
        let first = index.table.slots();
        index.table.push(first + 1, "first", Flags::FOLDER).unwrap();
        index.table.push(first, "second", Flags::FOLDER).unwrap();
        let saved = (header(&index), index.table, index.ids);
        assert!(NtfsIndex::restore(saved, SERIAL, POSITION.journal_id).is_err());
    }

    #[test]
    fn snapshot_waits_for_the_last_pending_rename() {
        let mut index = index();
        assert!(index.snapshot().is_some());
        index
            .apply_batch(UsnBatch {
                next_usn: 120,
                records: vec![
                    record(WIDE_ID, 5, 0x1000, "old"),
                    record(9, WIDE_ID, 0x1000, "child"),
                ],
            })
            .unwrap();
        assert!(index.snapshot().is_none());
        index
            .apply_batch(UsnBatch {
                next_usn: 130,
                records: vec![record(WIDE_ID, 5, 0x2000, "new")],
            })
            .unwrap();
        assert!(index.snapshot().is_none());
        index
            .apply_batch(UsnBatch {
                next_usn: 140,
                records: vec![record(9, WIDE_ID, 0x2000, "renamed-child")],
            })
            .unwrap();
        assert!(index.snapshot().is_some());
    }

    #[test]
    fn restored_position_requires_enumeration_after_a_journal_wrap() {
        let index = index();
        let saved = (header(&index), index.table, index.ids);
        let restored = NtfsIndex::restore(saved, SERIAL, POSITION.journal_id).unwrap();
        assert_eq!(
            restored.position.resume(POSITION.journal_id, 111),
            Resume::Reenumerate
        );
    }
}
