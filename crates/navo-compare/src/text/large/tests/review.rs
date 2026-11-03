use std::ops::ControlFlow;
use std::ops::Range;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::RecvTimeoutError;
use std::time::Duration;

use navo_core::encoding::Encoding;
use navo_core::paging::{ByteSource, TextLayout};
use navo_core::{FileIdentity, ProviderId, ProviderPath};

use super::super::*;
use super::{
    assert_tiles, covered, forge_at, forge_many, generate, hunks, line_ranges, reference, request,
    run, side, summary, tuning,
};
use crate::model::{
    CancelToken, EntryKind, ObjectId, ReadHint, Reason, RelPath, SideEntry, SideSource, Stamp,
};
use crate::text::side::{TextSide, detect};

struct Tally {
    bytes: Vec<u8>,
    reads: Arc<AtomicUsize>,
}

impl ByteSource for Tally {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        self.reads.fetch_add(1, Ordering::SeqCst);
        self.bytes.as_slice().read_at(offset, buf)
    }
}

fn wait_until_blocked(reads: &AtomicUsize) {
    let mut last = usize::MAX;
    let mut still = 0;
    for _ in 0..600 {
        std::thread::sleep(Duration::from_millis(50));
        let now = reads.load(Ordering::SeqCst);
        still = if now == last && now > 0 { still + 1 } else { 0 };
        last = now;
        if still >= 6 {
            return;
        }
    }
    panic!("the driver never blocked on the full channel");
}

#[test]
fn the_outcome_survives_a_full_event_channel() {
    let generated = generate(20_000);
    let reads = Arc::new(AtomicUsize::new(0));
    let tally = |text: &str| LargeSide {
        source: Arc::new(Tally {
            bytes: text.as_bytes().to_vec(),
            reads: Arc::clone(&reads),
        }),
        layout: TextLayout::default(),
    };
    let token = CancelToken::new();
    let diff = LargeDiff::start_tuned(
        request(tally(&generated.left), tally(&generated.right), 0),
        &token,
        tuning(1),
    );
    wait_until_blocked(&reads);
    token.cancel();
    for _ in 0..2_000 {
        if diff.outcome().is_some() {
            break;
        }
        std::thread::sleep(Duration::from_millis(5));
    }
    // The full buffer drains first; a Done may follow once draining makes room.
    let mut drained = 0;
    while drained < EVENT_BUFFER {
        match diff.events().recv_timeout(Duration::from_secs(30)) {
            Ok(event) => {
                assert!(
                    !matches!(event, DiffEvent::Done(_)),
                    "the channel was full at cancel"
                );
                drained += 1;
            }
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => panic!("the buffer must arrive after cancel"),
        }
    }
    assert_eq!(drained, EVENT_BUFFER, "the channel was full at cancel");
    assert_eq!(diff.outcome(), Some(Err(Reason::Stopped)));
    loop {
        match diff.events().recv_timeout(Duration::from_secs(30)) {
            Ok(_) => {}
            Err(RecvTimeoutError::Disconnected) => break,
            Err(RecvTimeoutError::Timeout) => panic!("the driver must end after cancel"),
        }
    }
}

fn utf16_le(text: &str) -> Vec<u8> {
    let mut bytes = vec![0xFF, 0xFE];
    for unit in text.encode_utf16() {
        bytes.extend(unit.to_le_bytes());
    }
    bytes
}

struct WholeFile(Vec<u8>, ProviderId);

impl SideSource for WholeFile {
    fn provider(&self) -> &ProviderId {
        &self.1
    }

    fn list(
        &self,
        _: &RelPath,
        _: &mut dyn FnMut(SideEntry) -> ControlFlow<()>,
        _: &CancelToken,
    ) -> Result<(), Reason> {
        Ok(())
    }

    fn open(&self, _: &RelPath, _: ReadHint) -> Result<Box<dyn std::io::Read + Send>, Reason> {
        Ok(Box::new(std::io::Cursor::new(self.0.clone())))
    }

    fn stamp(&self, _: &RelPath) -> Result<Stamp, Reason> {
        Ok(Stamp {
            size: self.0.len() as u64,
            ..Stamp::default()
        })
    }

    fn object_id(&self, _: &RelPath) -> Option<ObjectId> {
        None
    }

