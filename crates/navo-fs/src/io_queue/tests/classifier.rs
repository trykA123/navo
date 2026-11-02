use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Condvar, Mutex, mpsc};
use std::thread;
use std::time::Duration;

use super::*;

const WAIT: Duration = Duration::from_secs(5);

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

    fn wait(&self) -> bool {
        let open = self.open.lock().expect("gate lock");
        let (open, _) = self
            .changed
            .wait_timeout_while(open, WAIT, |open| !*open)
            .expect("gate lock");
        *open
    }
}

fn slot(volume: &str) -> JobSlot {
    JobSlot {
        volume: vol(volume),
        priority: Priority::Visible,
        owner: A,
    }
}

#[test]
fn first_worker_classifies_before_work_and_expands_to_eight_workers() {
    let classify_gate = Arc::new(Gate::default());
    let release = Arc::new(Gate::default());
    let (classifying, wait_classifying) = mpsc::channel();
    let gate = Arc::clone(&classify_gate);
    let queue = IoQueue::with(
        Limits::LISTING,
        Arc::new(move |_| {
            classifying.send(()).expect("classification reported");
            assert!(gate.wait());
            VolumeKind::Ssd
        }),
    );
    let (started, wait_started) = mpsc::channel();
    for index in 0..9 {
        let started = started.clone();
        let release = Arc::clone(&release);
        queue.submit(
            slot("V:"),
            Box::new(move |_| {
                started.send(index).expect("job reported");
                assert!(release.wait());
            }),
        );
    }
    wait_classifying
        .recv_timeout(WAIT)
        .expect("classifier started");
    assert!(wait_started.try_recv().is_err());
    classify_gate.open();
    for _ in 0..8 {
        wait_started
            .recv_timeout(WAIT)
            .expect("eight concurrent jobs");
    }
    assert!(wait_started.try_recv().is_err());
    release.open();
    wait_started.recv_timeout(WAIT).expect("last job runs");
}

#[test]
fn unknown_classification_runs_once_per_volume_on_a_worker() {
    let caller = thread::current().id();
    let calls = Arc::new(Mutex::new(HashMap::new()));
    let recorded = Arc::clone(&calls);
    let queue = IoQueue::with(
        Limits::LISTING,
        Arc::new(move |volume| {
            assert_ne!(thread::current().id(), caller);
            *recorded
                .lock()
                .expect("calls lock")
                .entry(volume.clone())
                .or_insert(0) += 1;
            VolumeKind::Unknown
        }),
    );
    let (done, wait_done) = mpsc::channel();
    for volume in ["V:", "W:", "V:", "W:"] {
        let done = done.clone();
        queue.submit(
            slot(volume),
            Box::new(move |_| done.send(()).expect("job ran")),
        );
        wait_done.recv_timeout(WAIT).expect("job completed");
    }
    assert_eq!(
        *calls.lock().expect("calls lock"),
        HashMap::from([(vol("V:"), 1), (vol("W:"), 1)])
    );
}

#[test]
fn panicking_classifier_keeps_unknown_limit_and_jobs_run() {
    let calls = Arc::new(AtomicUsize::new(0));
    let counted = Arc::clone(&calls);
    let queue = IoQueue::with(
        Limits::LISTING,
        Arc::new(move |_| {
            counted.fetch_add(1, Ordering::SeqCst);
            panic!("classifier failed on purpose");
        }),
    );
    let release = Arc::new(Gate::default());
    let gate = Arc::clone(&release);
    let (started, wait_started) = mpsc::channel();
    queue.submit(
        slot("V:"),
        Box::new(move |_| {
            started.send(()).expect("first job ran");
            assert!(gate.wait());
        }),
    );
    wait_started
        .recv_timeout(WAIT)
        .expect("first job runs after panic");
    let (done, wait_done) = mpsc::channel();
    queue.submit(
        slot("V:"),
        Box::new(move |_| done.send(()).expect("second job ran")),
    );
    assert!(wait_done.recv_timeout(Duration::from_millis(100)).is_err());
    release.open();
    wait_done.recv_timeout(WAIT).expect("next job runs");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}
