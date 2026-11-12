use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, RwLock, mpsc};
use std::time::Instant;

use navo_fs::{CancelToken, FakeFs, FakeOp, IoQueue, JobSlot, Priority, volume_of};

use super::super::{HostOptions, HostStatus, Worker};
use super::{Flags, Location, NO_PARENT, NameTable};
use crate::walker::Limits;

/// A worker over `table` and the `tree/left/big` root it merges, without a host thread.
fn worker_over(fs: &Arc<FakeFs>, table: NameTable) -> (Worker, Location) {
    let queue = Arc::new(IoQueue::new());
    let owner = queue.owner();
    let (root, big) = (fs.location("tree"), fs.location("tree/left/big"));
    let (updates, _statuses) = mpsc::channel();
    let (_sender, events) = mpsc::channel();
    let worker = Worker {
        status: HostStatus::default(),
        slot: JobSlot {
            volume: volume_of(&root),
            priority: Priority::Idle,
            owner: owner.id(),
        },
        options: HostOptions {
            provider: fs.clone(),
            queue,
            root,
            snapshot: PathBuf::from("merge.index"),
            limits: Limits {
                max_entries: 100_000,
                exclude: Vec::new(),
            },
        },
        clock: Arc::new(Instant::now),
        events,
        updates,
        table: Arc::new(RwLock::new(table)),
        cancel: CancelToken::new(),
        dirty: None,
        rescan: Vec::new(),
        retrying: false,
    };
    (worker, big)
}

/// `tree/left/big` over `files` old files, a dead slot before `left`, and an unrelated branch.
fn merge_fixture(files: usize) -> NameTable {
    let mut table = NameTable::new();
    let tree = table.push(NO_PARENT, "tree", Flags::FOLDER).unwrap();
    let gone = table.push(tree, "gone.txt", Flags::NONE).unwrap();
    let left = table.push(tree, "left", Flags::FOLDER).unwrap();
    let big = table.push(left, "big", Flags::FOLDER).unwrap();
    for at in 0..files {
        table
            .push(big, &format!("old-{at}.txt"), Flags::NONE)
            .unwrap();
    }
    table.push(left, "keep.txt", Flags::NONE).unwrap();
    let other = table.push(tree, "other", Flags::FOLDER).unwrap();
    table.push(other, "kept.txt", Flags::NONE).unwrap();
    table.remove(gone);
    table
}

/// A walked `big` subtree over `files` new files.
fn walked_fixture(files: usize) -> NameTable {
    let mut walked = NameTable::new();
    let big = walked.push(NO_PARENT, "big", Flags::FOLDER).unwrap();
    for at in 0..files {
        walked
            .push(big, &format!("new-{at}.txt"), Flags::NONE)
            .unwrap();
    }
    walked
}

fn paths(table: &NameTable) -> HashSet<String> {
    table
        .iter()
        .filter_map(|(id, _)| table.path_with(id, '/'))
        .collect()
}

fn kept_paths() -> HashSet<String> {
    HashSet::from([
        "tree".to_owned(),
        "tree/left".to_owned(),
        "tree/left/keep.txt".to_owned(),
        "tree/other".to_owned(),
        "tree/other/kept.txt".to_owned(),
    ])
}

#[test]
fn a_cancelled_merge_drops_both_subtrees_and_queues_the_root() {
    let fs = Arc::new(FakeFs::new());
    let (mut worker, root) = worker_over(&fs, merge_fixture(3));
    let walked = walked_fixture(4);
    worker.cancel.cancel();
    let shared = worker.table.clone();
    let mut table = shared.write().unwrap();
    assert_eq!(worker.merge_subtree(&mut table, &root, &walked), Ok(false));
    assert_eq!(paths(&table), kept_paths());
    assert!(
        table
            .iter()
            .all(|(id, _)| table.path_with(id, '/').is_some())
    );
    assert!(table.valid_structure());
    assert_eq!(worker.rescan, vec![root]);
}

#[test]
fn a_merged_subtree_hangs_off_the_renumbered_parent() {
    let fs = Arc::new(FakeFs::new());
    let (mut worker, root) = worker_over(&fs, merge_fixture(3));
    let walked = walked_fixture(4);
    let shared = worker.table.clone();
    let mut table = shared.write().unwrap();
    assert_eq!(worker.merge_subtree(&mut table, &root, &walked), Ok(true));
    let mut expected = kept_paths();
    expected.insert("tree/left/big".to_owned());
    expected.extend((0..4).map(|at| format!("tree/left/big/new-{at}.txt")));
    assert_eq!(paths(&table), expected);
    assert!(table.valid_structure());
}

#[test]
fn a_queued_root_is_walked_again_and_leaves_the_queue() {
    let fs = Arc::new(FakeFs::new());
    fs.add_folder("tree/left/big");
    fs.add_file("tree/left/big/new.txt", 1);
    let (mut worker, root) = worker_over(&fs, merge_fixture(3));
    worker.rescan.push(root);
    worker.scan_queued();
    assert!(worker.rescan.is_empty());
    assert_eq!(fs.calls("tree/left/big", FakeOp::List), 1);
    let shared = worker.table.clone();
    let table = shared.read().unwrap();
    let found = paths(&table);
    assert!(found.contains("tree/left/big/new.txt"));
    assert!(!found.contains("tree/left/big/old-0.txt"));
}
