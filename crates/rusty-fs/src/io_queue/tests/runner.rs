use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Receiver, Sender};
use std::sync::{Arc, Condvar, Mutex};
use std::thread;
use std::time::{Duration, Instant};

use super::*;
use crate::CancelToken;

const WAIT: Duration = Duration::from_secs(5);
const OVERLAP_WINDOW: Duration = Duration::from_millis(100);

#[derive(Default)]
struct Gate {
    open: Mutex<bool>,
    changed: Condvar,
}

impl Gate {
    fn open(&self) {
        *self.open.lock().expect("gate lock") = true;
        self.changed.notify_all();
    }

    fn wait(&self, timeout: Duration) -> bool {
        let open = self.open.lock().expect("gate lock");
        let (open, _) = self
            .changed
            .wait_timeout_while(open, timeout, |open| !*open)
            .expect("gate lock");
        *open
    }
}

fn wait_cancelled(token: &CancelToken) -> bool {
    let deadline = Instant::now() + WAIT;
    while !token.is_cancelled() && Instant::now() < deadline {
        thread::park_timeout(Duration::from_millis(1));
    }
    token.is_cancelled()
}

fn queue_with(kind: VolumeKind) -> IoQueue {
    let queue = IoQueue::new();
    queue.set_volume(vol("V:"), kind);
    queue
}

fn submit(
    queue: &IoQueue,
    priority: Priority,
    owner: OwnerId,
    job: impl FnOnce(&CancelToken) + Send + 'static,
) -> JobId {
    queue.submit(vol("V:"), priority, owner, Box::new(job))
}

fn signal<T: Send + 'static>(
    tx: &Sender<T>,
    value: T,
) -> impl FnOnce(&CancelToken) + Send + 'static {
    let tx = tx.clone();
    move |_| {
        let _ = tx.send(value);
    }
}

fn collect<T>(rx: &Receiver<T>, count: usize) -> Vec<T> {
    (0..count)
        .map(|_| {
            rx.recv_timeout(WAIT)
                .expect("job reported within the timeout")
        })
        .collect()
}

fn blocker(queue: &IoQueue, owner: OwnerId) -> Arc<Gate> {
    let (started, wait_started) = mpsc::channel();
    let release = Arc::new(Gate::default());
    let gate = Arc::clone(&release);
    submit(queue, Priority::Visible, owner, move |_| {
        let _ = started.send(());
        gate.wait(WAIT);
    });
    collect(&wait_started, 1);
    release
}

#[test]
fn io_queue_is_send_and_sync() {
    fn assert_send_sync<T: Send + Sync>() {}
    assert_send_sync::<IoQueue>();
    assert_send_sync::<OwnerGuard>();
}

#[test]
fn hdd_never_runs_two_jobs_at_once() {
    let queue = queue_with(VolumeKind::Hdd);
    let active = Arc::new(AtomicUsize::new(0));
    let high = Arc::new(AtomicUsize::new(0));
    let overlap = Arc::new(Gate::default());
    let (done, wait_done) = mpsc::channel();
    for _ in 0..4 {
        let (active, high, overlap, done) =
            (active.clone(), high.clone(), overlap.clone(), done.clone());
        submit(&queue, Priority::Visible, A, move |_| {
            let now = active.fetch_add(1, Ordering::SeqCst) + 1;
            high.fetch_max(now, Ordering::SeqCst);
            if now > 1 {
                overlap.open();
            }
            overlap.wait(OVERLAP_WINDOW);
            active.fetch_sub(1, Ordering::SeqCst);
            let _ = done.send(());
        });
    }
    collect(&wait_done, 4);
    assert_eq!(high.load(Ordering::SeqCst), 1);
}

#[test]
fn ssd_runs_eight_jobs_at_once() {
    let queue = queue_with(VolumeKind::Ssd);
    let arrived = Arc::new((Mutex::new(0usize), Condvar::new()));
    let (done, wait_done) = mpsc::channel();
    for _ in 0..8 {
        let (arrived, done) = (Arc::clone(&arrived), done.clone());
        submit(&queue, Priority::Visible, A, move |_| {
            let (count, all_here) = &*arrived;
            let mut count = count.lock().expect("barrier lock");
            *count += 1;
            all_here.notify_all();
            let (count, _) = all_here
                .wait_timeout_while(count, WAIT, |count| *count < 8)
                .expect("barrier lock");
            let _ = done.send(*count >= 8);
        });
    }
    assert_eq!(collect(&wait_done, 8), vec![true; 8]);
}