    fn file_id(&self, _: &RelPath) -> Option<FileIdentity> {
        None
    }

    fn cache_path(&self, _: &RelPath) -> ProviderPath {
        ProviderPath::from_text("whole.txt")
    }

    fn case_sensitive(&self) -> bool {
        true
    }

    fn read_only(&self) -> bool {
        false
    }
}

fn whole(bytes: &[u8]) -> crate::text::side::Whole {
    let source = WholeFile(bytes.to_vec(), ProviderId::local());
    let entry = SideEntry::new(
        RelPath::new(ProviderPath::from_text("whole.txt")),
        EntryKind::File,
        bytes.len() as u64,
        None,
    );
    let fallback = Encoding::for_label(b"utf-8").unwrap();
    match TextSide::decode(&source, &entry, fallback).unwrap() {
        TextSide::Whole(whole) => whole,
        TextSide::Large(_) => panic!("the sample fits the whole tier"),
    }
}

#[test]
fn utf16_le_batches_equal_the_whole_tier() {
    let generated = generate(30_000);
    let left = utf16_le(&generated.left);
    let right = utf16_le(&generated.right);
    let (left_whole, right_whole) = (whole(&left), whole(&right));
    let expected = diff_lines(
        left_whole.line_set(),
        right_whole.line_set(),
        &Options::default(),
    );
    let fallback = Encoding::for_label(b"utf-8").unwrap();
    let plain = |bytes: &[u8]| LargeSide {
        source: Arc::new(Tally {
            bytes: bytes.to_vec(),
            reads: Arc::new(AtomicUsize::new(0)),
        }),
        layout: detect(bytes, fallback),
    };
    let events = run(request(plain(&left), plain(&right), 0), tuning(256));
    let done = summary(&events);
    assert_eq!(done.restarts, 0, "no batch reads a misaligned line start");
    assert!(
        covered(&events).len() > 10,
        "many batches use sparse lookups"
    );
    assert!(!expected.hunks.is_empty());
    assert_eq!(hunks(&events), expected.hunks);
}

struct Slow {
    bytes: Vec<u8>,
    hold: Range<u64>,
    armed: Arc<AtomicBool>,
}

impl ByteSource for Slow {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        if self.armed.load(Ordering::SeqCst) && self.hold.contains(&offset) && buf.len() >= 1024 {
            std::thread::sleep(Duration::from_millis(300));
        }
        self.bytes.as_slice().read_at(offset, buf)
    }
}

#[test]
fn a_focused_batch_with_a_boundary_collision_still_arrives_first() {
    let generated = generate(20_000);
    let (left_line, right_line) = generated.modified[9];
    let focus = u64::from(left_line) + 1;
    let armed = Arc::new(AtomicBool::new(false));
    let near = u64::from(left_line).saturating_sub(300) * 9..(u64::from(left_line) + 300) * 9;
    let left = LargeSide {
        source: Arc::new(Slow {
            bytes: generated.left.clone().into_bytes(),
            hold: near,
            armed: Arc::clone(&armed),
        }),
        layout: TextLayout::default(),
    };
    let forged = forge_at(left_line, right_line);
    let arm = Arc::clone(&armed);
    let forge: Forge = Arc::new(
        move |left: &mut Vec<scan::Candidate>, right: &mut Vec<scan::Candidate>| {
            forged(left, right);
            arm.store(true, Ordering::SeqCst);
        },
    );
    let events = run(
        request(left, side(generated.right.as_bytes()), focus),
        Tuning {
            batch_lines: 1,
            forge: Some(forge),
            watchdog: Duration::from_secs(60),
            ..Tuning::default()
        },
    );
    let done = summary(&events);
    assert_eq!(done.dropped_anchors, 1);
    let batches = covered(&events);
    assert!(
        batches[0].left.contains(&focus),
        "the merged focused batch arrives first"
    );
    let rest: Vec<u64> = batches[1..].iter().map(|batch| batch.left.start).collect();
    let mut sorted = rest.clone();
    sorted.sort_unstable();
    assert_eq!(rest, sorted, "the other batches arrive top-down");
    assert_tiles(&events, done.left_lines, done.right_lines);
}

