use std::path::Path;
use std::time::Duration;

use rusty_index::file::{self, Header};
use rusty_index::ntfs::NtfsIndex;
use rusty_index::table::{Flags, NO_PARENT, NameTable};
use rusty_index::usn::{UsnBatch, UsnRecord};

use super::config::Drive;
use super::lifecycle::{SaveClock, restore_or_enumerate};

fn index() -> NtfsIndex {
    let mut table = NameTable::new();
    table.push(NO_PARENT, "C:", Flags::FOLDER).unwrap();
    NtfsIndex::restore((header(100), table, [(5, 0)].into()), 42, 7).unwrap()
}

fn header(next_usn: i64) -> Header {
    Header {
        volume_serial: 42,
        journal_id: 7,
        next_usn,
        root: "C:".into(),
        built_at: 0,
    }
}

fn save(clock: &mut SaveClock, path: &Path, index: &NtfsIndex, stopping: bool) {
    let snapshot = index.snapshot();
    if !clock.due(Duration::ZERO, snapshot, stopping) {
        return;
    }
    let (table, ids, position) = snapshot.unwrap();
    clock
        .save(
            path,
            (header(position.next_usn), table, ids),
            Duration::ZERO,
        )
        .unwrap();
}

#[test]
fn a_batch_over_ten_thousand_records_saves_before_the_timer() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("volume.index");
    let mut index = index();
    let mut clock = SaveClock::default();
    let batch = UsnBatch {
        next_usn: 10_101,
        records: (0..10_001)
            .map(|number| UsnRecord {
                file_id: 5,
                parent_id: 5,
                usn: 101 + number,
                reason: 0x8000,
                flags: Flags::FOLDER,
                name: "C:".into(),
                size: None,
            })
            .collect(),
    };
    clock.batch(Duration::ZERO, batch.records.len());
    assert!(index.apply_batch(batch).unwrap().is_empty());
    save(&mut clock, &path, &index, false);
    assert_eq!(file::load_with_ids(&path).unwrap().0.next_usn, 10_101);
    assert!(!clock.due(Duration::ZERO, index.snapshot(), false));
}

#[test]
fn a_ten_thousand_record_batch_waits_for_the_timer() {
    let mut clock = SaveClock::default();
    clock.batch(Duration::ZERO, 10_000);
    assert!(!clock.due(Duration::ZERO, Some(()), false));
    assert!(clock.due(Duration::from_secs(30), Some(()), false));
}

#[test]
fn stopping_saves_dirty_entries_before_the_timer() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("volume.index");
    let mut index = index();
    let mut clock = SaveClock::default();
    clock.changed(Duration::ZERO);
    save(&mut clock, &path, &index, true);
    index.table.push(0, "saved.txt", Flags::NONE).unwrap();
    clock.changed(Duration::ZERO);
    save(&mut clock, &path, &index, false);
    assert_eq!(file::load_with_ids(&path).unwrap().1.len(), 1);
    save(&mut clock, &path, &index, true);
    assert_eq!(
        file::load_with_ids(&path).unwrap().1.name(1),
        Some("saved.txt")
    );
}

#[test]
fn a_failed_large_batch_save_keeps_the_retry_backoff() {
    let directory = tempfile::tempdir().unwrap();
    let mut clock = SaveClock::default();
    clock.batch(Duration::ZERO, 10_001);
    let index = index();
    let (table, ids, position) = index.snapshot().unwrap();
    assert!(
        clock
            .save(
                directory.path(),
                (header(position.next_usn), table, ids),
                Duration::ZERO,
            )
            .is_err()
    );
    clock.batch(Duration::from_secs(1), 10_001);
    assert!(!clock.due(Duration::from_secs(29), index.snapshot(), false));
    assert!(clock.due(Duration::from_secs(30), index.snapshot(), false));
}

fn fallback_after_damage(damage: impl FnOnce(&mut [u8])) {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("volume.index");
    let mut clock = SaveClock::default();
    clock.changed(Duration::ZERO);
    save(&mut clock, &path, &index(), true);
    let mut bytes = std::fs::read(&path).unwrap();
    damage(&mut bytes);
    std::fs::write(&path, bytes).unwrap();
    let saved = file::load_with_ids(&path);
    assert!(saved.is_err());
    let mut enumerated = false;
    let restored = restore_or_enumerate(
        saved,
        Drive {
            letter: 'C',
            serial: 42,
            fixed: true,
            ntfs: true,
        },
        || {
            enumerated = true;
            Ok(index())
        },
    )
    .unwrap();
    assert!(enumerated);
    assert_eq!(restored.table.name(0), Some("C:"));
}

#[test]
fn an_old_version_snapshot_falls_back_to_a_full_read() {
    fallback_after_damage(|bytes| {
        bytes[16..18].copy_from_slice(&(file::FORMAT_VERSION - 1).to_le_bytes());
    });
}

#[test]
fn a_corrupt_snapshot_falls_back_to_a_full_read() {
    fallback_after_damage(|bytes| {
        let last = bytes.len() - 1;
        bytes[last] ^= 0x80;
    });
}
