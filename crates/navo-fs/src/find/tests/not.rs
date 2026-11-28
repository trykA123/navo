use std::fs::File;

use tempfile::TempDir;

use super::archives::zip_bytes;
use super::{names, search, write};

#[test]
fn not_lists_only_plain_files_without_the_text_and_skips_binary() {
    let tree = TempDir::new().unwrap();
    write(&tree, "has.txt", b"prefix needle suffix\n");
    write(&tree, "plain.txt", b"nothing matching here\n");
    write(&tree, "binary.bin", b"no match\0binary data");

    let batches = search(&tree, "needle not:on");
    assert_eq!(names(&batches), ["plain.txt"]);

    let found: Vec<_> = batches.iter().flat_map(|batch| &batch.files).collect();
    assert_eq!(found.len(), 1);
    assert!(found[0].hits.hits.is_empty());
    assert_eq!(found[0].hits.total, 0);
    assert_eq!(found[0].hits.skipped, None);

    let progress = &batches.last().unwrap().progress;
    assert_eq!(
        (
            progress.scanned,
            progress.matched,
            progress.hits,
            progress.skipped
        ),
        (3, 1, 0, 1)
    );
}

#[test]
fn not_applies_to_archive_entries_and_skips_binary_entries() {
    let tree = TempDir::new().unwrap();
    write(&tree, "empty.txt", b"");
    write(
        &tree,
        "pack.zip",
        zip_bytes(&[
            ("inner/has.txt", b"needle"),
            ("inner/clean.txt", b"other text"),
            ("inner/raw.bin", b"\0\x01\x02"),
        ]),
    );

    let batches = search(&tree, "needle not:on arc:on");
    let mut matched = names(&batches);
    matched.sort();
    assert_eq!(matched, ["empty.txt", "pack.zip\\inner\\clean.txt"]);

    for file in batches.iter().flat_map(|batch| &batch.files) {
        assert!(file.hits.hits.is_empty());
        assert_eq!(file.hits.total, 0);
        assert_eq!(file.hits.skipped, None);
    }

    let progress = &batches.last().unwrap().progress;
    assert_eq!(
        (progress.matched, progress.hits, progress.skipped),
        (2, 0, 2)
    );
}

#[test]
fn not_excludes_oversized_skipped_files() {
    let tree = TempDir::new().unwrap();
    write(&tree, "small.txt", b"clean");
    write(&tree, "oversized.txt", b"clean");
    File::options()
        .write(true)
        .open(tree.path().join("oversized.txt"))
        .unwrap()
        .set_len(64 * 1024 * 1024 + 1)
        .unwrap();

    let batches = search(&tree, "needle not:on");
    assert_eq!(names(&batches), ["small.txt"]);
    let progress = &batches.last().unwrap().progress;
    assert_eq!((progress.matched, progress.skipped), (1, 1));
}
