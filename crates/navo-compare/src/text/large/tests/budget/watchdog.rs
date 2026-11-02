use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::{Duration, Instant};

use navo_core::paging::{ByteSource, TextLayout};

use crate::model::Budget;
use crate::text::large::{Forge, LargeSide, Stage};

use super::super::budget::Stall;
use super::super::failure;
use super::super::*;
use super::super::{Swap, swap_side};
use super::{
    assert_tiles, assert_within, collect, covered, dense, generate, hunks, line_ranges, reference,
    request_budget, side, steps, summary,
};

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    ready: Condvar,
}

impl Gate {
    fn wait(&self) {
        let mut open = self.open.lock().unwrap();
        while !*open {
            open = self.ready.wait(open).unwrap();
        }
    }

    fn release(&self) {
        *self.open.lock().unwrap() = true;
        self.ready.notify_all();
    }
}

fn wait_ids(ids: &Arc<Mutex<Vec<u64>>>, wanted: impl Fn(&[u64]) -> bool) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while !wanted(&ids.lock().unwrap()) {
        assert!(Instant::now() < deadline, "the late worker never finished");
        std::thread::sleep(Duration::from_millis(1));
    }
}

#[test]
fn several_slow_sections_expire_and_two_extra_workers_finish_the_diff() {
    let generated = generate(30_000);
    let gate = Arc::new(Gate::default());
    let stalled = Arc::clone(&gate);
    let finished = Arc::new(Mutex::new(Vec::new()));
    let hook = Arc::clone(&finished);
    let expired = Arc::new(AtomicUsize::new(0));
    let expiries = Arc::clone(&expired);
    let stall: Stall = Arc::new(move |line| {
        if line < 2_000 {
            stalled.wait();
        }
    });
    let diff = LargeDiff::start_tuned(
        request_budget(
            side(generated.left.as_bytes()),
            side(generated.right.as_bytes()),
            0,
            Budget::default(),
        ),
        &CancelToken::new(),
        Tuning {
            batch_lines: 1,
            watchdog: Duration::from_millis(30),
            stall: Some(stall),
            finished: Some(Arc::new(move |id| hook.lock().unwrap().push(id))),
            expired: Some(Arc::new(move |_| {
                expiries.fetch_add(1, Ordering::SeqCst);
            })),
            ..Tuning::default()
        },
    );
    let events = collect(&diff);
    let done = summary(&events);
    assert!(
        expired.load(Ordering::SeqCst) >= 6,
        "six section deadlines fired"
    );
    assert_eq!(done.extra_workers, 2, "the whole diff gets two extras");
    assert!(steps(&events, StepReason::Watchdog).len() >= 6);
    gate.release();
    wait_ids(&finished, |ids| ids.len() >= 6);
    assert_within(&diff, Budget::default());
}

#[test]
fn a_late_worker_is_observed_and_its_result_is_dropped() {
    let generated = generate(10_000);
    let gate = Arc::new(Gate::default());
    let stalled = Arc::clone(&gate);
    let finished = Arc::new(Mutex::new(Vec::new()));
    let hook = Arc::clone(&finished);
    let expired = Arc::new(AtomicUsize::new(0));
    let expiries = Arc::clone(&expired);
    let stall: Stall = Arc::new(move |line| {
        if line == 0 {
            stalled.wait();
        }
    });
    let diff = LargeDiff::start_tuned(
        request_budget(
            side(generated.left.as_bytes()),
            side(generated.right.as_bytes()),
            0,
            Budget::default(),
        ),
        &CancelToken::new(),
        Tuning {
            batch_lines: 1,
            watchdog: Duration::from_secs(2),
            stall: Some(stall),
            finished: Some(Arc::new(move |id| hook.lock().unwrap().push(id))),
            expired: Some(Arc::new(move |_| {
                expiries.fetch_add(1, Ordering::SeqCst);
            })),
            ..Tuning::default()
        },
    );
    let events = collect(&diff);
    let done = summary(&events);
    assert_eq!(done.extra_workers, 1);
    assert_eq!(steps(&events, StepReason::Watchdog).len(), 1);
    assert!(expired.load(Ordering::SeqCst) >= 1);
    assert!(
        diff.events().try_recv().is_err(),
        "the late reply is not an event"
    );
    let coarse: Vec<_> = covered(&events)
        .iter()
        .flat_map(|covered| covered.coarse.iter())
        .filter(|range| range.start == 0)
        .collect();
    assert_eq!(coarse.len(), 1, "the old result is not applied again");
    gate.release();
    wait_ids(&finished, |ids| ids.contains(&0));
    assert_within(&diff, Budget::default());
}

