//! Hit storage tests for the find results window.

use navo_core::find::{FileHits, LineHit, Skip};
use navo_core::{EntryKind, ProviderPath};
use navo_fs::find::{FindProgress, FoundBatch, FoundFile};
use navo_fs::{Attributes, FsEntry};

use crate::find_results::store::{HitStore, LINE_CAP, StoreError, StoreStop};

fn entry(name: &str) -> FsEntry {
    FsEntry {
        name: ProviderPath::from_text(name),
        kind: EntryKind::File,
        size: 42,
        modified_ms: Some(1_700_000_000_000),
        attributes: Attributes::default(),
    }
}

fn line(text: &str, number: u64) -> LineHit {
    LineHit {
        line: number,
        column: 1,
        text: text.to_owned(),
    }
}

fn file(relative: &str, lines: Vec<LineHit>) -> FoundFile {
    let total = u32::try_from(lines.len()).unwrap_or(u32::MAX);
    FoundFile {
        relative: ProviderPath::from_text(relative),
        entry: entry(relative),
        hits: FileHits {
            hits: lines,
            total,
            skipped: None,
        },
    }
}

fn batch(files: Vec<FoundFile>) -> FoundBatch {
    FoundBatch {
        files,
        progress: FindProgress::default(),
    }
}

fn fill(store: &mut HitStore, lines_per_file: usize) {
    for folder in 0..40 {
        let relative = format!("src/folder{folder:02}/file.rs");
        let hits = (0..lines_per_file)
            .map(|number| line("a matched line that is long enough", number as u64))
            .collect();
        store
            .push(0, &batch(vec![file(&relative, hits)]))
            .expect("a small batch fits");
    }
}

#[test]
fn hundred_thousand_hits_stay_under_the_budget() {
    let mut store = HitStore::default();
    for folder in 0..50 {
        let files: Vec<FoundFile> = (0..20)
            .map(|index| {
                let relative = format!("src/folder{folder:02}/file{index:02}.rs");
                let hits = (0..100)
                    .map(|number| line("needle in a haystack", number))
                    .collect();
                file(&relative, hits)
            })
            .collect();
        store.push(0, &batch(files)).expect("a small batch fits");
    }
    assert_eq!(store.file_count(), 1_000);
    assert_eq!(store.folder_count(), 50);
    assert_eq!(store.hit_total(), 100_000);
    let memory = store.memory_bytes();
    assert!(
        memory < 25 * 1024 * 1024,
        "100,000 hits used {memory} bytes"
    );
}

#[test]
fn keeps_exactly_two_hundred_lines_at_the_cap() {
    let mut store = HitStore::default();
    let hits = (0..200)
        .map(|number| line("line", number))
        .collect::<Vec<_>>();
    store
        .push(0, &batch(vec![file("big.txt", hits)]))
        .expect("a small batch fits");

    let file = store.file(0).expect("the file was stored");
    assert_eq!(LINE_CAP, 200);
    assert_eq!(file.hits, 200);
    assert_eq!(file.lines().count(), 200);
}

#[test]
fn keeps_two_hundred_of_two_hundred_and_one_lines() {
    let mut store = HitStore::default();
    let hits = (0..201)
        .map(|number| line("line", number))
        .collect::<Vec<_>>();
    store
        .push(0, &batch(vec![file("big.txt", hits)]))
        .expect("a small batch fits");

    let file = store.file(0).expect("the file was stored");
    assert_eq!(file.hits, 201);
    assert_eq!(file.lines().count(), 200);
    assert_eq!(file.lines().next().map(|line| line.text), Some("line"));
}

#[test]
fn trim_keeps_three_lines_and_the_finished_index_frees_memory() {
    let mut store = HitStore::default();
    fill(&mut store, 10);
    let before = store.memory_bytes();
    store.trim_lines(3).expect("trim cannot overflow");
    let trimmed = store.memory_bytes();
    assert!(trimmed < before, "{trimmed} should be under {before}");
    assert_eq!(
        store.file(0).expect("the file was stored").lines().count(),
        3
    );

    store.finish_index();
    let finished = store.memory_bytes();
    assert!(finished < trimmed, "{finished} should be under {trimmed}");
}

#[test]
fn trimming_frees_the_arena() {
    let mut heavy = HitStore::default();
    fill(&mut heavy, 200);
    let before = heavy.memory_bytes();
    heavy.trim_lines(3).expect("trim cannot overflow");
    let after = heavy.memory_bytes();

    let mut kept = HitStore::default();
    fill(&mut kept, 3);
    let reference = kept.memory_bytes();

    assert!(after < before, "{after} should be under {before}");
    assert!(
        after <= reference * 2,
        "trimmed store keeps {after} bytes for about {reference} of data"
    );
}

