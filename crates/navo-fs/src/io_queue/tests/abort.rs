//! Abort store bookkeeping (all platforms) and the Windows proof that `cancel_owner` interrupts a
//! network job blocked in a synchronous read (NV5b).

use std::sync::{Arc, Mutex};

use super::*;
use crate::io_queue::abort_win::{Abort, AbortStore};
use crate::io_queue::runner::store_network_abort;

/// A fake thread handle that records which job it aborted and when it was closed.
struct Recorder {
    job: u64,
    log: Arc<Mutex<Vec<String>>>,
}

impl Abort for Recorder {
    fn abort(&self) {
        self.log.lock().unwrap().push(format!("abort {}", self.job));
    }
}

impl Drop for Recorder {
    fn drop(&mut self) {
        self.log.lock().unwrap().push(format!("close {}", self.job));
    }
}

/// Starts two jobs per owner on one network volume and stores a recorder for each running job.
fn running_jobs(log: &Arc<Mutex<Vec<String>>>) -> (Scheduler, AbortStore<Recorder>, Vec<JobId>) {
    let mut scheduler = scheduler_with("net", VolumeKind::Network);
    let mut store = AbortStore::default();
    let jobs: Vec<JobId> = [A, B, A, B]
        .into_iter()
        .map(|owner| scheduler.push(vol("net"), Priority::Visible, owner))
        .collect();
    while let Some(job) = scheduler.start_next_on(&vol("net")) {
        let log = Arc::clone(log);
        store.insert(job, Recorder { job: job.0, log });
    }
    (scheduler, store, jobs)
}

#[test]
fn cancel_owner_aborts_only_that_owners_running_jobs() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let (mut scheduler, store, jobs) = running_jobs(&log);
    assert_eq!(store.len(), 4);
    let cancelled = scheduler.cancel_owner(A);
    store.abort(&cancelled.running);
    let mut seen = log.lock().unwrap().clone();
    seen.sort();
    assert_eq!(
        seen,
        [
            format!("abort {}", jobs[0].0),
            format!("abort {}", jobs[2].0)
        ]
    );
}

#[test]
fn a_removed_job_is_closed_and_never_aborted_again() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let (_, mut store, jobs) = running_jobs(&log);
    store.remove(jobs[0]);
    store.abort(&[jobs[0]]);
    assert_eq!(*log.lock().unwrap(), [format!("close {}", jobs[0].0)]);
    assert_eq!(store.len(), 3);
}

#[test]
fn shutdown_aborts_every_stored_handle() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let (_, store, _) = running_jobs(&log);
    store.abort_all();
    let aborts = log
        .lock()
        .unwrap()
        .iter()
        .filter(|line| line.starts_with("abort"))
        .count();
    assert_eq!(aborts, 4);
}

#[cfg(windows)]
#[test]
fn cancel_owner_frees_a_network_job_blocked_in_read_file_within_a_second() {
    use std::io::Read;
    use std::sync::mpsc;
    use std::time::{Duration, Instant};

    let queue = IoQueue::new();
    queue.set_volume(&vol(r"\\192.0.2.1\share"), VolumeKind::Network);
    let owner = queue.owner();
    let (reader, writer) = std::io::pipe().expect("anonymous pipe");
    let (started, wait_start) = mpsc::channel();
    let (done, wait_done) = mpsc::channel();
    let slot = JobSlot {
        volume: vol(r"\\192.0.2.1\share"),
        priority: Priority::Visible,
        owner: owner.id(),
    };
    queue.submit(
        slot,
        Box::new(move |_| {
            let mut reader = reader;
            started.send(()).unwrap();
            let read = reader.read(&mut [0_u8; 16]);
            done.send(read.map_err(|e| e.raw_os_error())).unwrap();
        }),
    );
    wait_start
        .recv_timeout(Duration::from_secs(5))
        .expect("job started");
    std::thread::sleep(Duration::from_millis(200));
    let cancelled_at = Instant::now();
    let mut answer = Err(mpsc::RecvTimeoutError::Timeout);
    // A cancel that lands just before ReadFile blocks finds no pending call, so retry inside the second.
    while answer.is_err() && cancelled_at.elapsed() < Duration::from_secs(1) {
        queue.cancel_owner(owner.id());
        answer = wait_done.recv_timeout(Duration::from_millis(100));
    }
    drop(writer);
    let read = answer.expect("the blocked read returned within 1 s of cancel_owner");
    assert_eq!(read, Err(Some(995)), "ERROR_OPERATION_ABORTED");
}

/// AD16: a second worker can start on a volume still being classified, so an Unknown volume
/// stores the handle too, and a cancel of that job reaches it.
#[test]
fn a_job_on_an_unknown_volume_stores_a_handle_that_cancel_reaches() {
    let log = Arc::new(Mutex::new(Vec::new()));
    let mut scheduler = Scheduler::new();
    let mut store = AbortStore::default();
    let job = scheduler.push(vol("new"), Priority::Visible, A);
    assert_eq!(scheduler.start_next_on(&vol("new")), Some(job));
    let recorder = Recorder {
        job: job.0,
        log: Arc::clone(&log),
    };
    let kind = scheduler.kind(&vol("new"));
    assert_eq!(kind, VolumeKind::Unknown);
    store_network_abort(&mut store, kind, job, || Some(recorder));
    assert_eq!(store.len(), 1);
    store.abort(&scheduler.cancel_owner(A).running);
    assert_eq!(log.lock().unwrap()[0], format!("abort {}", job.0));
}

