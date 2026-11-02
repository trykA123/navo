use std::io::{self, Cursor, Read};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::mpsc;

use rusty_core::{FileIdentity, ProviderId, ProviderPath};

use crate::anchor::Candidate;
use crate::model::{
    Budget, CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource,
    Stamp,
};

use crate::bytes::{Align, ByteDiff, ByteEvent, ByteRegion, ByteSide, Request};

use super::*;

struct TestSource {
    provider: ProviderId,
    data: Arc<Vec<u8>>,
    opens: AtomicUsize,
    cancel: Option<CancelToken>,
}

impl TestSource {
    fn new(data: Arc<Vec<u8>>) -> Self {
        Self {
            provider: ProviderId::new("memory").unwrap(),
            data,
            opens: AtomicUsize::new(0),
            cancel: None,
        }
    }

    fn cancelling(data: Arc<Vec<u8>>, cancel: CancelToken) -> Self {
        Self {
            provider: ProviderId::new("memory").unwrap(),
            data,
            opens: AtomicUsize::new(0),
            cancel: Some(cancel),
        }
    }
}

impl SideSource for TestSource {
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
        let index = self.opens.fetch_add(1, Ordering::SeqCst);
        let reader = Cursor::new(Arc::clone(&self.data).as_ref().clone());
        match &self.cancel {
            Some(cancel) if index >= 1 => Ok(Box::new(CancelOnRead {
                inner: reader,
                cancel: cancel.clone(),
            })),
            _ => Ok(Box::new(reader)),
        }
    }

    fn stamp(&self, _rel: &RelPath) -> Result<Stamp, Reason> {
        Err(Reason::Unsupported)
    }

    fn object_id(&self, _rel: &RelPath) -> Option<ObjectId> {
        None
    }

    fn file_id(&self, _rel: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, _rel: &RelPath) -> ProviderPath {
        ProviderPath::from_text("file")
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        true
    }
}

struct CancelOnRead {
    inner: Cursor<Vec<u8>>,
    cancel: CancelToken,
}

impl Read for CancelOnRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        self.cancel.cancel();
        self.inner.read(buffer)
    }
}

struct ShortReads {
    inner: Cursor<Vec<u8>>,
    reads: Arc<AtomicUsize>,
    cancel: CancelToken,
    cancel_at: usize,
}

impl Read for ShortReads {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let started = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        assert!(started <= self.cancel_at, "a read started after the cancel");
        if started == self.cancel_at {
            self.cancel.cancel();
        }
        let want = buffer.len().min(1);
        self.inner.read(&mut buffer[..want])
    }
}

struct InterruptedReads {
    reads: Arc<AtomicUsize>,
    cancel: CancelToken,
}

impl Read for InterruptedReads {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        let started = self.reads.fetch_add(1, Ordering::SeqCst) + 1;
        assert_eq!(started, 1, "a read started after the cancel");
        self.cancel.cancel();
        Err(io::Error::from(io::ErrorKind::Interrupted))
    }
}

fn side(bytes: Vec<u8>, listed: usize) -> ByteSide {
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("file")),
        EntryKind::File,
        u64::try_from(listed).unwrap(),
        None,
    );
    ByteSide::new(Arc::new(TestSource::new(Arc::new(bytes))), entry)
}

fn memory_side(bytes: Vec<u8>) -> ByteSide {
    let len = bytes.len();
    side(bytes, len)
}

fn stale_side(bytes: Vec<u8>, listed: usize) -> ByteSide {
    side(bytes, listed)
}

fn cancelling_side(bytes: Vec<u8>, cancel: &CancelToken) -> ByteSide {
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("file")),
        EntryKind::File,
        u64::try_from(bytes.len()).unwrap(),
        None,
    );
    ByteSide::new(
        Arc::new(TestSource::cancelling(Arc::new(bytes), cancel.clone())),
        entry,
    )
}

fn events(left: ByteSide, right: ByteSide) -> Vec<ByteEvent> {
    let request = Request {
        left,
        right,
        align: Align::Content,
        budget: Budget::default(),
    };
    ByteDiff::start(request, &CancelToken::new())
        .events()
        .iter()
        .collect()
}

