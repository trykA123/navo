use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::time::Duration;

use imara_diff::{Algorithm, Diff, InternedInput};
use navo_core::paging::{ByteSource, TextLayout};

use super::*;
use crate::model::{Budget, CancelToken, Reason};
use crate::text::diff::diff_lines;
use crate::text::scan::Candidate as Scanned;
use crate::text::side::{Line, LineEnding, LineSet, detect};

mod budget;
mod cancel;
mod review;

struct Memory(Vec<u8>);

impl ByteSource for Memory {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.0.as_slice().read_at(offset, buf)
    }
}

pub(super) struct Generated {
    pub(super) left: String,
    pub(super) right: String,
    pub(super) modified: Vec<(u32, u32)>,
}

pub(super) fn generate(lines: u32) -> Generated {
    let mut left = String::new();
    let mut right = String::new();
    let mut modified = Vec::new();
    let mut right_line = 0_u32;
    for line in 0..lines {
        left.push_str(&format!("{line:08x}\n"));
        match line % 997 {
            200 => continue,
            500 => {
                right.push_str(&format!("m{line:07x}\n"));
                modified.push((line, right_line));
            }
            800 => {
                right.push_str(&format!("{line:08x}\nn{line:07x}\n"));
                right_line += 1;
            }
            _ => right.push_str(&format!("{line:08x}\n")),
        }
        right_line += 1;
    }
    Generated {
        left,
        right,
        modified,
    }
}

pub(super) fn side(bytes: &[u8]) -> LargeSide {
    let layout = detect(
        bytes,
        navo_core::encoding::Encoding::for_label(b"utf-8").unwrap(),
    );
    LargeSide {
        source: Arc::new(Memory(bytes.to_vec())),
        layout,
    }
}

pub(super) fn request(left: LargeSide, right: LargeSide, focus: u64) -> Request {
    Request {
        left,
        right,
        options: Options::default(),
        focus,
        budget: Budget::default(),
    }
}

pub(super) fn collect(diff: &LargeDiff) -> Vec<DiffEvent> {
    let mut events = Vec::new();
    loop {
        let event = diff
            .events()
            .recv_timeout(Duration::from_secs(120))
            .expect("the driver always ends with Done");
        let done = matches!(event, DiffEvent::Done(_));
        events.push(event);
        if done {
            return events;
        }
    }
}

pub(super) fn run(request: Request, tuning: Tuning) -> Vec<DiffEvent> {
    let diff = LargeDiff::start_tuned(request, &CancelToken::new(), tuning);
    collect(&diff)
}

pub(super) fn tuning(batch_lines: u64) -> Tuning {
    Tuning {
        batch_lines,
        watchdog: Duration::from_secs(60),
        ..Tuning::default()
    }
}

fn last_run(events: &[DiffEvent]) -> &[DiffEvent] {
    let start = events
        .iter()
        .rposition(|event| matches!(event, DiffEvent::Restarted))
        .map_or(0, |at| at + 1);
    &events[start..]
}

pub(super) fn covered(events: &[DiffEvent]) -> Vec<&Covered> {
    last_run(events)
        .iter()
        .filter_map(|event| match event {
            DiffEvent::Hunks(covered) => Some(covered),
            _ => None,
        })
        .collect()
}

pub(super) fn hunks(events: &[DiffEvent]) -> Vec<Hunk> {
    let mut all: Vec<Hunk> = covered(events)
        .into_iter()
        .flat_map(|covered| covered.hunks.clone())
        .collect();
    all.sort_by_key(|hunk| hunk.left_lines.start);
    all
}

pub(super) fn line_ranges(hunks: &[Hunk]) -> Vec<(Range<usize>, Range<usize>)> {
    hunks
        .iter()
        .map(|hunk| (hunk.left_lines.clone(), hunk.right_lines.clone()))
        .collect()
}

