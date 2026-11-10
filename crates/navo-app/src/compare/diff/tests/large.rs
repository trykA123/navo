//! Tests for the large text worker: events, line reads, restart state and cancel.

use std::cell::Cell;
use std::fs;
use std::ops::Range;
use std::path::Path;
use std::rc::Rc;
use std::sync::Arc;
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, Instant};

use navo_compare::model::{Budget, CancelToken, DiffStep};
use navo_compare::text::Options;
use navo_compare::text::diff::Hunk;
use navo_compare::text::large::{DiffEvent, Paged, PagedState, StepReason};
use navo_core::Location;
use navo_ui::compare_diff::{DiffSink, DiffUpdate, PairControl, PairRequest};

use crate::compare::diff::large::queue::LineQueue;
use crate::compare::diff::large::state::Step;
use crate::compare::diff::large::{Ctx, convert};
use crate::compare::diff::large::{LargeDiff, LargeInput, TestState};
use crate::compare::settings::CompareSettings;

const WAIT: Duration = Duration::from_secs(20);

fn location(path: &Path) -> Location {
    Location::local(navo_core::ProviderPath::from_text(
        path.to_str().unwrap_or_default(),
    ))
    .unwrap()
}

fn request() -> PairRequest {
    PairRequest {
        id: 9,
        path: "large.log".into(),
        left: None,
        right: None,
    }
}

fn sink() -> (DiffSink, Receiver<DiffUpdate>) {
    let (send, receive) = mpsc::channel();
    (
        Arc::new(move |update| {
            let _sent = send.send(update);
        }),
        receive,
    )
}

fn input(dir: &Path, left: &str, right: &str) -> LargeInput {
    LargeInput {
        left: Some(location(&dir.join(left))),
        right: Some(location(&dir.join(right))),
        settings: CompareSettings::default(),
        sources: crate::compare::diff::sources::Sources::without_spool(),
        options: Options::default(),
        fallback: encoding_rs::UTF_8,
        budget: Budget::default(),
        focus: 0,
    }
}

/// Write `lines` numbered lines, changing `changed` on the right side.
fn pair(dir: &Path, lines: usize, changed: usize) -> LargeInput {
    let mut left = String::new();
    let mut right = String::new();
    for index in 0..lines {
        left.push_str(&format!("line {index}\n"));
        if index == changed {
            right.push_str(&format!("changed {index}\n"));
        } else {
            right.push_str(&format!("line {index}\n"));
        }
    }
    fs::write(dir.join("left.log"), left).unwrap();
    fs::write(dir.join("right.log"), right).unwrap();
    input(dir, "left.log", "right.log")
}

fn next(receive: &Receiver<DiffUpdate>) -> DiffUpdate {
    receive
        .recv_timeout(WAIT)
        .expect("the worker reported an update")
}

#[test]
fn a_large_pair_reports_hunks_then_done() {
    let dir = tempfile::tempdir().unwrap();
    let pair = pair(dir.path(), 4000, 1234);
    let (sink, receive) = sink();
    let control = LargeDiff::start(request(), pair, sink);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "no finished large update");
        if let DiffUpdate::Large {
            hunks, progress, ..
        } = next(&receive)
        {
            let finished = progress.is_some_and(|p| p.total > 0 && p.done >= p.total);
            if finished && !hunks.is_empty() {
                assert!(has_line(&hunks, 1234));
                break;
            }
        }
    }
    drop(control);
}

#[test]
fn lines_returns_the_requested_window_from_both_sides() {
    let dir = tempfile::tempdir().unwrap();
    let pair = pair(dir.path(), 20, 10);
    let (sink, receive) = sink();
    let control = LargeDiff::start(request(), pair, sink);
    control.lines(0..3);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "no line update");
        if let DiffUpdate::LargeLines {
            rows, left, right, ..
        } = next(&receive)
        {
            assert_eq!(rows, 0..3);
            assert_eq!(left, ["line 0", "line 1", "line 2"]);
            assert_eq!(right, ["line 0", "line 1", "line 2"]);
            break;
        }
    }
    drop(control);
}