fn assert_applies(left: &str, right: &str, hunks: &[Hunk]) {
    let left: Vec<&str> = left.lines().collect();
    let right: Vec<&str> = right.lines().collect();
    let mut rebuilt = Vec::new();
    let (mut left_at, mut right_at) = (0, 0);
    for hunk in hunks {
        assert!(hunk.left_lines.start >= left_at && hunk.right_lines.start >= right_at);
        assert_eq!(
            hunk.left_lines.start - left_at,
            hunk.right_lines.start - right_at,
            "equal runs pair line for line"
        );
        rebuilt.extend_from_slice(&left[left_at..hunk.left_lines.start]);
        rebuilt.extend_from_slice(&right[hunk.right_lines.clone()]);
        left_at = hunk.left_lines.end;
        right_at = hunk.right_lines.end;
    }
    assert_eq!(left.len() - left_at, right.len() - right_at);
    rebuilt.extend_from_slice(&left[left_at..]);
    assert_eq!(rebuilt, right, "the hunks turn the left into the right");
}

#[test]
fn crossed_moves_give_a_valid_diff() {
    let keys = [0x10, 0x4a, 0xb9, 0xea];
    let crossed = [0xb9, 0xea, 0x10, 0x4a];
    let mut left = String::new();
    let mut right = String::new();
    for line in 0..3_000_u32 {
        left.push_str(&format!("{line:08x}\n"));
        let moved = keys.iter().position(|&key| key == line);
        let shown = moved.map_or(line, |at| crossed[at]);
        right.push_str(&format!("{shown:08x}\n"));
    }
    for batch_lines in [1, 64, u64::MAX] {
        let events = run(
            request(side(left.as_bytes()), side(right.as_bytes()), 0),
            tuning(batch_lines),
        );
        let done = summary(&events);
        assert_tiles(&events, done.left_lines, done.right_lines);
        assert_applies(&left, &right, &hunks(&events));
    }
}

#[test]
fn a_forged_terminal_anchor_is_checked_and_dropped() {
    let generated = generate(20_000);
    let right = format!("{}right tail\n", generated.right);
    let left_last = generated.left.lines().count() as u32 - 1;
    let right_last = right.lines().count() as u32 - 1;
    let expected = reference(&generated.left, &right);
    let plain = summary(&run(
        request(side(generated.left.as_bytes()), side(right.as_bytes()), 0),
        tuning(64),
    ));
    for batch_lines in [1, 64, u64::MAX] {
        let events = run(
            request(side(generated.left.as_bytes()), side(right.as_bytes()), 0),
            Tuning {
                batch_lines,
                forge: Some(forge_at(left_last, right_last)),
                watchdog: Duration::from_secs(60),
                ..Tuning::default()
            },
        );
        let done = summary(&events);
        assert_eq!(done.dropped_anchors, 1, "batch size {batch_lines}");
        assert_eq!(done.sections, plain.sections, "batch size {batch_lines}");
        assert_eq!(done.restarts, 0, "batch size {batch_lines}");
        assert_eq!(
            line_ranges(&hunks(&events)),
            expected,
            "batch size {batch_lines}"
        );
    }
}

#[test]
fn only_emitted_batches_count_sections_and_dropped_anchors() {
    let generated = generate(20_000);
    let plain = summary(&run(
        request(
            side(generated.left.as_bytes()),
            side(generated.right.as_bytes()),
            0,
        ),
        tuning(64),
    ));
    let forged: Vec<(u32, u32)> = [3, 4, 5, 6, 12]
        .iter()
        .map(|&at| generated.modified[at])
        .collect();
    let focus = u64::from(generated.modified[15].0);
    for batch_lines in [1, 64, 2_500, u64::MAX] {
        let events = run(
            request(
                side(generated.left.as_bytes()),
                side(generated.right.as_bytes()),
                focus,
            ),
            Tuning {
                batch_lines,
                forge: Some(forge_many(&forged)),
                watchdog: Duration::from_secs(60),
                ..Tuning::default()
            },
        );
        let done = summary(&events);
        assert_eq!(
            done.dropped_anchors,
            forged.len(),
            "batch size {batch_lines}"
        );
        assert_eq!(done.sections, plain.sections, "batch size {batch_lines}");
        assert_eq!(done.restarts, 0, "batch size {batch_lines}");
    }
}
