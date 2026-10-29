use std::sync::Arc;

use rusty_core::{ProviderPath, Selection, TabId};
use rusty_fs::{Attributes, EntryKind, FsEntry};

use super::TotalsCache;

fn entry(name: &str, kind: EntryKind, size: u64) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind,
        size,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

fn rows() -> Arc<[FsEntry]> {
    Arc::from(vec![
        entry("d", EntryKind::Folder, 0),
        entry("a", EntryKind::File, 10),
        entry("b", EntryKind::File, 20),
    ])
}

#[test]
fn same_rows_compute_totals_once() {
    let mut cache = TotalsCache::default();
    let rows = rows();
    let marked = Selection::new(3);
    let first = cache.status_of(TabId::new(1), &rows, &marked);
    let second = cache.status_of(TabId::new(1), &rows, &marked);
    assert_eq!(cache.computed, 1);
    assert_eq!((first.folders, first.files, first.bytes), (1, 2, 30));
    assert_eq!((second.folders, second.files, second.bytes), (1, 2, 30));
}

#[test]
fn new_rows_recompute() {
    let mut cache = TotalsCache::default();
    let marked = Selection::new(3);
    let first = rows();
    cache.status_of(TabId::new(1), &first, &marked);
    drop(first);
    let other = rows();
    let status = cache.status_of(TabId::new(1), &other, &marked);
    assert_eq!(cache.computed, 2);
    assert_eq!(status.bytes, 30);
}

#[test]
fn marked_totals_follow_selection() {
    let mut cache = TotalsCache::default();
    let rows = rows();
    let mut marked = Selection::new(3);
    marked.mark_rows([1, 2]);
    let status = cache.status_of(TabId::new(1), &rows, &marked);
    assert_eq!((status.marked, status.marked_bytes), (2, 30));
    marked.toggle(2);
    let status = cache.status_of(TabId::new(1), &rows, &marked);
    assert_eq!((status.marked, status.marked_bytes), (1, 10));
    assert_eq!(cache.computed, 1);
}