#[test]
fn a_restart_clears_hunks_steps_and_progress() {
    let mut state = TestState::new();
    state.add_hunks(&[hunk(0..2, 0..2)]);
    state.add_step(Step {
        step: DiffStep::Sections,
        section: 0..2,
        lines: 4,
        reason: StepReason::Paged,
    });
    state.progress = Some(navo_ui::compare_diff::LargeProgress {
        stage: navo_ui::compare_diff::LargeStage::Sections,
        done: 1,
        total: 1,
    });
    state.done = true;
    let (sink, receive) = sink();
    let ctx = Ctx { id: 9, sink };
    convert::apply(DiffEvent::Restarted, &mut state, &ctx);
    assert!(state.hunks.is_empty());
    assert!(state.steps.is_empty());
    assert!(state.progress.is_none());
    assert!(!state.done);
    assert!(matches!(next(&receive), DiffUpdate::Large { hunks, .. } if hunks.is_empty()));
}

#[test]
fn a_paged_sequence_is_freed_only_by_the_snapshot_that_carried_it() {
    let mut state = TestState::new();
    let (sink, receive) = sink();
    let ctx = Ctx { id: 9, sink };
    convert::send(&mut state, &ctx);
    let DiffUpdate::Large { ack: before, .. } = next(&receive) else {
        panic!("a large snapshot");
    };
    let paged = Paged {
        left: 0..2,
        right: 0..2,
        hunks: 1,
        state: PagedState::Rediffable,
    };
    convert::apply(DiffEvent::Paged { paged, seq: 7 }, &mut state, &ctx);
    let DiffUpdate::Large { ack: carried, .. } = next(&receive) else {
        panic!("a large snapshot");
    };
    assert_eq!(
        state.acks.acknowledge(before).seq,
        None,
        "an older snapshot never applied the eviction"
    );
    assert_eq!(state.acks.acknowledge(carried).seq, Some(7));
}

#[test]
fn a_focus_eviction_is_released_only_after_its_snapshot_is_acknowledged() {
    let mut state = TestState::new();
    let (sink, receive) = sink();
    let ctx = Ctx { id: 9, sink };
    let released = Rc::new(Cell::new(false));
    let flag = Rc::clone(&released);
    state.acks.hold(Box::new(move || flag.set(true)));
    assert!(
        !released.get(),
        "not released before the view has the snapshot"
    );
    convert::send(&mut state, &ctx);
    let DiffUpdate::Large { ack, .. } = next(&receive) else {
        panic!("a large snapshot");
    };
    for release in state.acks.acknowledge(ack.saturating_sub(1)).releases {
        release();
    }
    assert!(!released.get(), "an older acknowledgement keeps the charge");
    for release in state.acks.acknowledge(ack).releases {
        release();
    }
    assert!(released.get(), "the snapshot's acknowledgement releases it");
}

#[test]
fn close_cannot_slip_between_the_line_check_and_the_wait() {
    let queue = Arc::new(LineQueue::new(CancelToken::new()));
    let guard = queue.lines.lock().unwrap();
    assert!(!guard.closed);
    let (done, closed) = mpsc::channel();
    let closer = Arc::clone(&queue);
    let handle = thread::spawn(move || {
        closer.close();
        let _sent = done.send(());
    });
    assert!(
        closed.recv_timeout(Duration::from_millis(200)).is_err(),
        "close must wait for the reader to release the queue, or its wake-up is lost"
    );
    let (guard, waited) = queue
        .ready
        .wait_timeout_while(guard, WAIT, |lines| !lines.closed)
        .unwrap();
    assert!(!waited.timed_out(), "the close woke the waiting reader");
    drop(guard);
    closed.recv_timeout(WAIT).expect("close returned");
    handle.join().unwrap();
}

#[test]
fn close_wakes_a_waiting_line_reader() {
    let queue = Arc::new(LineQueue::new(CancelToken::new()));
    let (done, finished) = mpsc::channel();
    let reader = Arc::clone(&queue);
    let handle = thread::spawn(move || {
        let _sent = done.send(reader.next());
    });
    queue.close();
    let got = finished.recv_timeout(WAIT).expect("the reader woke up");
    assert_eq!(got, None, "a closed queue ends the reader");
    handle.join().unwrap();
}

