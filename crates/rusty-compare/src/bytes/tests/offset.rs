use std::io::{self, Cursor, Read};
use std::ops::ControlFlow;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use rusty_core::{FileIdentity, ProviderId, ProviderPath};

use crate::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};

use crate::bytes::{Align, ByteDiff, ByteEvent, ByteRegion, ByteSide};

use super::{BLOCK, Job, run};

type Factory = Box<dyn Fn() -> Box<dyn Read + Send> + Send + Sync>;

struct TestSource {
    provider: ProviderId,
    open: Factory,
}

impl TestSource {
    fn new(open: Factory) -> Self {
        Self {
            provider: ProviderId::new("memory").unwrap(),
            open,
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
        Ok((self.open)())
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

fn side(len: usize, open: impl Fn() -> Box<dyn Read + Send> + Send + Sync + 'static) -> ByteSide {
    let len = u64::try_from(len).unwrap();
    let source = Arc::new(TestSource::new(Box::new(open)));
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("file")),
        EntryKind::File,
        len,
        None,
    );
    ByteSide::new(source, entry)
}

fn memory_side(bytes: Vec<u8>) -> ByteSide {
    let len = bytes.len();
    stale_side(bytes, len)
}

fn stale_side(bytes: Vec<u8>, listed: usize) -> ByteSide {
    let data = Arc::new(bytes);
    side(listed, move || {
        Box::new(Cursor::new(Arc::clone(&data).as_ref().clone())) as Box<dyn Read + Send>
    })
}

struct CancellingRead {
    inner: Cursor<Vec<u8>>,
    cancel: CancelToken,
    first: bool,
}

impl Read for CancellingRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        if self.first {
            self.first = false;
            self.cancel.cancel();
        }
        self.inner.read(buffer)
    }
}

fn cancelling_side(bytes: Vec<u8>, cancel: CancelToken) -> ByteSide {
    let data = Arc::new(bytes);
    let len = data.len();
    side(len, move || {
        Box::new(CancellingRead {
            inner: Cursor::new(Arc::clone(&data).as_ref().clone()),
            cancel: cancel.clone(),
            first: true,
        }) as Box<dyn Read + Send>
    })
}

struct BrokenRead;

impl Read for BrokenRead {
    fn read(&mut self, _buffer: &mut [u8]) -> io::Result<usize> {
        Err(io::Error::other("broken"))
    }
}

fn broken_side(len: usize) -> ByteSide {
    side(len, || Box::new(BrokenRead) as Box<dyn Read + Send>)
}

fn start(left: ByteSide, right: ByteSide, align: Align) -> Receiver<ByteEvent> {
    ByteDiff::start(left, right, align, &CancelToken::new())
}

