//! Tests for copying one change: byte-exact results, picked lines and every refusal.

use std::collections::BTreeSet;
use std::fs;
use std::path::Path;
use std::sync::Arc;

use navo_compare::model::CancelToken;
use navo_compare::text::{Direction, Options};
use navo_core::{Location, ProviderPath};
use navo_fs::{CancelToken as FsCancel, LocalFs};
use navo_store::StoreDirs;
use navo_ui::browser::LogFactory;
use navo_ui::compare_diff::{HunkView, PairRequest};

use crate::compare::diff::convert::to_pair_text;
use crate::compare::diff::copy::{CopyContext, CopyRefusal, CopyRequest, copy_change};
use crate::compare::diff::load::{LoadInput, load_pair};
use crate::compare::local::LocalSide;
use crate::jobs::{SessionHolds, open};

const LEFT: &[u8] = b"a\nb\nc\n";
const RIGHT: &[u8] = b"a\nB\nc\n";

struct Fixture {
    work: tempfile::TempDir,
    _store: tempfile::TempDir,
    factory: LogFactory,
}

impl Fixture {
    fn new(left: &[u8], right: &[u8]) -> Self {
        let work = tempfile::tempdir().unwrap();
        fs::write(work.path().join("left.txt"), left).unwrap();
        fs::write(work.path().join("right.txt"), right).unwrap();
        let store = tempfile::tempdir().unwrap();
        let dirs = StoreDirs::in_profile(store.path());
        dirs.ensure().unwrap();
        let journal = open(&dirs.journal_file()).unwrap();
        let factory = SessionHolds::new(journal, dirs).compare_factory(None);
        Self {
            work,
            _store: store,
            factory,
        }
    }

    fn path(&self, name: &str) -> std::path::PathBuf {
        self.work.path().join(name)
    }

    fn read(&self, name: &str) -> Vec<u8> {
        fs::read(self.path(name)).unwrap()
    }

    /// The names in the work folder, sorted.
    fn names(&self) -> Vec<String> {
        let mut names: Vec<String> = fs::read_dir(self.work.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// The request the UI would send for hunk `index` of the pair as it is now.
    fn request(&self, index: usize, direction: Direction) -> CopyRequest {
        let hunk = self.hunk(index);
        CopyRequest {
            pair: PairRequest {
                id: 1,
                path: "x.txt".to_owned(),
                left: Some(location(&self.path("left.txt"))),
                right: Some(location(&self.path("right.txt"))),
            },
            left_bytes: hunk.left_bytes,
            right_bytes: hunk.right_bytes,
            digest: hunk.digest.unwrap(),
            direction,
            lines: None,
        }
    }

    fn hunk(&self, index: usize) -> HunkView {
        let input = LoadInput {
            left: Some(side(self.work.path(), "left.txt")),
            right: Some(side(self.work.path(), "right.txt")),
            options: Options::default(),
            fallback: encoding_rs::UTF_8,
        };
        let loaded = load_pair(&input, &CancelToken::new());
        to_pair_text(&loaded).unwrap().hunks[index].clone()
    }

    fn copy(&self, request: &CopyRequest) -> Result<(), CopyRefusal> {
        self.copy_with(request, &self.factory)
    }

    fn copy_with(&self, request: &CopyRequest, factory: &LogFactory) -> Result<(), CopyRefusal> {
        let options = Options::default();
        let cancel = FsCancel::new();
        let ops = LocalFs::new();
        let ctx = CopyContext {
            options: &options,
            fallback: encoding_rs::UTF_8,
            ops: &ops,
            factory,
            cancel: &cancel,
        };
        copy_change(request, &ctx).map(|_| ())
    }
}

fn location(path: &Path) -> Location {
    Location::local(ProviderPath::from_text(path.to_str().unwrap())).unwrap()
}

fn side(root: &Path, name: &str) -> crate::compare::diff::load::Side {
    use navo_compare::model::{EntryKind, RelPath, SideEntry};
    let size = fs::metadata(root.join(name)).unwrap().len();
    let source = Arc::new(LocalSide::new(location(root)).unwrap());
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text(name)),
        EntryKind::File,
        size,
        None,
    );
    (source, entry)
}

#[test]
fn copy_to_the_right_writes_the_left_text_byte_exact() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let request = fixture.request(0, Direction::LeftToRight);
    fixture.copy(&request).unwrap();
    assert_eq!(fixture.read("right.txt"), LEFT);
    assert_eq!(fixture.read("left.txt"), LEFT);
    let names = fixture.names();
    assert!(
        names.iter().all(|name| !name.ends_with(".navo-part")),
        "{names:?}"
    );
    let held = names
        .iter()
        .filter(|name| name.starts_with(".navo-held-"))
        .count();
    assert_eq!(held, 1, "the session hold keeps the old version: {names:?}");
}

