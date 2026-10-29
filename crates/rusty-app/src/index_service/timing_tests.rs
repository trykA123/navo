use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Barrier, mpsc};
use std::time::{Duration, Instant};

use rusty_index::file;
use rusty_index::ntfs::{NtfsIndex, test_support::MftFixture};
use rusty_index::pipe::{Reply, Request};
use rusty_index::usn::Resume;

use super::answer::{Shared, SharedVolume};
use super::batch::{apply_batch, published};
use super::config::Drive;
use super::lifecycle::restore_or_enumerate;

mod fixture;
use fixture::*;

#[derive(Clone, Copy, Debug)]
enum Start {
    Present,
    Stale,
    JournalReset,
}

struct StartupFixture {
    case: Start,
    path: std::path::PathBuf,
    mft: Option<MftFixture>,
    catchup: rusty_index::usn::UsnBatch,
}

impl StartupFixture {
    fn restore(
        self,
        shared: &super::answer::Shared,
        phase: (mpsc::SyncSender<()>, mpsc::Receiver<()>),
    ) -> (Duration, Duration) {
        let (reached, continued) = phase;
        let drive = Drive {
            letter: 'C',
            serial: 42,
            fixed: true,
            ntfs: true,
        };
        let started = Instant::now();
        let index = restore_or_enumerate(file::load_with_ids(&self.path), drive, || {
            panic!("a valid snapshot must be published before catch-up")
        })
        .unwrap();
        let restore = started.elapsed();
        if matches!(self.case, Start::JournalReset) {
            assert_eq!(index.position.resume(8, 0), Resume::Reenumerate);
            drop(index);
            reached.send(()).unwrap();
            continued.recv_timeout(WAIT).unwrap();
            let enumerated = self.mft.unwrap().read();
            assert!(enumerated.unresolved.is_empty());
            assert_eq!(enumerated.sizes_unknown, 0);
            let publish = publish(shared, enumerated.index, None);
            return (started.elapsed(), publish);
        }
        assert_eq!(index.position.resume(7, 0), Resume::Continue);
        let publish = publish(shared, index, Some(100));
        reached.send(()).unwrap();
        continued.recv_timeout(WAIT).unwrap();
        let mut state = shared.write().unwrap();
        let index = Arc::make_mut(state.index.as_mut().unwrap());
        assert_eq!(index.position.next_usn, 100);
        if matches!(self.case, Start::Stale) {
            assert!(index.apply_batch(self.catchup).unwrap().is_empty());
            assert_eq!(index.position.next_usn, 612);
            assert_eq!(index.table.size(index.ids[&101]), Some(1));
        }
        state.status.progress = None;
        (restore, publish)
    }
}

fn startup(case: Start) {
    let (_directory, path) = snapshot();
    let input = StartupFixture {
        case,
        path,
        mft: matches!(case, Start::JournalReset).then(|| MftFixture::new(ENTRIES, 8)),
        catchup: batch(512),
    };
    let (reached, phase) = mpsc::sync_channel(1);
    let (proceed, continued) = mpsc::sync_channel(1);
    let started = Instant::now();
    let volume = volume(None);
    std::thread::scope(|scope| {
        let shared = &volume.shared;
        let worker = scope.spawn(move || input.restore(shared, (reached, continued)));
        let offset = started.elapsed();
        let (first, first_query) = timed_answer(&volume, &query());
        let first_elapsed = offset + first_query;
        phase.recv_timeout(WAIT).unwrap();
        let offset = started.elapsed();
        let (during, during_query) = timed_answer(&volume, &query());
        let during_elapsed = offset + during_query;
        let (status, during_status) = timed_answer(&volume, &Request::default());
        proceed.send(()).unwrap();
        let (restore, publish) = worker.join().unwrap();
        let offset = started.elapsed();
        let (populated, populated_query) = timed_answer(&volume, &query());
        let populated_elapsed = if matches!(case, Start::JournalReset) {
            offset + populated_query
        } else {
            during_elapsed
        };
        verify_startup(
            case,
            &[first, during, status, populated],
            [
                first_elapsed,
                during_query,
                during_status,
                restore,
                publish,
                populated_elapsed,
                populated_query,
            ],
        );
    });
}

