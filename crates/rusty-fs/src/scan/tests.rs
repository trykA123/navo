use rusty_core::{FsErrorKind, ItemKind, ScannedItem};

use super::*;
use crate::{CancelToken, FakeFs};

fn tree() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("src/a/x.txt", 3);
    fs.add_file("src/a/b/y.txt", 4);
    fs.add_folder("src/a/empty");
    fs.add_file("src/top.txt", 5);
    fs.add_folder("dst/a");
    fs.add_file("dst/a/x.txt", 1);
    fs.add_file("dst/top (2).txt", 1);
    fs
}

fn shape(items: &[ScannedItem]) -> Vec<(String, u32, ItemKind, u64)> {
    items
        .iter()
        .map(|i| {
            (
                i.location.as_str().to_owned(),
                i.depth,
                i.kind.clone(),
                i.size,
            )
        })
        .collect()
}

#[test]
fn a_folder_is_listed_before_its_children_with_depths() {
    let fs = tree();
    let items = scan(&fs, &[fs.location("src/a")], &CancelToken::new()).unwrap();
    assert_eq!(
        shape(&items),
        [
            ("fake://src/a".to_owned(), 0, ItemKind::Folder, 0),
            ("fake://src/a/b".to_owned(), 1, ItemKind::Folder, 0),
            ("fake://src/a/b/y.txt".to_owned(), 2, ItemKind::File, 4),
            ("fake://src/a/empty".to_owned(), 1, ItemKind::Folder, 0),
            ("fake://src/a/x.txt".to_owned(), 1, ItemKind::File, 3),
        ]
    );
}

#[test]
fn several_sources_keep_their_order() {
    let fs = tree();
    let sources = [fs.location("src/top.txt"), fs.location("src/a/x.txt")];
    let items = scan(&fs, &sources, &CancelToken::new()).unwrap();
    assert_eq!(items.iter().map(|i| i.depth).collect::<Vec<_>>(), [0, 0]);
    assert_eq!(items[0].location, sources[0]);
}

#[test]
fn a_missing_source_is_an_error() {
    let fs = tree();
    let result = scan(&fs, &[fs.location("src/nope")], &CancelToken::new());
    assert_eq!(
        result.map_err(|e| e.kind).err(),
        Some(FsErrorKind::NotFound)
    );
}

#[test]
fn a_cancelled_scan_stops() {
    let fs = tree();
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = scan(&fs, &[fs.location("src/a")], &cancel);
    assert_eq!(
        result.map_err(|e| e.kind).err(),
        Some(FsErrorKind::Cancelled)
    );
}

#[test]
fn existing_lists_every_target_folder_that_exists() {
    let fs = tree();
    let items = scan(
        &fs,
        &[fs.location("src/a"), fs.location("src/top.txt")],
        &CancelToken::new(),
    )
    .unwrap();
    let existing = existing_at(&fs, &items, &fs.location("dst"), &CancelToken::new()).unwrap();
    let mut names: Vec<String> = existing
        .iter()
        .map(|e| e.location.as_str().to_owned())
        .collect();
    names.sort();
    assert_eq!(
        names,
        [
            "fake://dst/a",
            "fake://dst/a/x.txt",
            "fake://dst/top (2).txt"
        ]
    );
}

#[test]
fn a_missing_target_folder_has_nothing_existing() {
    let fs = tree();
    let items = scan(&fs, &[fs.location("src/top.txt")], &CancelToken::new()).unwrap();
    let existing = existing_at(&fs, &items, &fs.location("nowhere"), &CancelToken::new()).unwrap();
    assert!(existing.is_empty());
}
