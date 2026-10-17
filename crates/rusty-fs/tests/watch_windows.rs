#![cfg(windows)]
#![allow(
    clippy::print_stderr,
    reason = "a skipped share check is printed for the CI log"
)]
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::panic,
    reason = "test helpers fail the test on a broken fixture"
)]

mod support;

use std::fs;
use std::path::PathBuf;
use std::process::Command;

use rusty_fs::WatchChange;
use support::{name, wait_for_change, watch_path, watch_tree_path, watched_with};

#[test]
fn hiding_a_file_is_reported_as_a_modification() {
    let watched = watched_with(|dir| fs::write(dir.join("plain.txt"), b"x").unwrap());
    let status = Command::new("attrib")
        .arg("+h")
        .arg(watched.path("plain.txt"))
        .status()
        .unwrap();
    assert!(status.success());
    wait_for_change(&watched.events, &WatchChange::Modified(name("plain.txt")));
}

#[test]
fn a_folder_on_an_smb_share_is_watched() {
    let dir = tempfile::tempdir().unwrap();
    let local = dir.path().to_str().unwrap().to_owned();
    let Some((drive, rest)) = local.split_once(':') else {
        panic!("the temp folder {local} has no drive letter");
    };
    let share = PathBuf::from(format!(r"\\localhost\{drive}${rest}"));
    if !share.is_dir() {
        eprintln!(
            "share {} is not reachable, SMB watch skipped",
            share.display()
        );
        return;
    }
    let (_watch, events) = watch_path(&share);
    fs::write(dir.path().join("remote.txt"), b"x").unwrap();
    wait_for_change(&events, &WatchChange::Created(name("remote.txt")));
}

#[test]
fn a_tree_watch_reports_a_write_three_levels_deep_by_its_relative_path() {
    let dir = tempfile::tempdir().unwrap();
    fs::create_dir_all(dir.path().join(r"one\two\three")).unwrap();
    let (_watch, events) = watch_tree_path(dir.path());
    fs::write(dir.path().join(r"one\two\three\deep.bin"), b"x").unwrap();
    wait_for_change(
        &events,
        &WatchChange::Created(name(r"one\two\three\deep.bin")),
    );
}