fn verify_startup(case: Start, replies: &[Reply; 4], elapsed: [Duration; 7]) {
    assert_eq!(replies[0].status.volumes[0].progress, Some(0));
    assert!(replies[0].hits.is_empty());
    for reply in &replies[1..3] {
        if matches!(case, Start::JournalReset) {
            assert_eq!(reply.status.volumes[0].progress, Some(0));
            assert!(reply.hits.is_empty());
        } else {
            assert_eq!(reply.status.volumes[0].progress, Some(100));
            assert_eq!(reply.status.volumes[0].entries, u64::from(ENTRIES));
        }
    }
    if !matches!(case, Start::JournalReset) {
        assert_eq!(replies[1].hits[0].path, "C:\\needle.txt");
    }
    assert_eq!(replies[3].hits[0].path, "C:\\needle.txt");
    assert_eq!(replies[3].status.volumes[0].entries, u64::from(ENTRIES));
    assert_eq!(replies[3].status.volumes[0].progress, None);
    let metrics: Vec<_> = [
        "first building answer",
        "resume/enumeration query",
        "resume/enumeration status",
        "restore before publish",
        "publish write lock held",
        "first populated answer",
        "populated query",
    ]
    .into_iter()
    .zip(elapsed)
    .collect();
    for &(metric, elapsed) in &metrics {
        report(&format!("start {case:?}"), metric, elapsed);
    }
    within("first building answer", elapsed[0], ANSWER_BUDGET);
    within(
        "resume/enumeration status",
        elapsed[2],
        Duration::from_secs(1),
    );
    within("first populated answer", elapsed[5], Duration::from_secs(1));
}

fn publish(shared: &super::answer::Shared, index: NtfsIndex, progress: Option<u8>) -> Duration {
    let mut state = shared.write().unwrap();
    let started = Instant::now();
    state.index = Some(Arc::new(index));
    state.status.progress = progress;
    drop(state);
    started.elapsed()
}

#[test]
#[ignore = "timing: needs a quiet machine; run with --run-ignored all -j 1 timing_tests"]
fn snapshot_present_first_populated_answer_within_one_second() {
    startup(Start::Present);
}

#[test]
#[ignore = "timing: needs a quiet machine; run with --run-ignored all -j 1 timing_tests"]
fn stale_snapshot_first_populated_answer_within_one_second() {
    startup(Start::Stale);
}

#[test]
#[ignore = "timing, fails: a journal reset needs a full MFT read (about 4.5 s at 1 M entries) before the first populated answer"]
fn reset_journal_first_populated_answer_within_one_second() {
    startup(Start::JournalReset);
}

struct During {
    elapsed: Duration,
    query: Reply,
    answers: [Duration; 2],
    longest_wait: Duration,
}

struct SetOnDrop<'a>(&'a AtomicBool);

impl Drop for SetOnDrop<'_> {
    fn drop(&mut self) {
        self.0.store(true, Ordering::Release);
    }
}

fn longest_reader_wait(shared: &Shared, done: &AtomicBool) -> Duration {
    let mut longest = Duration::ZERO;
    while !done.load(Ordering::Acquire) {
        let started = Instant::now();
        drop(shared.read().unwrap());
        longest = longest.max(started.elapsed());
        std::thread::sleep(Duration::from_micros(200));
    }
    longest
}

fn during_operation(
    volume: &SharedVolume,
    label: &str,
    operation: impl FnOnce(&Barrier) -> Duration + Send,
) -> During {
    let entries = volume.shared.read().unwrap().status.entries;
    let begin = Barrier::new(4);
    let done = AtomicBool::new(false);
    let (elapsed, query, status, longest_wait) = std::thread::scope(|scope| {
        let (begin, done) = (&begin, &done);
        let worker = scope.spawn(move || {
            let _done = SetOnDrop(done);
            operation(begin)
        });
        let status = scope.spawn(move || {
            begin.wait();
            timed_answer(volume, &Request::default())
        });
        let probe = scope.spawn(move || {
            begin.wait();
            longest_reader_wait(&volume.shared, done)
        });
        begin.wait();
        let query = timed_answer(volume, &query());
        (
            worker.join().unwrap(),
            query,
            status.join().unwrap(),
            probe.join().unwrap(),
        )
    });
    let (query, query_elapsed) = query;
    let (status, status_elapsed) = status;
    report(label, "query answer", query_elapsed);
    report(label, "status answer", status_elapsed);
    report(label, "longest reader wait", longest_wait);
    assert_eq!(query.status.volumes[0].root, "C:");
    assert_eq!(status.status.volumes[0].root, "C:");
    assert_eq!(query.status.volumes[0].entries, entries);
    assert_eq!(status.status.volumes[0].entries, entries);
    During {
        elapsed,
        query,
        answers: [query_elapsed, status_elapsed],
        longest_wait,
    }
}

fn within_answer_budget(label: &str, during: &During) {
    assert_eq!(during.query.hits[0].path, "C:\\needle.txt", "{label}");
    for elapsed in during.answers {
        within(label, elapsed, ANSWER_BUDGET);
    }
    within(label, during.longest_wait, ANSWER_BUDGET);
}