#[test]
fn an_expiry_during_a_collision_still_tiles_and_finishes() {
    let generated = generate(20_000);
    let (left, right) = generated.modified[9];
    let gate = Arc::new(Gate::default());
    let stalled = Arc::clone(&gate);
    let expired = Arc::new(AtomicUsize::new(0));
    let expiries = Arc::clone(&expired);
    let stall: Stall = Arc::new(move |line| {
        if line == 0 {
            stalled.wait();
        }
    });
    let tuning = Tuning {
        batch_lines: 1,
        watchdog: Duration::from_millis(30),
        stall: Some(stall),
        expired: Some(Arc::new(move |_| {
            expiries.fetch_add(1, Ordering::SeqCst);
        })),
        forge: Some(super::super::forge_at(left, right)),
        ..Tuning::default()
    };
    let diff = LargeDiff::start_tuned(
        request_budget(
            side(generated.left.as_bytes()),
            side(generated.right.as_bytes()),
            u64::from(left) + 1,
            Budget::default(),
        ),
        &CancelToken::new(),
        tuning,
    );
    let events = collect(&diff);
    let done = summary(&events);
    assert!(done.dropped_anchors >= 1, "the collision was checked");
    assert!(expired.load(Ordering::SeqCst) >= 1, "the deadline fired");
    assert_tiles(&events, done.left_lines, done.right_lines);
    gate.release();
    assert_within(&diff, Budget::default());
}

fn assert_restart(events: &[DiffEvent], shrunk: &str, right: &str) {
    assert_eq!(line_ranges(&hunks(events)), reference(shrunk, right));
}

#[test]
fn a_watchdog_expiry_then_a_restart_stays_exact() {
    let generated = generate(20_000);
    let shrunk = generated.left[..generated.left.len() / 2].to_owned();
    let source = Arc::new(Swap::new(
        generated.left.clone().into_bytes(),
        shrunk.clone().into_bytes(),
        usize::MAX,
    ));
    let trigger = Arc::clone(&source);
    let gate = Arc::new(Gate::default());
    let stalled = Arc::clone(&gate);
    let once = Arc::new(AtomicBool::new(false));
    let expired = Arc::new(AtomicUsize::new(0));
    let expiries = Arc::clone(&expired);
    let swap = Arc::clone(&trigger);
    let swapped = Arc::new(AtomicBool::new(false));
    let stall: Stall = Arc::new(move |line| {
        if line == 0 && !once.swap(true, Ordering::SeqCst) {
            stalled.wait();
        }
    });
    let diff = LargeDiff::start_tuned(
        request_budget(
            swap_side(source),
            side(generated.right.as_bytes()),
            0,
            Budget::default(),
        ),
        &CancelToken::new(),
        Tuning {
            batch_lines: 1,
            watchdog: Duration::from_secs(2),
            stall: Some(stall),
            expired: Some(Arc::new(move |_| {
                expiries.fetch_add(1, Ordering::SeqCst);
                if !swapped.swap(true, Ordering::SeqCst) {
                    swap.swap();
                }
            })),
            ..Tuning::default()
        },
    );
    let events = collect(&diff);
    let done = summary(&events);
    assert!(
        done.restarts >= 1 && expired.load(Ordering::SeqCst) >= 1,
        "the source change restarts after a deadline"
    );
    assert_restart(&events, &shrunk, &generated.right);
    gate.release();
    assert_within(&diff, Budget::default());
}

struct BlockOnce {
    bytes: Vec<u8>,
    armed: Arc<AtomicBool>,
    slept: Arc<AtomicBool>,
}

impl ByteSource for BlockOnce {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        if self.armed.load(Ordering::SeqCst)
            && buf.len() >= 1024
            && !self.slept.swap(true, Ordering::SeqCst)
        {
            std::thread::sleep(Duration::from_millis(500));
        }
        self.bytes.as_slice().read_at(offset, buf)
    }
}

#[test]
fn a_blocked_sparse_seek_is_expired_by_the_watchdog() {
    let generated = generate(20_000);
    let armed = Arc::new(AtomicBool::new(false));
    let slept = Arc::new(AtomicBool::new(false));
    let left = LargeSide {
        source: Arc::new(BlockOnce {
            bytes: generated.left.clone().into_bytes(),
            armed: Arc::clone(&armed),
            slept,
        }),
        layout: TextLayout::default(),
    };
    let arm = Arc::clone(&armed);
    let forge: Forge = Arc::new(move |_, _| arm.store(true, Ordering::SeqCst));
    let expired = Arc::new(AtomicUsize::new(0));
    let expiries = Arc::clone(&expired);
    let diff = LargeDiff::start_tuned(
        request_budget(left, side(generated.right.as_bytes()), 0, Budget::default()),
        &CancelToken::new(),
        Tuning {
            batch_lines: 1,
            watchdog: Duration::from_millis(30),
            forge: Some(forge),
            expired: Some(Arc::new(move |_| {
                expiries.fetch_add(1, Ordering::SeqCst);
            })),
            ..Tuning::default()
        },
    );
    let events = collect(&diff);
    assert!(expired.load(Ordering::SeqCst) >= 1, "the seek was expired");
    assert!(!steps(&events, StepReason::Watchdog).is_empty());
    let done = summary(&events);
    assert_tiles(&events, done.left_lines, done.right_lines);
    assert_within(&diff, Budget::default());
}

