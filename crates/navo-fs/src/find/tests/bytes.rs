use tempfile::TempDir;

use super::archives::zip_bytes;
use super::{names, search, write};

#[test]
fn hex_matches_only_binary_file_with_those_bytes() {
    let tree = TempDir::new().unwrap();
    write(&tree, "exe.bin", b"\x4D\x5A\x90\x00\x03\x00binary");
    write(&tree, "other.bin", b"\x00\x01\x02\x03other");
    write(&tree, "plain.txt", b"4D5A text without matching bytes\n");

    let batches = search(&tree, "hex:4D5A");
    assert_eq!(names(&batches), ["exe.bin"]);

    let found: Vec<_> = batches.iter().flat_map(|batch| &batch.files).collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].hits.total, 1);
    assert_eq!(found[0].hits.skipped, None);
    assert_eq!(found[0].hits.hits[0].line, 1);
    assert_eq!(found[0].hits.hits[0].column, 1);
    assert!(
        found[0].hits.hits[0]
            .text
            .starts_with("00000000  4D 5A 90 00")
    );

    let progress = &batches.last().unwrap().progress;
    assert_eq!(
        (
            progress.scanned,
            progress.matched,
            progress.hits,
            progress.skipped
        ),
        (3, 1, 1, 0)
    );
}

#[test]
fn hex_scans_archive_entries_when_arc_is_on() {
    let tree = TempDir::new().unwrap();
    write(&tree, "plain.txt", b"no match");
    write(
        &tree,
        "pack.zip",
        zip_bytes(&[
            ("inner/prog.bin", b"\x00\x00\x4D\x5A\x00"),
            ("inner/miss.bin", b"\x00\x01\x02\x03"),
        ]),
    );

    let batches = search(&tree, "hex:4D5A ext:bin arc:on");
    assert_eq!(names(&batches), ["pack.zip\\inner\\prog.bin"]);

    let found: Vec<_> = batches.iter().flat_map(|batch| &batch.files).collect();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0].hits.total, 1);
    assert_eq!(found[0].hits.hits[0].line, 1);
    assert_eq!(found[0].hits.hits[0].column, 3);
    assert_eq!(found[0].hits.hits[0].text, "00000000  00 00 4D 5A 00");
}