#[test]
fn archive_parents_mark_the_file() {
    let mut store = HitStore::default();
    store
        .push(
            0,
            &batch(vec![
                file("src/main.rs", vec![line("x", 1)]),
                file("drop/archive.zip/inner/readme.txt", vec![line("x", 1)]),
                file("archive.zip", vec![line("x", 1)]),
            ]),
        )
        .expect("a small batch fits");

    assert!(!store.file(0).expect("the file was stored").in_archive);
    assert!(store.file(1).expect("the file was stored").in_archive);
    assert!(!store.file(2).expect("the file was stored").in_archive);
}

#[cfg(not(windows))]
#[test]
fn a_backslash_is_part_of_a_unix_name() {
    let mut store = HitStore::default();
    store
        .push(
            0,
            &batch(vec![file("backup.zip\\notes.txt", vec![line("x", 1)])]),
        )
        .expect("a small batch fits");

    let file = store.file(0).expect("the file was stored");
    assert_eq!(file.folder, "");
    assert_eq!(file.name, "backup.zip\\notes.txt");
    assert!(!file.in_archive);
}

#[test]
fn two_roots_keep_the_same_relative_path_apart() {
    let mut store = HitStore::default();
    store
        .push(0, &batch(vec![file("same/a.txt", vec![line("x", 1)])]))
        .expect("a small batch fits");
    store
        .push(1, &batch(vec![file("same/a.txt", vec![line("y", 1)])]))
        .expect("a small batch fits");

    let first = store.file(0).expect("the file was stored");
    let second = store.file(1).expect("the file was stored");
    assert_eq!(store.folder_count(), 2);
    assert_eq!(first.folder, second.folder);
    assert_ne!(first.folder_id, second.folder_id);
    assert_eq!(
        store
            .folder(first.folder_id)
            .expect("the folder exists")
            .root,
        0
    );
    assert_eq!(
        store
            .folder(second.folder_id)
            .expect("the folder exists")
            .root,
        1
    );
}

#[test]
fn empty_strings_round_trip() {
    let mut store = HitStore::default();
    store
        .push(0, &batch(vec![file("", vec![line("", 0)])]))
        .expect("a small batch fits");

    let file = store.file(0).expect("the file was stored");
    assert_eq!(file.folder, "");
    assert_eq!(file.name, "");
    assert_eq!(file.folder_id, 0);
    assert_eq!(file.lines().next().map(|line| line.text), Some(""));
}

#[test]
fn utf8_names_and_lines_round_trip() {
    let mut store = HitStore::default();
    store
        .push(
            0,
            &batch(vec![file("dünya/日本語.txt", vec![line("héllo 🌍", 7)])]),
        )
        .expect("a small batch fits");

    let file = store.file(0).expect("the file was stored");
    assert_eq!(file.folder, "dünya");
    assert_eq!(file.name, "日本語.txt");
    let line = file.lines().next().expect("one line was stored");
    assert_eq!(line.text, "héllo 🌍");
    assert_eq!(line.line, 7);
}

#[test]
fn offset_overflow_fails_the_whole_batch() {
    let mut store = HitStore::with_limit(12);
    let error = store
        .push(
            0,
            &batch(vec![
                file("a.txt", vec![line("x", 1)]),
                file("toolongname.txt", vec![line("x", 1)]),
            ]),
        )
        .expect_err("the batch passes the injected ceiling");

    assert_eq!(error, StoreError::OffsetFull);
    assert_eq!(store.file_count(), 0);
    assert_eq!(store.folder_count(), 0);
    assert_eq!(store.stopped(), Some(StoreStop::Offsets));
}

#[test]
fn stop_lines_keeps_the_hit_count() {
    let mut store = HitStore::default();
    store
        .push(0, &batch(vec![file("a.txt", vec![line("one", 1)])]))
        .expect("a small batch fits");
    store.stop_lines();
    store
        .push(0, &batch(vec![file("b.txt", vec![line("two", 1)])]))
        .expect("a small batch fits");

    assert_eq!(store.file(1).expect("the file was stored").hits, 1);
    assert_eq!(
        store.file(1).expect("the file was stored").lines().count(),
        0
    );
    assert_eq!(store.hit_total(), 2);
}

#[test]
fn skipped_files_are_reported() {
    let mut store = HitStore::default();
    let mut skipped = file("blob.bin", Vec::new());
    skipped.hits.skipped = Some(Skip::Binary);
    store
        .push(0, &batch(vec![skipped]))
        .expect("a small batch fits");

    assert!(store.file(0).expect("the file was stored").skipped);
    assert_eq!(store.file(0).expect("the file was stored").hits, 0);
}