struct Capture {
    bytes: Vec<u8>,
    armed: Arc<AtomicBool>,
    names: Arc<Mutex<Vec<String>>>,
}

impl ByteSource for Capture {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        if self.armed.load(Ordering::SeqCst) && buf.len() >= 1024 {
            let name = std::thread::current().name().unwrap_or("none").to_owned();
            self.names.lock().unwrap().push(name);
        }
        self.bytes.as_slice().read_at(offset, buf)
    }
}

fn assert_finished(events: &[DiffEvent]) {
    let done = summary(events);
    assert_tiles(events, done.left_lines, done.right_lines);
}

fn capture(bytes: &str, names: &Arc<Mutex<Vec<String>>>, armed: &Arc<AtomicBool>) -> LargeSide {
    LargeSide {
        source: Arc::new(Capture {
            bytes: bytes.as_bytes().to_vec(),
            armed: Arc::clone(armed),
            names: Arc::clone(names),
        }),
        layout: TextLayout::default(),
    }
}

#[test]
fn recovery_reads_run_off_the_scheduler() {
    let generated = generate(30_000);
    let gate = Arc::new(Gate::default());
    let stalled = Arc::clone(&gate);
    let finished = Arc::new(Mutex::new(Vec::new()));
    let hook = Arc::clone(&finished);
    let expired = Arc::new(AtomicUsize::new(0));
    let expiries = Arc::clone(&expired);
    let armed = Arc::new(AtomicBool::new(false));
    let arm = Arc::clone(&armed);
    let names = Arc::new(Mutex::new(Vec::new()));
    let stall: Stall = Arc::new(move |line| {
        if line < 2_000 {
            stalled.wait();
        }
    });
    let left = capture(&generated.left, &names, &armed);
    let diff = LargeDiff::start_tuned(
        request_budget(left, side(generated.right.as_bytes()), 0, Budget::default()),
        &CancelToken::new(),
        Tuning {
            batch_lines: 1,
            watchdog: Duration::from_millis(30),
            stall: Some(stall),
            finished: Some(Arc::new(move |id| hook.lock().unwrap().push(id))),
            expired: Some(Arc::new(move |_| {
                if expiries.fetch_add(1, Ordering::SeqCst) + 1 >= 6 {
                    arm.store(true, Ordering::SeqCst);
                }
            })),
            ..Tuning::default()
        },
    );
    let events = collect(&diff);
    assert!(
        expired.load(Ordering::SeqCst) >= 6,
        "workers were abandoned"
    );
    let names = names.lock().unwrap().clone();
    assert!(
        names.iter().any(|name| name == "large-diff-recovery"),
        "a coarse read ran on a recovery thread: {names:?}"
    );
    assert!(
        !names.iter().any(|name| name == "large-diff"),
        "the scheduler performed no source read"
    );
    assert_finished(&events);
    gate.release();
    wait_ids(&finished, |ids| ids.len() >= 6);
    assert_within(&diff, Budget::default());
}

struct SlowCapture {
    bytes: Vec<u8>,
    armed: Arc<AtomicBool>,
    gate: Arc<Gate>,
}

impl ByteSource for SlowCapture {
    fn len(&self) -> u64 {
        self.bytes.len() as u64
    }

    fn read_at(&self, offset: u64, buf: &mut [u8]) -> Result<usize, String> {
        if self.armed.load(Ordering::SeqCst) && buf.len() >= 1024 {
            self.gate.wait();
        }
        self.bytes.as_slice().read_at(offset, buf)
    }
}

#[test]
fn a_full_ledger_finishes_with_a_stalled_worker() {
    let (left, right) = dense(20_000);
    let gate = Arc::new(Gate::default());
    let stalled = Arc::clone(&gate);
    let armed = Arc::new(AtomicBool::new(false));
    let arm = Arc::clone(&armed);
    let unblock = Arc::clone(&gate);
    let source = Arc::new(SlowCapture {
        bytes: left.into_bytes(),
        armed,
        gate: stalled,
    });
    let forge: Forge = Arc::new(move |_, _| arm.store(true, Ordering::SeqCst));
    let diff = LargeDiff::start_tuned(
        request_budget(
            LargeSide {
                source,
                layout: TextLayout::default(),
            },
            side(right.as_bytes()),
            0,
            Budget::default(),
        ),
        &CancelToken::new(),
        Tuning {
            batch_lines: u64::MAX,
            watchdog: Duration::from_millis(30),
            forge: Some(forge),
            hold_available: true,
            expired: Some(Arc::new(move |_| {
                unblock.release();
            })),
            ..Tuning::default()
        },
    );
    let events = collect(&diff);
    assert_finished(&events);
    assert_within(&diff, Budget::default());
}

