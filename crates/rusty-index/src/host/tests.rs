use std::collections::HashSet;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use rusty_core::{FsError, FsErrorKind, ProviderId};
use rusty_fs::{
    CancelToken, Capabilities, FakeFs, FakeOp, Fault, FsEntry, ListSink, Watch, WatchChange,
    WatchSink,
};

use super::*;
use crate::file::{self, Header};
use crate::table::{Flags, NO_PARENT};

const WAIT: Duration = Duration::from_secs(5);

struct Rig {
    fs: Arc<FakeFs>,
    queue: Arc<IoQueue>,
    folder: tempfile::TempDir,
    now: Arc<AtomicU64>,
    base: Instant,
}

impl Rig {
    fn new() -> Self {
        let fs = Arc::new(FakeFs::new().with_watch());
        fs.add_folder("tree");
        Self {
            fs,
            queue: Arc::new(IoQueue::new()),
            folder: tempfile::tempdir().unwrap(),
            now: Arc::new(AtomicU64::new(0)),
            base: Instant::now(),
        }
    }

    fn path(&self) -> PathBuf {
        self.folder.path().join("tree.index")
    }

    fn options(&self, provider: Arc<dyn Provider>) -> HostOptions {
        HostOptions {
            provider,
            queue: Arc::clone(&self.queue),
            root: self.fs.location("tree"),
            snapshot: self.path(),
            limits: Limits {
                max_entries: 100_000,
                exclude: vec!["skip".to_owned()],
            },
        }
    }

    fn start(&self) -> Host {
        self.start_provider(self.fs.clone())
    }

    fn start_provider(&self, provider: Arc<dyn Provider>) -> Host {
        let (now, base) = (Arc::clone(&self.now), self.base);
        Host::start_with_clock(
            self.options(provider),
            Arc::new(move || base + Duration::from_millis(now.load(Ordering::SeqCst))),
        )
        .unwrap()
    }

    fn tick(&self, host: &Host, milliseconds: u64) {
        self.now.store(milliseconds, Ordering::SeqCst);
        let (sender, receiver) = mpsc::channel();
        host.sender.send(Message::Barrier(sender)).unwrap();
        receiver.recv_timeout(WAIT).unwrap();
    }

    fn gate(&self, after_list: bool) -> (Arc<Gate>, Receiver<()>, Sender<()>) {
        let (entered, waiting) = mpsc::channel();
        let (release, released) = mpsc::channel();
        (
            Arc::new(Gate {
                fs: Arc::clone(&self.fs),
                entered,
                release: Mutex::new(released),
                first: AtomicBool::new(true),
                after_list,
            }),
            waiting,
            release,
        )
    }

    fn saved(&self, names: &[&str]) {
        let mut table = NameTable::new();
        table.push(NO_PARENT, "tree", Flags::FOLDER).unwrap();
        for name in names {
            table.push(0, name, Flags::NONE).unwrap();
        }
        file::save(
            &self.path(),
            &Header {
                volume_serial: 0,
                journal_id: 0,
                next_usn: 0,
                root: self.fs.location("tree").as_str().to_owned(),
                built_at: 0,
            },
            &table,
        )
        .unwrap();
    }
}

fn names(table: &NameTable) -> HashSet<String> {
    table.iter().map(|(_, name)| name.to_owned()).collect()
}

fn settled(host: &Host) -> HostStatus {
    loop {
        let status = host.statuses.recv_timeout(WAIT).unwrap();
        if status.progress.is_none() && status.entries > 0 {
            return status;
        }
    }
}

fn built(host: &Host) -> HostStatus {
    while host.statuses.recv_timeout(WAIT).unwrap().progress.is_none() {}
    settled(host)
}

struct Gate {
    fs: Arc<FakeFs>,
    entered: Sender<()>,
    release: Mutex<Receiver<()>>,
    first: AtomicBool,
    after_list: bool,
}

impl Gate {
    fn wait(&self) {
        self.entered.send(()).unwrap();
        self.release.lock().unwrap().recv_timeout(WAIT).unwrap();
    }
}

impl Provider for Gate {
    fn id(&self) -> ProviderId {
        self.fs.id()
    }

    fn capabilities(&self) -> Capabilities {
        self.fs.capabilities()
    }

    fn metadata(&self, location: &Location) -> Result<FsEntry, FsError> {
        self.fs.metadata(location)
    }

    fn list(
        &self,
        folder: &Location,
        sink: &mut dyn ListSink,
        cancel: &CancelToken,
    ) -> Result<(), FsError> {
        let gated = self.first.swap(false, Ordering::SeqCst);
        if gated && !self.after_list {
            self.wait();
        }
        let result = self.fs.list(folder, sink, cancel);
        if gated && self.after_list {
            self.wait();
        }
        result
    }

