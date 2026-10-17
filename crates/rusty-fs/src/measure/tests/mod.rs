use rusty_core::{FsErrorKind, ProviderPath};

use super::*;
use crate::{Attributes, CancelToken, EntryKind, FakeFs, FakeOp, Fault, FsEntry};

fn tree() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_file("d/a.bin", 100);
    fs.add_file("d/sub/b.bin", 20);
    fs.add_file("d/sub/deeper/c.bin", 3);
    fs.add_folder("d/empty");
    fs
}

fn measure(fs: &FakeFs, path: &str) -> Result<FolderSize, rusty_core::FsError> {
    folder_size(fs, &fs.location(path), &CancelToken::new())
}

#[test]
fn sizes_add_up_through_every_level() {
    let size = measure(&tree(), "d").unwrap();
    assert_eq!((size.bytes, size.files, size.folders), (123, 3, 3));
}

#[test]
fn an_empty_folder_is_zero() {
    let size = measure(&tree(), "d/empty").unwrap();
    assert_eq!((size.bytes, size.files, size.folders), (0, 0, 0));
}

#[test]
fn links_are_counted_but_not_followed() {
    let fs = tree();
    fs.add_entry(
        "d",
        FsEntry {
            name: ProviderPath::from_text("loop"),
            kind: EntryKind::Link,
            size: 0,
            modified_ms: None,
            attributes: Attributes::default(),
        },
    );
    let size = measure(&fs, "d").unwrap();
    assert_eq!((size.bytes, size.files), (123, 4));
}

#[test]
fn an_unreadable_subfolder_is_skipped_and_counted() {
    let fs = tree();
    fs.fault(
        "d/sub",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let size = measure(&fs, "d").unwrap();
    assert_eq!((size.bytes, size.skipped), (100, 1));
}

#[test]
fn an_unreadable_top_folder_is_an_error() {
    let fs = tree();
    fs.fault("d", FakeOp::List, 1, Fault::Fail(FsErrorKind::AccessDenied));
    assert_eq!(
        measure(&fs, "d").map_err(|e| e.kind).err(),
        Some(FsErrorKind::AccessDenied)
    );
}

#[test]
fn cancelling_stops_the_walk() {
    let fs = tree();
    let cancel = CancelToken::new();
    cancel.cancel();
    let result = folder_size(&fs, &fs.location("d"), &cancel);
    assert_eq!(
        result.map_err(|e| e.kind).err(),
        Some(FsErrorKind::Cancelled)
    );
}

mod profile;
mod props;
mod ranking;
mod runner;
