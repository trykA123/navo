//! Abort store bookkeeping (all platforms) and the Windows proof that `cancel_owner` interrupts a
//! network job blocked in a synchronous read (NV5b).

use std::sync::{Arc, Mutex};

use super::*;
use crate::io_queue::abort_win::{Abort, AbortStore};

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