#[test]
fn queued_jobs_run_by_priority() {
    let queue = queue_with(VolumeKind::Hdd);
    let release = blocker(&queue, A);
    let (order, wait_order) = mpsc::channel();
    submit(&queue, Priority::Idle, A, signal(&order, Priority::Idle));
    submit(
        &queue,
        Priority::Prefetch,
        A,
        signal(&order, Priority::Prefetch),
    );
    submit(
        &queue,
        Priority::Visible,
        A,
        signal(&order, Priority::Visible),
    );
    release.open();
    assert_eq!(
        collect(&wait_order, 3),
        vec![Priority::Visible, Priority::Prefetch, Priority::Idle]
    );
}

#[test]
fn reprioritised_job_runs_first() {
    let queue = queue_with(VolumeKind::Hdd);
    let release = blocker(&queue, A);
    let (order, wait_order) = mpsc::channel();
    submit(&queue, Priority::Prefetch, A, signal(&order, "prefetch"));
    let idle = submit(&queue, Priority::Idle, A, signal(&order, "raised"));
    queue.reprioritise(idle, Priority::Visible);
    release.open();
    assert_eq!(collect(&wait_order, 2), vec!["raised", "prefetch"]);
}

fn running_job_that_reports_cancel(queue: &IoQueue, owner: OwnerId) -> Receiver<bool> {
    let (started, wait_started) = mpsc::channel();
    let (cancelled, wait_cancel) = mpsc::channel();
    submit(queue, Priority::Visible, owner, move |token| {
        let _ = started.send(());
        let _ = cancelled.send(wait_cancelled(token));
    });
    collect(&wait_started, 1);
    wait_cancel
}

#[test]
fn dropping_the_owner_skips_its_queued_jobs() {
    let queue = queue_with(VolumeKind::Hdd);
    let guard = queue.owner();
    let _running = running_job_that_reports_cancel(&queue, guard.id());
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    submit(&queue, Priority::Visible, guard.id(), move |_| {
        flag.store(true, Ordering::SeqCst)
    });
    drop(guard);
    let other = queue.owner();
    let (after, wait_after) = mpsc::channel();
    submit(&queue, Priority::Visible, other.id(), signal(&after, ()));
    collect(&wait_after, 1);
    assert!(!ran.load(Ordering::SeqCst));
}

#[test]
fn dropping_the_owner_cancels_its_running_job() {
    let queue = queue_with(VolumeKind::Hdd);
    let guard = queue.owner();
    let cancelled = running_job_that_reports_cancel(&queue, guard.id());
    drop(guard);
    assert_eq!(collect(&cancelled, 1), vec![true]);
}

#[test]
fn cancel_owner_cancels_its_running_job() {
    let queue = queue_with(VolumeKind::Hdd);
    let cancelled = running_job_that_reports_cancel(&queue, A);
    queue.cancel_owner(A);
    assert_eq!(collect(&cancelled, 1), vec![true]);
}

#[test]
fn owners_get_distinct_ids() {
    let queue = IoQueue::new();
    assert_ne!(queue.owner().id(), queue.owner().id());
}

#[test]
fn panicking_job_does_not_stop_later_jobs() {
    let queue = queue_with(VolumeKind::Hdd);
    submit(&queue, Priority::Visible, A, |_| {
        panic!("job failed on purpose")
    });
    let (done, wait_done) = mpsc::channel();
    submit(&queue, Priority::Visible, A, signal(&done, ()));
    assert!(wait_done.recv_timeout(WAIT).is_ok());
}

fn shutdown_with_running_and_queued() -> (Receiver<bool>, Arc<AtomicBool>, Receiver<()>) {
    let queue = queue_with(VolumeKind::Hdd);
    let cancelled = running_job_that_reports_cancel(&queue, A);
    let ran = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&ran);
    submit(&queue, Priority::Visible, A, move |_| {
        flag.store(true, Ordering::SeqCst)
    });
    let (returned, wait_returned) = mpsc::channel();
    thread::spawn(move || {
        queue.shutdown();
        let _ = returned.send(());
    });
    (cancelled, ran, wait_returned)
}

#[test]
fn shutdown_returns_after_running_jobs_end() {
    let (_, _, returned) = shutdown_with_running_and_queued();
    assert!(returned.recv_timeout(WAIT).is_ok());
}

#[test]
fn shutdown_cancels_running_jobs() {
    let (cancelled, _, _) = shutdown_with_running_and_queued();
    assert_eq!(collect(&cancelled, 1), vec![true]);
}

#[test]
fn shutdown_drops_queued_jobs_unrun() {
    let (_, ran, returned) = shutdown_with_running_and_queued();
    collect(&returned, 1);
    assert!(!ran.load(Ordering::SeqCst));
}