/// One fake synchronous call per handle: `abort` frees it only while it is pending, the way
/// `CancelSynchronousIo` fails harmlessly on a thread with no call in flight (AD16b).
#[derive(Default)]
struct FakeIo {
    calls: Mutex<Vec<FakeCall>>,
    wake: std::sync::Condvar,
}

#[derive(Default)]
struct FakeCall {
    pending: bool,
    freed: bool,
    aborts: usize,
}

/// The abort handle of call `index` on a shared `FakeIo`.
struct FakeHandle {
    io: Arc<FakeIo>,
    index: usize,
}

impl Abort for FakeHandle {
    fn abort(&self) {
        let mut calls = self.io.calls.lock().unwrap();
        let call = &mut calls[self.index];
        call.aborts += 1;
        if call.pending {
            call.freed = true;
            self.io.wake.notify_all();
        }
    }
}

impl FakeIo {
    /// Blocks call `index` until an abort frees it or `limit` passes; true when aborted.
    fn block(&self, index: usize, limit: std::time::Duration) -> bool {
        let mut calls = self.calls.lock().unwrap();
        calls[index].pending = true;
        let (mut calls, _) = self
            .wake
            .wait_timeout_while(calls, limit, |calls| !calls[index].freed)
            .unwrap();
        calls[index].pending = false;
        calls[index].freed
    }

    fn aborts(&self, index: usize) -> usize {
        self.calls.lock().unwrap()[index].aborts
    }
}

/// A one-worker network queue whose jobs store `FakeHandle`s, numbered in start order.
fn fake_queue(io: &Arc<FakeIo>) -> IoQueue {
    let limits = Limits {
        network: 1,
        ..Limits::LISTING
    };
    let factory_io = Arc::clone(io);
    let queue = IoQueue::with_abort(
        limits,
        Arc::new(|_| VolumeKind::Network),
        Arc::new(move || {
            let mut calls = factory_io.calls.lock().unwrap();
            calls.push(FakeCall::default());
            let handle = FakeHandle {
                io: Arc::clone(&factory_io),
                index: calls.len() - 1,
            };
            Some(Box::new(handle) as crate::io_queue::abort_win::BoxedAbort)
        }),
    );
    queue.set_volume(&vol("net"), VolumeKind::Network);
    queue
}

fn net_slot(owner: OwnerId) -> JobSlot {
    JobSlot {
        volume: vol("net"),
        priority: Priority::Visible,
        owner,
    }
}

/// Submits a job that reports its start, waits for `go`, then enters fake call `index`.
fn blocking_job(
    queue: &IoQueue,
    io: &Arc<FakeIo>,
    owner: OwnerId,
    index: usize,
) -> (
    std::sync::mpsc::Receiver<()>,
    std::sync::mpsc::Sender<()>,
    std::sync::mpsc::Receiver<bool>,
) {
    use std::sync::mpsc;
    let (started, wait_start) = mpsc::channel();
    let (go, wait_go) = mpsc::channel::<()>();
    let (done, wait_done) = mpsc::channel();
    let io = Arc::clone(io);
    queue.submit(
        net_slot(owner),
        Box::new(move |_| {
            started.send(()).unwrap();
            let _ = wait_go.recv();
            done.send(io.block(index, std::time::Duration::from_secs(2)))
                .unwrap();
        }),
    );
    (wait_start, go, wait_done)
}

/// AD16b: a cancel that lands after the job registered its handle but before it entered its
/// blocking call is re-sent, so the call still ends well under a second.
#[test]
fn a_cancel_before_the_blocking_call_is_resent_until_the_job_ends() {
    use std::time::{Duration, Instant};
    let io = Arc::new(FakeIo::default());
    let queue = fake_queue(&io);
    let (started, go, done) = blocking_job(&queue, &io, A, 0);
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    queue.cancel_owner(A);
    assert_eq!(io.aborts(0), 1, "the first abort found no pending call");
    let entered = Instant::now();
    go.send(()).unwrap();
    let aborted = done.recv_timeout(Duration::from_secs(5)).unwrap();
    assert!(aborted, "a re-sent abort freed the blocking call");
    assert!(entered.elapsed() < Duration::from_millis(500));
    assert!(io.aborts(0) >= 2);
}

/// AD16b: the re-send stops once the cancelled job ends and never reaches the next job that
/// worker runs.
#[test]
fn the_abort_resend_stops_at_job_end_and_spares_the_next_job() {
    use std::time::Duration;
    let io = Arc::new(FakeIo::default());
    let queue = fake_queue(&io);
    let (started, go, done) = blocking_job(&queue, &io, A, 0);
    let (next_started, next_go, next_done) = blocking_job(&queue, &io, B, 1);
    started.recv_timeout(Duration::from_secs(5)).unwrap();
    queue.cancel_owner(A);
    go.send(()).unwrap();
    assert!(done.recv_timeout(Duration::from_secs(5)).unwrap());
    next_started.recv_timeout(Duration::from_secs(5)).unwrap();
    let first_aborts = io.aborts(0);
    next_go.send(()).unwrap();
    // The next job's call runs its full 2 s: no abort may free it.
    assert!(!next_done.recv_timeout(Duration::from_secs(5)).unwrap());
    assert_eq!(io.aborts(1), 0, "the next job was never aborted");
    assert_eq!(io.aborts(0), first_aborts, "re-sends stopped at job end");
}
