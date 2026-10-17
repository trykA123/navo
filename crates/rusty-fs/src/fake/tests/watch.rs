use std::sync::mpsc::{self, Receiver};
use std::time::Duration;

use rusty_core::{FsErrorKind, ProviderPath};

use crate::{FakeFs, FakeOp, Fault, Provider, ProviderOps, Watch, WatchChange, WatchEvent};

const PATIENCE: Duration = Duration::from_secs(5);

fn name(text: &str) -> ProviderPath {
    ProviderPath::from_text(text)
}

fn fake() -> FakeFs {
    let fs = FakeFs::new().with_watch();
    fs.add_folder("home/sub");
    fs.add_file("home/a.txt", 1);
    fs
}

fn watch(fs: &FakeFs, folder: &str) -> (Watch, Receiver<WatchEvent>) {
    let (sender, events) = mpsc::channel();
    let watch = fs
        .watch(
            &fs.location(folder),
            Box::new(move |event| {
                let _ = sender.send(event);
            }),
        )
        .unwrap();
    (watch, events)
}

fn next_changes(events: &Receiver<WatchEvent>) -> Vec<WatchChange> {
    match events.recv_timeout(PATIENCE) {
        Ok(WatchEvent::Changed(changes)) => changes,
        other => panic!("expected changes, got {other:?}"),
    }
}

#[test]
fn test_helpers_report_the_latest_change_of_each_name() {
    let fs = fake();
    let (_watch, events) = watch(&fs, "home");
    fs.add_file("home/b.txt", 1);
    fs.add_file("home/a.txt", 2);
    fs.remove("home/b.txt");
    fs.rename_entry("home/a.txt", "home/c.txt");
    fs.add_file("home/sub/deep.txt", 1);
    assert_eq!(
        next_changes(&events),
        [
            WatchChange::Removed(name("b.txt")),
            WatchChange::Modified(name("a.txt")),
            WatchChange::Renamed {
                from: name("a.txt"),
                to: name("c.txt")
            },
        ]
    );
}

#[test]
fn operations_report_their_changes() {
    let fs = fake();
    let (_watch, events) = watch(&fs, "home");
    fs.create_folder(&fs.location("home/made")).unwrap();
    fs.rename(&fs.location("home/a.txt"), &fs.location("home/sub/a.txt"))
        .unwrap();
    assert_eq!(
        next_changes(&events),
        [
            WatchChange::Created(name("made")),
            WatchChange::Removed(name("a.txt")),
        ]
    );
}

#[test]
fn an_injected_overflow_and_failure_reach_the_watcher() {
    let fs = fake();
    let (_watch, events) = watch(&fs, "home");
    fs.overflow_watchers("home");
    assert_eq!(events.recv_timeout(PATIENCE), Ok(WatchEvent::Overflow));
    fs.fail_watchers("home", FsErrorKind::NetworkLost);
    let failed = events.recv_timeout(PATIENCE).unwrap();
    assert!(matches!(failed, WatchEvent::Failed(e) if e.kind == FsErrorKind::NetworkLost));
}

#[test]
fn dropping_a_watch_unregisters_it() {
    let fs = fake();
    let (first, _) = watch(&fs, "home");
    let (_second, _) = watch(&fs, "/home");
    assert_eq!(fs.watchers("home"), 2);
    drop(first);
    assert_eq!(fs.watchers("home"), 1);
}

#[test]
fn watching_needs_the_capability_and_an_existing_folder() {
    let plain = FakeFs::new();
    plain.add_folder("home");
    assert!(
        plain
            .watch(&plain.location("home"), Box::new(|_| {}))
            .is_err()
    );
    let fs = fake();
    assert!(
        fs.watch(&fs.location("home/a.txt"), Box::new(|_| {}))
            .is_err()
    );
    assert!(fs.watch(&fs.location("nowhere"), Box::new(|_| {})).is_err());
}

#[test]
fn a_watch_fault_fails_the_start() {
    let fs = fake();
    fs.fault(
        "home",
        FakeOp::Watch,
        1,
        Fault::Fail(FsErrorKind::AccessDenied),
    );
    let started = fs.watch(&fs.location("home"), Box::new(|_| {}));
    assert_eq!(
        started.err().map(|e| e.kind),
        Some(FsErrorKind::AccessDenied)
    );
    assert_eq!(fs.calls("home", FakeOp::Watch), 1);
}

fn watch_tree(fs: &FakeFs, folder: &str) -> (Watch, Receiver<WatchEvent>) {
    let (sender, events) = mpsc::channel();
    let watch = fs
        .watch_tree(
            &fs.location(folder),
            Box::new(move |event| {
                let _ = sender.send(event);
            }),
        )
        .unwrap();
    (watch, events)
}

#[test]
fn a_tree_watch_reports_deep_changes_by_their_path_below_the_folder() {
    let fs = fake();
    let (_watch, events) = watch_tree(&fs, "home");
    assert_eq!(fs.watchers("home"), 1);
    fs.add_file("home/sub/deep/x.bin", 3);
    fs.rename_entry("home/sub/deep/x.bin", "home/sub/deep/y.bin");
    fs.remove("home/a.txt");
    assert_eq!(
        next_changes(&events),
        [
            WatchChange::Created(name("sub/deep")),
            WatchChange::Created(name("sub/deep/x.bin")),
            WatchChange::Renamed {
                from: name("sub/deep/x.bin"),
                to: name("sub/deep/y.bin")
            },
            WatchChange::Removed(name("a.txt")),
        ]
    );
}

#[test]
fn a_tree_watch_ignores_changes_outside_its_folder() {
    let fs = fake();
    fs.add_folder("other");
    let (_watch, events) = watch_tree(&fs, "home/sub");
    fs.add_file("home/b.txt", 1);
    fs.add_file("other/c.txt", 1);
    fs.add_file("home/sub/d.txt", 1);
    assert_eq!(next_changes(&events), [WatchChange::Created(name("d.txt"))]);
}
