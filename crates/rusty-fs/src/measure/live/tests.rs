mod held;

use std::sync::Arc;
use std::sync::mpsc::{self, Receiver, Sender};
use std::time::{Duration, Instant};

use crate::{
    CancelToken, FakeFs, FakeOp, FolderProfile, IoQueue, LiveHandle, LiveRequest, LiveSink,
    LiveSnapshot, ProfileState, Provider, Tally, folder_profile, spawn_live,
};
use held::Held;

const PATIENCE: Duration = Duration::from_secs(10);
const ABOUT_A_SECOND: Duration = Duration::from_secs(2);

struct Probe(Sender<LiveSnapshot>);

impl LiveSink for Probe {
    fn snapshot(&mut self, live: &LiveSnapshot) {
        let _ = self.0.send(live.clone());
    }

    fn progress_every(&self) -> Duration {
        Duration::ZERO
    }
}

struct Run {
    handle: LiveHandle,
    snapshots: Receiver<LiveSnapshot>,
    _queue: Arc<IoQueue>,
}

impl Run {
    fn wait_for(&self, done: impl Fn(&LiveSnapshot) -> bool) -> LiveSnapshot {
        let deadline = Instant::now() + PATIENCE;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            match self.snapshots.recv_timeout(left) {
                Ok(snapshot) if done(&snapshot) => return snapshot,
                Ok(_) => {}
                Err(error) => panic!("no matching snapshot: {error}"),
            }
        }
    }

    fn ready(&self) -> LiveSnapshot {
        self.wait_for(|s| s.profile.state == ProfileState::Done && s.is_live && !s.is_recounting)
    }
}

fn start(provider: Arc<dyn Provider>, folder: &FakeFs, base: Option<FolderProfile>) -> Run {
    let queue = Arc::new(IoQueue::new());
    let (send, snapshots) = mpsc::channel();
    let request = LiveRequest {
        provider,
        folder: folder.location("d"),
        base,
    };
    let handle = spawn_live(&queue, request, Probe(send));
    Run {
        handle,
        snapshots,
        _queue: queue,
    }
}

fn live(fs: &Arc<FakeFs>) -> Run {
    start(fs.clone(), fs, None)
}

fn counted(fs: &FakeFs) -> FolderProfile {
    folder_profile(
        fs,
        &fs.location("d"),
        &CancelToken::new(),
        &mut |_: &FolderProfile| {},
    )
}

fn kind(profile: &FolderProfile, extension: &str) -> Option<Tally> {
    profile
        .kinds
        .iter()
        .find(|(name, _)| name == extension)
        .map(|(_, tally)| *tally)
}

fn deep_tree() -> Arc<FakeFs> {
    let fs = FakeFs::new().with_watch();
    fs.add_file("d/a.bin", 100);
    fs.add_file("d/one/b.txt", 20);
    fs.add_file("d/one/two/three/c.txt", 3);
    Arc::new(fs)
}

#[test]
fn a_file_written_three_levels_deep_updates_totals_and_its_kind_within_one_refresh() {
    let fs = deep_tree();
    let run = live(&fs);
    let before = run.ready().profile.totals;
    let written = Instant::now();
    fs.add_file("d/one/two/three/new.log", 4_096);
    let next = run.wait_for(|s| s.profile.totals != before);
    assert!(
        written.elapsed() < ABOUT_A_SECOND,
        "{:?}",
        written.elapsed()
    );
    assert_eq!(next.profile.totals.bytes, before.bytes + 4_096);
    assert_eq!(next.profile.totals.files, before.files + 1);
    assert_eq!(
        kind(&next.profile, "log"),
        Some(Tally {
            bytes: 4_096,
            files: 1
        })
    );
    assert_eq!(next.counts, 1);
}

#[test]
fn a_file_that_keeps_growing_is_counted_once_at_its_latest_size() {
    let fs = deep_tree();
    let run = live(&fs);
    let before = run.ready().profile.totals;
    fs.add_file("d/one/copy.iso", 1_000);
    run.wait_for(|s| s.profile.totals.files == before.files + 1);
    fs.add_file("d/one/copy.iso", 5_000);
    let next = run.wait_for(|s| s.profile.totals.bytes == before.bytes + 5_000);
    assert_eq!(next.profile.totals.files, before.files + 1);
    assert_eq!(next.profile.largest[0].path, "one/copy.iso");
    assert_eq!(next.counts, 1);
}

fn fifteen_sizes() -> Arc<FakeFs> {
    let fs = FakeFs::new().with_watch();
    for index in 1..=15_u64 {
        fs.add_file(&format!("d/f{}/n{index}.bin", index % 3), index * 100);
    }
    Arc::new(fs)
}

#[test]
fn a_deleted_large_file_leaves_the_largest_ten_correct() {
    let fs = fifteen_sizes();
    let run = live(&fs);
    let before = run.ready().profile.totals;
    fs.remove("d/f0/n15.bin");
    let next = run.wait_for(|s| s.profile.totals.files == before.files - 1);
    assert_eq!(next.profile.totals.bytes, before.bytes - 1_500);
    assert_eq!(next.profile.largest, counted(&fs).largest);
    assert_eq!(next.profile.largest[0].path, "f2/n14.bin");
    assert_eq!(next.counts, 1);
}

