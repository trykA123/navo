//! Tests for the Stream detail lane: ordering, resends, throttling and cancellation.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver};
use std::sync::{Arc, Barrier, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use navo_ui::compare_diff::{CardDetail, PairKind, StreamControl, StreamUpdate};

use crate::compare::diff::stream::{DetailFn, StreamParts, StreamSession};
use crate::compare::diff::tests::stream::support::{
    PairData, channel, items, not_viewed, simple_load,
};

const WAIT: Duration = Duration::from_secs(20);
const BUDGET: u64 = 64 * 1024 * 1024;

/// One in-memory pair with a NUL byte on each side, so it loads as `Binary`.
fn binary_data(id: u64) -> HashMap<u64, PairData> {
    HashMap::from([(
        id,
        PairData {
            left: b"a\0b".to_vec(),
            right: b"a\0c".to_vec(),
            blocked: false,
        },
    )])
}

/// The next detail update, skipping cards and progress; panics after `WAIT`.
fn next_detail(receiver: &Receiver<StreamUpdate>) -> (u64, Arc<CardDetail>) {
    loop {
        if let StreamUpdate::Detail { id, detail, .. } = receiver.recv_timeout(WAIT).unwrap() {
            return (id, detail);
        }
    }
}

#[test]
fn a_focused_binary_card_gets_its_detail_after_its_card() {
    let data = binary_data(3);
    let jobs = Arc::new(Mutex::new(Vec::new()));
    let detail: DetailFn = {
        let jobs = Arc::clone(&jobs);
        Arc::new(move |job, _input, emit| {
            jobs.lock().unwrap().push(job.id);
            emit(CardDetail::Failed("detail".to_owned()), true);
        })
    };
    let (sink, receiver) = channel();
    let session = StreamSession::start_with(
        sink,
        BUDGET,
        StreamParts {
            load: simple_load(data.clone()),
            viewed: not_viewed(),
            detail: Some(detail),
        },
    );
    session.focus(vec![3]);
    session.set_items(1, items(&data, true));

    let mut card_seen = false;
    loop {
        match receiver.recv_timeout(WAIT).unwrap() {
            StreamUpdate::Card { id: 3, card, .. } => {
                assert_eq!(card.kind, PairKind::Binary);
                card_seen = true;
            }
            StreamUpdate::Detail { id: 3, detail, .. } => {
                assert!(card_seen, "the detail arrived before its card");
                assert_eq!(*detail, CardDetail::Failed("detail".to_owned()));
                break;
            }
            _ => {}
        }
    }
    assert_eq!(*jobs.lock().unwrap(), vec![3]);
}

#[test]
fn refocus_resends_held_details() {
    let data = binary_data(2);
    let calls = Arc::new(AtomicUsize::new(0));
    let detail: DetailFn = {
        let calls = Arc::clone(&calls);
        Arc::new(move |_job, _input, emit| {
            calls.fetch_add(1, Ordering::SeqCst);
            emit(CardDetail::Failed("held".to_owned()), true);
        })
    };
    let (sink, receiver) = channel();
    let session = StreamSession::start_with(
        sink,
        BUDGET,
        StreamParts {
            load: simple_load(data.clone()),
            viewed: not_viewed(),
            detail: Some(detail),
        },
    );
    session.focus(vec![2]);
    session.set_items(1, items(&data, true));

    let first = next_detail(&receiver);
    session.focus(vec![2]);
    let second = next_detail(&receiver);
    assert_eq!(first.1, second.1);
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

#[test]
fn non_last_snapshots_are_throttled_and_the_last_always_arrives() {
    let data = binary_data(1);
    let detail: DetailFn = Arc::new(|_job, _input, emit| {
        emit(CardDetail::Failed("one".to_owned()), false);
        emit(CardDetail::Failed("two".to_owned()), false);
        emit(CardDetail::Failed("three".to_owned()), false);
        emit(CardDetail::Failed("last".to_owned()), true);
    });
    let (sink, receiver) = channel();
    let session = StreamSession::start_with(
        sink,
        BUDGET,
        StreamParts {
            load: simple_load(data.clone()),
            viewed: not_viewed(),
            detail: Some(detail),
        },
    );
    session.focus(vec![1]);
    session.set_items(1, items(&data, true));

    let first = next_detail(&receiver);
    let second = next_detail(&receiver);
    assert_eq!(*first.1, CardDetail::Failed("one".to_owned()));
    assert_eq!(*second.1, CardDetail::Failed("last".to_owned()));
}

#[test]
fn dropping_the_session_cancels_the_running_job() {
    let data = binary_data(4);
    let started = Arc::new(AtomicBool::new(false));
    let token = Arc::new(Mutex::new(None));
    let detail: DetailFn = {
        let started = Arc::clone(&started);
        let token = Arc::clone(&token);
        Arc::new(move |job, _input, _emit| {
            *token.lock().unwrap() = Some(job.cancel.clone());
            started.store(true, Ordering::Release);
            while !job.cancel.is_cancelled() {
                thread::sleep(Duration::from_millis(5));
            }
        })
    };
    let (sink, _receiver) = channel();
    let session = StreamSession::start_with(
        sink,
        BUDGET,
        StreamParts {
            load: simple_load(data.clone()),
            viewed: not_viewed(),
            detail: Some(detail),
        },
    );
    session.focus(vec![4]);
    session.set_items(1, items(&data, true));

    let deadline = Instant::now() + WAIT;
    while !started.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "the detail job did not start");
        thread::sleep(Duration::from_millis(5));
    }
    let cancel = token.lock().unwrap().clone().unwrap();
    let before = Instant::now();
    drop(session);
    assert!(
        before.elapsed() < Duration::from_secs(1),
        "the drop waited for the detail job"
    );
    let deadline = Instant::now() + WAIT;
    while !cancel.is_cancelled() {
        assert!(Instant::now() < deadline, "the job was not cancelled");
        thread::sleep(Duration::from_millis(5));
    }
}