pub(super) fn reference(left: &str, right: &str) -> Vec<(Range<usize>, Range<usize>)> {
    let input = InternedInput::new(left, right);
    let diff = Diff::compute(Algorithm::Histogram, &input);
    diff.hunks()
        .map(|hunk| {
            (
                hunk.before.start as usize..hunk.before.end as usize,
                hunk.after.start as usize..hunk.after.end as usize,
            )
        })
        .collect()
}

pub(super) fn summary(events: &[DiffEvent]) -> Summary {
    match events.last() {
        Some(DiffEvent::Done(Ok(summary))) => summary.clone(),
        other => panic!("the diff must finish, got {other:?}"),
    }
}

pub(super) fn failure(events: &[DiffEvent]) -> Reason {
    match events.last() {
        Some(DiffEvent::Done(Err(reason))) => *reason,
        other => panic!("the diff must fail, got {other:?}"),
    }
}

pub(super) fn assert_tiles(events: &[DiffEvent], left_lines: u64, right_lines: u64) {
    let mut ranges: Vec<(Range<u64>, Range<u64>)> = covered(events)
        .into_iter()
        .map(|covered| (covered.left.clone(), covered.right.clone()))
        .collect();
    ranges.sort_by_key(|(left, right)| (left.start, right.start));
    let (mut left_at, mut right_at) = (0, 0);
    for (left, right) in ranges {
        assert_eq!(
            (left.start, right.start),
            (left_at, right_at),
            "batches cover each line once"
        );
        left_at = left.end;
        right_at = right.end;
    }
    assert_eq!(
        (left_at, right_at),
        (left_lines, right_lines),
        "batches reach both ends"
    );
}

#[test]
fn two_million_unique_lines_equal_whole_file_histogram() {
    let generated = generate(2_000_000);
    let events = run(
        request(
            side(generated.left.as_bytes()),
            side(generated.right.as_bytes()),
            0,
        ),
        Tuning::default(),
    );
    let done = summary(&events);
    assert!(
        done.sections > 1_000,
        "the sample splits a 2M-line pair into many sections"
    );
    assert_eq!(done.dropped_anchors, 0);
    assert_eq!(
        line_ranges(&hunks(&events)),
        reference(&generated.left, &generated.right)
    );
    assert_tiles(&events, done.left_lines, done.right_lines);
}

fn split(bytes: &[u8]) -> (Vec<Line>, Vec<u64>) {
    let bom = if bytes.starts_with(&[0xEF, 0xBB, 0xBF]) {
        3
    } else {
        0
    };
    let mut lines = Vec::new();
    let mut starts = vec![bom as u64];
    let mut at = bom;
    for (index, segment) in bytes[bom..]
        .split_inclusive(|byte| *byte == b'\n')
        .enumerate()
    {
        let text = std::str::from_utf8(segment).unwrap();
        let (body, ending) = match text.strip_suffix('\n') {
            Some(rest) => match rest.strip_suffix('\r') {
                Some(body) => (body, LineEnding::CrLf),
                None => (rest, LineEnding::Lf),
            },
            None => (text, LineEnding::None),
        };
        lines.push(Line {
            text: body.to_owned(),
            ending,
            bom: bom > 0 && index == 0,
        });
        at += segment.len();
        starts.push(at as u64);
    }
    (lines, starts)
}

#[test]
fn byte_ranges_match_the_whole_tier_with_bom_and_crlf() {
    let generated = generate(30_000);
    let mut left = vec![0xEF, 0xBB, 0xBF];
    left.extend(generated.left.replace("0\n", "0\r\n").bytes());
    left.extend(b"tail without newline");
    let mut right = vec![0xEF, 0xBB, 0xBF];
    right.extend(generated.right.bytes());
    right.extend(b"other tail");
    let events = run(request(side(&left), side(&right), 0), tuning(512));
    let (left_lines, left_starts) = split(&left);
    let (right_lines, right_starts) = split(&right);
    let whole = diff_lines(
        LineSet {
            lines: &left_lines,
            starts: &left_starts,
            bom: true,
            bytes: Some(&left),
        },
        LineSet {
            lines: &right_lines,
            starts: &right_starts,
            bom: true,
            bytes: Some(&right),
        },
        &Options::default(),
    );
    assert!(!whole.hunks.is_empty(), "the sides differ");
    assert_eq!(hunks(&events), whole.hunks);
    let done = summary(&events);
    assert_eq!(done.left_lines, left_lines.len() as u64);
}

