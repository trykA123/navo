//! Folder roll-up: a folder row reports that its subtree holds a difference.

use std::collections::BTreeMap;

use super::fake::FakeSource;
use super::*;

/// The last version of each row by name, as a screen would hold them.
fn latest(batches: &[RowBatch]) -> BTreeMap<String, Row> {
    let mut by_id = BTreeMap::new();
    for row in batches.iter().flat_map(|batch| batch.rows.iter()) {
        by_id.insert(row.id, row.clone());
    }
    by_id
        .into_values()
        .map(|row| (row_name(&row), row))
        .collect()
}

fn flagged(rows: &BTreeMap<String, Row>, name: &str) -> bool {
    rows.get(name)
        .unwrap_or_else(|| panic!("row {name} should exist"))
        .contains_differences
}

fn pair_of_trees(change: bool) -> (FakeSource, FakeSource) {
    let mut left = FakeSource::new(true);
    let mut right = FakeSource::new(true);
    for source in [&mut left, &mut right] {
        for folder in ["a", "a/b", "c"] {
            source.add_folder(folder);
        }
        source.add_file("a/b/y.txt", b"same".to_vec());
        source.add_file("c/z.txt", b"same".to_vec());
    }
    left.add_file("a/b/x.txt", b"aaaa".to_vec());
    right.add_file("a/b/x.txt", if change { b"bbbb" } else { b"aaaa" }.to_vec());
    (left, right)
}

#[test]
fn a_change_deep_in_a_folder_flags_every_folder_above_it() {
    let (left, right) = pair_of_trees(true);
    let rows = latest(&collect(&start(left, right, CheckMode::Content)));

    assert!(flagged(&rows, "a"));
    assert!(flagged(&rows, "a/b"));
    assert!(!flagged(&rows, "c"));
    assert!(!flagged(&rows, "a/b/x.txt"));
    assert_eq!(rows["a"].raw_status, Status::Same);
}

#[test]
fn a_tree_without_changes_flags_no_folder() {
    let (left, right) = pair_of_trees(false);
    let rows = latest(&collect(&start(left, right, CheckMode::Content)));

    assert!(rows.values().all(|row| !row.contains_differences));
    assert_eq!(rows["a"].raw_status, Status::Same);
}

#[test]
fn an_orphan_file_flags_its_folders_without_a_content_check() {
    let (mut left, right) = pair_of_trees(false);
    left.add_file("c/only.txt", b"x".to_vec());
    let rows = latest(&collect(&start(left, right, CheckMode::SizeTime)));

    assert!(flagged(&rows, "c"));
    assert!(!flagged(&rows, "a"));
    assert_eq!(rows["c"].raw_status, Status::Same);
}

#[test]
fn a_checking_child_does_not_flag_its_folder_until_a_difference_resolves() {
    let mut rollup = crate::session::rollup::Rollup::default();
    let folder = generation_with(Generation::new(1), Status::Same, EntryKind::Folder);
    let child = Row {
        id: 1,
        ..generation_with(Generation::new(1), Status::Checking, EntryKind::File)
    };
    assert!(
        rollup
            .add_batch(std::slice::from_ref(&folder), None)
            .is_empty()
    );
    assert!(rollup.add_batch(&[child], Some(folder.id)).is_empty());
    assert!(rollup.resolve(Some(folder.id), Status::Checking).is_empty());
    assert!(rollup.resolve(Some(folder.id), Status::Same).is_empty());
    let changed = rollup.resolve(
        Some(folder.id),
        Status::Different {
            first_difference: None,
        },
    );
    assert_eq!(changed.len(), 1);
    assert!(changed[0].contains_differences);
    assert_eq!(changed[0].raw_status, Status::Same);
}

#[test]
fn a_folder_that_cannot_be_listed_flags_its_parents() {
    let (mut left, right) = pair_of_trees(false);
    left.fail_listing("a/b", Reason::ReadError);
    let rows = latest(&collect(&start(left, right, CheckMode::Content)));

    assert!(flagged(&rows, "a"));
    assert!(!flagged(&rows, "c"));
}

#[test]
fn a_refresh_recomputes_the_roll_up() {
    let (left, right) = pair_of_trees(true);
    let mut session = start(left, right, CheckMode::Content);
    collect(&session);
    session.refresh();
    let rows = latest(&collect(&session));

    assert!(flagged(&rows, "a"));
    assert!(flagged(&rows, "a/b"));
    assert!(!flagged(&rows, "c"));
}

fn diff() -> Status {
    Status::Different {
        first_difference: None,
    }
}

/// Folders a (id 10) > b (11) > c (12), each tracked by a fresh roll-up.
fn nested() -> crate::session::rollup::Rollup {
    let mut rollup = crate::session::rollup::Rollup::default();
    for (id, parent) in [(10, None), (11, Some(10)), (12, Some(11))] {
        let folder = Row {
            id,
            ..generation_with(Generation::new(1), Status::Same, EntryKind::Folder)
        };
        rollup.add_batch(&[folder], parent);
    }
    rollup
}

fn counts(rows: &[Row]) -> BTreeMap<u64, u32> {
    rows.iter().map(|r| (r.id, r.differences_inside)).collect()
}

#[test]
fn nested_folders_count_every_difference_below_them() {
    let mut rollup = nested();
    for _ in 0..3 {
        rollup.resolve(Some(12), diff());
    }
    rollup.resolve(Some(10), diff());
    let rows = rollup.take_dirty(std::time::Instant::now(), true);

    assert_eq!(counts(&rows), BTreeMap::from([(10, 4), (11, 3), (12, 3)]));
}

#[test]
fn take_dirty_waits_100_ms_between_batches() {
    let t = std::time::Instant::now();
    let ms = std::time::Duration::from_millis;
    let mut rollup = nested();
    rollup.resolve(Some(12), diff());
    rollup.resolve(Some(12), diff());
    assert_eq!(rollup.take_dirty(t, false).len(), 3);
    rollup.resolve(Some(12), diff());
    assert!(rollup.take_dirty(t + ms(99), false).is_empty());
    assert_eq!(counts(&rollup.take_dirty(t + ms(100), false))[&12], 3);
    assert!(rollup.take_dirty(t + ms(100), true).is_empty());
    rollup.resolve(Some(12), diff());
    assert_eq!(rollup.take_dirty(t + ms(101), true).len(), 3);
}

#[test]
fn only_the_first_difference_returns_a_folder_at_once() {
    let mut rollup = nested();
    let first = rollup.resolve(Some(12), diff());
    assert_eq!(counts(&first).len(), 3);
    assert!(first.iter().all(|row| row.contains_differences));
    assert!(rollup.resolve(Some(12), diff()).is_empty());
    let held = rollup.take_dirty(std::time::Instant::now(), true);
    assert_eq!(counts(&held), BTreeMap::from([(10, 2), (11, 2), (12, 2)]));
}

#[test]
fn a_same_or_checking_resolve_changes_no_count() {
    let mut rollup = nested();
    rollup.resolve(Some(12), Status::Same);
    rollup.resolve(Some(12), Status::Checking);
    assert!(
        rollup
            .take_dirty(std::time::Instant::now(), true)
            .is_empty()
    );
}