#[test]
fn a_session_without_a_detail_fn_sends_no_details() {
    let data = binary_data(5);
    let (sink, receiver) = channel();
    let session = StreamSession::start_with(
        sink,
        BUDGET,
        StreamParts {
            load: simple_load(data.clone()),
            viewed: not_viewed(),
            detail: None,
        },
    );
    session.focus(vec![5]);
    session.set_items(1, items(&data, true));

    loop {
        if let StreamUpdate::Progress { built: 1, .. } = receiver.recv_timeout(WAIT).unwrap() {
            break;
        }
    }
    let next = receiver.recv_timeout(Duration::from_millis(300));
    assert!(!matches!(next, Ok(StreamUpdate::Detail { .. })));
}

#[test]
fn a_panicking_detail_job_does_not_stop_the_lane() {
    let mut data = binary_data(10);
    data.extend(binary_data(11));
    let panicked = Arc::new(AtomicBool::new(false));
    let panics = Arc::new(AtomicUsize::new(0));
    let detail: DetailFn = {
        let panicked = Arc::clone(&panicked);
        let panics = Arc::clone(&panics);
        Arc::new(move |job, _input, emit| {
            if job.id == 10 {
                panics.fetch_add(1, Ordering::SeqCst);
                panicked.store(true, Ordering::SeqCst);
                panic!("the detail job under test panicked");
            }
            emit(CardDetail::Failed("second".to_owned()), true);
        })
    };
    let (sink, receiver) = channel();
    let session = StreamSession::start_with(
        sink,
        BUDGET,
        StreamParts {
            load: simple_load(data.clone()),
            viewed: not_viewed(),
            detail: Some(detail),
        },
    );
    session.focus(vec![10]);
    session.set_items(1, items(&data, true));

    let deadline = Instant::now() + WAIT;
    while !panicked.load(Ordering::Acquire) {
        assert!(Instant::now() < deadline, "the first job did not panic");
        thread::sleep(Duration::from_millis(5));
    }
    session.focus(vec![11]);
    let (id, detail) = next_detail(&receiver);
    assert_eq!(id, 11);
    assert_eq!(*detail, CardDetail::Failed("second".to_owned()));
    assert_eq!(
        panics.load(Ordering::SeqCst),
        1,
        "the panicking id was retried"
    );
}

#[test]
fn focus_away_does_not_overlap_a_second_detail_job() {
    let mut data = binary_data(20);
    data.extend(binary_data(21));
    let active = Arc::new(AtomicUsize::new(0));
    let maximum = Arc::new(AtomicUsize::new(0));
    let first_done = Arc::new(AtomicBool::new(false));
    let (entered, jobs) = mpsc::channel();
    let held = Arc::new(Barrier::new(2));
    let detail: DetailFn = {
        let active = Arc::clone(&active);
        let maximum = Arc::clone(&maximum);
        let first_done = Arc::clone(&first_done);
        let held = Arc::clone(&held);
        Arc::new(move |job, _input, emit| {
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            maximum.fetch_max(now, Ordering::SeqCst);
            entered.send(job.id).unwrap();
            if job.id == 20 {
                held.wait();
                active.fetch_sub(1, Ordering::SeqCst);
                first_done.store(true, Ordering::SeqCst);
                return;
            }
            assert!(
                first_done.load(Ordering::SeqCst),
                "the second job started before the first returned"
            );
            active.fetch_sub(1, Ordering::SeqCst);
            emit(CardDetail::Failed("second".to_owned()), true);
        })
    };
    let (sink, receiver) = channel();
    let session = StreamSession::start_with(
        sink,
        BUDGET,
        StreamParts {
            load: simple_load(data.clone()),
            viewed: not_viewed(),
            detail: Some(detail),
        },
    );
    session.focus(vec![20]);
    session.set_items(1, items(&data, true));
    assert_eq!(jobs.recv_timeout(WAIT).unwrap(), 20);

    session.focus(vec![21]);
    held.wait();
    assert_eq!(jobs.recv_timeout(WAIT).unwrap(), 21);
    assert_eq!(maximum.load(Ordering::SeqCst), 1);
    assert_eq!(next_detail(&receiver).0, 21);
}

#[test]
fn a_generation_change_drops_the_old_jobs_late_snapshot() {
    let data = binary_data(30);
    let calls = Arc::new(AtomicUsize::new(0));
    let (started, entered) = mpsc::channel();
    let held = Arc::new(Barrier::new(2));
    let detail: DetailFn = {
        let calls = Arc::clone(&calls);
        let held = Arc::clone(&held);
        Arc::new(move |_job, _input, emit| {
            let call = calls.fetch_add(1, Ordering::SeqCst) + 1;
            if call == 1 {
                started.send(()).unwrap();
                held.wait();
                emit(CardDetail::Failed("late".to_owned()), true);
            }
        })
    };
    let (sink, receiver) = channel();
    let session = StreamSession::start_with(
        sink,
        BUDGET,
        StreamParts {
            load: simple_load(data.clone()),
            viewed: not_viewed(),
            detail: Some(detail),
        },
    );
    session.focus(vec![30]);
    session.set_items(1, items(&data, true));
    entered.recv_timeout(WAIT).unwrap();

    session.set_items(2, items(&data, true));
    held.wait();
    let deadline = Instant::now() + Duration::from_millis(300);
    while Instant::now() < deadline {
        match receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(StreamUpdate::Detail { .. }) => panic!("a stale detail escaped"),
            Ok(_) | Err(mpsc::RecvTimeoutError::Timeout) => {}
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
        }
    }
}
