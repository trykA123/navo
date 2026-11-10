//! Tests that a side the loader cannot hash never gets a Viewed key (VR1 finding 5).

use std::fs;
use std::path::Path;
use std::sync::Arc;

use navo_compare::model::{CancelToken, EntryKind, Reason, RelPath, SideEntry};
use navo_compare::text::Options;
use navo_compare::text::side::WHOLE_LIMIT;
use navo_core::{Location, ProviderPath};
use navo_ui::compare_diff::PairKind;

use crate::compare::diff::load::{LoadInput, Side, load_pair};
use crate::compare::diff::sides::unreadable_side;
use crate::compare::local::LocalSide;

fn location(path: &Path) -> Location {
    let text = path.to_str().unwrap_or_default();
    Location::local(ProviderPath::from_text(text)).unwrap()
}

/// A local side whose listed size is `size`, not the real file size.
fn side(root: &Path, rel: &str, bytes: &[u8], size: u64) -> Side {
    fs::write(root.join(rel), bytes).unwrap();
    let source = Arc::new(LocalSide::new(location(root)).unwrap());
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text(rel)),
        EntryKind::File,
        size,
        None,
    );
    (source, entry)
}

fn input(left: Option<Side>, right: Option<Side>) -> LoadInput {
    LoadInput {
        left,
        right,
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}

#[test]
fn a_text_pair_and_a_one_sided_pair_are_viewable() {
    let dir = tempfile::tempdir().unwrap();
    let left = side(dir.path(), "a.txt", b"one\n", 4);
    let right = side(dir.path(), "b.txt", b"two\n", 4);
    let both = load_pair(&input(Some(left), Some(right)), &CancelToken::new());
    assert!(both.viewable);
    let only = side(dir.path(), "c.txt", b"three\n", 6);
    let one = load_pair(&input(Some(only), None), &CancelToken::new());
    assert!(one.viewable);
}

#[test]
fn two_large_pairs_are_not_viewable() {
    let dir = tempfile::tempdir().unwrap();
    let over = WHOLE_LIMIT + 1;
    let first = load_pair(
        &input(
            Some(side(dir.path(), "a.txt", b"left one\n", over)),
            Some(side(dir.path(), "b.txt", b"right one\n", over)),
        ),
        &CancelToken::new(),
    );
    assert!(matches!(first.kind, PairKind::Large { .. }));
    assert!(!first.viewable);
}

#[test]
fn an_unreadable_side_is_unavailable_and_not_viewable() {
    let dir = tempfile::tempdir().unwrap();
    let readable = side(dir.path(), "a.txt", b"one\n", 4);
    let gone = unreadable_side(&location(&dir.path().join("gone.txt")), Reason::NotFound);
    let loaded = load_pair(&input(Some(readable), Some(gone)), &CancelToken::new());
    assert!(matches!(loaded.kind, PairKind::Unavailable(_)));
    assert!(!loaded.viewable);
}

#[test]
fn a_cancelled_load_is_not_viewable() {
    let dir = tempfile::tempdir().unwrap();
    let cancel = CancelToken::new();
    cancel.cancel();
    let loaded = load_pair(
        &input(Some(side(dir.path(), "a.txt", b"one\n", 4)), None),
        &cancel,
    );
    assert!(!loaded.viewable);
}
