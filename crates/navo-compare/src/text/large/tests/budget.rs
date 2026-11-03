use std::ops::Range;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use navo_core::paging::{ByteSource, LineIndex, TextLayout};

use super::super::batch::{Context, SideData};
use super::super::budget::{Ceiling, Ledger, Stall, bytes_of, rediff};
use super::super::*;
use super::{
    Swap, assert_tiles, collect, covered, generate, hunks, line_ranges, reference, side, summary,
    swap_side,
};
use crate::anchor::SectionPair;
use crate::model::{Budget, CancelToken};
use crate::text::normalise::Options;

mod raw;
mod watchdog;

const LINE_BYTES: usize = 18;

/// Lines `<tag><16 hex digits>\n`; a tag of `b'='` repeats the left side's text.
struct Numbered {
    tags: Vec<(u64, u8)>,
    lines: u64,
}

impl Numbered {
    fn tag(&self, line: u64) -> u8 {
        self.tags
            .iter()
            .rev()
            .find(|(from, _)| *from <= line)
            .map_or(b'=', |(_, tag)| *tag)
    }

    fn text(&self, line: u64) -> [u8; LINE_BYTES] {
        const HEX: &[u8; 16] = b"0123456789abcdef";
        let mut text = [0_u8; LINE_BYTES];
        text[0] = self.tag(line);
        for (at, byte) in line.to_be_bytes().iter().enumerate() {
            text[1 + at * 2] = HEX[usize::from(byte >> 4)];
            text[2 + at * 2] = HEX[usize::from(byte & 0x0f)];
        }
        text[LINE_BYTES - 1] = b'\n';
        text
    }
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

fn numbered(tags: &[(u64, u8)], lines: u64) -> LargeSide {
    LargeSide {
        source: Arc::new(Numbered {
            tags: tags.to_vec(),
            lines,
        }),
        layout: TextLayout::default(),
    }
}

pub(super) fn request_budget(
    left: LargeSide,
    right: LargeSide,
    focus: u64,
    budget: Budget,
) -> Request {
    Request {
        left,
        right,
        options: Options::default(),
        focus,
        budget,
    }
}

pub(super) fn assert_within(diff: &LargeDiff, budget: Budget) {
    assert!(
        diff.peak_memory() <= budget.bytes(),
        "counted {} bytes over a {} byte budget",
        diff.peak_memory(),
        budget.bytes()
    );
    assert!(diff.memory_used() <= budget.bytes());
}

pub(super) fn slow() -> Tuning {
    Tuning {
        watchdog: Duration::from_secs(60),
        ..Tuning::default()
    }
}

pub(super) fn steps(events: &[DiffEvent], wanted: StepReason) -> Vec<Range<u64>> {
    events
        .iter()
        .filter_map(|event| match event {
            DiffEvent::Step {
                section, reason, ..
            } if *reason == wanted => Some(section.clone()),
            _ => None,
        })
        .collect()
}

#[test]
#[ignore = "measures process-wide peak RSS; tests/large_memory.rs runs it alone"]
fn four_million_different_lines_at_256_mib_give_a_coarse_block() {
    let budget = Budget::from_mib(256);
    let lines = 4_000_000;
    #[cfg(target_os = "linux")]
    reset_peak_rss();
    let diff = LargeDiff::start_tuned(
        request_budget(
            numbered(&[(0, b'L')], lines),
            numbered(&[(0, b'R')], lines),
            0,
            budget,
        ),
        &CancelToken::new(),
        slow(),
    );
    let events = collect(&diff);
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
    assert_eq!(steps(&events, StepReason::SectionCap), vec![0..lines]);
    assert_within(&diff, budget);
    #[cfg(target_os = "linux")]
    assert!(
        peak_rss_bytes() <= budget.bytes(),
        "the real process peak passed the budget"
    );
}

#[cfg(target_os = "linux")]
fn reset_peak_rss() {
    let _ = std::fs::write("/proc/self/clear_refs", b"5");
}

#[cfg(target_os = "linux")]
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
fn only_the_section_over_the_token_cap_turns_coarse_with_exact_bytes() {
    let (top, middle, bottom) = (40_000_u64, 600_000_u64, 40_000_u64);
    let lines = top + middle + bottom;
    let budget = Budget::from_mib(256);
    let diff = LargeDiff::start_tuned(
        request_budget(
            numbered(&[(top, b'L'), (top + middle, b'=')], lines),
            numbered(&[(top, b'R'), (top + middle, b'=')], lines),
            0,
            budget,
        ),
        &CancelToken::new(),
        slow(),
    );
    let events = collect(&diff);
    summary(&events);
    let coarse = steps(&events, StepReason::TokenCap);
    assert_eq!(coarse.len(), 1, "one section is coarse, not the file");
    let block = hunks(&events);
    assert_eq!(
        block.len(),
        1,
        "the equal sections stay exact and add no hunks"
    );
    let start = (coarse[0].start * LINE_BYTES as u64)..(coarse[0].end * LINE_BYTES as u64);
    assert_eq!(block[0].left_bytes, start);
    assert_eq!(block[0].right_bytes, start);
    assert!(coarse[0].start <= top && coarse[0].end >= top + middle);
    assert!(covered(&events).len() > 2, "section boundaries are kept");
    assert_within(&diff, budget);
}

#[test]
fn a_section_over_the_cap_re_anchors_densely_and_stays_exact() {
    let generated = generate(1_200_000);
    let hidden = 50_000..1_150_000_u32;
    let keep = hidden.clone();
    let forge: Forge = Arc::new(move |left: &mut Vec<_>, right: &mut Vec<_>| {
        let outside = |candidate: &crate::text::scan::Candidate| !keep.contains(&candidate.line);
        left.retain(outside);
        right.retain(outside);
    });
    let budget = Budget::from_mib(256);
    let diff = LargeDiff::start_tuned(
        request_budget(
            side(generated.left.as_bytes()),
            side(generated.right.as_bytes()),
            0,
            budget,
        ),
        &CancelToken::new(),
        Tuning {
            forge: Some(forge),
            ..slow()
        },
    );
    let events = collect(&diff);
    summary(&events);
    assert!(
        !events
            .iter()
            .any(|event| matches!(event, DiffEvent::Step { .. })),
        "re-anchoring keeps line-level hunks"
    );
    assert_eq!(
        line_ranges(&hunks(&events)),
        reference(&generated.left, &generated.right)
    );
    assert_within(&diff, budget);
}

#[test]
fn a_coarse_block_at_the_top_starts_at_byte_zero_with_a_bom() {
    let mut left = vec![0xEF, 0xBB, 0xBF];
    let mut right = left.clone();
    for line in 0..600_000_u32 {
        left.extend(format!("l{line:08x}\n").bytes());
        right.extend(format!("r{line:08x}\n").bytes());
    }
    let events = super::run(
        request_budget(side(&left), side(&right), 0, Budget::from_mib(256)),
        slow(),
    );
    summary(&events);
    let all = hunks(&events);
    assert_eq!(steps(&events, StepReason::TokenCap).len(), 1);
    assert_eq!(all.len(), 1);
    assert_eq!(all[0].left_bytes, 0..left.len() as u64);
    assert_eq!(all[0].right_bytes, 0..right.len() as u64);
}

#[test]
fn a_focus_change_at_completion_still_pages_the_old_view() {
    let (left, right) = dense(800_000);
    let tuning = Tuning {
        batch_lines: 512,
        before_done: Some(Arc::new(|focus: &AtomicU64| {
            focus.store(400_000, Ordering::Relaxed);
        })),
        ..slow()
    };
    let budget = Budget::from_mib(32);
    let diff = LargeDiff::start_tuned(
        request_budget(side(left.as_bytes()), side(right.as_bytes()), 0, budget),
        &CancelToken::new(),
        tuning,
    );
    let events = collect(&diff);
    summary(&events);
    let view = collect_view(&diff, &events);
    assert!(view.evicted > 0, "the late focus paged the old view");
    assert!(
        !view.hunks.iter().any(|hunk| hunk.left_lines.start < 2_048),
        "detail near the old focus is gone"
    );
    assert_within(&diff, budget);
}

fn dense(lines: u32) -> (String, String) {
    let mut left = String::new();
    let mut right = String::new();
    for line in 0..lines {
        let base = format!("{line:08x}\n");
        left.push_str(&base);
        if line % 4 != 2 {
            right.push_str(&base);
        }
    }
    (left, right)
}

/// The view's hunk list: applies `Hunks` and drops what `Paged` events page out.
#[derive(Default)]
struct View {
    hunks: Vec<Hunk>,
    paged: Vec<Paged>,
    evicted: usize,
}

impl View {
    fn apply(&mut self, event: &DiffEvent) {
        match event {
            DiffEvent::Hunks(covered) => {
                let mut blocks: Vec<usize> = covered
                    .paged
                    .iter()
                    .map(|p| p.left.start as usize)
                    .collect();
                blocks.extend(covered.coarse.iter().map(|range| range.start));
                self.hunks.extend(
                    covered
                        .hunks
                        .iter()
                        .filter(|hunk| !blocks.contains(&hunk.left_lines.start))
                        .cloned(),
                );
                self.paged.extend(covered.paged.iter().cloned());
            }
            DiffEvent::Paged { paged, .. } => {
                self.evicted += 1;
                let left = paged.left.start as usize..paged.left.end as usize;
                self.hunks.retain(|hunk| {
                    let at = hunk.left_lines.start;
                    !(left.contains(&at) || (left.is_empty() && at == left.start))
                });
                self.paged.push(paged.clone());
            }
            _ => {}
        }
    }

