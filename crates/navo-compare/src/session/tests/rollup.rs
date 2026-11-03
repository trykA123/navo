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
}

#[test]
fn an_orphan_file_flags_its_folders_without_a_content_check() {
    let (mut left, right) = pair_of_trees(false);
    left.add_file("c/only.txt", b"x".to_vec());
    let rows = latest(&collect(&start(left, right, CheckMode::SizeTime)));

    assert!(flagged(&rows, "c"));
    assert!(!flagged(&rows, "a"));
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
