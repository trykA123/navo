use std::io::{self, Cursor, Read};
use std::ops::ControlFlow;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex, mpsc};
use std::time::Duration;

use rusty_core::{FileIdentity, ProviderId, ProviderPath};

use crate::model::{
    Budget, CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource,
    Stamp,
};

use crate::bytes::{Align, ByteDiff, ByteEvent, ByteRegion, ByteSide, Request};

use super::*;

fn single(at: u64, len: u64) -> ByteRegion {
    ByteRegion::single(at..at + len, at..at + len)
}

#[test]
fn scattered_changes_four_kib_apart_merge_at_the_minimum_budget() {
    let budget = Budget::from_mib(Budget::MIN_MIB);
    let mut list = Regions::new(budget);
    let mut raw = 0_u64;
    for at in (0..6 * 1024 * 1024 * 1024_u64).step_by(4096) {
        list.push(single(at, 1));
        raw += 1;
    }
    assert!(list.used() <= list.limit());
    assert_eq!(list.used(), list.entries.capacity() as u64 * REGION_BYTES);
    let mut emitted = 0;
    let mut changed = 0;
    let mut merged = false;
    while let Some(region) = list.finish() {
        merged |= region.regions > 1;
        emitted += region.regions;
        changed += region.changed;
    }
    assert!(merged);
    assert_eq!(emitted, raw);
    assert_eq!(changed, raw);
}

#[test]
fn under_the_limit_regions_stay_separate() {
    let mut list = Regions::new(Budget::from_mib(Budget::MIN_MIB));
    let first = ByteRegion::single(0..4, 0..6);
    let second = single(10, 2);
    list.push(first.clone());
    list.push(second.clone());
    assert_eq!(list.finish(), Some(first));
    assert_eq!(list.finish(), Some(second));
    assert_eq!(list.finish(), None);
    assert!(list.used() <= list.limit());
}

#[test]
fn merging_keeps_outer_ranges_and_counts() {
    let mut list = Regions::new(Budget::from_bytes(REGION_BYTES * 16));
    for index in 0..10_u64 {
        let at = index * 100;
        list.push(ByteRegion::single(at..at + 1, at..at + 2));
    }
    let out: Vec<_> = std::iter::from_fn(|| list.finish()).collect();
    assert_eq!(out.len(), 4);
    let merged = out.last().unwrap();
    assert_eq!(merged.regions, 7);
    assert_eq!(merged.changed, 14);
    assert_eq!(merged.left, 300..901);
    assert_eq!(merged.right, 300..902);
    assert_eq!(out.iter().map(|region| region.regions).sum::<u64>(), 10);
    assert!(list.used() <= list.limit());
}

#[test]
fn region_records_count_the_retained_capacity() {
    let mut list = Regions::new(Budget::from_bytes(u64::MAX));
    for index in 0..5_u64 {
        list.push(single(index * 10, 1));
    }
    assert_eq!(list.used(), list.entries.capacity() as u64 * REGION_BYTES);
    assert_eq!(list.used(), 8 * REGION_BYTES);
}

struct MemorySource {
    provider: ProviderId,
    data: Arc<Vec<u8>>,
}

impl SideSource for MemorySource {
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
        Ok(Box::new(Cursor::new(
            Arc::clone(&self.data).as_ref().clone(),
        )))
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

fn memory_side(data: Vec<u8>) -> ByteSide {
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("file")),
        EntryKind::File,
        u64::try_from(data.len()).unwrap(),
        None,
    );
    ByteSide::new(
        Arc::new(MemorySource {
            provider: ProviderId::new("memory").unwrap(),
            data: Arc::new(data),
        }),
        entry,
    )
}

fn collect(left: ByteSide, right: ByteSide, align: Align, budget: Budget) -> Vec<ByteRegion> {
    let request = Request {
        left,
        right,
        align,
        budget,
    };
    let diff = ByteDiff::start(request, &CancelToken::new());
    let events: Vec<ByteEvent> = diff.events().iter().collect();
    assert_eq!(events.last(), Some(&ByteEvent::Done));
    assert_memory(&diff, budget);
    regions(events)
}

