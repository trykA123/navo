#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test on a broken fixture"
)]

use std::fs::{self, File, OpenOptions};
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::SystemTime;

use navo_compare::model::{Budget, CancelToken};
use navo_compare::text::apply::{ApplyError, Direction, TargetFacts, apply_hunk};
use navo_compare::text::diff::Hunk;
use navo_compare::text::large::{Covered, DiffEvent, LargeDiff, LargeSide, Request};
use navo_compare::text::side::LineEnding;
use navo_compare::text::{HunkDigest, HunkDigests, Options};
use navo_core::paging::{ByteSource, TextLayout};
use navo_fs::PagedFile;
use tempfile::TempDir;

const SOURCE: &[u8] = b"one\ntwo\nthree\n";
const TARGET: &[u8] = b"one\nTWO\nthree\n";

fn write_file(dir: &TempDir, name: &str, bytes: &[u8]) -> PathBuf {
    let path = dir.path().join(name);
    fs::write(&path, bytes).unwrap();
    path
}

fn slice(bytes: &[u8], start: u64, end: u64) -> &[u8] {
    let start = usize::try_from(start).unwrap();
    let end = usize::try_from(end).unwrap();
    &bytes[start..end]
}

fn digests(hunk: &Hunk, source: &[u8], target: &[u8]) -> HunkDigests {
    let mut digests = HunkDigests::default();
    digests.push(
        hunk,
        HunkDigest::of(
            slice(source, hunk.left_bytes.start, hunk.left_bytes.end),
            slice(target, hunk.right_bytes.start, hunk.right_bytes.end),
        ),
    );
    digests
}

fn change_hunk(source: &[u8], target: &[u8]) -> (Hunk, HunkDigests) {
    let hunk = Hunk {
        left_lines: 1..2,
        right_lines: 1..2,
        left_bytes: 4..8,
        right_bytes: 4..8,
    };
    let digests = digests(&hunk, source, target);
    (hunk, digests)
}

fn deletion_hunk(source: &[u8], target: &[u8]) -> (Hunk, HunkDigests) {
    let hunk = Hunk {
        left_lines: 1..1,
        right_lines: 1..2,
        left_bytes: 4..4,
        right_bytes: 4..8,
    };
    let digests = digests(&hunk, source, target);
    (hunk, digests)
}

fn facts() -> TargetFacts {
    TargetFacts {
        dominant: LineEnding::Lf,
        final_newline: true,
    }
}

fn copy(
    target: &PagedFile,
    source: &PagedFile,
    hunk: &Hunk,
    digests: &HunkDigests,
) -> Result<Vec<u8>, ApplyError> {
    let mut out = Vec::new();
    apply_hunk(
        target,
        source,
        hunk,
        digests,
        Direction::LeftToRight,
        facts(),
        &mut out,
    )?;
    Ok(out)
}