#[test]
fn a_renamed_folder_keeps_the_counts_and_moves_its_paths() {
    let fs = Arc::new(FakeFs::new().with_watch());
    fs.add_file("d/a/big.bin", 900);
    fs.add_file("d/a/b/c.txt", 5);
    fs.add_file("d/x.bin", 1);
    let run = live(&fs);
    let ready = run.ready().profile;
    fs.rename_entry("d/a", "d/z");
    let next = run.wait_for(|s| s.profile.largest[0].path == "z/big.bin");
    assert_eq!(next.profile.totals, ready.totals);
    assert_eq!(next.profile.kinds, ready.kinds);
    assert_eq!(next.profile.largest, counted(&fs).largest);
    assert_eq!(next.counts, 1);
    assert_eq!(fs.calls("d", FakeOp::List), 1);
}

#[test]
fn a_new_folder_with_files_adds_its_whole_subtree_once() {
    let fs = deep_tree();
    let run = live(&fs);
    let before = run.ready().profile.totals;
    fs.add_file("d/one/fresh/inner/x.bin", 10);
    fs.add_file("d/one/fresh/y.bin", 20);
    let next = run.wait_for(|s| s.profile.totals.files == before.files + 2);
    let expected = counted(&fs);
    assert_eq!(next.profile.totals, expected.totals);
    assert_eq!(next.profile.kinds, expected.kinds);
    assert_eq!(next.counts, 1);
}

#[test]
fn a_busy_folder_is_patched_without_recounting() {
    let fs = deep_tree();
    let run = live(&fs);
    run.ready();
    for index in 0..40 {
        fs.add_file(&format!("d/one/two/busy{index}.tmp"), 10 + index);
    }
    for index in 0..20 {
        fs.remove(&format!("d/one/two/busy{index}.tmp"));
    }
    fs.add_file("d/one/two/busy30.tmp", 999);
    let expected = counted(&fs);
    let next = run.wait_for(|s| s.profile.totals == expected.totals);
    assert_eq!(next.profile.kinds, expected.kinds);
    assert_eq!(next.profile.largest, expected.largest);
    assert_eq!(next.counts, 1);
    assert_eq!(fs.calls("d", FakeOp::List), 1);
}

#[test]
fn an_overflow_triggers_exactly_one_recount() {
    let fs = deep_tree();
    let run = live(&fs);
    run.ready();
    let listed = fs.calls("d", FakeOp::List);
    fs.overflow_watchers("d");
    let after = run.wait_for(|s| s.counts == 2 && !s.is_recounting);
    assert_eq!(after.profile.state, ProfileState::Done);
    assert_eq!(fs.calls("d", FakeOp::List), listed + 1);
}

#[test]
fn a_recount_is_asked_for_by_the_handle() {
    let fs = deep_tree();
    let run = live(&fs);
    run.ready();
    run.handle.recount();
    run.wait_for(|s| s.counts == 2 && !s.is_recounting);
    assert_eq!(fs.calls("d", FakeOp::List), 2);
}

#[test]
fn changes_during_a_recount_cause_exactly_one_follow_up() {
    let fs = deep_tree();
    let held = Held::new(&fs, "d", 2);
    let run = start(held.clone(), &fs, None);
    run.ready();
    run.handle.recount();
    held.wait_until_held();
    fs.add_file("d/one/during.bin", 7);
    fs.add_file("d/one/two/during.txt", 8);
    run.wait_for(|s| s.changes >= 2 && s.is_recounting);
    held.release();
    let last = run.wait_for(|s| s.counts == 3 && !s.is_recounting);
    assert_eq!(last.profile.totals, counted(&fs).totals);
    assert_eq!(fs.calls("d", FakeOp::List), 3);
}

#[test]
fn a_cached_profile_is_watched_without_a_first_count() {
    let fs = deep_tree();
    let run = start(fs.clone(), &fs, Some(counted(&fs)));
    let ready = run.ready();
    assert_eq!(ready.counts, 0);
    fs.add_file("d/one/two/late.txt", 5);
    let next = run.wait_for(|s| s.profile.totals.files == ready.profile.totals.files + 1);
    assert_eq!(next.profile.totals, counted(&fs).totals);
    assert_eq!(fs.calls("d", FakeOp::List), 1);
}

#[test]
fn a_count_records_when_it_finished() {
    let fs = deep_tree();
    let run = live(&fs);
    let ready = run.ready();
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or_default();
    let counted_ms = ready.profile.counted_ms.expect("count time");
    assert!((now - counted_ms).abs() < 60_000, "{counted_ms} vs {now}");
}

#[test]
fn dropping_the_handle_stops_the_watch() {
    let fs = deep_tree();
    let run = live(&fs);
    run.ready();
    assert_eq!(fs.watchers("d"), 1);
    let Run { handle, .. } = run;
    drop(handle);
    let deadline = Instant::now() + PATIENCE;
    while fs.watchers("d") > 0 && Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert_eq!(fs.watchers("d"), 0);
}