#[test]
fn copy_to_the_left_writes_the_right_text_byte_exact() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let request = fixture.request(0, Direction::RightToLeft);
    fixture.copy(&request).unwrap();
    assert_eq!(fixture.read("left.txt"), RIGHT);
    assert_eq!(fixture.read("right.txt"), RIGHT);
}

#[test]
fn a_crlf_target_with_no_final_newline_keeps_its_style() {
    let fixture = Fixture::new(b"a\nb\nc", b"a\r\nB\r\nc");
    let request = fixture.request(0, Direction::LeftToRight);
    fixture.copy(&request).unwrap();
    assert_eq!(fixture.read("right.txt"), b"a\r\nb\r\nc");
}

#[test]
fn only_the_picked_lines_are_copied() {
    let fixture = Fixture::new(b"a\nb1\nb2\nb3\nc\n", b"a\nB1\nB2\nB3\nc\n");
    let mut request = fixture.request(0, Direction::LeftToRight);
    request.lines = Some(BTreeSet::from([2]));
    fixture.copy(&request).unwrap();
    assert_eq!(fixture.read("right.txt"), b"a\nB1\nb2\nB3\nc\n");
}

#[test]
fn an_empty_pick_is_refused_and_copies_nothing() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let mut request = fixture.request(0, Direction::LeftToRight);
    request.lines = Some(BTreeSet::new());
    assert_eq!(fixture.copy(&request), Err(CopyRefusal::NotCopyable));
    assert_eq!(fixture.read("right.txt"), RIGHT);
    assert_eq!(fixture.names(), ["left.txt", "right.txt"]);
}

#[test]
fn a_target_edited_inside_the_change_after_it_was_shown_is_refused_untouched() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let request = fixture.request(0, Direction::LeftToRight);
    fs::write(fixture.path("right.txt"), b"a\nX\nc\n").unwrap();
    assert_eq!(fixture.copy(&request), Err(CopyRefusal::Changed));
    assert_eq!(fixture.read("right.txt"), b"a\nX\nc\n");
    assert_eq!(fixture.names(), ["left.txt", "right.txt"]);
}

#[test]
fn a_target_that_grew_before_the_change_is_refused_untouched() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let request = fixture.request(0, Direction::LeftToRight);
    fs::write(fixture.path("right.txt"), b"new\na\nB\nc\n").unwrap();
    assert_eq!(fixture.copy(&request), Err(CopyRefusal::Changed));
    assert_eq!(fixture.read("right.txt"), b"new\na\nB\nc\n");
    assert_eq!(fixture.names(), ["left.txt", "right.txt"]);
}

#[test]
fn a_source_edited_inside_the_change_with_the_same_length_is_refused() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let request = fixture.request(0, Direction::LeftToRight);
    fs::write(fixture.path("left.txt"), b"a\nZ\nc\n").unwrap();
    assert_eq!(fixture.copy(&request), Err(CopyRefusal::Changed));
    assert_eq!(fixture.read("right.txt"), RIGHT);
    assert_eq!(fixture.names(), ["left.txt", "right.txt"]);
}

#[test]
fn a_source_edit_that_moves_the_change_is_refused() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let request = fixture.request(0, Direction::RightToLeft);
    fs::write(fixture.path("right.txt"), b"a\nBB\nc\n").unwrap();
    assert_eq!(fixture.copy(&request), Err(CopyRefusal::Changed));
    assert_eq!(fixture.read("left.txt"), LEFT);
    assert_eq!(fixture.names(), ["left.txt", "right.txt"]);
}

#[test]
fn a_target_edited_between_the_diff_and_the_write_is_stopped_by_the_stamp() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let request = fixture.request(0, Direction::LeftToRight);
    let inner = Arc::clone(&fixture.factory);
    let target = fixture.path("right.txt");
    let racing: LogFactory = Arc::new(move |plan| {
        fs::write(&target, b"a\nB\nc\nlate\n").unwrap();
        inner(plan)
    });
    assert_eq!(
        fixture.copy_with(&request, &racing),
        Err(CopyRefusal::Changed)
    );
    assert_eq!(fixture.read("right.txt"), b"a\nB\nc\nlate\n");
    assert_eq!(fixture.names(), ["left.txt", "right.txt"]);
}

#[test]
fn a_git_target_is_refused_as_read_only() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let mut request = fixture.request(0, Direction::LeftToRight);
    request.pair.right =
        Some(navo_fs::git::location(fixture.work.path(), "HEAD", "right.txt").unwrap());
    assert_eq!(fixture.copy(&request), Err(CopyRefusal::ReadOnly));
    assert_eq!(fixture.names(), ["left.txt", "right.txt"]);
}

#[test]
fn a_one_sided_pair_is_not_copyable() {
    let fixture = Fixture::new(LEFT, RIGHT);
    let mut request = fixture.request(0, Direction::LeftToRight);
    request.pair.right = None;
    assert_eq!(fixture.copy(&request), Err(CopyRefusal::NotCopyable));
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
