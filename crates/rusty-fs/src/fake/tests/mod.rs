use std::ops::ControlFlow;

use rusty_core::{FsError, FsErrorKind, Location, ProviderId, ProviderPath};

use super::*;
use crate::{Attributes, CancelToken, EntryBatch, EntryKind, FsEntry, Provider};

mod concurrency;
mod faults;
mod listing;
mod ops;
mod props;
mod tree;
mod watch;

fn list_all(fs: &FakeFs, path: &str) -> (Result<(), FsError>, Vec<EntryBatch>) {
    list_at(fs, &fs.location(path))
}

fn list_at(fs: &FakeFs, location: &Location) -> (Result<(), FsError>, Vec<EntryBatch>) {
    let mut batches = Vec::new();
    let mut sink = |batch: EntryBatch| {
        batches.push(batch);
        ControlFlow::Continue(())
    };
    let result = fs.list(location, &mut sink, &CancelToken::new());
    (result, batches)
}

fn names(batches: &[EntryBatch]) -> Vec<String> {
    batches
        .iter()
        .flat_map(|b| &b.entries)
        .map(|e| e.name.to_text())
        .collect()
}

fn shape(batches: &[EntryBatch]) -> Vec<(usize, bool)> {
    batches.iter().map(|b| (b.entries.len(), b.last)).collect()
}

fn kind_of<T>(result: Result<T, FsError>) -> Option<FsErrorKind> {
    result.err().map(|e| e.kind)
}

fn file_entry(name: ProviderPath) -> FsEntry {
    FsEntry {
        name,
        kind: EntryKind::File,
        size: 1,
        modified_ms: None,
        attributes: Attributes::default(),
    }
}

fn with_files(count: usize) -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("d");
    for i in 0..count {
        fs.add_file(&format!("d/f{i:02}"), 1);
    }
    fs
}

fn file_names(count: usize) -> Vec<String> {
    (0..count).map(|i| format!("f{i:02}")).collect()
}

#[test]
fn provider_id_is_fake() {
    assert_eq!(FakeFs::new().id(), ProviderId::new("fake").unwrap());
}

#[test]
fn every_capability_but_watching_is_on() {
    let all = Capabilities {
        write: true,
        watch: false,
        recycle: true,
        rename: true,
    };
    assert_eq!(FakeFs::new().capabilities(), all);
}

#[test]
fn watching_is_on_when_asked_for() {
    assert!(FakeFs::new().with_watch().capabilities().watch);
}

#[test]
fn location_text_has_the_fake_scheme() {
    assert_eq!(FakeFs::new().location("a/b").as_str(), "fake://a/b");
}

#[test]
fn leading_slash_names_the_same_location() {
    let fs = FakeFs::new();
    assert_eq!(fs.location("/a/b"), fs.location("a/b"));
}

#[test]
fn empty_path_is_the_root() {
    assert_eq!(FakeFs::new().location("").as_str(), "fake:///");
}

#[test]
fn root_location_path_is_a_slash() {
    assert_eq!(FakeFs::new().location("").path().as_str(), Some("/"));
}

#[test]
fn cancel_token_starts_clear() {
    assert!(!CancelToken::new().is_cancelled());
}

#[test]
fn cancel_is_seen_by_clones() {
    let token = CancelToken::new();
    let clone = token.clone();
    token.cancel();
    assert!(clone.is_cancelled());
}