#[test]
#[ignore = "timing: needs a quiet machine; run with --run-ignored all -j 1 timing_tests"]
fn busy_pipe_answers_use_the_20ms_lock_budget() {
    let volume = volume(Some(synthetic_index()));
    let writing = volume.shared.write().unwrap();
    for (label, request) in [("query", query()), ("status", Request::default())] {
        let (reply, elapsed) = timed_answer(&volume, &request);
        report("held writer", label, elapsed);
        assert!(reply.hits.is_empty());
        assert_eq!(reply.status.volumes[0].progress, Some(100));
        assert_eq!(reply.status.volumes[0].entries, u64::from(ENTRIES));
        within("held writer pipe answer", elapsed, ANSWER_BUDGET);
    }
    drop(writing);
}

fn usn_batch(records: u32) {
    let volume = volume(Some(synthetic_index()));
    let batch = batch(records);
    let label = format!("USN {records} records");
    let during = during_operation(&volume, &label, |begin| {
        begin.wait();
        let started = Instant::now();
        let unresolved = apply_batch(&volume.shared, batch).unwrap();
        let elapsed = started.elapsed();
        assert!(unresolved.is_empty());
        elapsed
    });
    report(&label, "batch applied in", during.elapsed);
    let state = volume.shared.read().unwrap();
    let index = state.index.as_ref().unwrap();
    assert_eq!(index.position.next_usn, 100 + i64::from(records));
    let last = index.ids[&(u128::from(records) + 100)];
    assert_eq!(index.table.size(last), Some(u64::from(records)));
    assert!(index.pending_renames().is_empty());
    within_answer_budget(&label, &during);
}

#[test]
#[ignore = "timing: needs a quiet machine; run with --run-ignored all -j 1 timing_tests"]
fn usn_512_records_lock_and_pipe_answers_within_50ms() {
    usn_batch(512);
}

#[test]
#[ignore = "timing: needs a quiet machine; run with --run-ignored all -j 1 timing_tests"]
fn stress_usn_batch_lock_and_pipe_answers_within_50ms() {
    usn_batch(RECORDS);
}

#[test]
#[ignore = "timing: needs a quiet machine; run with --run-ignored all -j 1 timing_tests"]
fn snapshot_save_lock_and_pipe_answers_within_50ms() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("c.index");
    let volume = volume(Some(synthetic_index()));
    let during = during_operation(&volume, "snapshot save", |begin| {
        begin.wait();
        let started = Instant::now();
        let index = published(&volume.shared).unwrap();
        let held = started.elapsed();
        report("snapshot save", "read lock held", held);
        within("snapshot save read lock", held, ANSWER_BUDGET);
        save(&path, &index);
        started.elapsed()
    });
    report("snapshot save", "save finished in", during.elapsed);
    assert_eq!(
        file::load_with_ids(&path).unwrap().1.len(),
        ENTRIES as usize
    );
    within_answer_budget("snapshot save", &during);
}

#[test]
#[ignore = "timing: needs a quiet machine; run with --run-ignored all -j 1 timing_tests"]
fn first_mft_read_releases_table_lock_and_pipe_answers_within_50ms() {
    let mft = MftFixture::new(ENTRIES, 7);
    let volume = volume(None);
    let during = during_operation(&volume, "first MFT read", |begin| {
        begin.wait();
        let started = Instant::now();
        let enumerated = mft.read();
        let elapsed = started.elapsed();
        assert!(enumerated.unresolved.is_empty());
        assert_eq!(enumerated.index.table.len(), ENTRIES as usize);
        let held = publish(&volume.shared, enumerated.index, None);
        report("first MFT read", "publish write lock held", held);
        within("MFT publish", held, ANSWER_BUDGET);
        elapsed
    });
    report("first MFT read", "enumeration", during.elapsed);
    for elapsed in during.answers {
        within("first MFT read answer", elapsed, ANSWER_BUDGET);
    }
    within(
        "first MFT read reader wait",
        during.longest_wait,
        ANSWER_BUDGET,
    );
}

#[test]
#[ignore = "timing: needs a quiet machine; run with --run-ignored all -j 1 timing_tests"]
fn ready_status_pipe_answers_within_50ms() {
    let volume = volume(Some(synthetic_index()));
    let (reply, elapsed) = timed_answer(&volume, &Request::default());
    report("ready", "status answer", elapsed);
    assert_eq!(reply.status.volumes[0].progress, None);
    assert_eq!(reply.status.volumes[0].entries, u64::from(ENTRIES));
    within("ready status", elapsed, ANSWER_BUDGET);
}
