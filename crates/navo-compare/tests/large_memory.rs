use std::sync::Arc;
use std::time::Duration;

use navo_compare::model::{Budget, CancelToken};
use navo_compare::text::large::{Covered, DiffEvent, LargeDiff, LargeSide, Request, StepReason};
use navo_compare::text::normalise::Options;
use navo_core::paging::{ByteSource, TextLayout};

const LINE_BYTES: usize = 18;

struct Numbered {
    tag: u8,
    lines: u64,
}

impl ByteSource for Numbered {
    fn len(&self) -> u64 {
        self.lines * LINE_BYTES as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        let len = self.len();
        let mut done = 0;
        let mut at = offset;
        while done < buf.len() && at < len {
            let line = at / LINE_BYTES as u64;
            let column = usize::try_from(at % LINE_BYTES as u64).unwrap_or(0);
            let text = self.text(line);
            let take = (LINE_BYTES - column).min(buf.len() - done);
            buf[done..done + take].copy_from_slice(&text[column..column + take]);
            done += take;
            at += take as u64;
        }
        Ok(done)
    }
}

impl Numbered {
    fn text(&self, line: u64) -> [u8; LINE_BYTES] {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = [0_u8; LINE_BYTES];
        text[0] = self.tag;
        for (at, byte) in line.to_be_bytes().iter().enumerate() {
            text[1 + at * 2] = HEX[usize::from(byte >> 4)];
            text[2 + at * 2] = HEX[usize::from(byte & 0x0f)];
        }
        text[LINE_BYTES - 1] = b'\n';
        text
    }
}

fn numbered(tag: u8, lines: u64) -> LargeSide {
    LargeSide {
        source: Arc::new(Numbered { tag, lines }),
        layout: TextLayout::default(),
    }
}

fn covered(events: &[DiffEvent]) -> Vec<&Covered> {
    events
        .iter()
        .filter_map(|event| match event {
            DiffEvent::Hunks(covered) => Some(covered),
            _ => None,
        })
        .collect()
}

fn steps(events: &[DiffEvent], wanted: StepReason) -> usize {
    events
        .iter()
        .filter(|event| matches!(event, DiffEvent::Step { reason, .. } if *reason == wanted))
        .count()
}

#[expect(clippy::panic, reason = "the test helper reports an unfinished diff")]
fn summary(events: &[DiffEvent]) -> navo_compare::text::large::Summary {
    match events.last() {
        Some(DiffEvent::Done(Ok(summary))) => summary.clone(),
        other => panic!("the diff must finish, got {other:?}"),
    }
}

#[cfg(target_os = "linux")]
fn reset_peak_rss() {
    let _ = std::fs::write("/proc/self/clear_refs", b"5");
}

#[cfg(target_os = "linux")]
#[expect(
    clippy::expect_used,
    reason = "the test helper reads the kernel status"
)]
fn peak_rss_bytes() -> u64 {
    let status = std::fs::read_to_string("/proc/self/status").expect("status is readable");
    let line = status
        .lines()
        .find(|line| line.starts_with("VmHWM:"))
        .expect("the kernel reports the peak resident set");
    let kib = line
        .split_whitespace()
        .nth(1)
        .expect("the peak has a number")
        .parse::<u64>()
        .expect("the peak is a number");
    kib.saturating_mul(1024)
}

#[test]
fn four_million_different_lines_at_256_mib_give_a_coarse_block() {
    let budget = Budget::from_mib(256);
    let lines = 4_000_000;
    #[cfg(target_os = "linux")]
    reset_peak_rss();
    let request = Request {
        left: numbered(b'L', lines),
        right: numbered(b'R', lines),
        options: Options::default(),
        focus: 0,
        budget,
    };
    let diff = LargeDiff::start(request, &CancelToken::new());
    let mut events = Vec::new();
    loop {
        let event = diff
            .events()
            .recv_timeout(Duration::from_secs(120))
            .expect("the driver always ends with Done");
        let done = matches!(event, DiffEvent::Done(_));
        events.push(event);
        if done {
            break;
        }
    }
    let done = summary(&events);
    assert_eq!((done.left_lines, done.right_lines), (lines, lines));
    let covered = covered(&events);
    assert_eq!(covered.len(), 1, "the whole pair is one section");
    let hunk = &covered[0].hunks[..];
    assert_eq!(hunk.len(), 1);
    assert_eq!(hunk[0].left_lines, 0..lines as usize);
    assert_eq!(hunk[0].left_bytes, 0..lines * LINE_BYTES as u64);
    assert_eq!(hunk[0].right_bytes, 0..lines * LINE_BYTES as u64);
    assert_eq!(covered[0].coarse, vec![0..lines as usize]);
    assert_eq!(
        steps(&events, StepReason::SectionCap) + steps(&events, StepReason::Watchdog),
        1
    );
    assert!(diff.peak_memory() <= budget.bytes());
    assert!(diff.memory_used() <= budget.bytes());
    #[cfg(target_os = "linux")]
    assert!(
        peak_rss_bytes() <= budget.bytes(),
        "the real process peak passed the budget"
    );
}