fn regions(events: &[ByteEvent]) -> Vec<ByteRegion> {
    events
        .iter()
        .filter_map(|event| match event {
            ByteEvent::Region(region) => Some(region.clone()),
            _ => None,
        })
        .collect()
}

fn completed(events: &[ByteEvent]) -> Vec<ByteRegion> {
    assert_eq!(events.last(), Some(&ByteEvent::Done));
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, ByteEvent::Failed(_)))
    );
    regions(events)
}

fn bytes(len: usize, seed: u64) -> Vec<u8> {
    let mut state = seed.wrapping_mul(6_364_136_223_846_793_005).wrapping_add(1);
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 32) as u8
        })
        .collect()
}

fn fresh_byte(base: &[u8], at: usize) -> u8 {
    (0..=255_u8)
        .find(|&value| {
            let before = at.checked_sub(1).and_then(|index| base.get(index));
            before != Some(&value)
                && base.get(at) != Some(&value)
                && base.get(at + 1) != Some(&value)
        })
        .unwrap()
}

fn with_insertion(base: &[u8], at: usize, len: usize, seed: u64) -> Vec<u8> {
    let mut inserted = bytes(len, seed);
    if let Some(first) = inserted.first_mut() {
        *first = fresh_byte(base, at.saturating_sub(1));
    }
    if let Some(last) = inserted.last_mut() {
        *last = fresh_byte(base, at);
    }
    let mut right = Vec::with_capacity(base.len() + len);
    right.extend_from_slice(&base[..at]);
    right.extend_from_slice(&inserted);
    right.extend_from_slice(&base[at..]);
    right
}

fn range(start: usize, end: usize) -> ByteRegion {
    let start = u64::try_from(start).unwrap();
    let end = u64::try_from(end).unwrap();
    ByteRegion::single(start..end, start..end)
}

#[test]
fn insertions_of_one_byte_and_four_kib_leave_one_region() {
    let base = bytes(400_000, 1);
    for (case, len) in [1_usize, 4096].into_iter().enumerate() {
        for (place, at) in [0, base.len() / 2, base.len()].into_iter().enumerate() {
            let seed = 10 + u64::try_from(case * 3 + place).unwrap();
            let right = with_insertion(&base, at, len, seed);
            let found = completed(&events(memory_side(base.clone()), memory_side(right)));
            assert_eq!(found.len(), 1, "insertion of {len} at {at}");
            assert_eq!(
                found[0],
                ByteRegion::single(
                    u64::try_from(at).unwrap()..u64::try_from(at).unwrap(),
                    u64::try_from(at).unwrap()..u64::try_from(at + len).unwrap(),
                ),
                "insertion of {len} at {at}"
            );
        }
    }
}

#[test]
fn insertions_at_arbitrary_offsets_leave_one_region() {
    let base = bytes(400_000, 21);
    for (case, at) in [1_usize, 5_000, 100_000, 250_000, 399_999]
        .into_iter()
        .enumerate()
    {
        for len in [1_usize, 4096] {
            let seed = 30 + u64::try_from(case * 2 + len % 3).unwrap();
            let right = with_insertion(&base, at, len, seed);
            let found = completed(&events(memory_side(base.clone()), memory_side(right)));
            assert_eq!(found.len(), 1, "insertion of {len} at {at}");
            assert_eq!(
                found[0],
                ByteRegion::single(
                    u64::try_from(at).unwrap()..u64::try_from(at).unwrap(),
                    u64::try_from(at).unwrap()..u64::try_from(at + len).unwrap(),
                ),
                "insertion of {len} at {at}"
            );
        }
    }
}

#[test]
fn patched_ranges_refine_to_exact_bytes() {
    let base = bytes(400_000, 2);
    let patches = [(80_000_usize, 7_usize), (260_000, 5)];
    let mut right = base.clone();
    for &(at, len) in &patches {
        for index in at..at + len {
            right[index] = !base[index];
        }
    }
    let expected: Vec<ByteRegion> = patches
        .iter()
        .map(|&(at, len)| range(at, at + len))
        .collect();
    assert_eq!(
        completed(&events(memory_side(base), memory_side(right))),
        expected
    );
}