    fn watch_tree(&self, folder: &Location, sink: WatchSink) -> Result<Watch, FsError> {
        self.fs.watch_tree(folder, sink)
    }
}

#[test]
fn loads_the_snapshot_before_rescanning_changes_made_while_closed() {
    let rig = Rig::new();
    rig.saved(&["old.txt", "skip"]);
    rig.fs.add_file("tree/new.txt", 1);
    let (gate, waiting, release) = rig.gate(false);
    let host = rig.start_provider(gate);
    waiting.recv_timeout(WAIT).unwrap();
    assert_eq!(settled(&host).entries, 2);
    assert!(names(&host.table().read().unwrap()).contains("old.txt"));
    release.send(()).unwrap();
    assert_eq!(built(&host).entries, 2);
    let found = names(&host.table().read().unwrap());
    assert!(found.contains("new.txt"));
    assert!(!found.contains("old.txt"));
}

#[test]
fn applies_created_and_deleted_files_in_one_watcher_batch() {
    let rig = Rig::new();
    rig.fs.add_file("tree/old.txt", 1);
    let host = rig.start();
    built(&host);
    rig.fs.add_file("tree/new.txt", 1);
    rig.fs.remove("tree/old.txt");
    let status = settled(&host);
    assert_eq!(status.entries, 2);
    assert!(status.errors.is_empty());
    assert_eq!(
        names(&host.table().read().unwrap()),
        HashSet::from(["tree".into(), "new.txt".into()])
    );
    assert_eq!(rig.fs.calls("tree", FakeOp::List), 1);
}

#[test]
fn overflow_rescans_the_root_and_recovers_the_missing_changes() {
    let rig = Rig::new();
    rig.fs.add_file("tree/old.txt", 1);
    let host = rig.start();
    built(&host);
    rig.fs.remove("tree/old.txt");
    rig.fs.add_file("tree/deep/new.txt", 1);
    rig.fs.overflow_watchers("tree");
    assert_eq!(built(&host).entries, 3);
    assert_eq!(rig.fs.calls("tree", FakeOp::List), 2);
    assert_eq!(
        names(&host.table().read().unwrap()),
        HashSet::from(["tree".into(), "deep".into(), "new.txt".into()])
    );
}

#[test]
fn saves_only_after_thirty_seconds_without_changes() {
    let rig = Rig::new();
    let host = rig.start();
    built(&host);
    rig.tick(&host, 29_999);
    assert!(!rig.path().exists());
    rig.fs.add_file("tree/later.txt", 1);
    settled(&host);
    rig.tick(&host, 30_000);
    assert!(!rig.path().exists());
    rig.tick(&host, 59_999);
    let (_, table) = file::load(&rig.path()).unwrap();
    assert!(names(&table).contains("later.txt"));
    rig.tick(&host, 90_000);
    assert!(!rusty_store::backup_path(&rig.path()).exists());
}

#[test]
fn drop_stops_the_watcher_and_saves_exactly_once() {
    let rig = Rig::new();
    rig.fs.add_file("tree/kept.txt", 1);
    let host = rig.start();
    built(&host);
    assert_eq!(rig.fs.watchers("tree"), 1);
    drop(host);
    assert_eq!(rig.fs.watchers("tree"), 0);
    assert_eq!(file::load(&rig.path()).unwrap().1.len(), 2);
    assert!(!rusty_store::backup_path(&rig.path()).exists());
    assert!(Arc::strong_count(&rig.fs) == 1);
}

#[test]
fn excluded_names_never_enter_loaded_walked_or_watched_tables() {
    let rig = Rig::new();
    rig.saved(&["skip", "loaded.txt"]);
    rig.fs.add_file("tree/skip/hidden.txt", 1);
    rig.fs.add_file("tree/kept.txt", 1);
    let host = rig.start();
    built(&host);
    assert!(!names(&host.table().read().unwrap()).contains("skip"));
    assert_eq!(rig.fs.calls("tree/skip", FakeOp::List), 0);
    rig.fs.add_file("tree/skip/new.txt", 1);
    settled(&host);
    assert!(!names(&host.table().read().unwrap()).contains("new.txt"));
    assert_eq!(rig.fs.calls("tree/skip/new.txt", FakeOp::Metadata), 0);
}

fn progress_rescan(rig: &Rig) -> (Host, Sender<()>) {
    rig.saved(&["snapshot.txt"]);
    for index in 0..10_000 {
        rig.fs.add_file(&format!("tree/file-{index}.txt"), 1);
    }
    let (gate, waiting, release) = rig.gate(true);
    let host = rig.start_provider(gate);
    waiting.recv_timeout(WAIT).unwrap();
    loop {
        let status = host.statuses.recv_timeout(WAIT).unwrap();
        if status
            .progress
            .is_some_and(|progress| progress.entries >= 10_000)
        {
            assert_eq!(status.entries, 10_000);
            return (host, release);
        }
    }
}