fn regions(events: Vec<ByteEvent>) -> Vec<ByteRegion> {
    events
        .into_iter()
        .filter_map(|event| match event {
            ByteEvent::Region(region) => Some(region),
            _ => None,
        })
        .collect()
}

fn bytes(len: usize) -> Vec<u8> {
    let mut state = 0x9E37_79B9_7F4A_7C15_u64;
    (0..len)
        .map(|_| {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            (state >> 32) as u8
        })
        .collect()
}

fn patched(base: &[u8], every: usize) -> Vec<u8> {
    let mut right = base.to_vec();
    for index in (every..right.len()).step_by(every) {
        right[index] = !base[index];
    }
    right
}

fn sum_regions(regions: &[ByteRegion]) -> u64 {
    regions.iter().map(|region| region.regions).sum()
}

fn sum_changed(regions: &[ByteRegion]) -> u64 {
    regions.iter().map(|region| region.changed).sum()
}

fn covers(outer: &ByteRegion, inner: &ByteRegion) -> bool {
    outer.left.start <= inner.left.start
        && inner.left.end <= outer.left.end
        && outer.right.start <= inner.right.start
        && inner.right.end <= outer.right.end
}

fn assert_covers(merged: &[ByteRegion], raw: &[ByteRegion]) {
    for region in raw {
        assert!(
            merged.iter().any(|outer| covers(outer, region)),
            "no merged region covers {:?}",
            region
        );
    }
}

#[test]
fn an_unmerged_run_keeps_exact_ranges_and_counts() {
    let base = bytes(64 * 1024);
    let right = patched(&base, 4 * 1024);
    let regions = collect(
        memory_side(base),
        memory_side(right),
        Align::Offset,
        Budget::default(),
    );
    assert_eq!(regions.len(), 15);
    for region in &regions {
        assert_eq!(region.regions, 1);
        assert_eq!(region.changed, 1);
        assert_eq!(region.left, region.right);
        assert_eq!(region.left.end - region.left.start, 1);
    }
}

#[test]
fn merged_offset_regions_cover_changed_bytes_and_add_up() {
    let base = bytes(128 * 1024);
    let right = patched(&base, 32);
    let raw = collect(
        memory_side(base.clone()),
        memory_side(right.clone()),
        Align::Offset,
        Budget::default(),
    );
    let merged = collect(
        memory_side(base),
        memory_side(right),
        Align::Offset,
        Budget::from_bytes(REGION_BYTES * 8 * 4),
    );
    assert!(
        merged.iter().any(|region| region.regions > 1),
        "raw {} merged {}",
        raw.len(),
        merged.len()
    );
    assert_eq!(sum_regions(&merged), u64::try_from(raw.len()).unwrap());
    assert_eq!(sum_changed(&merged), sum_changed(&raw));
    assert_covers(&merged, &raw);
    assert_ordered(&merged);
}

#[test]
fn merged_content_regions_cover_changed_bytes_and_add_up() {
    let base = bytes(6000);
    let right = patched(&base, 32);
    let raw = collect(
        memory_side(base.clone()),
        memory_side(right.clone()),
        Align::Content,
        Budget::default(),
    );
    let merged = collect(
        memory_side(base),
        memory_side(right),
        Align::Content,
        Budget::from_bytes(REGION_BYTES * 8 * 4),
    );
    assert!(
        merged.iter().any(|region| region.regions > 1),
        "raw {} merged {}",
        raw.len(),
        merged.len()
    );
    assert_eq!(sum_regions(&merged), u64::try_from(raw.len()).unwrap());
    assert_eq!(sum_changed(&merged), sum_changed(&raw));
    assert_covers(&merged, &raw);
    assert_ordered(&merged);
}