#[test]
fn a_huge_left_window_leaves_right_text_and_reports_the_real_range() {
    let dir = tempfile::tempdir().unwrap();
    let wide = "x".repeat(8 * 1024);
    let mut left = String::new();
    let mut right = String::new();
    for index in 0..1_000 {
        left.push_str(&format!("{index} {wide}\n"));
        right.push_str(&format!("line {index}\n"));
    }
    fs::write(dir.path().join("left.log"), left).unwrap();
    fs::write(dir.path().join("right.log"), right).unwrap();
    let (sink, receive) = sink();
    let control = LargeDiff::start(request(), input(dir.path(), "left.log", "right.log"), sink);
    control.lines(0..1_000);
    let deadline = Instant::now() + WAIT;
    let (rows, left, right) = loop {
        assert!(Instant::now() < deadline, "no line update");
        if let DiffUpdate::LargeLines {
            rows, left, right, ..
        } = next(&receive)
        {
            break (rows, left, right);
        }
    };
    let text: usize = left.iter().chain(&right).map(String::len).sum();
    assert!(text <= 4 * 1024 * 1024, "the window stays within 4 MiB");
    assert!(!right.is_empty(), "the left side cannot starve the right");
    assert!(rows.end < 1_000, "a cut window says so");
    let count = usize::try_from(rows.end - rows.start).unwrap();
    assert_eq!(left.len(), count, "the range is what was returned");
    assert_eq!(right.len(), count, "both sides cover the same rows");
    assert_eq!(
        left[count - 1].len(),
        wide.len() + format!("{} ", count - 1).len()
    );
    drop(control);
}

#[test]
fn the_line_worker_caps_a_window_at_two_thousand_rows() {
    let dir = tempfile::tempdir().unwrap();
    let pair = pair(dir.path(), 3_000, 10);
    let (sink, receive) = sink();
    let control = LargeDiff::start(request(), pair, sink);
    control.lines(0..3_000);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "no capped line update");
        if let DiffUpdate::LargeLines { left, .. } = next(&receive) {
            assert_eq!(left.len(), 2_000);
            break;
        }
    }
    drop(control);
}

#[test]
fn the_display_reader_opens_its_own_sources() {
    let dir = tempfile::tempdir().unwrap();
    let mut pair = pair(dir.path(), 20, 10);
    let sources = crate::compare::diff::sources::Sources::without_spool();
    pair.sources = sources.clone();
    let (sink, receive) = sink();
    let control = LargeDiff::start(request(), pair, sink);
    control.lines(0..3);
    let deadline = Instant::now() + WAIT;
    loop {
        assert!(Instant::now() < deadline, "no line update");
        if matches!(next(&receive), DiffUpdate::LargeLines { .. }) {
            break;
        }
    }
    assert!(
        sources.opens() >= 4,
        "two engine sources and two reader sources"
    );
    drop(control);
}

#[test]
fn a_file_changed_after_indexing_sends_a_failure_for_the_window() {
    let dir = tempfile::tempdir().unwrap();
    let pair = pair(dir.path(), 20, 10);
    let (sink, receive) = sink();
    let control = LargeDiff::start(request(), pair, sink);
    control.lines(0..3);
    wait_for(&receive, |update| {
        matches!(update, DiffUpdate::LargeLines { .. })
    });
    fs::write(dir.path().join("right.log"), "line 0\n").unwrap();
    control.lines(25..28);
    wait_for(&receive, |update| {
        matches!(
            update,
            DiffUpdate::ChangedOnDisk { request: 9 } | DiffUpdate::Failed { request: 9, .. }
        )
    });
    drop(control);
}

/// Wait until `wanted` matches an update, failing the test after `WAIT`.
fn wait_for(receive: &Receiver<DiffUpdate>, wanted: impl Fn(&DiffUpdate) -> bool) {
    let deadline = Instant::now() + WAIT;
    while !wanted(&next(receive)) {
        assert!(Instant::now() < deadline, "the expected update never came");
    }
}

#[test]
fn dropping_a_running_diff_returns_at_once() {
    let dir = tempfile::tempdir().unwrap();
    let pair = pair(dir.path(), 1000, 10);
    let (sink, _receive) = sink();
    let control = LargeDiff::start(request(), pair, sink);
    let started = Instant::now();
    drop(control);
    assert!(started.elapsed() < Duration::from_secs(1));
}

/// True when any hunk covers `line`.
fn has_line(hunks: &[navo_ui::compare_diff::LargeHunk], line: u64) -> bool {
    hunks
        .iter()
        .any(|hunk| hunk.left_rows.contains(&line) || hunk.right_rows.contains(&line))
}

/// A hunk covering the given line ranges, with one byte per line.
fn hunk(left: Range<usize>, right: Range<usize>) -> Hunk {
    Hunk {
        left_lines: left.clone(),
        right_lines: right.clone(),
        left_bytes: 0..left.end as u64,
        right_bytes: 0..right.end as u64,
    }
}
