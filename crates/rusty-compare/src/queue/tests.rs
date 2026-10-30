use std::thread;
use std::time::{Duration, Instant};

use super::*;

fn job(event: CheckEvent) -> Job {
    match event {
        CheckEvent::Job(job) => job,
        CheckEvent::Stopped(id) => panic!("expected a job, got stopped row {id}"),
    }
}

fn stopped(event: CheckEvent) -> u64 {
    match event {
        CheckEvent::Stopped(id) => id,
        CheckEvent::Job(job) => panic!("expected a stopped row, got job {}", job.id),
    }
}

fn spec(id: u64, size: u64, volumes: (u64, u64)) -> JobSpec {
    JobSpec { id, size, volumes }
}

#[test]
fn visible_rows_run_before_smaller_rows() {
    let queue = Queue::new(1);
    queue.push(1, 100, (0, 0));
    queue.push(2, 1, (0, 0));
    queue.focus(&[1]);

    assert_eq!(job(queue.next().expect("visible job")).id, 1);
    queue.complete(1);
    assert_eq!(job(queue.next().expect("small job")).id, 2);
}

#[test]
fn no_more_than_the_worker_limit_runs_at_once() {
    let queue = Queue::new(2);
    queue.push(1, 1, (0, 0));
    queue.push(2, 1, (0, 0));
    queue.push(3, 1, (0, 0));

    assert!(queue.try_next().is_some());
    assert!(queue.try_next().is_some());
    assert!(queue.try_next().is_none());
    queue.complete(1);
    assert!(queue.try_next().is_some());
    assert!(worker_limit() <= MAX_WORKERS);
}

#[test]
fn at_most_twelve_workers_run_at_once() {
    let queue = Queue::new(MAX_WORKERS);
    for id in 1..=u64::try_from(MAX_WORKERS + 1).unwrap() {
        queue.push(id, 1, (0, 0));
    }
    for _ in 0..MAX_WORKERS {
        assert!(queue.try_next().is_some());
    }
    assert!(queue.try_next().is_none());
    queue.complete(1);
    assert!(queue.try_next().is_some());
}

#[test]
fn map_is_used_only_when_ten_pairs_remain() {
    let queue = Queue::new(1);
    let batch: Vec<JobSpec> = (1..=11).map(|id| spec(id, 1, (0, 0))).collect();
    queue.push_batch(&batch);

    let first = job(queue.next().expect("first job"));
    assert_eq!(first.read_hint, ReadHint::Buffered);
    queue.complete(first.id);
    let second = job(queue.next().expect("second job"));
    assert_eq!(second.read_hint, ReadHint::Map);
}

#[test]
fn large_jobs_hold_both_volumes() {
    let queue = Queue::new(3);
    queue.push(1, LARGE_FILE + 1, (10, 11));
    queue.push(2, LARGE_FILE + 1, (10, 12));
    queue.push(3, LARGE_FILE + 1, (20, 21));

    let first = job(queue.next().expect("first volume pair"));
    let second = job(queue.next().expect("disjoint volume pair"));
    assert_eq!((first.id, second.id), (1, 3));
    assert!(queue.try_next().is_none());
    queue.complete(1);
    assert_eq!(job(queue.next().expect("shared volume pair")).id, 2);
}

#[test]
fn a_shared_pair_counts_as_one_volume() {
    let queue = Queue::new(2);
    queue.push(1, LARGE_FILE + 1, (10, 10));
    queue.push(2, LARGE_FILE + 1, (10, 10));

    assert_eq!(job(queue.next().expect("first large job")).id, 1);
    assert!(queue.try_next().is_none());
    queue.complete(1);
    assert_eq!(job(queue.next().expect("second large job")).id, 2);
}

#[test]
fn a_paused_batch_can_be_stopped_before_it_is_published() {
    let queue = Queue::new(1);
    queue.pause();
    queue.push_batch(&[spec(7, 1, (0, 0))]);
    assert!(queue.stop(7));
    queue.resume();
    assert_eq!(stopped(queue.next().expect("stopped event")), 7);
}

#[test]
fn progress_events_are_limited_to_four_per_second() {
    let queue = Queue::new(1);
    queue.push(1, LARGE_FILE + 1, (0, 0));
    let running = job(queue.next().expect("large job"));
    let total = LARGE_FILE + 1;
    let started = Instant::now();
    let first = queue
        .progress(running.id, 1, total)
        .expect("first progress");
    assert_eq!(first.bytes_done, 1);
    for _ in 0..10 {
        assert!(queue.progress(running.id, 2, total).is_none());
    }
    let deadline = started + Duration::from_secs(5);
    while queue.progress(running.id, 3, total).is_none() {
        assert!(Instant::now() < deadline, "progress stayed rate-limited");
        thread::sleep(Duration::from_millis(5));
    }
    assert!(started.elapsed() >= PROGRESS_INTERVAL);
}

#[test]
fn stop_one_cancels_an_active_job_and_removes_a_queued_one() {
    let queue = Queue::new(1);
    queue.push(1, 1, (0, 0));
    queue.push(2, 1, (0, 0));
    let running = job(queue.next().expect("first job"));
    assert!(queue.stop(1));
    assert!(running.cancel.is_cancelled());
    queue.complete(1);

    assert!(queue.stop(2));
    assert_eq!(stopped(queue.next().expect("stopped event")), 2);
}

#[test]
fn stop_all_stops_every_row() {
    let queue = Queue::new(1);
    queue.push(1, 1, (0, 0));
    queue.push(2, 1, (0, 0));
    queue.push(3, 1, (0, 0));
    let running = job(queue.next().expect("first job"));
    queue.stop_all();
    queue.push(4, 1, (0, 0));
    assert!(running.cancel.is_cancelled());

    let first = stopped(queue.next().expect("stopped row"));
    let second = stopped(queue.next().expect("stopped row"));
    let third = stopped(queue.next().expect("stopped row"));
    let mut ids = [first, second, third];
    ids.sort_unstable();
    assert_eq!(ids, [2, 3, 4]);
    queue.complete(1);
}

impl Queue {
    pub(crate) fn retained_bytes(&self) -> usize {
        let state = self.state();
        let job = std::mem::size_of::<u64>() + std::mem::size_of::<JobState>();
        let candidate = std::mem::size_of::<(u64, u64)>() + 3 * std::mem::size_of::<usize>();
        let mut bytes = state.jobs.capacity() * job;
        bytes += state.visible.capacity() * std::mem::size_of::<u64>();
        bytes += (state.visible_order.len() + state.other_order.len()) * candidate;
        bytes += state.stopped.capacity() * std::mem::size_of::<u64>();
        bytes += state.last_progress.capacity()
            * (std::mem::size_of::<u64>() + std::mem::size_of::<Instant>());
        bytes
    }
}

#[test]
fn completion_reports_whether_a_stop_won() {
    let queue = Queue::new(1);
    queue.push(1, 1, (0, 0));
    let running = job(queue.next().expect("first job"));
    assert!(queue.stop(running.id));
    assert_eq!(queue.complete(running.id), Settlement::Stopped);

    queue.push(2, 1, (0, 0));
    let running = job(queue.next().expect("second job"));
    assert_eq!(queue.complete(running.id), Settlement::Checked);
    assert!(!queue.stop(running.id));
}