    fn rebuild(&self, diff: &LargeDiff) -> Vec<Hunk> {
        let mut all = self.hunks.clone();
        for paged in &self.paged {
            let again = match diff.hunks_for(paged.left.clone()) {
                Ok(Rediff::Hunks { hunks, .. }) => hunks,
                Ok(Rediff::Coarse { reason, .. }) => {
                    panic!("a successful re-diff is coarse: {reason:?}")
                }
                Err(reason) => panic!("a successful re-diff failed: {reason:?}"),
            };
            assert_eq!(
                again.len() as u64,
                paged.hunks,
                "the count matches the diff"
            );
            all.extend(again);
        }
        all.sort_by_key(|hunk| (hunk.left_lines.start, hunk.right_lines.start));
        all
    }
}

fn collect_view(diff: &LargeDiff, events: &[DiffEvent]) -> View {
    let mut view = View::default();
    let mut seq = 0;
    events.iter().for_each(|event| {
        view.apply(event);
        if let DiffEvent::Paged { seq: at, .. } = event {
            seq = seq.max(*at);
        }
    });
    diff.acknowledge_evictions(seq);
    view
}

#[test]
fn paged_hunks_diffed_again_equal_the_first_run() {
    let (left, right) = dense(800_000);
    let tuning = Tuning {
        batch_lines: 512,
        ..slow()
    };
    let full = LargeDiff::start_tuned(
        request_budget(
            side(left.as_bytes()),
            side(right.as_bytes()),
            0,
            Budget::default(),
        ),
        &CancelToken::new(),
        tuning.clone(),
    );
    let reference = hunks(&collect(&full));
    drop(full);
    let budget = Budget::from_mib(32);
    let diff = LargeDiff::start_tuned(
        request_budget(side(left.as_bytes()), side(right.as_bytes()), 0, budget),
        &CancelToken::new(),
        tuning,
    );
    let events = collect(&diff);
    summary(&events);
    let view = collect_view(&diff, &events);
    assert!(
        view.evicted > 0,
        "hunks emitted before paging started are paged out"
    );
    assert!(!view.paged.is_empty());
    assert!(
        steps(&events, StepReason::Budget).is_empty(),
        "no section turns coarse"
    );
    let held = view.hunks.len() as u64 * super::super::budget::HUNK_SIZE;
    assert!(
        held <= budget.bytes() / 4,
        "the view keeps at most a quarter of the budget"
    );
    assert_eq!(view.rebuild(&diff), reference);
    assert_within(&diff, budget);
}

#[test]
fn rediffed_sections_are_counted_and_page_out_again() {
    let (left, right) = dense(800_000);
    let tuning = Tuning {
        batch_lines: 512,
        ..slow()
    };
    let budget = Budget::from_mib(32);
    let diff = LargeDiff::start_tuned(
        request_budget(side(left.as_bytes()), side(right.as_bytes()), 0, budget),
        &CancelToken::new(),
        tuning,
    );
    let events = collect(&diff);
    summary(&events);
    let view = collect_view(&diff, &events);
    let chosen: Vec<_> = view
        .paged
        .iter()
        .filter(|p| p.left.end < 100_000)
        .take(8)
        .collect();
    assert_eq!(chosen.len(), 8, "enough sections were paged near the start");
    let before = diff.memory_used();
    let mut held = Vec::new();
    for paged in &chosen {
        let Rediff::Hunks { hunks: again, .. } = diff.hunks_for(paged.left.clone()).unwrap() else {
            panic!("a successful re-diff is not coarse");
        };
        held.push(again);
    }
    let hunks: usize = held.iter().map(Vec::len).sum();
    let block = bytes_of::<Hunk>(1) + bytes_of::<Paged>(1);
    assert!(
        diff.memory_used() >= before + bytes_of::<Hunk>(hunks) - 8 * block,
        "the returned hunks are counted"
    );
    let evictions = diff.set_focus(700_000);
    for paged in &chosen {
        assert!(
            evictions.paged().iter().any(|e| e.left == paged.left),
            "a re-diffed section pages out again"
        );
    }
    let charged = diff.memory_used();
    held.clear();
    evictions.acknowledge();
    assert!(diff.memory_used() + bytes_of::<Hunk>(hunks) <= charged + 8 * block);
    assert_within(&diff, budget);
}

#[test]
fn moving_the_focus_pages_out_the_sections_left_behind() {
    let (left, right) = dense(800_000);
    let (gate, wait) = mpsc::channel::<()>();
    let wait = Mutex::new(wait);
    let once = AtomicBool::new(false);
    let stall: Stall = Arc::new(move |line| {
        if (700_000..700_512).contains(&line) && !once.swap(true, Ordering::SeqCst) {
            let _ = wait.lock().unwrap().recv_timeout(Duration::from_secs(30));
        }
    });
    let budget = Budget::from_mib(32);
    let diff = LargeDiff::start_tuned(
        request_budget(side(left.as_bytes()), side(right.as_bytes()), 0, budget),
        &CancelToken::new(),
        Tuning {
            batch_lines: 512,
            stall: Some(stall),
            ..slow()
        },
    );
    let mut view = View::default();
    let mut moved = false;
    loop {
        let event = diff
            .events()
            .recv_timeout(Duration::from_secs(120))
            .unwrap();
        view.apply(&event);
        if !moved && matches!(event, DiffEvent::Paged { .. }) {
            moved = true;
            let before = view.hunks.iter().any(|hunk| hunk.left_lines.start < 2_048);
            assert!(before, "sections near the first focus keep their hunks");
            diff.set_focus(400_000).acknowledge();
            gate.send(()).unwrap();
        }
        if matches!(event, DiffEvent::Done(_)) {
            break;
        }
    }
    assert!(moved, "paging started");
    assert!(
        !view.hunks.iter().any(|hunk| hunk.left_lines.start < 2_048),
        "sections near the old focus drop to counts"
    );
    assert_within(&diff, budget);
}

#[test]
fn focus_after_done_pages_the_old_view() {
    let (left, right) = dense(800_000);
    let tuning = Tuning {
        batch_lines: 512,
        ..slow()
    };
    let budget = Budget::from_mib(32);
    let diff = LargeDiff::start_tuned(
        request_budget(side(left.as_bytes()), side(right.as_bytes()), 0, budget),
        &CancelToken::new(),
        tuning,
    );
    let events = collect(&diff);
    summary(&events);
    let mut view = collect_view(&diff, &events);
    assert!(
        view.hunks.iter().any(|hunk| hunk.left_lines.start < 2_048),
        "the first view keeps detail"
    );
    let held = diff.memory_used();
    let evictions = diff.set_focus(400_000);
    let during = diff.memory_used();
    assert!(during >= held, "the hunk charge moved into the evictions");
    assert!(!evictions.paged().is_empty());
    evictions.apply(|paged| {
        view.apply(&DiffEvent::Paged {
            paged: paged.clone(),
            seq: 0,
        })
    });
    assert!(diff.memory_used() < during, "acknowledgement released it");
    assert!(
        !view.hunks.iter().any(|hunk| hunk.left_lines.start < 2_048),
        "old-focus detail paged after Done"
    );
    assert_within(&diff, budget);
}

#[test]
fn streamed_evictions_hold_memory_until_acknowledged() {
    let (left, right) = dense(800_000);
    let tuning = Tuning {
        batch_lines: 512,
        ..slow()
    };
    let budget = Budget::from_mib(32);
    let diff = LargeDiff::start_tuned(
        request_budget(side(left.as_bytes()), side(right.as_bytes()), 0, budget),
        &CancelToken::new(),
        tuning,
    );
    let events = collect(&diff);
    summary(&events);
    let mut view = View::default();
    events.iter().for_each(|event| view.apply(event));
    assert!(view.evicted > 0, "the scheduler streamed an eviction");
    let upto = events
        .iter()
        .filter_map(|event| match event {
            DiffEvent::Paged { seq, .. } => Some(*seq),
            _ => None,
        })
        .max()
        .unwrap_or(0);
    let held = diff.memory_used();
    diff.acknowledge_evictions(upto);
    assert!(
        diff.memory_used() < held,
        "the streamed charge stays reserved until acknowledged"
    );
    assert_within(&diff, budget);
}

fn paged_seq(event: &DiffEvent) -> Option<u64> {
    match event {
        DiffEvent::Paged { seq, .. } => Some(*seq),
        _ => None,
    }
}

fn max_paged(events: &[DiffEvent]) -> Option<u64> {
    events.iter().filter_map(paged_seq).max()
}

fn first_paged(events: &[DiffEvent]) -> Option<u64> {
    events.iter().find_map(paged_seq)
}

#[test]
fn sequences_stay_monotonic_across_a_restart() {
    let (left, right) = dense(800_000);
    let shrunk = left[..left.len() / 2].to_owned();
    let source = Arc::new(Swap::new(
        left.into_bytes(),
        shrunk.into_bytes(),
        usize::MAX,
    ));
    let trigger = Arc::clone(&source);
    let swapped = Arc::new(AtomicBool::new(false));
    let stall: Stall = Arc::new(move |line| {
        if line >= 600_000 && !swapped.swap(true, Ordering::SeqCst) {
            trigger.swap();
        }
    });
    let budget = Budget::from_mib(32);
    let diff = LargeDiff::start_tuned(
        request_budget(swap_side(source), side(right.as_bytes()), 0, budget),
        &CancelToken::new(),
        Tuning {
            batch_lines: 512,
            stall: Some(stall),
            ..slow()
        },
    );
    let events = collect(&diff);
    let done = summary(&events);
    assert!(done.restarts >= 1);
    let restart = events
        .iter()
        .position(|event| matches!(event, DiffEvent::Restarted))
        .expect("the source change restarts");
    let first_max = max_paged(&events[..restart]).expect("the first run pages");
    let first_again = first_paged(&events[restart..]).expect("the second run pages");
    assert!(first_again > first_max, "sequences continue after restart");
    let held = diff.memory_used();
    diff.acknowledge_evictions(first_max);
    assert_eq!(
        diff.memory_used(),
        held,
        "a run-1 ack must not release run-2 holds"
    );
}

fn rediff_context(
    source: &Arc<dyn ByteSource + Send + Sync>,
    ledger: &Arc<Ledger>,
) -> (Context, SectionPair) {
    let layout = TextLayout::default();
    let side = || SideData {
        source: Arc::clone(source),
        layout,
        len: 2,
        lines: 1,
        index: LineIndex::new(),
    };
    (
        Context {
            left: side(),
            right: side(),
            options: Options::default(),
            _charge: ledger.empty(),
        },
        SectionPair {
            left: 0..1,
            right: 0..1,
        },
    )
}

#[test]
fn a_rediff_budget_failure_is_a_classified_coarse_result() {
    let large = side(b"a\n");
    let ledger = Arc::new(Ledger::new(Budget::from_bytes(1)));
    let (context, pair) = rediff_context(&large.source, &ledger);
    let (result, _charge) = rediff(
        &context,
        &pair,
        &ledger,
        Ceiling {
            cancel: &CancelToken::new(),
            watchdog: Duration::from_secs(1),
        },
    )
    .unwrap();
    assert!(matches!(
        result,
        Rediff::Coarse {
            reason: StepReason::Budget,
            ..
        }
    ));
}

#[test]
fn a_rediff_deadline_is_a_classified_coarse_result() {
    let large = side(b"a\n");
    let ledger = Arc::new(Ledger::new(Budget::default()));
    let (context, pair) = rediff_context(&large.source, &ledger);
    let (result, _charge) = rediff(
        &context,
        &pair,
        &ledger,
        Ceiling {
            cancel: &CancelToken::new(),
            watchdog: Duration::ZERO,
        },
    )
    .unwrap();
    assert!(matches!(
        result,
        Rediff::Coarse {
            reason: StepReason::Watchdog,
            ..
        }
    ));
}
