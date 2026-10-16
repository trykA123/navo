#![cfg(any(windows, target_os = "linux"))]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test on a broken fixture"
)]

mod support;

use std::collections::HashSet;
use std::fs;
use std::io::Write;
use std::time::Duration;

use rusty_core::FsErrorKind;
use rusty_fs::{WatchChange, WatchEvent};
use support::{name, wait_for, wait_for_change, watched, watched_with};

const BURST: usize = 2_000;

#[test]
fn a_created_file_is_reported() {
    let watched = watched();
    fs::write(watched.path("made.txt"), b"x").unwrap();
    wait_for_change(&watched.events, &WatchChange::Created(name("made.txt")));
}

#[test]
fn a_deleted_file_is_reported() {
    let watched = watched_with(|dir| fs::write(dir.join("doomed.txt"), b"x").unwrap());
    fs::remove_file(watched.path("doomed.txt")).unwrap();
    wait_for_change(&watched.events, &WatchChange::Removed(name("doomed.txt")));
}

#[test]
fn a_renamed_file_is_reported_as_one_rename() {
    let watched = watched_with(|dir| fs::write(dir.join("before.txt"), b"x").unwrap());
    fs::rename(watched.path("before.txt"), watched.path("after.txt")).unwrap();
    let rename = WatchChange::Renamed {
        from: name("before.txt"),
        to: name("after.txt"),
    };
    wait_for_change(&watched.events, &rename);
}

#[test]
fn a_modified_file_is_reported() {
    let watched = watched_with(|dir| fs::write(dir.join("grows.log"), b"x").unwrap());
    let mut file = fs::OpenOptions::new()
        .append(true)
        .open(watched.path("grows.log"))
        .unwrap();
    file.write_all(b"more").unwrap();
    file.sync_all().unwrap();
    drop(file);
    wait_for_change(&watched.events, &WatchChange::Modified(name("grows.log")));
}

#[test]
fn a_burst_of_two_thousand_files_arrives_in_a_few_batches() {
    let watched = watched();
    for index in 0..BURST {
        fs::File::create(watched.path(&format!("f{index:04}.txt"))).unwrap();
    }
    let seen = wait_for(&watched.events, |seen| {
        let created: HashSet<_> = seen
            .changes
            .iter()
            .filter_map(|change| match change {
                WatchChange::Created(name) => Some(name.to_text()),
                _ => None,
            })
            .filter(|name| name.starts_with('f'))
            .collect();
        seen.overflowed || created.len() == BURST
    });
    assert!(
        seen.batches <= 50,
        "{} files came in {} batches",
        BURST,
        seen.batches
    );
}

#[test]
fn a_change_inside_a_subfolder_is_not_reported() {
    let watched = watched_with(|dir| fs::create_dir(dir.join("sub")).unwrap());
    fs::write(watched.path("sub").join("inner.txt"), b"x").unwrap();
    fs::write(watched.path("marker.txt"), b"x").unwrap();
    let seen = wait_for_change(&watched.events, &WatchChange::Created(name("marker.txt")));
    assert!(!seen.changes.iter().any(|change| matches!(
        change,
        WatchChange::Created(found) | WatchChange::Modified(found) if *found == name("inner.txt")
    )));
}

#[test]
fn dropping_the_watch_ends_the_event_stream() {
    let mut watched = watched();
    drop(watched.watch.take());
    fs::write(watched.path("late.txt"), b"x").unwrap();
    assert!(watched.events.recv_timeout(Duration::from_secs(5)).is_err());
}

#[test]
fn watching_a_missing_folder_reports_not_found_from_its_thread() {
    let dir = tempfile::tempdir().unwrap();
    let (_watch, events) = support::start_watch(&dir.path().join("nope"));
    let failed = events.recv_timeout(support::PATIENCE).unwrap();
    assert!(matches!(failed, WatchEvent::Failed(error) if error.kind == FsErrorKind::NotFound));
}

#[cfg(target_os = "linux")]
#[test]
fn removing_the_watched_folder_fails_the_watch() {
    let dir = tempfile::tempdir().unwrap();
    let folder = dir.path().join("watched");
    fs::create_dir(&folder).unwrap();
    let (_watch, events) = support::watch_path(&folder);
    fs::remove_dir_all(&folder).unwrap();
    use std::time::Instant;
    let deadline = Instant::now() + support::PATIENCE;
    let failed = std::iter::from_fn(|| {
        events
            .recv_timeout(deadline.saturating_duration_since(Instant::now()))
            .ok()
    })
    .find(|event| !matches!(event, WatchEvent::Changed(_)));
    assert!(
        matches!(failed, Some(WatchEvent::Failed(error)) if error.kind == FsErrorKind::NotFound)
    );
}