#[test]
fn rescan_progress_keeps_the_loaded_snapshot_searchable() {
    let rig = Rig::new();
    let (host, release) = progress_rescan(&rig);
    let found = names(&host.table().read().unwrap());
    release.send(()).unwrap();
    assert!(found == HashSet::from(["tree".into(), "snapshot.txt".into()]));
    assert_eq!(settled(&host).entries, 10_001);
}

#[test]
fn drop_during_rescan_preserves_snapshot_bytes() {
    let rig = Rig::new();
    let (host, release) = progress_rescan(&rig);
    let before = std::fs::read(rig.path()).unwrap();
    let cancel = host.cancel.clone();
    cancel.pause();
    let closing = thread::spawn(move || drop(host));
    cancel.wait_while_paused();
    release.send(()).unwrap();
    closing.join().unwrap();
    assert!(std::fs::read(rig.path()).unwrap() == before);
    assert!(!rusty_store::backup_path(&rig.path()).exists());
}

#[test]
fn failed_rescan_preserves_the_live_table_and_snapshot() {
    let rig = Rig::new();
    rig.saved(&["snapshot.txt"]);
    let before = std::fs::read(rig.path()).unwrap();
    rig.fs.fault(
        "tree",
        FakeOp::List,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let host = rig.start();
    assert!(!built(&host).errors.is_empty());
    let found = names(&host.table().read().unwrap());
    rig.tick(&host, 30_000);
    drop(host);
    assert!(found == HashSet::from(["tree".into(), "snapshot.txt".into()]));
    assert!(std::fs::read(rig.path()).unwrap() == before);
}

#[test]
fn close_after_saving_completed_work_does_not_save_again() {
    let rig = Rig::new();
    let host = rig.start();
    built(&host);
    rig.tick(&host, 30_000);
    let before = std::fs::read(rig.path()).unwrap();
    drop(host);
    assert!(std::fs::read(rig.path()).unwrap() == before);
    assert!(!rusty_store::backup_path(&rig.path()).exists());
}

#[test]
fn renaming_a_deep_file_walks_only_its_parent() {
    let rig = Rig::new();
    rig.fs.add_file("tree/left/deep/old.txt", 1);
    rig.fs.add_file("tree/unrelated/kept.txt", 1);
    let host = rig.start();
    built(&host);
    rig.fs
        .rename_entry("tree/left/deep/old.txt", "tree/left/deep/new.txt");
    assert!(built(&host).errors.is_empty());
    let found = names(&host.table().read().unwrap());
    assert!(found.contains("new.txt"));
    assert!(!found.contains("old.txt"));
    assert!(found.contains("kept.txt"));
    assert_eq!(rig.fs.calls("tree", FakeOp::List), 1);
    assert_eq!(rig.fs.calls("tree/left", FakeOp::List), 1);
    assert_eq!(rig.fs.calls("tree/left/deep", FakeOp::List), 2);
    assert_eq!(rig.fs.calls("tree/unrelated", FakeOp::List), 1);
}

#[test]
fn created_folder_with_files_walks_only_its_parent() {
    let rig = Rig::new();
    rig.fs.add_folder("tree/left/deep");
    rig.fs.add_file("tree/unrelated/kept.txt", 1);
    let host = rig.start();
    built(&host);
    rig.fs.add_file("tree/left/deep/created/child.txt", 1);
    assert!(built(&host).errors.is_empty());
    let shared = host.table();
    let table = shared.read().unwrap();
    let paths: HashSet<_> = table
        .iter()
        .filter_map(|(id, _)| table.path_with(id, '/'))
        .collect();
    assert!(paths.contains("tree/left/deep/created/child.txt"));
    assert!(paths.contains("tree/unrelated/kept.txt"));
    assert_eq!(rig.fs.calls("tree", FakeOp::List), 1);
    assert_eq!(rig.fs.calls("tree/left", FakeOp::List), 1);
    assert_eq!(rig.fs.calls("tree/left/deep", FakeOp::List), 2);
    assert_eq!(rig.fs.calls("tree/left/deep/created", FakeOp::List), 1);
    assert_eq!(rig.fs.calls("tree/unrelated", FakeOp::List), 1);
}

#[test]
fn watch_events_during_rescan_are_applied_after_the_swap() {
    let rig = Rig::new();
    let (gate, waiting, release) = rig.gate(true);
    let host = rig.start_provider(gate);
    waiting.recv_timeout(WAIT).unwrap();
    rig.fs.add_file("tree/queued.txt", 1);
    host.sender
        .send(Message::Watch(WatchEvent::Changed(vec![
            WatchChange::Created(ProviderPath::from_text("queued.txt")),
        ])))
        .unwrap();
    release.send(()).unwrap();
    built(&host);
    rig.tick(&host, 0);
    assert!(names(&host.table().read().unwrap()).contains("queued.txt"));
}
