use proptest::prelude::*;
use rusty_core::ProviderPath;

use super::sort_order;
use crate::view::{SortBy, SortKey};
use crate::{Attributes, CancelToken, EntryKind, FsEntry};

fn entry(name: &str, kind: EntryKind, size: u64, modified_ms: i64) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind,
        size,
        modified_ms: Some(modified_ms),
        attributes: Attributes::default(),
    }
}

fn sample() -> Vec<FsEntry> {
    vec![
        entry("b.txt", EntryKind::File, 300, 3),
        entry("zeta", EntryKind::Folder, 0, 9),
        entry("a.rs", EntryKind::File, 100, 1),
        entry("C.md", EntryKind::File, 200, 2),
        entry("alpha", EntryKind::Folder, 0, 8),
    ]
}

fn names(rows: &[FsEntry], order: &[u32]) -> Vec<String> {
    order
        .iter()
        .map(|&i| rows[i as usize].name.to_text())
        .collect()
}

fn run(by: SortBy, descending: bool, folders_first: bool) -> Vec<String> {
    let rows = sample();
    let key = SortKey { by, descending };
    let order = sort_order(&rows, key, folders_first, &CancelToken::new()).unwrap();
    names(&rows, &order)
}

#[test]
fn name_ascending_and_descending() {
    assert_eq!(
        run(SortBy::Name, false, false),
        ["a.rs", "alpha", "b.txt", "C.md", "zeta"]
    );
    assert_eq!(
        run(SortBy::Name, true, false),
        ["zeta", "C.md", "b.txt", "alpha", "a.rs"]
    );
}

#[test]
fn ext_ascending_and_descending() {
    assert_eq!(
        run(SortBy::Ext, false, false),
        ["alpha", "zeta", "C.md", "a.rs", "b.txt"]
    );
    assert_eq!(
        run(SortBy::Ext, true, false),
        ["b.txt", "a.rs", "C.md", "zeta", "alpha"]
    );
}

#[test]
fn size_ascending_and_descending() {
    assert_eq!(
        run(SortBy::Size, false, false),
        ["alpha", "zeta", "a.rs", "C.md", "b.txt"]
    );
    assert_eq!(
        run(SortBy::Size, true, false),
        ["b.txt", "C.md", "a.rs", "alpha", "zeta"]
    );
}

#[test]
fn date_ascending_and_descending() {
    assert_eq!(
        run(SortBy::Date, false, false),
        ["a.rs", "C.md", "b.txt", "alpha", "zeta"]
    );
    assert_eq!(
        run(SortBy::Date, true, false),
        ["zeta", "alpha", "b.txt", "C.md", "a.rs"]
    );
}

#[test]
fn folders_first_groups_folders() {
    assert_eq!(
        run(SortBy::Name, false, true),
        ["alpha", "zeta", "a.rs", "b.txt", "C.md"]
    );
    assert_eq!(
        run(SortBy::Name, true, true),
        ["zeta", "alpha", "C.md", "b.txt", "a.rs"]
    );
}

#[test]
fn ties_fall_back_to_the_lower_name() {
    let rows = vec![
        entry("b", EntryKind::File, 5, 1),
        entry("A", EntryKind::File, 5, 1),
    ];
    let key = SortKey {
        by: SortBy::Size,
        descending: false,
    };
    let order = sort_order(&rows, key, false, &CancelToken::new()).unwrap();
    assert_eq!(names(&rows, &order), ["A", "b"]);
}

#[test]
fn equal_names_keep_row_order() {
    let rows = vec![
        entry("a", EntryKind::File, 1, 1),
        entry("A", EntryKind::File, 2, 2),
        entry("a", EntryKind::File, 3, 3),
    ];
    let key = SortKey::default();
    let order = sort_order(&rows, key, false, &CancelToken::new()).unwrap();
    assert_eq!(order, [0, 1, 2]);
}

#[test]
fn a_cancelled_token_returns_none() {
    let cancel = CancelToken::new();
    cancel.cancel();
    assert!(sort_order(&sample(), SortKey::default(), true, &cancel).is_none());
}

proptest! {
    #[test]
    fn the_order_is_a_sorted_permutation(
        names in proptest::collection::vec("[a-cA-C]{0,3}(\\.[a-c]{1,2})?", 0..40),
        sizes in proptest::collection::vec(0u64..4, 40),
        descending in any::<bool>(),
        folders_first in any::<bool>(),
        by in 0usize..4,
    ) {
        let rows: Vec<FsEntry> = names
            .iter()
            .enumerate()
            .map(|(i, n)| {
                let kind = if sizes[i] == 0 { EntryKind::Folder } else { EntryKind::File };
                entry(n, kind, sizes[i], i64::from(u8::try_from(i).unwrap() % 3))
            })
            .collect();
        let by = [SortBy::Name, SortBy::Ext, SortBy::Size, SortBy::Date][by];
        let key = SortKey { by, descending };
        let order = sort_order(&rows, key, folders_first, &CancelToken::new()).unwrap();
        let mut seen = order.clone();
        seen.sort_unstable();
        prop_assert_eq!(seen, (0..u32::try_from(rows.len()).unwrap()).collect::<Vec<_>>());
        let again = sort_order(&materialize_rows(&rows, &order), key, folders_first, &CancelToken::new()).unwrap();
        let identity: Vec<u32> = (0..u32::try_from(rows.len()).unwrap()).collect();
        prop_assert_eq!(again, identity);
    }
}

fn materialize_rows(rows: &[FsEntry], order: &[u32]) -> Vec<FsEntry> {
    crate::view::materialize(rows, order)
}