#[derive(Clone)]
struct Probe {
    reads: Arc<AtomicU64>,
    after_cancel: Arc<AtomicU64>,
    entered: mpsc::Sender<()>,
    resume: Arc<Mutex<mpsc::Receiver<()>>>,
    stopped: mpsc::Sender<()>,
}

struct VirtualSource {
    provider: ProviderId,
    len: u64,
    cancel: CancelToken,
    probe: Probe,
}

impl SideSource for VirtualSource {
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
        Ok(Box::new(PatternRead {
            at: 0,
            len: self.len,
            cancel: self.cancel.clone(),
            probe: self.probe.clone(),
        }))
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

struct PatternRead {
    at: u64,
    len: u64,
    cancel: CancelToken,
    probe: Probe,
}

impl Read for PatternRead {
    fn read(&mut self, buffer: &mut [u8]) -> io::Result<usize> {
        let read = self.probe.reads.fetch_add(1, Ordering::SeqCst);
        if self.cancel.is_cancelled() {
            let count = u64::try_from(buffer.len()).unwrap();
            self.probe.after_cancel.fetch_add(count, Ordering::SeqCst);
        }
        if read == 0 {
            self.probe.entered.send(()).unwrap();
            self.probe
                .resume
                .lock()
                .unwrap()
                .recv_timeout(Duration::from_secs(5))
                .unwrap();
        }
        let left = (self.len - self.at).min(u64::try_from(buffer.len()).unwrap());
        let count = usize::try_from(left).unwrap();
        buffer[..count].fill(0x5A);
        self.at += left;
        Ok(count)
    }
}

impl Drop for PatternRead {
    fn drop(&mut self) {
        self.probe.stopped.send(()).unwrap();
    }
}

fn virtual_side(len: u64, cancel: &CancelToken, probe: &Probe) -> ByteSide {
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("file")),
        EntryKind::File,
        len,
        None,
    );
    ByteSide::new(
        Arc::new(VirtualSource {
            provider: ProviderId::new("virtual").unwrap(),
            len,
            cancel: cancel.clone(),
            probe: probe.clone(),
        }),
        entry,
    )
}

fn check_handle_drop(align: Align) {
    let cancel = CancelToken::new();
    let (entered, started) = mpsc::channel();
    let (resume, released) = mpsc::channel();
    let (stopped, finished) = mpsc::channel();
    let probe = Probe {
        reads: Arc::new(AtomicU64::new(0)),
        after_cancel: Arc::new(AtomicU64::new(0)),
        entered,
        resume: Arc::new(Mutex::new(released)),
        stopped,
    };
    let len = 3 * super::super::offset::BLOCK;
    let request = Request {
        left: virtual_side(u64::try_from(len).unwrap(), &cancel, &probe),
        right: memory_side(vec![0x5A; len]),
        align,
        budget: Budget::default(),
    };
    let diff = ByteDiff::start(request, &cancel);
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    drop(diff);
    assert!(cancel.is_cancelled());
    resume.send(()).unwrap();
    finished.recv_timeout(Duration::from_secs(5)).unwrap();
    assert_eq!(probe.reads.load(Ordering::SeqCst), 1);
    assert_eq!(probe.after_cancel.load(Ordering::SeqCst), 0);
}

fn assert_ordered(regions: &[ByteRegion]) {
    for pair in regions.windows(2) {
        assert!(pair[0].left.end <= pair[1].left.start);
        assert!(pair[0].right.end <= pair[1].right.start);
    }
}

fn assert_memory(diff: &ByteDiff, budget: Budget) {
    let (construction, regions) = diff.retained_capacity();
    assert!(construction <= budget.bytes() / 2);
    assert!(regions <= budget.bytes() / 4);
    assert!(regions > 0, "worker must retain its output regions");
}

