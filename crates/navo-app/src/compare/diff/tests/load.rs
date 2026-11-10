//! Tests for `load_pair`: text, binary, large, one-sided, image, keys and cancel.

use std::fs;
use std::io::{self, Read};
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use navo_compare::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_compare::text::Options;
use navo_compare::text::side::{SNIFF_LEN, WHOLE_LIMIT};
use navo_core::{FileIdentity, Location, ProviderId, ProviderPath};
use navo_ui::compare_diff::PairKind;

use crate::compare::diff::load::{LoadInput, Side, load_pair};
use crate::compare::local::LocalSide;

fn location(path: &Path) -> Location {
    let text = path.to_str().unwrap_or_default();
    Location::local(ProviderPath::from_text(text)).unwrap()
}

fn write(root: &Path, rel: &str, bytes: &[u8]) {
    let path = root.join(rel);
    fs::write(path, bytes).unwrap();
}

fn local_side(root: &Path, rel: &str) -> Side {
    let size = fs::metadata(root.join(rel)).unwrap().len();
    let side = Arc::new(LocalSide::new(location(root)).unwrap());
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text(rel)),
        EntryKind::File,
        size,
        None,
    );
    (side, entry)
}

fn input(root: &Path, left: &str, right: Option<&str>) -> LoadInput {
    LoadInput {
        left: Some(local_side(root, left)),
        right: right.map(|name| local_side(root, name)),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    }
}

#[test]
fn a_text_pair_has_both_lines_and_one_hunk() {
    let dir = tempfile::tempdir().unwrap();
    let (left, right) = (b"alpha\nbeta\n", b"alpha\ngamma\n");
    write(dir.path(), "left.txt", left);
    write(dir.path(), "right.txt", right);
    let loaded = load_pair(
        &input(dir.path(), "left.txt", Some("right.txt")),
        &CancelToken::new(),
    );
    assert_eq!(loaded.kind, PairKind::Text);
    assert_eq!(
        loaded.left.as_ref().map(|whole| whole.bytes.as_slice()),
        Some(&left[..])
    );
    assert_eq!(
        loaded.right.as_ref().map(|whole| whole.bytes.as_slice()),
        Some(&right[..])
    );
    assert_eq!(
        loaded
            .alignment
            .as_ref()
            .map(|alignment| alignment.hunks.len()),
        Some(1)
    );
    assert_eq!(&loaded.viewed[..32], blake3::hash(left).as_bytes());
    assert_eq!(&loaded.viewed[32..], blake3::hash(right).as_bytes());
}

#[test]
fn the_same_content_gives_the_same_key_twice() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "a.txt", b"same\n");
    write(dir.path(), "b.txt", b"same\n");
    let input = input(dir.path(), "a.txt", Some("b.txt"));
    let first = load_pair(&input, &CancelToken::new());
    let second = load_pair(&input, &CancelToken::new());
    assert_eq!(first.viewed, second.viewed);
    assert_eq!(&first.viewed[..32], &first.viewed[32..]);
}

#[test]
fn a_binary_pair_has_no_alignment_and_still_gets_a_key() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = b"a\0b";
    write(dir.path(), "left.bin", bytes);
    write(dir.path(), "right.bin", bytes);
    let loaded = load_pair(
        &input(dir.path(), "left.bin", Some("right.bin")),
        &CancelToken::new(),
    );
    assert_eq!(loaded.kind, PairKind::Binary);
    assert!(loaded.alignment.is_none());
    assert_eq!(&loaded.viewed[..32], blake3::hash(bytes).as_bytes());
}

#[test]
fn an_image_extension_is_not_decoded_as_text() {
    let dir = tempfile::tempdir().unwrap();
    let bytes = b"\x89PNG\r\n\x1a\nnot-really";
    write(dir.path(), "left.png", bytes);
    write(dir.path(), "right.png", bytes);
    let loaded = load_pair(
        &input(dir.path(), "left.png", Some("right.png")),
        &CancelToken::new(),
    );
    assert_eq!(loaded.kind, PairKind::Image);
    assert!(loaded.left.is_none() && loaded.right.is_none());
    assert_eq!(&loaded.viewed[..32], blake3::hash(bytes).as_bytes());
}

