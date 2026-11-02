use std::io::{Cursor, Write};

use tempfile::TempDir;
use zip::write::SimpleFileOptions;

use super::{names, search, write};

fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
    for (name, bytes) in entries {
        writer
            .start_file(*name, SimpleFileOptions::default())
            .unwrap();
        writer.write_all(bytes).unwrap();
    }
    writer.finish().unwrap().into_inner()
}

#[test]
fn zip_hits_match_entry_filters_and_keep_archive_paths() {
    let tree = TempDir::new().unwrap();
    write(
        &tree,
        "archive.zip",
        zip_bytes(&[("inner/file.c", b"needle\n"), ("other.txt", b"needle")]),
    );
    let batches = search(&tree, "needle ext:c size:<10 arc:on");
    assert_eq!(names(&batches), ["archive.zip\\inner\\file.c"]);
    let file = &batches
        .iter()
        .flat_map(|batch| &batch.files)
        .next()
        .unwrap();
    assert_eq!(file.entry.name.to_text(), "file.c");
    assert_eq!(file.hits.total, 1);
    assert_eq!(file.hits.hits[0].line, 1);
    assert_eq!(
        names(&search(&tree, "ext:c arc:on")),
        ["archive.zip\\inner\\file.c"]
    );
}

#[test]
fn arc_off_does_not_search_zip_entries() {
    let tree = TempDir::new().unwrap();
    write(&tree, "archive.zip", zip_bytes(&[("file.c", b"needle")]));
    assert!(names(&search(&tree, "needle ext:c arc:off")).is_empty());
}

#[test]
fn broken_archive_is_skipped_without_failing_search() {
    let tree = TempDir::new().unwrap();
    write(&tree, "broken.zip", b"PK\x03\x04broken");
    let batches = search(&tree, "needle arc:on");
    assert!(names(&batches).is_empty());
    assert_eq!(batches.last().unwrap().progress.skipped, 1);
}

#[test]
fn nested_archives_are_not_entered() {
    let tree = TempDir::new().unwrap();
    let nested = zip_bytes(&[("deep.c", b"needle")]);
    write(&tree, "archive.zip", zip_bytes(&[("nested.zip", &nested)]));
    let batches = search(&tree, "needle ext:c arc:on");
    assert!(names(&batches).is_empty());
    assert_eq!(batches.last().unwrap().progress.scanned, 2);
}

#[test]
fn the_archive_itself_still_matches_by_name() {
    let tree = TempDir::new().unwrap();
    write(&tree, "archive.zip", zip_bytes(&[("file.c", b"needle")]));
    assert_eq!(names(&search(&tree, "ext:zip arc:on")), ["archive.zip"]);
}