#[test]
fn a_region_over_sixty_four_kib_starts_at_the_first_differing_byte() {
    let base = bytes(600_000, 3);
    let at = 160_000;
    let len = 120_000;
    let mut right = base.clone();
    for index in at..at + len {
        right[index] = !base[index];
    }
    let found = completed(&events(memory_side(base), memory_side(right)));
    assert_eq!(found.len(), 1);
    let region = &found[0];
    assert_eq!(region.left.start, u64::try_from(at).unwrap());
    assert_eq!(region.right.start, u64::try_from(at).unwrap());
    assert!(region.left.end - region.left.start > REFINE_LIMIT);
}

#[test]
fn equal_sides_emit_no_region() {
    let data = bytes(200_000, 4);
    let events = events(memory_side(data.clone()), memory_side(data));
    assert!(regions(&events).is_empty());
    assert_eq!(events.last(), Some(&ByteEvent::Done));
}

#[test]
fn empty_sides_emit_no_region() {
    let empty = events(memory_side(Vec::new()), memory_side(Vec::new()));
    assert!(regions(&empty).is_empty());
    assert_eq!(empty.last(), Some(&ByteEvent::Done));
    let added = events(memory_side(Vec::new()), memory_side(bytes(64, 7)));
    assert_eq!(added.last(), Some(&ByteEvent::Done));
    assert_eq!(regions(&added), vec![ByteRegion::single(0..0, 0..64)]);
}

#[test]
fn chunk_records_keep_about_forty_bytes_each() {
    let data = bytes(1024 * 1024, 5);
    let (events, _keep) = mpsc::sync_channel(crate::bytes::EVENT_BUFFER);
    let cancel = CancelToken::new();
    let total = u64::try_from(data.len()).unwrap();
    let mut reporter = Reporter::new(&events, &cancel, total, Budget::default());
    let chunks = chunks(
        Box::new(Cursor::new(data.clone())),
        &mut reporter,
        Budget::default(),
        0,
    )
    .unwrap()
    .unwrap();
    assert_eq!(chunks.len, total);
    assert!(chunks.entries.len() > 64);
    let record = std::mem::size_of::<Candidate<[u8; 32]>>();
    assert_eq!(record, 40);
    let tracked = chunks.entries.len() * record;
    assert!(tracked <= data.len() / 100, "records use {tracked} bytes");
}

#[test]
fn cancel_before_start_emits_nothing() {
    let cancel = CancelToken::new();
    cancel.cancel();
    let request = Request {
        left: memory_side(vec![1; 64]),
        right: memory_side(vec![2; 64]),
        align: Align::Content,
        budget: Budget::default(),
    };
    let found: Vec<ByteEvent> = ByteDiff::start(request, &cancel).events().iter().collect();
    assert!(found.is_empty());
}

#[test]
fn a_side_that_does_not_match_its_listed_size_fails() {
    let data = bytes(100_000, 6);
    let found = events(stale_side(data.clone(), data.len() + 1), memory_side(data));
    assert!(found.contains(&ByteEvent::Failed(Reason::ChangedOnDisk)));
    assert!(!found.contains(&ByteEvent::Done));
}

#[test]
fn cancel_during_the_refinement_read_emits_no_region_or_done() {
    let base = bytes(200_000, 31);
    let mut right = base.clone();
    for index in 100..103 {
        right[index] = !base[index];
    }
    let cancel = CancelToken::new();
    let left = memory_side(base);
    let right = cancelling_side(right, &cancel);
    let request = Request {
        left,
        right,
        align: Align::Content,
        budget: Budget::default(),
    };
    let found: Vec<ByteEvent> = ByteDiff::start(request, &cancel).events().iter().collect();
    assert!(cancel.is_cancelled());
    assert!(!found.iter().any(|event| matches!(
        event,
        ByteEvent::Region(_) | ByteEvent::Done | ByteEvent::Failed(_)
    )));
}

#[test]
fn a_cancelled_reporter_sends_nothing() {
    let (events, receiver) = mpsc::sync_channel(crate::bytes::EVENT_BUFFER);
    let cancel = CancelToken::new();
    cancel.cancel();
    let mut reporter = Reporter::new(&events, &cancel, 0, Budget::default());
    assert!(!reporter.send(ByteEvent::Done));
    drop(events);
    assert!(receiver.iter().next().is_none());
}