#[test]
fn focused_section_arrives_first_then_line_order() {
    let generated = generate(20_000);
    let focus = 10_000;
    let events = run(
        request(
            side(generated.left.as_bytes()),
            side(generated.right.as_bytes()),
            focus,
        ),
        tuning(256),
    );
    let done = summary(&events);
    let provisional = events
        .iter()
        .position(|event| matches!(event, DiffEvent::Provisional { .. }))
        .expect("a provisional top diff comes first");
    let first_hunks = events
        .iter()
        .position(|event| matches!(event, DiffEvent::Hunks(_)))
        .expect("hunks arrive");
    assert!(
        provisional < first_hunks,
        "the provisional diff precedes section hunks"
    );
    let batches = covered(&events);
    assert!(batches.len() > 10, "many batches make the order meaningful");
    assert!(
        batches[0].left.contains(&focus),
        "the focused batch arrives first"
    );
    let rest: Vec<u64> = batches[1..]
        .iter()
        .map(|covered| covered.left.start)
        .collect();
    let mut sorted = rest.clone();
    sorted.sort_unstable();
    assert_eq!(rest, sorted, "the other batches arrive top-down");
    assert_tiles(&events, done.left_lines, done.right_lines);
    assert_eq!(
        line_ranges(&hunks(&events)),
        reference(&generated.left, &generated.right)
    );
}

#[test]
fn provisional_diff_aligns_the_top() {
    let generated = generate(5_000);
    let events = run(
        request(
            side(generated.left.as_bytes()),
            side(generated.right.as_bytes()),
            0,
        ),
        Tuning::default(),
    );
    let provisional = events
        .iter()
        .find_map(|event| match event {
            DiffEvent::Provisional { hunks, .. } => Some(hunks.clone()),
            _ => None,
        })
        .expect("a provisional diff arrives");
    assert_eq!(
        line_ranges(&provisional),
        reference(&generated.left, &generated.right)
    );
}

pub(super) fn forge_at(left_line: u32, right_line: u32) -> Forge {
    forge_many(&[(left_line, right_line)])
}

pub(super) fn forge_many(pairs: &[(u32, u32)]) -> Forge {
    const FORGED: u64 = 0x0DD0_C0DE_0000_0020;
    let pairs = pairs.to_vec();
    Arc::new(move |left: &mut Vec<Scanned>, right: &mut Vec<Scanned>| {
        let forged = |hash: u64| (FORGED..FORGED + 0x100).contains(&hash);
        let lines = |pick: fn(&(u32, u32)) -> u32| pairs.iter().map(pick).collect::<Vec<u32>>();
        let (left_lines, right_lines) = (lines(|pair| pair.0), lines(|pair| pair.1));
        left.retain(|candidate| !left_lines.contains(&candidate.line) && !forged(candidate.hash));
        right.retain(|candidate| !right_lines.contains(&candidate.line) && !forged(candidate.hash));
        for (at, &(left_line, right_line)) in pairs.iter().enumerate() {
            let hash = FORGED + at as u64;
            left.push(Scanned {
                hash,
                line: left_line,
            });
            right.push(Scanned {
                hash,
                line: right_line,
            });
        }
        left.sort_by_key(|candidate| candidate.line);
        right.sort_by_key(|candidate| candidate.line);
    })
}

#[test]
fn a_forced_hash_collision_drops_the_anchor_and_stays_exact() {
    let generated = generate(20_000);
    let (left_line, right_line) = generated.modified[7];
    let expected = reference(&generated.left, &generated.right);
    for batch_lines in [1, 64, u64::MAX] {
        let events = run(
            request(
                side(generated.left.as_bytes()),
                side(generated.right.as_bytes()),
                0,
            ),
            Tuning {
                batch_lines,
                forge: Some(forge_at(left_line, right_line)),
                watchdog: Duration::from_secs(60),
                ..Tuning::default()
            },
        );
        let done = summary(&events);
        assert_eq!(done.dropped_anchors, 1, "batch size {batch_lines}");
        assert_eq!(
            line_ranges(&hunks(&events)),
            expected,
            "batch size {batch_lines}"
        );
        assert_tiles(&events, done.left_lines, done.right_lines);
    }
}