fn restore_time(path: &Path, time: SystemTime) {
    OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

#[test]
fn an_unchanged_real_source_still_copies() {
    let dir = tempfile::tempdir().unwrap();
    let target_path = write_file(&dir, "target.txt", TARGET);
    let source_path = write_file(&dir, "source.txt", SOURCE);
    let target = PagedFile::open(&target_path).unwrap();
    let source = PagedFile::open(&source_path).unwrap();

    let (hunk, digests) = change_hunk(SOURCE, TARGET);
    let out = copy(&target, &source, &hunk, &digests).unwrap();
    assert_eq!(out, SOURCE);
}

#[test]
fn a_same_length_same_time_edit_is_caught_by_the_hash_and_stays_refused() {
    let dir = tempfile::tempdir().unwrap();
    let target_path = write_file(&dir, "target.txt", TARGET);
    let source_path = write_file(&dir, "source.txt", SOURCE);
    let target = PagedFile::open(&target_path).unwrap();
    let source = PagedFile::open(&source_path).unwrap();
    let mut probe = [0u8; 10];
    assert_eq!(source.read_at(0, &mut probe).unwrap(), 10);
    let opened = fs::metadata(&source_path).unwrap().modified().unwrap();

    fs::write(&source_path, b"one\nTWX\nthree\n").unwrap();
    restore_time(&source_path, opened);
    assert!(
        source.revalidate().unwrap(),
        "the metadata still matches the open baseline"
    );

    let (hunk, digests) = change_hunk(SOURCE, TARGET);
    for _ in 0..2 {
        let result = copy(&target, &source, &hunk, &digests);
        assert!(matches!(result, Err(ApplyError::Changed)), "{result:?}");
    }
}

#[test]
fn a_same_length_same_time_edit_of_the_target_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let target_path = write_file(&dir, "target.txt", TARGET);
    let source_path = write_file(&dir, "source.txt", SOURCE);
    let target = PagedFile::open(&target_path).unwrap();
    let source = PagedFile::open(&source_path).unwrap();
    let mut probe = [0u8; 10];
    assert_eq!(target.read_at(0, &mut probe).unwrap(), 10);
    let opened = fs::metadata(&target_path).unwrap().modified().unwrap();

    fs::write(&target_path, b"one\nTWX\nthree\n").unwrap();
    restore_time(&target_path, opened);
    assert!(
        target.revalidate().unwrap(),
        "the metadata still matches the open baseline"
    );

    let (hunk, digests) = change_hunk(SOURCE, TARGET);
    for _ in 0..2 {
        let result = copy(&target, &source, &hunk, &digests);
        assert!(matches!(result, Err(ApplyError::Changed)), "{result:?}");
    }
}

#[test]
fn the_guard_keeps_a_shared_changed_notice() {
    let dir = tempfile::tempdir().unwrap();
    let target_path = write_file(&dir, "target.txt", TARGET);
    let source_path = write_file(&dir, "source.txt", SOURCE);
    let target = PagedFile::open(&target_path).unwrap();
    let source = PagedFile::open(&source_path).unwrap();
    source.probe_end().unwrap();

    fs::write(&source_path, b"one\nTWX\nthree\n").unwrap();
    let later = SystemTime::now() + std::time::Duration::from_secs(60);
    restore_time(&source_path, later);
    source.probe_end().unwrap();

    let (hunk, digests) = change_hunk(SOURCE, TARGET);
    for _ in 0..2 {
        let result = copy(&target, &source, &hunk, &digests);
        assert!(matches!(result, Err(ApplyError::Changed)), "{result:?}");
    }
    assert!(
        source.take_changed(),
        "the viewer's notice was not consumed"
    );
}

#[test]
fn an_empty_source_range_with_a_grown_source_stays_refused() {
    let dir = tempfile::tempdir().unwrap();
    let target_path = write_file(&dir, "target.txt", TARGET);
    let source_path = write_file(&dir, "source.txt", b"one\n");
    let target = PagedFile::open(&target_path).unwrap();
    let source = PagedFile::open(&source_path).unwrap();

    OpenOptions::new()
        .append(true)
        .open(&source_path)
        .unwrap()
        .write_all(b"two\n")
        .unwrap();

    let (hunk, digests) = deletion_hunk(b"one\n", TARGET);
    for _ in 0..2 {
        let result = copy(&target, &source, &hunk, &digests);
        assert!(matches!(result, Err(ApplyError::Changed)), "{result:?}");
    }
}

struct RawFile {
    file: File,
    len: u64,
}

impl RawFile {
    fn open(path: &Path) -> Result<Self, String> {
        let file = File::open(path).map_err(|error| error.to_string())?;
        let len = file.metadata().map_err(|error| error.to_string())?.len();
        Ok(Self { file, len })
    }
}

impl ByteSource for RawFile {
    fn len(&self) -> u64 {
        self.len
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let mut file = self.file.try_clone().map_err(|error| error.to_string())?;
        file.seek(SeekFrom::Start(offset))
            .map_err(|error| error.to_string())?;
        file.read(buf).map_err(|error| error.to_string())
    }
}

fn replace_with_same_stamp(path: &Path, bytes: &[u8]) {
    let time = fs::metadata(path).unwrap().modified().unwrap();
    let replacement = path.with_file_name("replacement.txt");
    fs::write(&replacement, bytes).unwrap();
    fs::rename(&replacement, path).unwrap();
    OpenOptions::new()
        .write(true)
        .open(path)
        .unwrap()
        .set_modified(time)
        .unwrap();
}