fn cancel_after_six(expired: &Arc<AtomicUsize>, token: &CancelToken) {
    let wait = Arc::clone(expired);
    let cancel = token.clone();
    std::thread::spawn(move || {
        while wait.load(Ordering::SeqCst) < 6 {
            std::thread::sleep(Duration::from_millis(1));
        }
        cancel.cancel();
    });
}

fn collect_short(diff: &LargeDiff) -> Vec<DiffEvent> {
    let mut events = Vec::new();
    loop {
        let event = diff
            .events()
            .recv_timeout(Duration::from_secs(5))
            .expect("cancel must end the diff");
        let done = matches!(event, DiffEvent::Done(_));
        events.push(event);
        if done {
            return events;
        }
    }
}

#[test]
fn cancel_ends_a_blocked_recovery_read() {
    let (left, right) = dense(20_000);
    let gate = Arc::new(Gate::default());
    let stalled = Arc::clone(&gate);
    let armed = Arc::new(AtomicBool::new(false));
    let arm = Arc::clone(&armed);
    let expired = Arc::new(AtomicUsize::new(0));
    let expiries = Arc::clone(&expired);
    let token = CancelToken::new();
    cancel_after_six(&expired, &token);
    let source = Arc::new(SlowCapture {
        bytes: left.into_bytes(),
        armed,
        gate: stalled,
    });
    let forge: Forge = Arc::new(move |_, _| arm.store(true, Ordering::SeqCst));
    let diff = LargeDiff::start_tuned(
        request_budget(
            LargeSide {
                source,
                layout: TextLayout::default(),
            },
            side(right.as_bytes()),
            0,
            Budget::default(),
        ),
        &token,
        Tuning {
            batch_lines: 100,
            watchdog: Duration::from_millis(30),
            forge: Some(forge),
            expired: Some(Arc::new(move |_| {
                expiries.fetch_add(1, Ordering::SeqCst);
            })),
            ..Tuning::default()
        },
    );
    let events = collect_short(&diff);
    assert_eq!(failure(&events), Reason::Stopped);
    gate.release();
}

fn wait_for_waiting(diff: &LargeDiff) -> u32 {
    loop {
        match diff.events().recv_timeout(Duration::from_secs(5)) {
            Ok(DiffEvent::Progress(progress)) if progress.stage == Stage::Waiting => return 1,
            Ok(_) => {}
            Err(_) => panic!("the waiting state is visible"),
        }
    }
}

#[test]
fn a_full_ledger_backs_off_and_reports_waiting() {
    let (left, right) = dense(2_000);
    let gate = Arc::new(Gate::default());
    let stalled = Arc::clone(&gate);
    let armed = Arc::new(AtomicBool::new(false));
    let arm = Arc::clone(&armed);
    let unblock = Arc::clone(&gate);
    let token = CancelToken::new();
    let source = Arc::new(SlowCapture {
        bytes: left.into_bytes(),
        armed,
        gate: stalled,
    });
    let forge: Forge = Arc::new(move |_, _| arm.store(true, Ordering::SeqCst));
    let diff = LargeDiff::start_tuned(
        request_budget(
            LargeSide {
                source,
                layout: TextLayout::default(),
            },
            side(right.as_bytes()),
            0,
            Budget::default(),
        ),
        &token,
        Tuning {
            batch_lines: u64::MAX,
            watchdog: Duration::from_millis(30),
            forge: Some(forge),
            hold_all: true,
            expired: Some(Arc::new(move |_| unblock.release())),
            ..Tuning::default()
        },
    );
    let mut waiting = wait_for_waiting(&diff);
    let deadline = Instant::now() + Duration::from_millis(250);
    while Instant::now() < deadline {
        let left = deadline.saturating_duration_since(Instant::now());
        match diff.events().recv_timeout(left) {
            Ok(DiffEvent::Progress(progress)) if progress.stage == Stage::Waiting => waiting += 1,
            Ok(_) => {}
            Err(_) => break,
        }
    }
    assert!(waiting <= 8, "retries follow the backoff, got {waiting}");
    token.cancel();
    let events = collect_short(&diff);
    assert_eq!(failure(&events), Reason::Stopped);
    gate.release();
}
