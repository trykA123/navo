use std::sync::{Barrier, mpsc};
use std::time::{Duration, Instant};

use rusty_index::file;
use rusty_index::ntfs::{NtfsError, NtfsIndex, test_support::MftFixture};
use rusty_index::pipe::{Reply, Request};
use rusty_index::usn::Resume;

use super::answer::SharedVolume;
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
    fn restore(self, phase: (mpsc::SyncSender<()>, mpsc::Receiver<()>)) -> (NtfsIndex, Duration) {
        let (reached, continued) = phase;
        let drive = Drive {
            letter: 'C',
            serial: 42,
            fixed: true,
            ntfs: true,
        };
        let started = Instant::now();
        let index = restore_or_enumerate(
            file::load_with_ids(&self.path),
            drive,
            |index| {
                if matches!(self.case, Start::JournalReset) {
                    assert_eq!(index.position.resume(8, 0), Resume::Reenumerate);
                    return Err(NtfsError::Reenumerate);
                }
                reached.send(()).unwrap();
                continued.recv_timeout(WAIT).unwrap();
                assert_eq!(index.position.resume(7, 0), Resume::Continue);
                if matches!(self.case, Start::Stale) {
                    assert!(index.apply_batch(self.catchup).unwrap().is_empty());
                }
                Ok(())
            },
            || {
                reached.send(()).unwrap();
                continued.recv_timeout(WAIT).unwrap();
                let enumerated = self.mft.unwrap().read();
                assert!(enumerated.unresolved.is_empty());
                assert_eq!(enumerated.sizes_unknown, 0);
                Ok(enumerated.index)
            },
        )
        .unwrap();
        (index, started.elapsed())
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
        let worker = scope.spawn(move || {
            let (index, restore) = input.restore((reached, continued));
            (restore, publish(shared, index))
        });
        let offset = started.elapsed();
        let (first, first_query) = timed_answer(&volume, &query());
        let first_elapsed = offset + first_query;
        phase.recv_timeout(WAIT).unwrap();
        let (during, during_query) = timed_answer(&volume, &query());
        let (status, during_status) = timed_answer(&volume, &Request::default());
        proceed.send(()).unwrap();
        let (restore, publish) = worker.join().unwrap();
        let offset = started.elapsed();
        let (populated, populated_query) = timed_answer(&volume, &query());
        let populated_elapsed = offset + populated_query;
        verify_startup(
            case,
            [first, during, status, populated],
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

fn verify_startup(case: Start, replies: [Reply; 4], elapsed: [Duration; 7]) {
    for reply in &replies[..3] {
        assert_eq!(reply.status.volumes[0].progress, Some(0));
        assert!(reply.hits.is_empty());
    }
    assert_eq!(replies[3].hits[0].path, "C:\\needle.txt");
    assert_eq!(replies[3].status.volumes[0].entries, u64::from(ENTRIES));
    let metrics: Vec<_> = [
        "first building answer",
        "resume/enumeration query",
        "resume/enumeration status",
        "restore before publish (includes phase handshake)",
        "publish write lock held",
        "first populated answer",
        "populated query",
    ]
    .into_iter()
    .zip(elapsed)
    .collect();
    measure_within(&format!("start {case:?}"), &metrics);
}

fn publish(shared: &super::answer::Shared, index: NtfsIndex) -> Duration {
    let mut state = shared.write().unwrap();
    let started = Instant::now();
    state.index = Some(index);
    state.status.progress = None;
    drop(state);
    started.elapsed()
}

#[test]
#[ignore = "IX1: snapshot load/validation and populated search exceed 50 ms at 1M entries"]
fn snapshot_present_first_populated_answer_within_50ms() {
    startup(Start::Present);
}

#[test]
#[ignore = "IX1: stale snapshot resume and populated search exceed 50 ms at 1M entries"]
fn stale_snapshot_first_populated_answer_within_50ms() {
    startup(Start::Stale);
}

#[test]
#[ignore = "IX1: journal reset reenumeration and populated search exceed 50 ms at 1M entries"]
fn reset_journal_first_populated_answer_within_50ms() {
    startup(Start::JournalReset);
}

fn during_operation(
    volume: &SharedVolume,
    label: &str,
    operation: impl FnOnce(&Barrier) -> Duration + Send,
) -> (Duration, [Duration; 2]) {
    let entries = volume.shared.read().unwrap().status.entries;
    let begin = Barrier::new(3);
    let (hold, query, status) = std::thread::scope(|scope| {
        let begin = &begin;
        let worker = scope.spawn(move || operation(begin));
        let status = scope.spawn(move || {
            begin.wait();
            timed_answer(volume, &Request::default())
        });
        begin.wait();
        let query = timed_answer(volume, &query());
        (worker.join().unwrap(), query, status.join().unwrap())
    });
    let (query, query_elapsed) = query;
    let (status, status_elapsed) = status;
    report(label, "query answer", query_elapsed);
    report(label, "status answer", status_elapsed);
    assert_eq!(query.status.volumes[0].root, "C:");
    assert_eq!(status.status.volumes[0].root, "C:");
    assert_eq!(query.status.volumes[0].entries, entries);
    assert_eq!(status.status.volumes[0].entries, entries);
    (hold, [query_elapsed, status_elapsed])
}

#[test]
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
    let (hold, answers) = during_operation(&volume, &label, |begin| {
        let mut state = volume.shared.write().unwrap();
        begin.wait();
        let started = Instant::now();
        let unresolved = state.index.as_mut().unwrap().apply_batch(batch).unwrap();
        drop(state);
        let elapsed = started.elapsed();
        assert!(unresolved.is_empty());
        elapsed
    });
    report(&label, "write lock held (excluding rendezvous)", hold);
    let state = volume.shared.read().unwrap();
    let index = state.index.as_ref().unwrap();
    assert_eq!(index.position.next_usn, 100 + i64::from(records));
    for elapsed in answers {
        within(&label, elapsed, ANSWER_BUDGET);
    }
    within(&label, hold, ANSWER_BUDGET);
}

#[test]
#[ignore = "IX1: a 64KiB-sized USN batch releases the lock before the unbounded search finishes"]
fn usn_512_records_lock_and_pipe_answers_within_50ms() {
    usn_batch(512);
}

#[test]
#[ignore = "IX1: synthetic 200K-record stress batch holds the write lock over 50 ms"]
fn stress_usn_batch_lock_and_pipe_answers_within_50ms() {
    usn_batch(RECORDS);
}

#[test]
#[ignore = "IX1: snapshot save read lock and concurrent searches exceed 50 ms at 1M entries"]
fn snapshot_save_lock_and_pipe_answers_within_50ms() {
    let directory = tempfile::tempdir().unwrap();
    let path = directory.path().join("c.index");
    let volume = volume(Some(synthetic_index()));
    let (hold, answers) = during_operation(&volume, "snapshot save", |begin| {
        let state = volume.shared.read().unwrap();
        begin.wait();
        let started = Instant::now();
        save(&path, state.index.as_ref().unwrap());
        drop(state);
        started.elapsed()
    });
    report("snapshot save", "write lock held", Duration::ZERO);
    report(
        "snapshot save",
        "read lock held (excluding rendezvous)",
        hold,
    );
    assert_eq!(
        file::load_with_ids(&path).unwrap().1.len(),
        ENTRIES as usize
    );
    for elapsed in answers {
        within("snapshot save answer", elapsed, ANSWER_BUDGET);
    }
    within("snapshot save read lock", hold, ANSWER_BUDGET);
}

#[test]
fn first_mft_read_releases_table_lock_and_pipe_answers_within_50ms() {
    let mft = MftFixture::new(ENTRIES, 7);
    let volume = volume(None);
    let (elapsed, answers) = during_operation(&volume, "first MFT read", |begin| {
        begin.wait();
        let started = Instant::now();
        let enumerated = mft.read();
        let elapsed = started.elapsed();
        assert!(enumerated.unresolved.is_empty());
        assert_eq!(enumerated.index.table.len(), ENTRIES as usize);
        let held = publish(&volume.shared, enumerated.index);
        report("first MFT read", "publish write lock held", held);
        within("MFT publish", held, ANSWER_BUDGET);
        elapsed
    });
    report("first MFT read", "enumeration", elapsed);
    report(
        "first MFT read",
        "write lock held during I/O",
        Duration::ZERO,
    );
    for elapsed in answers {
        within("first MFT read answer", elapsed, ANSWER_BUDGET);
    }
}

#[test]
#[ignore = "IX1: ready status scans 1M entries; informational baseline sits near the 50 ms budget"]
fn ready_status_pipe_answers_within_50ms() {
    let volume = volume(Some(synthetic_index()));
    let (reply, elapsed) = timed_answer(&volume, &Request::default());
    report("ready", "status answer", elapsed);
    assert_eq!(reply.status.volumes[0].progress, None);
    assert_eq!(reply.status.volumes[0].entries, u64::from(ENTRIES));
    within("ready status", elapsed, ANSWER_BUDGET);
}