#[test]
fn a_collision_on_the_focused_batch_boundary_stays_exact() {
    let generated = generate(20_000);
    let (left_line, right_line) = generated.modified[9];
    let events = run(
        request(
            side(generated.left.as_bytes()),
            side(generated.right.as_bytes()),
            u64::from(left_line) + 1,
        ),
        Tuning {
            batch_lines: 1,
            forge: Some(forge_at(left_line, right_line)),
            watchdog: Duration::from_secs(60),
            ..Tuning::default()
        },
    );
    assert_eq!(summary(&events).dropped_anchors, 1);
    assert_eq!(
        line_ranges(&hunks(&events)),
        reference(&generated.left, &generated.right)
    );
}

#[test]
fn equal_and_empty_sides_finish_without_hunks() {
    let generated = generate(3_000);
    let events = run(
        request(
            side(generated.left.as_bytes()),
            side(generated.left.as_bytes()),
            0,
        ),
        tuning(100),
    );
    assert!(hunks(&events).is_empty());
    assert_eq!(summary(&events).left_lines, 3_000);
    let events = run(request(side(b""), side(b""), 0), Tuning::default());
    assert!(hunks(&events).is_empty());
    assert_eq!(summary(&events).left_lines, 0);
    let events = run(
        request(side(b""), side(b"one line here\n"), 0),
        Tuning::default(),
    );
    assert_eq!(line_ranges(&hunks(&events)), vec![(0..0, 0..1)]);
}

struct Swap {
    before: Vec<u8>,
    after: Vec<u8>,
    reads: AtomicUsize,
    swap_after: usize,
    swapped: AtomicBool,
    unreported: AtomicBool,
}

impl Swap {
    fn new(before: Vec<u8>, after: Vec<u8>, swap_after: usize) -> Self {
        Self {
            before,
            after,
            reads: AtomicUsize::new(0),
            swap_after,
            swapped: AtomicBool::new(false),
            unreported: AtomicBool::new(false),
        }
    }

    fn swap(&self) {
        if !self.swapped.swap(true, Ordering::SeqCst) {
            self.unreported.store(true, Ordering::SeqCst);
        }
    }

    fn bytes(&self) -> &[u8] {
        if self.swapped.load(Ordering::SeqCst) {
            &self.after
        } else {
            &self.before
        }
    }
}

impl ByteSource for Swap {
    fn len(&self) -> u64 {
        self.bytes().len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        if self.reads.fetch_add(1, Ordering::SeqCst) + 1 >= self.swap_after {
            self.swap();
        }
        self.bytes().read_at(offset, buf)
    }

    fn take_changed(&self) -> bool {
        self.unreported.swap(false, Ordering::SeqCst)
    }
}

fn swap_side(source: Arc<Swap>) -> LargeSide {
    LargeSide {
        source,
        layout: TextLayout::default(),
    }
}

#[test]
fn a_source_shrinking_during_the_scan_restarts() {
    let generated = generate(600_000);
    let shrunk = generated.left[..generated.left.len() / 2].to_owned();
    let source = Arc::new(Swap::new(
        generated.left.clone().into_bytes(),
        shrunk.clone().into_bytes(),
        3,
    ));
    let events = run(
        request(swap_side(source), side(generated.right.as_bytes()), 0),
        Tuning::default(),
    );
    assert!(
        events
            .iter()
            .any(|event| matches!(event, DiffEvent::Restarted))
    );
    let done = summary(&events);
    assert!(done.restarts >= 1);
    assert_eq!(
        line_ranges(&hunks(&events)),
        reference(&shrunk, &generated.right)
    );
}

