//! Proves a fresh old-name record restarts its wait, so a live file is never removed (IXH1).

use super::*;

const CREATE: u32 = 0x100;

fn record(file_id: u128, reason: u32, name: &str) -> UsnRecord {
    UsnRecord {
        file_id,
        parent_id: 7,
        usn: 0,
        reason,
        flags: Flags::NONE,
        name: name.into(),
        size: None,
    }
}

fn read(records: Vec<UsnRecord>, renames: &mut PendingRenames) -> Vec<TableChange> {
    UsnBatch {
        next_usn: 0,
        records,
    }
    .into_changes(renames)
}

/// Leaves file 42's old name waiting at the last read it may be carried.
fn old_name_at_its_last_read(renames: &mut PendingRenames) {
    assert!(read(vec![record(42, RENAME_OLD_NAME, "a")], renames).is_empty());
    for other in 0..u128::from(MAX_CARRIED_READS) {
        read(vec![record(50 + other, CREATE, "elm")], renames);
    }
    assert_eq!(renames.waiting, vec![(42, MAX_CARRIED_READS)]);
}

fn removes(changes: &[TableChange], file_id: u128) -> bool {
    changes.contains(&TableChange::Remove { file_id })
}

#[test]
fn a_second_rename_in_the_read_that_pairs_the_first_keeps_the_file() {
    let mut renames = PendingRenames::default();
    old_name_at_its_last_read(&mut renames);
    let changes = read(
        vec![
            record(42, RENAME_NEW_NAME, "b"),
            record(42, RENAME_OLD_NAME, "b"),
        ],
        &mut renames,
    );
    assert!(
        !removes(&changes, 42),
        "a renamed live file was removed: {changes:?}"
    );
    assert_eq!(renames.waiting, vec![(42, 0)]);
    let changes = read(vec![record(42, RENAME_NEW_NAME, "c")], &mut renames);
    assert!(matches!(
        changes.as_slice(),
        [TableChange::Rename { file_id: 42, .. }]
    ));
}

#[test]
fn a_repeated_old_name_restarts_the_wait() {
    let mut renames = PendingRenames::default();
    old_name_at_its_last_read(&mut renames);
    let changes = read(vec![record(42, RENAME_OLD_NAME, "a")], &mut renames);
    assert!(
        !removes(&changes, 42),
        "a waiting old name was removed: {changes:?}"
    );
    assert_eq!(renames.waiting, vec![(42, 0)]);
}

#[test]
fn a_restarted_old_name_still_expires_without_a_partner() {
    let mut renames = PendingRenames::default();
    old_name_at_its_last_read(&mut renames);
    read(vec![record(42, RENAME_OLD_NAME, "a")], &mut renames);
    for other in 0..u128::from(MAX_CARRIED_READS) {
        let changes = read(vec![record(60 + other, CREATE, "elm")], &mut renames);
        assert!(!removes(&changes, 42));
    }
    let changes = read(vec![record(70, CREATE, "elm")], &mut renames);
    assert!(removes(&changes, 42));
    assert!(renames.is_empty());
}
