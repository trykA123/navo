//! The large diff's spool folder is removed when the control drops.

use std::fs;
use std::path::Path;
use std::thread;
use std::time::Instant;

use crate::compare::diff::large::LargeDiff;

use super::large::{WAIT, pair, request, sink};

#[test]
fn dropping_a_large_diff_removes_its_spool_folder() {
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join("spool");
    let mut pair = pair(dir.path(), 1000, 10);
    pair.sources = crate::compare::diff::sources::Sources::new(Some(spool.clone()));
    assert!(
        has_process_folder(&spool),
        "the spool owner created its folder"
    );
    let (sink, _receive) = sink();
    let control = LargeDiff::start(request(), pair, sink);
    drop(control);
    let deadline = Instant::now() + WAIT;
    while has_process_folder(&spool) {
        assert!(
            Instant::now() < deadline,
            "the spool folder outlived the large diff"
        );
        thread::yield_now();
    }
}

/// True when `base` holds a per-process spool folder (the gate file is not one).
fn has_process_folder(base: &Path) -> bool {
    fs::read_dir(base).is_ok_and(|entries| {
        entries
            .flatten()
            .any(|entry| entry.file_type().is_ok_and(|kind| kind.is_dir()))
    })
}