#[test]
fn a_replacement_detected_before_probe_end_stays_refused_after_it() {
    let dir = tempfile::tempdir().unwrap();
    let target_path = write_file(&dir, "target.txt", TARGET);
    let source_path = write_file(&dir, "source.txt", SOURCE);
    let target = PagedFile::open(&target_path).unwrap();
    let source = PagedFile::open(&source_path).unwrap();
    let (hunk, digests) = change_hunk(SOURCE, TARGET);
    replace_with_same_stamp(&target_path, b"Xne\nTWO\nthree\n");

    let first = copy(&target, &source, &hunk, &digests);
    assert!(matches!(first, Err(ApplyError::Changed)), "{first:?}");
    target.probe_end().unwrap();
    let second = copy(&target, &source, &hunk, &digests);
    assert!(matches!(second, Err(ApplyError::Changed)), "{second:?}");
}

fn diff_with_mutation(left_path: &Path, right_path: &Path, opened: SystemTime) -> Covered {
    let armed = Arc::new(AtomicBool::new(false));
    let hook_path = left_path.to_owned();
    let before_hash = Arc::new(move || {
        if armed.swap(true, Ordering::AcqRel) {
            return;
        }
        let mut file = OpenOptions::new().write(true).open(&hook_path).unwrap();
        file.seek(SeekFrom::Start(0)).unwrap();
        file.write_all(b"X").unwrap();
        file.set_modified(opened).unwrap();
    });
    let request = Request {
        left: LargeSide {
            source: Arc::new(RawFile::open(left_path).unwrap()),
            layout: TextLayout::default(),
        },
        right: LargeSide {
            source: Arc::new(RawFile::open(right_path).unwrap()),
            layout: TextLayout::default(),
        },
        options: Options::default(),
        focus: 0,
        budget: Budget::default(),
    };
    let diff = LargeDiff::start_with_hook(request, &CancelToken::new(), before_hash);
    let mut covered = None;
    loop {
        match diff.events().recv().unwrap() {
            DiffEvent::Hunks(hunks) => covered = Some(hunks),
            DiffEvent::Done(_) => break,
            _ => {}
        }
    }
    assert_eq!(
        fs::read(left_path).unwrap()[0],
        b'X',
        "the hook mutated the source"
    );
    covered.expect("the diff emits hunks")
}

#[test]
fn an_edit_after_the_diff_read_is_not_certified_by_the_digests() {
    const LEN: usize = 3 * 1024 * 1024;
    let dir = tempfile::tempdir().unwrap();
    let left_path = write_file(&dir, "left.txt", &vec![b'A'; LEN]);
    let right_path = write_file(&dir, "right.txt", &vec![b'B'; LEN]);
    let source = PagedFile::open(&left_path).unwrap();
    let target = PagedFile::open(&right_path).unwrap();
    let opened = fs::metadata(&left_path).unwrap().modified().unwrap();
    let covered = diff_with_mutation(&left_path, &right_path, opened);
    assert_eq!(covered.hunks.len(), 1);
    let mut out = Vec::new();
    let result = apply_hunk(
        &target,
        &source,
        &covered.hunks[0],
        &covered.digests,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: false,
        },
        &mut out,
    );
    assert!(matches!(result, Err(ApplyError::Changed)), "{result:?}");
    assert!(out.is_empty());
    let changed = fs::read(&left_path).unwrap();
    let target_bytes = fs::read(&right_path).unwrap();
    let mut stale = HunkDigests::default();
    stale.push(&covered.hunks[0], HunkDigest::of(&changed, &target_bytes));
    let mut stale_out = Vec::new();
    let accepted = apply_hunk(
        &target,
        &source,
        &covered.hunks[0],
        &stale,
        Direction::LeftToRight,
        TargetFacts {
            dominant: LineEnding::Lf,
            final_newline: false,
        },
        &mut stale_out,
    );
    assert!(
        accepted.is_ok(),
        "a digest taken after the edit would certify it"
    );
}