#[test]
fn a_source_shrinking_during_the_sections_restarts() {
    let generated = generate(60_000);
    let shrunk = generated.left[..generated.left.len() / 3].to_owned();
    let source = Arc::new(Swap::new(
        generated.left.clone().into_bytes(),
        shrunk.clone().into_bytes(),
        usize::MAX,
    ));
    let trigger = Arc::clone(&source);
    let forge: Forge = Arc::new(move |_: &mut Vec<Scanned>, _: &mut Vec<Scanned>| trigger.swap());
    let events = run(
        request(swap_side(source), side(generated.right.as_bytes()), 0),
        Tuning {
            batch_lines: 1_000,
            forge: Some(forge),
            watchdog: Duration::from_secs(60),
            ..Tuning::default()
        },
    );
    let restarted = events
        .iter()
        .position(|event| matches!(event, DiffEvent::Restarted))
        .expect("the shrink restarts the diff");
    assert!(
        events[restarted..]
            .iter()
            .any(|event| matches!(event, DiffEvent::Provisional { .. })),
        "the restarted run aligns the top again"
    );
    assert_eq!(summary(&events).restarts, 1);
    assert_eq!(
        line_ranges(&hunks(&events)),
        reference(&shrunk, &generated.right)
    );
}

struct Restless(Vec<u8>);

impl ByteSource for Restless {
    fn len(&self) -> u64 {
        self.0.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.0.as_slice().read_at(offset, buf)
    }

    fn take_changed(&self) -> bool {
        true
    }
}

#[test]
fn a_source_that_keeps_changing_gives_up() {
    let generated = generate(2_000);
    let restless = LargeSide {
        source: Arc::new(Restless(generated.left.into_bytes())),
        layout: TextLayout::default(),
    };
    let events = run(
        request(restless, side(generated.right.as_bytes()), 0),
        Tuning::default(),
    );
    let restarts = events
        .iter()
        .filter(|event| matches!(event, DiffEvent::Restarted))
        .count();
    assert_eq!(restarts, MAX_RESTARTS as usize);
    assert_eq!(failure(&events), Reason::ChangedOnDisk);
}

#[derive(Default)]
struct Watch {
    reads: AtomicUsize,
    after_cancel: AtomicUsize,
    bytes_after_cancel: AtomicUsize,
    armed: AtomicBool,
}

enum Fault {
    Cancel { at: usize, token: CancelToken },
    CancelWhenArmed(CancelToken),
    PanicWhenArmed,
    FailAt(usize),
}

struct Faulty {
    bytes: Vec<u8>,
    watch: Arc<Watch>,
    fault: Fault,
}

impl ByteSource for Faulty {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let read = self.watch.reads.fetch_add(1, Ordering::SeqCst) + 1;
        let armed = self.watch.armed.load(Ordering::SeqCst);
        match &self.fault {
            Fault::Cancel { token, .. } | Fault::CancelWhenArmed(token) if token.is_cancelled() => {
                self.watch.after_cancel.fetch_add(1, Ordering::SeqCst);
                self.watch
                    .bytes_after_cancel
                    .fetch_add(buf.len(), Ordering::SeqCst);
            }
            Fault::Cancel { at, token } if read == *at => token.cancel(),
            Fault::CancelWhenArmed(token) if armed => token.cancel(),
            Fault::PanicWhenArmed if armed => panic!("a test source fails inside a worker"),
            Fault::FailAt(at) if read >= *at => return Err("device gone".to_owned()),
            _ => {}
        }
        self.bytes.as_slice().read_at(offset, buf)
    }
}

fn faulty(bytes: &str, watch: &Arc<Watch>, fault: Fault) -> LargeSide {
    LargeSide {
        source: Arc::new(Faulty {
            bytes: bytes.as_bytes().to_vec(),
            watch: Arc::clone(watch),
            fault,
        }),
        layout: TextLayout::default(),
    }
}

fn arm(watch: &Arc<Watch>) -> Forge {
    let watch = Arc::clone(watch);
    Arc::new(move |_: &mut Vec<Scanned>, _: &mut Vec<Scanned>| {
        watch.armed.store(true, Ordering::SeqCst);
    })
}

