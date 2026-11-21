//! Tests for the copy request port: matching the shown change, the ends of a request and the
//! refusal texts.

use std::fs;
use std::path::Path;

use navo_compare::model::CancelToken;
use navo_compare::text::Direction;
use navo_core::{Location, ProviderId, ProviderPath};
use navo_ui::compare_diff::{HunkView, PairRequest};

use super::{CopyRefusal, CopyRequest, ends_of, load_input, matched, stamp_of, target_whole};
use crate::compare::diff::convert::to_pair_text;
use crate::compare::diff::load::{Loaded, load_pair};

const LEFT: &[u8] = b"a\nb\nc\n";
const RIGHT: &[u8] = b"a\nB\nc\n";

fn location(path: &Path) -> Location {
    Location::local(ProviderPath::from_text(path.to_str().unwrap())).unwrap()
}

/// Two files on disk and the request the UI would send for hunk 0.
fn fixture(left: &[u8], right: &[u8]) -> (tempfile::TempDir, CopyRequest) {
    let work = tempfile::tempdir().unwrap();
    fs::write(work.path().join("left.txt"), left).unwrap();
    fs::write(work.path().join("right.txt"), right).unwrap();
    let mut request = CopyRequest {
        pair: PairRequest {
            id: 1,
            path: "x.txt".to_owned(),
            left: Some(location(&work.path().join("left.txt"))),
            right: Some(location(&work.path().join("right.txt"))),
        },
        left_bytes: 0..0,
        right_bytes: 0..0,
        digest: [0; 16],
        direction: Direction::LeftToRight,
        lines: None,
    };
    let hunk = hunk_of(&load(&request));
    request.left_bytes = hunk.left_bytes;
    request.right_bytes = hunk.right_bytes;
    request.digest = hunk.digest.unwrap();
    (work, request)
}

fn load(request: &CopyRequest) -> Loaded {
    load_pair(&load_input(request).unwrap(), &CancelToken::new())
}

fn hunk_of(loaded: &Loaded) -> HunkView {
    to_pair_text(loaded).unwrap().hunks[0].clone()
}

#[test]
fn the_shown_change_is_found_again_and_names_its_target_text() {
    let (_work, request) = fixture(LEFT, RIGHT);
    let loaded = load(&request);
    assert_eq!(matched(&request, &loaded).unwrap().index, 0);
    assert_eq!(
        target_whole(request.direction, &loaded).unwrap().bytes,
        RIGHT
    );
}

#[test]
fn a_target_edited_inside_the_change_is_changed() {
    let (work, request) = fixture(LEFT, RIGHT);
    fs::write(work.path().join("right.txt"), b"a\nX\nc\n").unwrap();
    let loaded = load(&request);
    assert_eq!(
        matched(&request, &loaded).unwrap_err(),
        CopyRefusal::Changed
    );
}

#[test]
fn a_source_edited_inside_the_change_with_the_same_length_is_changed() {
    let (work, request) = fixture(LEFT, RIGHT);
    fs::write(work.path().join("left.txt"), b"a\nZ\nc\n").unwrap();
    let loaded = load(&request);
    assert_eq!(
        matched(&request, &loaded).unwrap_err(),
        CopyRefusal::Changed
    );
}

#[test]
fn a_source_edit_that_moves_the_change_is_changed() {
    let (work, mut request) = fixture(LEFT, RIGHT);
    request.direction = Direction::RightToLeft;
    fs::write(work.path().join("right.txt"), b"a\nBB\nc\n").unwrap();
    let loaded = load(&request);
    assert_eq!(
        matched(&request, &loaded).unwrap_err(),
        CopyRefusal::Changed
    );
}

#[test]
fn a_non_local_target_is_read_only() {
    let (_work, mut request) = fixture(LEFT, RIGHT);
    let git = Location::with_provider(
        ProviderId::new("git").unwrap(),
        ProviderPath::from_text("/x"),
    )
    .unwrap();
    request.pair.right = Some(git);
    assert_eq!(ends_of(&request).unwrap_err(), CopyRefusal::ReadOnly);
}

/// A target on the `git-remote` provider is never copyable.
#[test]
fn a_git_remote_target_is_read_only() {
    let (_work, mut request) = fixture(LEFT, RIGHT);
    let remote = Location::with_provider(
        ProviderId::new("git-remote").unwrap(),
        ProviderPath::from_text("https://h/r@main/x"),
    )
    .unwrap();
    request.pair.right = Some(remote);
    assert_eq!(ends_of(&request).unwrap_err(), CopyRefusal::ReadOnly);
}

/// A target on the `git-checkout` provider is never copyable.
#[test]
fn a_git_checkout_target_is_read_only() {
    let (_work, mut request) = fixture(LEFT, RIGHT);
    let checkout = Location::with_provider(
        ProviderId::new("git-checkout").unwrap(),
        ProviderPath::from_text("/x"),
    )
    .unwrap();
    request.pair.right = Some(checkout);
    assert_eq!(ends_of(&request).unwrap_err(), CopyRefusal::ReadOnly);
}

#[test]
fn a_one_sided_pair_is_not_copyable() {
    let (_work, mut request) = fixture(LEFT, RIGHT);
    request.pair.right = None;
    assert_eq!(ends_of(&request).unwrap_err(), CopyRefusal::NotCopyable);
}

#[test]
fn the_stamp_carries_size_time_and_the_blake3_of_the_bytes() {
    let (work, _request) = fixture(LEFT, RIGHT);
    let seen = navo_fs::walk::stat(&work.path().join("right.txt")).unwrap();
    let stamp = stamp_of(&seen, RIGHT);
    assert_eq!(stamp.size, RIGHT.len() as u64);
    assert_eq!(stamp.modified_ms, seen.modified_ms);
    assert_eq!(stamp.hash, *blake3::hash(RIGHT).as_bytes());
}

#[test]
fn every_refusal_has_a_status_line_text() {
    for refusal in [
        CopyRefusal::ReadOnly,
        CopyRefusal::NotCopyable,
        CopyRefusal::Changed,
        CopyRefusal::Network,
        CopyRefusal::InUse,
        CopyRefusal::Failed("disk full".to_owned()),
    ] {
        assert!(!refusal.text().starts_with("compare-copy"), "{refusal:?}");
    }
}

#[test]
fn the_new_refusals_say_nothing_was_copied() {
    assert!(CopyRefusal::Network.text().contains("network drives"));
    assert!(CopyRefusal::InUse.text().contains("Nothing was copied."));
}
