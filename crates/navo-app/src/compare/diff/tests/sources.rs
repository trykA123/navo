//! Tests for the source factory: local positional reads, the Git spool and its cleanup.

use std::fs;
use std::io::{self, Read};
use std::ops::ControlFlow;
use std::path::Path;
use std::sync::Arc;

use navo_compare::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use navo_compare::text::side::WHOLE_LIMIT;
use navo_core::{FileIdentity, Location, ProviderId, ProviderPath};

use crate::compare::diff::sources::{GitBlob, Sources, Spill};
use crate::compare::settings::CompareSettings;

mod spool;

fn location(path: &Path) -> Location {
    Location::local(ProviderPath::from_text(path.to_str().unwrap_or_default())).unwrap()
}

fn entry(rel: &str, size: u64) -> SideEntry {
    SideEntry::new(
        RelPath::new(ProviderPath::from_text(rel)),
        EntryKind::File,
        size,
        None,
    )
}

#[test]
fn two_users_get_their_own_local_source() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.txt");
    fs::write(&path, b"alpha\nbeta\n").unwrap();
    let sources = Sources::without_spool();
    let first = sources
        .open(
            &location(&path),
            &CompareSettings::default(),
            &CancelToken::new(),
        )
        .unwrap();
    let second = sources
        .open(
            &location(&path),
            &CompareSettings::default(),
            &CancelToken::new(),
        )
        .unwrap();
    assert!(!Arc::ptr_eq(&first, &second), "each caller opens its own");
    assert_eq!(sources.opens(), 2);
    let mut first_bytes = [0u8; 5];
    let mut second_bytes = [0u8; 5];
    assert_eq!(first.read_at(0, &mut first_bytes).unwrap(), 5);
    assert_eq!(second.read_at(0, &mut second_bytes).unwrap(), 5);
    assert_eq!(&first_bytes, b"alpha");
    assert_eq!(&second_bytes, b"alpha");
}

#[test]
fn a_blob_far_over_the_limit_spools_byte_identical() {
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join("compare-spool");
    let sources = Sources::new(Some(spool.clone()));
    let bytes = pattern(5 * MIB + 12_345);
    let fake = FakeSide::bytes(&bytes);
    let source = sources
        .open_git_with(
            blob(&fake, &entry("big.bin", 10)),
            &CancelToken::new(),
            small(),
        )
        .unwrap();
    assert_eq!(source.len(), bytes.len() as u64, "the tail is spooled too");
    let mut copy = vec![0u8; bytes.len()];
    assert_eq!(source.read_at(0, &mut copy).unwrap(), bytes.len());
    assert!(copy == bytes, "the spool holds the blob byte for byte");
    assert_eq!(spool_files(&spool).len(), 1);
}

#[test]
fn a_full_disk_fails_the_spool_with_a_clear_reason_and_removes_it() {
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join("compare-spool");
    let sources = Sources::new(Some(spool.clone()));
    let fake = FakeSide::bytes(&pattern(5 * MIB));
    let spill = Spill {
        limit: MIB as u64,
        writer: |file| {
            Box::new(FullAfter {
                file,
                left: 2 * MIB,
            })
        },
    };
    let result = sources.open_git_with(
        blob(&fake, &entry("big.bin", 10)),
        &CancelToken::new(),
        spill,
    );
    assert!(
        matches!(result, Err(Reason::DiskFull)),
        "the user must learn the disk is full"
    );
    assert!(spool_files(&spool).is_empty(), "a partial spool is removed");
}

#[test]
fn a_cancel_during_the_spool_copy_stops_it_and_removes_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join("compare-spool");
    let sources = Sources::new(Some(spool.clone()));
    let cancel = CancelToken::new();
    let fake = FakeSide::cancelling(4 * MIB, 2 * MIB, cancel.clone());
    let result = sources.open_git_with(blob(&fake, &entry("slow.bin", 10)), &cancel, small());
    assert!(
        matches!(result, Err(Reason::Stopped)),
        "a cancel between chunks stops the copy"
    );
    assert!(spool_files(&spool).is_empty(), "a stopped spool is removed");
}

#[test]
fn a_git_blob_over_the_limit_is_spooled_then_removed() {
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join("compare-spool");
    let sources = Sources::new(Some(spool.clone()));
    let size = WHOLE_LIMIT + 1;
    let fake = FakeSide::zeros(size);
    let source = sources
        .open_git(&fake, &entry("big.bin", size), &CancelToken::new())
        .unwrap();
    assert_eq!(source.len(), size);
    let mut buffer = [0u8; 4];
    assert_eq!(source.read_at(0, &mut buffer).unwrap(), 4);
    assert_eq!(spool_files(&spool).len(), 1);
    drop(source);
    assert_eq!(
        spool_files(&spool).len(),
        0,
        "the source owns its spool file"
    );
    drop(sources);
}