#[test]
fn minimum_region_budget_boundaries_hold_through_byte_diff() {
    let base = bytes(6000);
    let right = patched(&base, 32);
    for align in [Align::Offset, Align::Content] {
        let raw = collect(
            memory_side(base.clone()),
            memory_side(right.clone()),
            align,
            Budget::default(),
        );
        for bytes in [
            REGION_BYTES * 8 * 4 - 1,
            REGION_BYTES * 8 * 4,
            REGION_BYTES * 8 * 4 + 1,
        ] {
            let budget = Budget::from_bytes(bytes);
            let merged = collect(
                memory_side(base.clone()),
                memory_side(right.clone()),
                align,
                budget,
            );
            assert!(merged.iter().any(|region| region.regions > 1));
            assert_eq!(sum_regions(&merged), sum_regions(&raw));
            assert_eq!(sum_changed(&merged), sum_changed(&raw));
            assert_covers(&merged, &raw);
            assert_ordered(&merged);
        }
    }
}

#[test]
fn chunk_budget_fallback_finishes_with_offset_coverage() {
    let base = bytes(1024 * 1024);
    let right = patched(&base, 4096);
    let budget = Budget::from_bytes(REGION_BYTES * 8 * 4);
    let raw = collect(
        memory_side(base.clone()),
        memory_side(right.clone()),
        Align::Offset,
        Budget::default(),
    );
    let diff = ByteDiff::start(
        Request {
            left: memory_side(base),
            right: memory_side(right),
            align: Align::Content,
            budget,
        },
        &CancelToken::new(),
    );
    let found: Vec<_> = diff.events().iter().collect();
    assert_eq!(found.last(), Some(&ByteEvent::Done));
    let step = found
        .iter()
        .position(|event| *event == ByteEvent::Step(crate::model::DiffStep::Coarse))
        .unwrap();
    assert!(
        !found[..step]
            .iter()
            .any(|event| matches!(event, ByteEvent::Region(_)))
    );
    assert_memory(&diff, budget);
    let merged = regions(found);
    assert_eq!(sum_regions(&merged), sum_regions(&raw));
    assert_eq!(sum_changed(&merged), sum_changed(&raw));
    assert_covers(&merged, &raw);
    assert_ordered(&merged);
}

#[test]
fn anchor_construction_capacity_obeys_the_half_budget_boundary() {
    let base = bytes(6000);
    let right = patched(&base, 32);
    let by_offset = collect(
        memory_side(base.clone()),
        memory_side(right.clone()),
        Align::Offset,
        Budget::default(),
    );
    let by_content = collect(
        memory_side(base.clone()),
        memory_side(right.clone()),
        Align::Content,
        Budget::default(),
    );
    let (records, anchors) = crate::bytes::chunk::construction_bytes(4, 4);
    let threshold = 2 * (records + anchors);
    for bytes in [threshold - 1, threshold, threshold + 1] {
        let raw = if bytes < threshold {
            &by_offset
        } else {
            &by_content
        };
        let budget = Budget::from_bytes(bytes);
        let diff = ByteDiff::start(
            Request {
                left: memory_side(base.clone()),
                right: memory_side(right.clone()),
                align: Align::Content,
                budget,
            },
            &CancelToken::new(),
        );
        let events: Vec<_> = diff.events().iter().collect();
        assert_eq!(events.last(), Some(&ByteEvent::Done));
        assert_eq!(
            events.contains(&ByteEvent::Step(crate::model::DiffStep::Coarse)),
            bytes < threshold
        );
        assert_memory(&diff, budget);
        let merged = regions(events);
        assert_eq!(sum_regions(&merged), sum_regions(raw));
        assert_eq!(sum_changed(&merged), sum_changed(raw));
        assert_covers(&merged, raw);
        assert_ordered(&merged);
    }
}

#[test]
fn dropping_the_offset_handle_stops_reads_within_one_block() {
    check_handle_drop(Align::Offset);
}

#[test]
fn dropping_the_content_handle_stops_reads_within_one_block() {
    check_handle_drop(Align::Content);
}
