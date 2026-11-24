//! Hit storage tests for the find results window.

use navo_core::find::{FileHits, LineHit, Skip};
use navo_core::{EntryKind, ProviderPath};
use navo_fs::find::{FindProgress, FoundBatch, FoundFile};
use navo_fs::{Attributes, FsEntry};

use crate::find_results::store::{HitStore, LINE_CAP};

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
        store.push(0, &batch(files));
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
fn keeps_at_most_two_hundred_lines_per_file() {
    let mut store = HitStore::default();
    let hits: Vec<LineHit> = (0..250)
        .map(|number| line(&format!("line {number}"), number))
        .collect();
    store.push(0, &batch(vec![file("big.txt", hits)]));

    let file = store.file(0);
    assert_eq!(LINE_CAP, 200);
    assert_eq!(file.hits, 250);
    assert_eq!(file.lines().count(), 200);
    assert_eq!(file.lines().next().map(|line| line.text), Some("line 0"));
}

#[test]
fn trim_keeps_three_lines_and_the_finished_index_frees_memory() {
    let mut store = HitStore::default();
    for folder in 0..60 {
        let relative = format!("src/folder{folder:02}/file.rs");
        let hits = (0..10)
            .map(|number| line("a matched line", number))
            .collect();
        store.push(0, &batch(vec![file(&relative, hits)]));
    }
    let before = store.memory_bytes();
    store.trim_lines(3);
    let trimmed = store.memory_bytes();
    assert!(trimmed < before, "{trimmed} should be under {before}");
    assert_eq!(store.file(0).lines().count(), 3);

    store.finish_index();
    let finished = store.memory_bytes();
    assert!(finished < trimmed, "{finished} should be under {trimmed}");
}

#[test]
fn archive_parents_mark_the_file() {
    let mut store = HitStore::default();
    store.push(
        0,
        &batch(vec![
            file("src/main.rs", vec![line("x", 1)]),
            file("drop/archive.zip/inner/readme.txt", vec![line("x", 1)]),
            file("drop\\nested.tar.gz\\readme.txt", vec![line("x", 1)]),
            file("archive.zip", vec![line("x", 1)]),
        ]),
    );

    assert!(!store.file(0).in_archive);
    assert!(store.file(1).in_archive);
    assert!(store.file(2).in_archive);
    assert!(!store.file(3).in_archive);
}

#[test]
fn stop_lines_keeps_the_hit_count() {
    let mut store = HitStore::default();
    store.push(0, &batch(vec![file("a.txt", vec![line("one", 1)])]));
    store.stop_lines();
    store.push(0, &batch(vec![file("b.txt", vec![line("two", 1)])]));

    assert_eq!(store.file(1).hits, 1);
    assert_eq!(store.file(1).lines().count(), 0);
    assert_eq!(store.hit_total(), 2);
}

#[test]
fn skipped_files_are_reported() {
    let mut store = HitStore::default();
    let mut skipped = file("blob.bin", Vec::new());
    skipped.hits.skipped = Some(Skip::Binary);
    store.push(0, &batch(vec![skipped]));

    assert!(store.file(0).skipped);
    assert_eq!(store.file(0).hits, 0);
}