#[test]
fn cancel_during_the_scan_stops_within_one_block() {
    let generated = generate(700_000);
    let token = CancelToken::new();
    let watch = Arc::new(Watch::default());
    let left = faulty(
        &generated.left,
        &watch,
        Fault::Cancel {
            at: 4,
            token: token.clone(),
        },
    );
    let right = faulty(
        &generated.right,
        &watch,
        Fault::Cancel {
            at: 4,
            token: token.clone(),
        },
    );
    let diff = LargeDiff::start(request(left, right, 0), &token);
    let events = collect(&diff);
    assert_eq!(failure(&events), Reason::Stopped);
    assert!(
        watch.after_cancel.load(Ordering::SeqCst) <= 3,
        "each of the three reading threads starts at most one read after cancel"
    );
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, DiffEvent::Hunks(_)))
    );
}

#[test]
fn cancel_during_the_sections_stops_within_one_block() {
    let generated = generate(700_000);
    let token = CancelToken::new();
    let watch = Arc::new(Watch::default());
    let left = faulty(
        &generated.left,
        &watch,
        Fault::CancelWhenArmed(token.clone()),
    );
    let right = side(generated.right.as_bytes());
    let diff = LargeDiff::start_tuned(
        request(left, right, 0),
        &token,
        Tuning {
            batch_lines: u64::MAX,
            forge: Some(arm(&watch)),
            watchdog: Duration::from_secs(60),
            ..Tuning::default()
        },
    );
    let events = collect(&diff);
    assert_eq!(failure(&events), Reason::Stopped);
    assert!(
        watch.bytes_after_cancel.load(Ordering::SeqCst) <= crate::text::scan::BLOCK,
        "the worker stops before its next block"
    );
}

#[test]
fn a_panicking_worker_ends_the_diff() {
    let generated = generate(20_000);
    let watch = Arc::new(Watch::default());
    let left = faulty(&generated.left, &watch, Fault::PanicWhenArmed);
    let events = run(
        request(left, side(generated.right.as_bytes()), 0),
        Tuning {
            batch_lines: 500,
            forge: Some(arm(&watch)),
            watchdog: Duration::from_secs(60),
            ..Tuning::default()
        },
    );
    assert_eq!(failure(&events), Reason::WorkerPanicked);
}

#[test]
fn a_read_error_ends_the_diff() {
    let generated = generate(20_000);
    let watch = Arc::new(Watch::default());
    let left = faulty(&generated.left, &watch, Fault::FailAt(2));
    let events = run(
        request(left, side(generated.right.as_bytes()), 0),
        Tuning::default(),
    );
    assert_eq!(failure(&events), Reason::ReadError);
}

struct Counting {
    bytes: Vec<u8>,
    reads: Arc<AtomicUsize>,
}

impl ByteSource for Counting {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.bytes.as_slice().read_at(offset, buf)
    }
}

#[test]
fn dropping_the_diff_cancels_its_work() {
    let generated = generate(2_000_000);
    let token = CancelToken::new();
    let reads = Arc::new(AtomicUsize::new(0));
    let counting = |text: &str| LargeSide {
        source: Arc::new(Counting {
            bytes: text.as_bytes().to_vec(),
            reads: Arc::clone(&reads),
        }),
        layout: side(text.as_bytes()).layout,
    };
    let left = counting(&generated.left);
    let right = counting(&generated.right);
    let diff = LargeDiff::start(request(left, right, 0), &token);
    for _ in 0..5_000 {
        if reads.load(Ordering::SeqCst) > 0 {
            break;
        }
        std::thread::sleep(Duration::from_millis(1));
    }
    drop(diff);
    assert!(token.is_cancelled());
    std::thread::sleep(Duration::from_millis(300));
    let settled = reads.load(Ordering::SeqCst);
    std::thread::sleep(Duration::from_millis(300));
    assert_eq!(reads.load(Ordering::SeqCst), settled);
}
