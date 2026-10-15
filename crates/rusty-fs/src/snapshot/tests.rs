use rusty_core::{FsErrorKind, ProviderPath};

use super::*;
use crate::{Attributes, EntryKind, FakeFs, FakeOp, Fault, FsEntry};

fn names(snapshot: &FolderSnapshot) -> Vec<String> {
    snapshot
        .listing
        .entries()
        .iter()
        .map(|e| e.name.clone())
        .collect()
}

fn load(fs: &FakeFs, path: &str) -> FolderSnapshot {
    load_folder(fs, &fs.location(path), &CancelToken::new()).unwrap()
}

#[test]
fn folders_come_first_then_names_without_case() {
    let fs = FakeFs::new();
    fs.add_file("d/beta.txt", 1);
    fs.add_folder("d/Zeta");
    fs.add_file("d/Alpha.txt", 1);
    fs.add_folder("d/apps");
    assert_eq!(
        names(&load(&fs, "d")),
        ["apps", "Zeta", "Alpha.txt", "beta.txt"]
    );
}

#[test]
fn details_line_up_with_the_listing_rows() {
    let fs = FakeFs::new();
    fs.add_file("d/b.txt", 20);
    fs.add_file("d/a.txt", 10);
    let snapshot = load(&fs, "d");
    let sizes: Vec<u64> = snapshot.details.iter().map(|e| e.size).collect();
    assert_eq!(sizes, [10, 20]);
}

#[test]
fn listing_marks_folders() {
    let fs = FakeFs::new();
    fs.add_folder("d/sub");
    fs.add_file("d/f.txt", 1);
    let snapshot = load(&fs, "d");
    let folders: Vec<bool> = snapshot
        .listing
        .entries()
        .iter()
        .map(|e| e.is_folder)
        .collect();
    assert_eq!(folders, [true, false]);
}

#[test]
fn every_batch_is_collected() {
    let fs = FakeFs::new().with_batch_size(3);
    for i in 0..10 {
        fs.add_file(&format!("d/f{i:02}"), 1);
    }
    assert_eq!(load(&fs, "d").listing.len(), 10);
}

#[test]
fn listing_names_are_the_lossless_text_form() {
    let fs = FakeFs::new();
    fs.add_folder("d");
    let odd = ProviderPath::from_wide(&[0x61, 0xD800]);
    fs.add_entry(
        "d",
        FsEntry {
            name: odd.clone(),
            kind: EntryKind::File,
            size: 0,
            modified_ms: None,
            attributes: Attributes::default(),
        },
    );
    let snapshot = load(&fs, "d");
    assert_eq!(snapshot.listing.entries()[0].name, odd.to_text());
    assert_eq!(snapshot.details[0].name, odd);
}

#[test]
fn errors_pass_through() {
    let fs = FakeFs::new();
    fs.add_folder("d");
    fs.fault("d", FakeOp::List, 1, Fault::Fail(FsErrorKind::AccessDenied));
    let result = load_folder(&fs, &fs.location("d"), &CancelToken::new());
    assert_eq!(
        result.map(|_| ()).map_err(|e| e.kind),
        Err(FsErrorKind::AccessDenied)
    );
}

#[test]
fn a_cancelled_load_is_cancelled() {
    let fs = FakeFs::new();
    fs.add_file("d/a", 1);
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = load_folder(&fs, &fs.location("d"), &cancel);
    assert_eq!(
        result.map(|_| ()).map_err(|e| e.kind),
        Err(FsErrorKind::Cancelled)
    );
}

#[test]
fn a_link_sorts_with_files() {
    let fs = FakeFs::new();
    fs.add_folder("d");
    fs.add_folder("d/zz");
    fs.add_entry(
        "d",
        FsEntry {
            name: ProviderPath::from_text("aa"),
            kind: EntryKind::Link,
            size: 0,
            modified_ms: None,
            attributes: Attributes::default(),
        },
    );
    assert_eq!(names(&load(&fs, "d")), ["zz", "aa"]);
}