#[test]
fn cancelled_chunking_starts_no_more_reads() {
    let data = bytes(4096, 41);
    let cancel = CancelToken::new();
    let left_reads = Arc::new(AtomicUsize::new(0));
    let right_reads = Arc::new(AtomicUsize::new(0));
    let left = Box::new(ShortReads {
        inner: Cursor::new(data.clone()),
        reads: Arc::clone(&left_reads),
        cancel: cancel.clone(),
        cancel_at: 3,
    });
    let right = Box::new(ShortReads {
        inner: Cursor::new(data.clone()),
        reads: Arc::clone(&right_reads),
        cancel: cancel.clone(),
        cancel_at: 3,
    });
    let (events, _keep) = mpsc::sync_channel(crate::bytes::EVENT_BUFFER);
    let total = u64::try_from(data.len()).unwrap();
    let mut reporter = Reporter::new(&events, &cancel, total, Budget::default());
    assert!(matches!(
        chunks(left, &mut reporter, Budget::default(), 0),
        Err(Reason::Stopped)
    ));
    assert_eq!(left_reads.load(Ordering::SeqCst), 3);
    assert!(matches!(
        chunks(right, &mut reporter, Budget::default(), 0),
        Err(Reason::Stopped)
    ));
    assert_eq!(right_reads.load(Ordering::SeqCst), 0);
}

#[test]
fn interrupted_retries_stop_after_a_cancel() {
    let cancel = CancelToken::new();
    let left_reads = Arc::new(AtomicUsize::new(0));
    let mut left = super::Cursor::new(
        Box::new(InterruptedReads {
            reads: Arc::clone(&left_reads),
            cancel: cancel.clone(),
        }),
        cancel.clone(),
        1,
    );
    let mut buffer = [0_u8; 1];
    assert_eq!(left.read_into(&mut buffer), Err(Reason::Stopped));
    let right_reads = Arc::new(AtomicUsize::new(0));
    let mut right = super::Cursor::new(
        Box::new(InterruptedReads {
            reads: Arc::clone(&right_reads),
            cancel: cancel.clone(),
        }),
        cancel,
        1,
    );
    assert_eq!(right.read_into(&mut buffer), Err(Reason::Stopped));
    assert_eq!(left_reads.load(Ordering::SeqCst), 1);
    assert_eq!(right_reads.load(Ordering::SeqCst), 0);

    let cancel = CancelToken::new();
    let reads = Arc::new(AtomicUsize::new(0));
    let mut cursor = super::Cursor::new(
        Box::new(InterruptedReads {
            reads: Arc::clone(&reads),
            cancel: cancel.clone(),
        }),
        cancel,
        0,
    );
    assert_eq!(cursor.check_end(), Err(Reason::Stopped));
    assert_eq!(reads.load(Ordering::SeqCst), 1);
}

fn capped_gap(size: usize, seed: u64) -> (Vec<ByteEvent>, usize) {
    let left = bytes(size, seed);
    let at = size / 2;
    let mut right = left.clone();
    right[at] = fresh_byte(&left, at);
    let len = u64::try_from(size).unwrap();
    let cancel = CancelToken::new();
    let (events, receiver) = mpsc::sync_channel(crate::bytes::EVENT_BUFFER);
    let mut reporter = Reporter::new(&events, &cancel, 0, Budget::default());
    let mut sides = Cursors {
        left: super::Cursor::new(Box::new(Cursor::new(left)), cancel.clone(), len),
        right: super::Cursor::new(Box::new(Cursor::new(right)), cancel.clone(), len),
    };
    let mut scratch = vec![0_u8; SCAN_BLOCK];
    let gap = Gap {
        left: 0..len,
        right: 0..len,
    };
    emit_gap(&mut sides, gap, &mut reporter, &mut scratch).unwrap();
    reporter.finish();
    drop(events);
    (receiver.iter().collect(), at)
}

#[test]
fn refinement_cap_switches_at_sixty_four_kib_per_side() {
    for size in [65_535_usize, 65_536] {
        let (found, at) = capped_gap(size, 71);
        assert_eq!(regions(&found), vec![range(at, at + 1)], "{size}");
    }
    let (found, at) = capped_gap(65_537, 71);
    assert_eq!(regions(&found), vec![range(at, 65_537)]);
}