#[test]
fn an_oversized_stream_is_not_trusted_by_its_listed_size() {
    let dir = tempfile::tempdir().unwrap();
    let spool = dir.path().join("compare-spool");
    let sources = Sources::new(Some(spool.clone()));
    let real = WHOLE_LIMIT + 1;
    let fake = FakeSide::zeros(real);
    let source = sources
        .open_git(&fake, &entry("big.bin", 10), &CancelToken::new())
        .unwrap();
    assert_eq!(source.len(), real, "the stream decides the real size");
    assert_eq!(spool_files(&spool).len(), 1, "over the limit means spooled");
}

#[test]
fn a_small_git_blob_stays_in_memory() {
    let dir = tempfile::tempdir().unwrap();
    let sources = Sources::new(Some(dir.path().join("compare-spool")));
    let fake = FakeSide::bytes(b"hello\n");
    let source = sources
        .open_git(&fake, &entry("small.txt", 6), &CancelToken::new())
        .unwrap();
    let mut buffer = [0u8; 6];
    assert_eq!(source.read_at(0, &mut buffer).unwrap(), 6);
    assert_eq!(&buffer, b"hello\n");
    assert!(spool_files(&dir.path().join("compare-spool")).is_empty());
}

/// Every spool file below `root`, without the owner and sweep locks.
fn spool_files(root: &Path) -> Vec<std::path::PathBuf> {
    let mut found = Vec::new();
    let Ok(entries) = fs::read_dir(root) else {
        return found;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_dir() {
            found.extend(spool_files(&path));
        } else if path
            .file_name()
            .is_some_and(|name| name != "owner.lock" && name != "sweep.lock")
        {
            found.push(path);
        }
    }
    found
}

const MIB: usize = 1024 * 1024;

/// A 1 MiB memory limit, so a few MiB exercise the spool path.
/// Pair a fake side with one entry as a Git blob.
fn blob<'a>(source: &'a dyn SideSource, entry: &'a SideEntry) -> GitBlob<'a> {
    GitBlob { source, entry }
}

fn small() -> Spill {
    Spill {
        limit: MIB as u64,
        ..Spill::default()
    }
}

/// Bytes that differ along the whole length, so a lost or moved chunk shows.
fn pattern(len: usize) -> Vec<u8> {
    (0..len)
        .map(|at| (at % 251) as u8 ^ (at >> 16) as u8)
        .collect()
}

/// A spool writer that reports a full disk after `left` bytes.
struct FullAfter {
    file: fs::File,
    left: usize,
}

impl io::Write for FullAfter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        if self.left == 0 {
            return Err(io::Error::from(io::ErrorKind::StorageFull));
        }
        let count = buf.len().min(self.left);
        self.left -= count;
        self.file.write(&buf[..count])
    }

    fn flush(&mut self) -> io::Result<()> {
        self.file.flush()
    }
}

/// A reader of `left` zero bytes that cancels `cancel` once `after` bytes were read.
struct CancellingReader {
    left: usize,
    read: usize,
    after: usize,
    cancel: CancelToken,
}

impl Read for CancellingReader {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let count = buf.len().min(64 * 1024).min(self.left);
        buf[..count].fill(0);
        self.left -= count;
        self.read += count;
        if self.read >= self.after {
            self.cancel.cancel();
        }
        Ok(count)
    }
}

/// A Git side whose reads produce the test bytes.
struct FakeSide {
    bytes: Vec<u8>,
    size: u64,
    provider: ProviderId,
    cancel_after: Option<(usize, CancelToken)>,
}

impl FakeSide {
    fn zeros(size: u64) -> Self {
        Self {
            bytes: Vec::new(),
            size,
            provider: ProviderId::new("git").unwrap(),
            cancel_after: None,
        }
    }

    fn bytes(bytes: &[u8]) -> Self {
        Self {
            bytes: bytes.to_vec(),
            size: bytes.len() as u64,
            provider: ProviderId::new("git").unwrap(),
            cancel_after: None,
        }
    }

    fn cancelling(size: usize, after: usize, cancel: CancelToken) -> Self {
        Self {
            bytes: Vec::new(),
            size: size as u64,
            provider: ProviderId::new("git").unwrap(),
            cancel_after: Some((MIB + after, cancel)),
        }
    }
}

impl SideSource for FakeSide {
    fn provider(&self) -> &ProviderId {
        &self.provider
    }

    fn list(
        &self,
        _folder: &RelPath,
        _sink: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        _cancel: &CancelToken,
    ) -> Result<(), Reason> {
        Err(Reason::Unsupported)
    }

    fn open(&self, _rel: &RelPath, _hint: ReadHint) -> Result<Box<dyn Read + Send>, Reason> {
        if let Some((after, cancel)) = &self.cancel_after {
            return Ok(Box::new(CancellingReader {
                left: usize::try_from(self.size).unwrap(),
                read: 0,
                after: *after,
                cancel: cancel.clone(),
            }));
        }
        if self.bytes.is_empty() && self.size > 0 {
            return Ok(Box::new(io::repeat(0).take(self.size)));
        }
        Ok(Box::new(io::Cursor::new(self.bytes.clone())))
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
        true
    }
}
