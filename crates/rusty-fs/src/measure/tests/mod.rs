use rusty_core::{FsErrorKind, ProviderPath};

use super::*;
use crate::{Attributes, CancelToken, EntryKind, FakeFs, FakeOp, Fault, FsEntry, LinkKey};

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
    assert_eq!(size.on_disk, 0);
    assert!(size.on_disk_known);
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

#[test]
fn hard_links_across_folders_count_once_per_walk() {
    let fs = FakeFs::new();
    for path in ["d/a.bin", "d/sub/b.bin"] {
        fs.add_file(path, 100);
        fs.set_on_disk(path, 64);
        fs.set_link(path, LinkKey(1, 2));
    }
    for _ in 0..2 {
        let size = measure(&fs, "d").unwrap();
        assert_eq!((size.bytes, size.on_disk, size.files), (100, 64, 1));
        assert_eq!(size.links_merged, 1);
        assert!(size.on_disk_known);
        assert!(!size.links_unchecked);
    }
}

#[test]
fn unknown_on_disk_sizes_fall_back_to_apparent_sizes() {
    let fs = tree();
    fs.set_on_disk("d/a.bin", 64);
    let size = measure(&fs, "d").unwrap();
    assert_eq!(size.on_disk, 87);
    assert!(!size.on_disk_known);
}

#[test]
fn known_on_disk_sizes_are_summed_including_zero() {
    let fs = tree();
    fs.set_on_disk("d/a.bin", 64);
    fs.set_on_disk("d/sub/b.bin", 32);
    fs.set_on_disk("d/sub/deeper/c.bin", 0);
    let size = measure(&fs, "d").unwrap();
    assert_eq!(size.on_disk, 96);
    assert!(size.on_disk_known);
}

#[test]
fn folders_are_never_link_merged() {
    let fs = tree();
    for path in ["d/sub", "d/sub/deeper", "d/empty"] {
        fs.set_link(path, LinkKey(1, 2));
    }
    let size = measure(&fs, "d").unwrap();
    assert_eq!((size.bytes, size.files, size.folders), (123, 3, 3));
    assert_eq!(size.links_merged, 0);
}

#[test]
fn reaching_the_link_cap_stops_merging_and_marks_the_result() {
    let fs = FakeFs::new();
    for (name, key) in [('a', 1), ('b', 1), ('c', 2), ('d', 1), ('e', 3), ('f', 3)] {
        let path = format!("d/{name}");
        fs.add_file(&path, 10);
        fs.set_on_disk(&path, 4);
        fs.set_link(&path, LinkKey(key, 1));
    }
    let mut size = FolderSize::default();
    let root = fs.location("d");
    let cancel = CancelToken::new();
    Walk {
        provider: &fs,
        root: &root,
        cancel: &cancel,
    }
    .run_with_link_limit(&mut size, &mut SizeOnly, 2)
    .unwrap();
    assert_eq!((size.bytes, size.on_disk, size.files), (50, 20, 5));
    assert_eq!(size.links_merged, 1);
    assert!(size.links_unchecked);
}

#[test]
fn cancellation_during_size_streaming_stops_before_the_next_entry() {
    struct Stop<'a>(&'a CancelToken);

    impl Visit for Stop<'_> {
        fn needs_fs_entries(&self) -> bool {
            false
        }

        fn file(&mut self, _: &Location, _: &FsEntry) {}

        fn batch_end(&mut self, _: &FolderSize) {
            self.0.cancel();
        }
    }

    let fs = tree();
    let root = fs.location("d");
    let cancel = CancelToken::new();
    let mut totals = FolderSize::default();
    let result = Walk {
        provider: &fs,
        root: &root,
        cancel: &cancel,
    }
    .run(&mut totals, &mut Stop(&cancel));
    assert_eq!(result.unwrap_err().kind, FsErrorKind::Cancelled);
    assert_eq!(totals.files + totals.folders, 1);
    assert_eq!(fs.calls("d/sub", FakeOp::List), 0);
}

mod profile;
mod props;
mod ranking;
mod runner;
