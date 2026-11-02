use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Barrier};
use std::time::Duration;

use super::*;

const WAIT: Duration = Duration::from_secs(5);

fn slot(owner: OwnerId) -> JobSlot {
    JobSlot {
        volume: vol("V:"),
        priority: Priority::Visible,
        owner,
    }
}

fn ssd_queue() -> IoQueue {
    let queue = IoQueue::with(Limits::LISTING, Arc::new(|_| VolumeKind::Ssd));
    queue.set_volume(&vol("V:"), VolumeKind::Ssd);
    queue
}

#[test]
fn a_finished_job_delivers_its_result_once() {
    let queue = ssd_queue();
    let owner = queue.owner();
    let (tx, rx) = mpsc::channel();
    queue.submit_then(
        slot(owner.id()),
        |_| 42,
        move |value| tx.send(value).unwrap(),
    );
    assert_eq!(rx.recv_timeout(WAIT), Ok(42));
    assert!(rx.recv_timeout(Duration::from_millis(100)).is_err());
}

#[test]
fn a_job_cancelled_while_working_never_delivers() {
    let queue = ssd_queue();
    let owner = queue.owner();
    let started = Arc::new(Barrier::new(2));
    let cancelled = Arc::new(Barrier::new(2));
    let (tx, rx) = mpsc::channel();
    let (in_work, may_finish) = (Arc::clone(&started), Arc::clone(&cancelled));
    let (done_tx, done_rx) = mpsc::channel();
    queue.submit_then(
        slot(owner.id()),
        move |_| {
            in_work.wait();
            may_finish.wait();
            done_tx.send(()).unwrap();
        },
        move |()| tx.send(()).unwrap(),
    );
    started.wait();
    drop(owner);
    cancelled.wait();
    done_rx.recv_timeout(WAIT).unwrap();
    assert!(rx.recv_timeout(Duration::from_millis(200)).is_err());
}

#[test]
fn a_cancel_raced_against_the_end_of_work_never_delivers_after_the_drop_returns() {
    let queue = ssd_queue();
    let late = Arc::new(AtomicUsize::new(0));
    for _ in 0..1_000 {
        let owner = queue.owner();
        let dropped = Arc::new(AtomicBool::new(false));
        let (seen, counter) = (Arc::clone(&dropped), Arc::clone(&late));
        queue.submit_then(
            slot(owner.id()),
            |_| (),
            move |()| {
                if seen.load(Ordering::SeqCst) {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            },
        );
        drop(owner);
        dropped.store(true, Ordering::SeqCst);
    }
    drop(queue);
    assert_eq!(late.load(Ordering::SeqCst), 0);
}

#[test]
fn dropping_the_owner_waits_for_a_delivery_in_progress() {
    let queue = ssd_queue();
    let owner = queue.owner();
    let delivered = Arc::new(AtomicBool::new(false));
    let finished = Arc::clone(&delivered);
    let (entered_tx, entered) = mpsc::channel();
    queue.submit_then(
        slot(owner.id()),
        |_| (),
        move |()| {
            entered_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            finished.store(true, Ordering::SeqCst);
        },
    );
    entered.recv_timeout(WAIT).unwrap();
    drop(owner);
    assert!(delivered.load(Ordering::SeqCst));
}

#[test]
fn a_panicking_job_does_not_deliver_and_the_queue_keeps_working() {
    let queue = ssd_queue();
    let owner = queue.owner();
    let (tx, rx) = mpsc::channel();
    let failed = tx.clone();
    queue.submit_then(
        slot(owner.id()),
        |_| -> u8 { panic!("work failed on purpose") },
        move |_| failed.send(0).unwrap(),
    );
    queue.submit_then(
        slot(owner.id()),
        |_| 7,
        move |value| tx.send(value).unwrap(),
    );
    assert_eq!(rx.recv_timeout(WAIT), Ok(7));
}

#[test]
fn delivering_may_cancel_its_own_owner_without_a_deadlock() {
    let queue = ssd_queue();
    let owner = queue.owner();
    let (tx, rx) = mpsc::channel();
    queue.submit_then(
        slot(owner.id()),
        |_| (),
        move |()| {
            drop(owner);
            tx.send(()).unwrap();
        },
    );
    assert_eq!(rx.recv_timeout(WAIT), Ok(()));
}

#[test]
fn delivering_on_one_queue_still_waits_for_another_queues_delivery() {
    let (first, second) = (ssd_queue(), ssd_queue());
    let (first_owner, second_owner) = (first.owner(), second.owner());
    let delivered = Arc::new(AtomicBool::new(false));
    let finished = Arc::clone(&delivered);
    let (entered_tx, entered) = mpsc::channel();
    second.submit_then(
        slot(second_owner.id()),
        |_| (),
        move |()| {
            entered_tx.send(()).unwrap();
            std::thread::sleep(Duration::from_millis(100));
            finished.store(true, Ordering::SeqCst);
        },
    );
    entered.recv_timeout(WAIT).unwrap();
    let (tx, rx) = mpsc::channel();
    first.submit_then(
        slot(first_owner.id()),
        |_| (),
        move |()| {
            drop(second_owner);
            tx.send(delivered.load(Ordering::SeqCst)).unwrap();
        },
    );
    assert_eq!(rx.recv_timeout(WAIT), Ok(true));
}
