//! A scripted fake benchmark must leave the folder equal; a writing one must be caught.

use std::io::Read;

use super::mutate;
use super::*;
use crate::fixture;
use crate::source::{EntryKind, WalkConfig};

/// Lists and reads every file through the read-only source, like the real tool.
fn read_only_benchmark(source: &ReadOnlySource) -> u64 {
    let mut ids = Vec::new();
    source.walk(WalkConfig::default(), |record| {
        if record.kind == EntryKind::File {
            ids.push(record.id);
        }
    });
    let mut total = 0;
    for id in ids {
        if let Ok(mut file) = source.open_read(&id) {
            let mut bytes = Vec::new();
            total += file.read_to_end(&mut bytes).unwrap_or(0) as u64;
        }
    }
    total
}

#[test]
fn read_only_benchmark_leaves_the_folder_equal() {
    let fx = fixture::set().unwrap();
    let source = crate::source::test_open(fx.root()).unwrap();
    let before = Manifest::capture(&source);
    assert!(read_only_benchmark(&source) > 0);
    assert!(read_only_benchmark(&source) > 0);
    let after = Manifest::capture(&source);
    let diff = Manifest::diff(&before, &after);
    assert_eq!(diff.verdict, Verdict::Equal);
    assert_eq!(before.digest_hex(), after.digest_hex());
}

#[test]
fn raw_writes_by_a_fake_benchmark_are_caught() {
    type Write = fn(&fixture::Fixture) -> std::io::Result<()>;
    let writes: [(&str, Write); 3] = [
        ("new file", |fx| {
            mutate::add_file(&fx.root().join("bench.tmp"))
        }),
        ("append", |fx| mutate::append(&fx.privacy_file)),
        ("delete", |fx| mutate::remove_file(&fx.rare_file)),
    ];
    for (label, write) in writes {
        let fx = fixture::set().unwrap();
        let source = crate::source::test_open(fx.root()).unwrap();
        let before = Manifest::capture(&source);
        read_only_benchmark(&source);
        write(&fx).unwrap();
        let diff = Manifest::diff(&before, &Manifest::capture(&source));
        assert_eq!(diff.verdict, Verdict::Changed, "{label}");
    }
}