#[test]
fn a_one_sided_text_side_is_diffed_against_empty() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "only.txt", b"one\ntwo\n");
    let loaded = load_pair(&input(dir.path(), "only.txt", None), &CancelToken::new());
    assert_eq!(loaded.kind, PairKind::OneSided { left: true });
    assert!(loaded.left.is_some());
    assert!(loaded.right.is_none());
    let alignment = loaded.alignment.as_ref().unwrap();
    assert_eq!(alignment.hunks.len(), 1);
    assert_eq!(&loaded.viewed[32..], &[0u8; 32]);
}

#[test]
fn a_65_mib_side_is_large_and_only_the_sniff_is_read() {
    let dir = tempfile::tempdir().unwrap();
    write(dir.path(), "small.txt", b"hello\n");
    let reads = Arc::new(AtomicU64::new(0));
    let size = 65 * 1024 * 1024;
    let input = LoadInput {
        left: Some((
            Arc::new(CountingSource::new(size, Arc::clone(&reads))),
            entry(size),
        )),
        right: Some(local_side(dir.path(), "small.txt")),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    };
    let loaded = load_pair(&input, &CancelToken::new());
    assert_eq!(
        loaded.kind,
        PairKind::Large {
            left: size,
            right: 6
        }
    );
    assert_eq!(reads.load(Ordering::SeqCst), SNIFF_LEN as u64);
    assert!(reads.load(Ordering::SeqCst) < WHOLE_LIMIT + 1);
    assert!(loaded.alignment.is_none());
    assert_eq!(&loaded.viewed[..32], &[0u8; 32]);
}

#[test]
fn a_cancelled_load_returns_stopped_before_reading() {
    let reads = Arc::new(AtomicU64::new(0));
    let size = 65 * 1024 * 1024;
    let input = LoadInput {
        left: Some((
            Arc::new(CountingSource::new(size, Arc::clone(&reads))),
            entry(size),
        )),
        right: None,
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
    };
    let cancel = CancelToken::new();
    cancel.cancel();
    let loaded = load_pair(&input, &cancel);
    assert_eq!(loaded.kind, PairKind::Unavailable("compare-reason-stopped"));
    assert!(loaded.alignment.is_none());
    assert_eq!(reads.load(Ordering::SeqCst), 0);
}

fn entry(size: u64) -> SideEntry {
    SideEntry::new(
        RelPath::new(ProviderPath::from_text("big.txt")),
        EntryKind::File,
        size,
        None,
    )
}

/// A generated file that counts how many bytes the loader reads.
struct CountingSource {
    size: u64,
    reads: Arc<AtomicU64>,
    provider: ProviderId,
}

impl CountingSource {
    fn new(size: u64, reads: Arc<AtomicU64>) -> Self {
        Self {
            size,
            reads,
            provider: ProviderId::local(),
        }
    }
}

impl SideSource for CountingSource {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        _folder: &RelPath,
        _sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        _cancel: &CancelToken,
    ) -> Result<(), Reason> {
        Ok(())
    }

    fn open(&self, _rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        Ok(Box::new(Generated {
            remaining: self.size,
            reads: Arc::clone(&self.reads),
        }))
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        Ok(Stamp {
            size: self.size,
            modified: None,
            file_id: None,
        })
    }

    fn object_id(&self, _rel: &RelPath) -> Option<ObjectId> {
        None
    }

    fn file_id(&self, _rel: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, rel: &RelPath) -> ProviderPath {
        rel.as_path().clone()
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        false
    }
}

/// Yields `a` bytes up to `remaining` and records the count.
struct Generated {
    remaining: u64,
    reads: Arc<AtomicU64>,
}

impl Read for Generated {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.remaining == 0 {
            return Ok(0);
        }
        let room = usize::try_from(self.remaining).unwrap_or(usize::MAX);
        let count = buffer.len().min(room);
        buffer[..count].fill(b'a');
        self.remaining -= u64::try_from(count).unwrap_or(u64::MAX);
        self.reads
            .fetch_add(u64::try_from(count).unwrap_or(u64::MAX), Ordering::SeqCst);
        Ok(count)
    }
}