fn events(left: ByteSide, right: ByteSide) -> Vec<ByteEvent> {
    start(left, right, Align::Offset).iter().collect()
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

fn region(start: usize, end: usize) -> ByteRegion {
    let start = u64::try_from(start).unwrap();
    let end = u64::try_from(end).unwrap();
    ByteRegion {
        left: start..end,
        right: start..end,
    }
}

fn patched(len: usize, ranges: &[(usize, usize)]) -> (Vec<u8>, Vec<u8>) {
    let left = vec![0u8; len];
    let mut right = left.clone();
    for &(start, end) in ranges {
        right[start..end].fill(0xFF);
    }
    (left, right)
}

#[test]
fn equal_sides_emit_no_region() {
    let bytes = vec![0x5A; 2 * BLOCK + 7];
    let events = events(memory_side(bytes.clone()), memory_side(bytes));
    assert!(regions(&events).is_empty());
    assert_eq!(events.last(), Some(&ByteEvent::Done));
}

#[test]
fn patched_ranges_at_block_edges_are_exact() {
    let len = 2 * BLOCK + 64;
    for (start, end) in [
        (BLOCK - 1, BLOCK + 1),
        (BLOCK, BLOCK + 1),
        (BLOCK + 1, BLOCK + 3),
        (BLOCK - 2, BLOCK),
    ] {
        let (left, right) = patched(len, &[(start, end)]);
        let events = events(memory_side(left), memory_side(right));
        assert_eq!(regions(&events), vec![region(start, end)], "{start}..{end}");
        assert_eq!(events.last(), Some(&ByteEvent::Done));
    }
}

#[test]
fn changes_under_sixteen_apart_merge() {
    let (left, right) = patched(4096, &[(100, 102), (117, 119)]);
    assert_eq!(
        regions(&events(memory_side(left), memory_side(right))),
        vec![region(100, 119)]
    );
}

#[test]
fn changes_sixteen_apart_stay_separate() {
    let (left, right) = patched(4096, &[(100, 102), (118, 120)]);
    assert_eq!(
        regions(&events(memory_side(left), memory_side(right))),
        vec![region(100, 102), region(118, 120)]
    );
}

#[test]
fn a_longer_right_side_gets_an_empty_left_tail() {
    let left = vec![0u8; 128];
    let mut right = left.clone();
    right.extend_from_slice(&[1, 2, 3, 4]);
    let tail = ByteRegion {
        left: 128..128,
        right: 128..132,
    };
    assert_eq!(
        regions(&events(memory_side(left), memory_side(right))),
        vec![tail]
    );
}

#[test]
fn a_tail_longer_than_a_block_keeps_the_short_side_empty() {
    let left = vec![0u8; 100];
    let mut right = left.clone();
    right.resize(2 * BLOCK + 100, 1);
    let tail = ByteRegion {
        left: 100..100,
        right: 100..u64::try_from(right.len()).unwrap(),
    };
    assert_eq!(
        regions(&events(memory_side(left), memory_side(right))),
        vec![tail]
    );
}

#[test]
fn a_longer_left_side_gets_an_empty_right_tail() {
    let right = vec![0u8; 128];
    let mut left = right.clone();
    left.extend_from_slice(&[1, 2, 3, 4]);
    let tail = ByteRegion {
        left: 128..132,
        right: 128..128,
    };
    assert_eq!(
        regions(&events(memory_side(left), memory_side(right))),
        vec![tail]
    );
}

#[test]
fn a_tail_merges_with_a_change_under_sixteen_away() {
    let left = vec![0u8; 128];
    let mut right = left.clone();
    right[120..123].fill(9);
    right.extend_from_slice(&[1, 2, 3, 4]);
    let merged = ByteRegion {
        left: 120..128,
        right: 120..132,
    };
    assert_eq!(
        regions(&events(memory_side(left), memory_side(right))),
        vec![merged]
    );
}

#[test]
fn a_tail_sixteen_from_a_change_stays_separate() {
    let left = vec![0u8; 128];
    let mut right = left.clone();
    right[110..112].fill(9);
    right.extend_from_slice(&[1, 2, 3, 4]);
    let tail = ByteRegion {
        left: 128..128,
        right: 128..132,
    };
    assert_eq!(
        regions(&events(memory_side(left), memory_side(right))),
        vec![region(110, 112), tail]
    );
}

#[test]
fn regions_arrive_top_down() {
    let (left, right) = patched(4096, &[(10, 12), (100, 105), (1000, 1002)]);
    assert_eq!(
        regions(&events(memory_side(left), memory_side(right))),
        vec![region(10, 12), region(100, 105), region(1000, 1002)]
    );
}

#[test]
fn cancel_stops_within_one_block_without_done() {
    let cancel = CancelToken::new();
    let data = vec![0u8; 3 * BLOCK];
    let left = cancelling_side(data.clone(), cancel.clone());
    let right = memory_side(data);
    let events: Vec<ByteEvent> = ByteDiff::start(left, right, Align::Offset, &cancel)
        .iter()
        .collect();
    assert!(!events.contains(&ByteEvent::Done));
    let progress = events
        .iter()
        .filter(|event| matches!(event, ByteEvent::Progress { .. }))
        .count();
    assert_eq!(progress, 1);
}

#[test]
fn cancel_before_start_emits_nothing() {
    let cancel = CancelToken::new();
    cancel.cancel();
    let events: Vec<ByteEvent> = ByteDiff::start(
        memory_side(vec![1; 64]),
        memory_side(vec![2; 64]),
        Align::Offset,
        &cancel,
    )
    .iter()
    .collect();
    assert!(events.is_empty());
}

#[test]
fn cancel_frees_a_worker_blocked_on_a_full_event_channel() {
    let cancel = CancelToken::new();
    let mut right = vec![0u8; BLOCK];
    for at in (0..BLOCK).step_by(17) {
        right[at] = 1;
    }
    let left = memory_side(vec![0u8; BLOCK]);
    let right = memory_side(right);
    let (events, receiver) = mpsc::sync_channel(crate::bytes::EVENT_BUFFER);
    let (finished, done) = mpsc::channel();
    std::thread::scope(|scope| {
        let cancel_ref = &cancel;
        let events_ref = &events;
        let worker = scope.spawn(move || {
            run(Job {
                left,
                right,
                align: Align::Offset,
                cancel: cancel_ref,
                events: events_ref,
            });
            finished.send(()).expect("the test receiver lives");
        });
        std::thread::sleep(Duration::from_millis(20));
        cancel.cancel();
        let ended = done.recv_timeout(Duration::from_secs(1)).is_ok();
        assert!(
            ended,
            "cancel must free a worker blocked on a full event channel"
        );
        worker.join().unwrap();
    });
    drop(receiver);
}

#[test]
fn a_short_side_against_the_listed_size_fails() {
    let left = stale_side(vec![0u8; 100], 128);
    let right = memory_side(vec![0u8; 100]);
    let events = events(left, right);
    assert!(events.contains(&ByteEvent::Failed(Reason::ChangedOnDisk)));
    assert!(!events.contains(&ByteEvent::Done));
    assert!(regions(&events).is_empty());
}

#[test]
fn a_grown_side_against_the_listed_size_fails() {
    let left = stale_side(vec![0u8; 140], 128);
    let right = memory_side(vec![0u8; 128]);
    let events = events(left, right);
    assert!(events.contains(&ByteEvent::Failed(Reason::ChangedOnDisk)));
    assert!(!events.contains(&ByteEvent::Done));
    assert_eq!(
        regions(&events),
        vec![ByteRegion {
            left: 128..140,
            right: 128..128,
        }]
    );
}

#[test]
fn a_read_error_becomes_failed_without_done() {
    let events = events(broken_side(64), memory_side(vec![0; 64]));
    assert_eq!(events, vec![ByteEvent::Failed(Reason::ReadError)]);
}

#[test]
fn content_alignment_is_unsupported_for_now() {
    let events = start(memory_side(vec![1]), memory_side(vec![2]), Align::Content)
        .iter()
        .collect::<Vec<_>>();
    assert_eq!(events, vec![ByteEvent::Failed(Reason::Unsupported)]);
}
