//! Tests for composing a copy: the bytes the fill writes for a hunk, picked lines, a removal and
//! every refusal. The output goes to a `Vec<u8>`; the journal is not involved.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;

use navo_compare::model::CancelToken;
use navo_compare::text::Direction;
use navo_core::{Location, ProviderPath};
use navo_ui::compare_diff::PairRequest;

use super::{CopyInput, apply, apply_many, open_both};
use crate::compare::diff::convert::to_pair_text;
use crate::compare::diff::load::{Loaded, load_pair};
use crate::compare::diff::write::request::{
    CopyRefusal, CopyRequest, ends_of, load_input, matched,
};

const LEFT: &[u8] = b"a\nb\nc\n";
const RIGHT: &[u8] = b"a\nB\nc\n";

fn location(path: &Path) -> Location {
    Location::local(ProviderPath::from_text(path.to_str().unwrap())).unwrap()
}

/// Two files on disk and the request the UI would send for hunk 0.
fn fixture(left: &[u8], right: &[u8], direction: Direction) -> (tempfile::TempDir, CopyRequest) {
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
        direction,
        lines: None,
    };
    let hunk = to_pair_text(&load(&request)).unwrap().hunks[0].clone();
    request.left_bytes = hunk.left_bytes;
    request.right_bytes = hunk.right_bytes;
    request.digest = hunk.digest.unwrap();
    (work, request)
}

fn load(request: &CopyRequest) -> Loaded {
    load_pair(&load_input(request).unwrap(), &CancelToken::new())
}

/// Compose the request against the files as they are now and return the bytes or the refusal.
fn compose(request: &CopyRequest) -> Result<Vec<u8>, CopyRefusal> {
    let loaded = load(request);
    let shown = matched(request, &loaded)?;
    let (source, target) = ends_of(request)?;
    let files = open_both(&path(&target), &path(&source))?;
    let mut out = Vec::new();
    apply(request, &shown, &files, &mut out)?;
    Ok(out)
}

fn path(at: &Location) -> std::path::PathBuf {
    navo_fs::os_path(at).unwrap()
}

#[test]
fn copy_to_the_right_composes_the_left_text_byte_exact() {
    let (_work, request) = fixture(LEFT, RIGHT, Direction::LeftToRight);
    assert_eq!(compose(&request).unwrap(), LEFT);
}

#[test]
fn copy_to_the_left_composes_the_right_text_byte_exact() {
    let (_work, request) = fixture(LEFT, RIGHT, Direction::RightToLeft);
    assert_eq!(compose(&request).unwrap(), RIGHT);
}

#[test]
fn a_crlf_target_with_no_final_newline_keeps_its_style() {
    let (_work, request) = fixture(b"a\nb\nc", b"a\r\nB\r\nc", Direction::LeftToRight);
    assert_eq!(compose(&request).unwrap(), b"a\r\nb\r\nc");
}

#[test]
fn only_the_picked_lines_are_composed() {
    let (_work, mut request) = fixture(
        b"a\nb1\nb2\nb3\nc\n",
        b"a\nB1\nB2\nB3\nc\n",
        Direction::LeftToRight,
    );
    request.lines = Some(BTreeSet::from([2]));
    assert_eq!(compose(&request).unwrap(), b"a\nB1\nb2\nB3\nc\n");
}

#[test]
fn copying_a_line_the_source_lacks_removes_it_from_the_target() {
    let (_work, request) = fixture(b"a\nc\n", b"a\nb\nc\n", Direction::LeftToRight);
    assert_eq!(compose(&request).unwrap(), b"a\nc\n");
}

#[test]
fn an_empty_pick_is_refused() {
    let (_work, mut request) = fixture(LEFT, RIGHT, Direction::LeftToRight);
    request.lines = Some(BTreeSet::new());
    assert_eq!(compose(&request), Err(CopyRefusal::NotCopyable));
}

#[test]
fn many_hunks_compose_in_one_pass() {
    let (_work, request) = fixture(
        b"a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n",
        b"a\nB\nc\nd\ne\nf\ng\nh\nI\nj\n",
        Direction::LeftToRight,
    );
    let loaded = load(&request);
    let (source, target) = ends_of(&request).unwrap();
    let files = open_both(&path(&target), &path(&source)).unwrap();
    let lines = BTreeSet::new();
    let mut out = Vec::new();
    apply_many(
        &CopyInput {
            loaded: &loaded,
            indices: &[0, 1],
            lines: &lines,
            direction: Direction::LeftToRight,
        },
        &files,
        &mut out,
    )
    .unwrap();
    assert_eq!(out, b"a\nb\nc\nd\ne\nf\ng\nh\ni\nj\n");
}

#[test]
fn a_target_edited_after_it_was_opened_is_refused_changed() {
    let (work, request) = fixture(LEFT, RIGHT, Direction::LeftToRight);
    let loaded = load(&request);
    let shown = matched(&request, &loaded).unwrap();
    let (source, target) = ends_of(&request).unwrap();
    let files = open_both(&path(&target), &path(&source)).unwrap();
    fs::write(work.path().join("right.txt"), b"a\nX\nc\nmore\n").unwrap();
    let mut out = Vec::new();
    assert_eq!(
        apply(&request, &shown, &files, &mut out),
        Err(CopyRefusal::Changed)
    );
}

#[test]
fn a_missing_file_fails_to_open_with_its_reason() {
    let work = tempfile::tempdir().unwrap();
    let missing = work.path().join("none.txt");
    assert!(matches!(
        open_both(&missing, &missing),
        Err(CopyRefusal::Failed(_))
    ));
}

#[test]
fn a_target_that_grew_before_the_change_is_refused_changed() {
    let (work, request) = fixture(LEFT, RIGHT, Direction::LeftToRight);
    fs::write(work.path().join("right.txt"), b"new\na\nB\nc\n").unwrap();
    assert_eq!(compose(&request), Err(CopyRefusal::Changed));
}

#[test]
fn a_source_edited_to_the_same_length_is_refused_changed() {
    let (work, request) = fixture(LEFT, RIGHT, Direction::LeftToRight);
    fs::write(work.path().join("left.txt"), b"a\nZ\nc\n").unwrap();
    assert_eq!(compose(&request), Err(CopyRefusal::Changed));
}

#[test]
fn an_edit_that_moves_the_change_is_refused_changed() {
    let (work, request) = fixture(LEFT, RIGHT, Direction::RightToLeft);
    fs::write(work.path().join("right.txt"), b"a\nBB\nc\n").unwrap();
    assert_eq!(compose(&request), Err(CopyRefusal::Changed));
}

#[test]
fn a_same_length_edit_inside_the_change_is_refused_changed() {
    let (work, request) = fixture(LEFT, RIGHT, Direction::LeftToRight);
    fs::write(work.path().join("right.txt"), b"a\nX\nc\n").unwrap();
    assert_eq!(compose(&request), Err(CopyRefusal::Changed));
}

#[test]
fn a_one_sided_pair_is_refused_not_copyable() {
    let (_work, mut request) = fixture(LEFT, RIGHT, Direction::LeftToRight);
    request.pair.left = None;
    assert_eq!(ends_of(&request), Err(CopyRefusal::NotCopyable));
}

#[test]
fn every_refusal_has_a_status_line_text() {
    for refusal in [
        CopyRefusal::ReadOnly,
        CopyRefusal::NotCopyable,
        CopyRefusal::Changed,
        CopyRefusal::Failed("disk full".to_owned()),
    ] {
        assert!(!refusal.text().starts_with("compare-copy"), "{refusal:?}");
    }
}
