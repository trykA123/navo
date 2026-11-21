//! Tests for `cache::evict` (NC223 rule 6): which keys stay, the byte budget, locks, staging,
//! trash, foreign names and links. Sizes are faked with sparse files.
#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    reason = "test: a failed setup step should abort the test"
)]

use std::fs::{File, OpenOptions};
use std::path::Path;
use std::time::{Duration, SystemTime};

use sidewise_git_remote::cache::evict;
use tempfile::TempDir;

const MIB: u64 = 1 << 20;
const GIB: u64 = 1 << 30;

/// The 32 hex digit key number `n`.
fn key(n: u32) -> String {
    format!("{n:032x}")
}

/// Makes `<key>.git` holding one sparse file of `bytes`, and `<key>.used` aged `age` seconds
/// (no `.used` when `age` is `None`).
fn entry(root: &Path, key: &str, bytes: u64, age: Option<u64>) {
    let live = root.join(format!("{key}.git"));
    std::fs::create_dir_all(live.join("objects")).unwrap();
    File::create(live.join("objects/pack"))
        .unwrap()
        .set_len(bytes)
        .unwrap();
    if let Some(age) = age {
        let used = File::create(root.join(format!("{key}.used"))).unwrap();
        used.set_modified(SystemTime::now() - Duration::from_secs(age))
            .unwrap();
    }
}

/// True when `<root>/<name>` exists (without following a link).
fn has(root: &Path, name: &str) -> bool {
    std::fs::symlink_metadata(root.join(name)).is_ok()
}

/// Opens and locks `<key>.lock` the way another Sidewise window would.
fn hold(root: &Path, key: &str) -> File {
    let file = OpenOptions::new()
        .create(true)
        .truncate(false)
        .write(true)
        .open(root.join(format!("{key}.lock")))
        .unwrap();
    file.lock().unwrap();
    file
}

#[test]
fn keeps_the_fetching_key_plus_the_15_newest() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    entry(root, &key(0), MIB, None);
    for n in 1..=20 {
        entry(root, &key(n), MIB, Some(u64::from(n) * 10));
    }
    entry(root, &key(21), MIB, None);
    let report = evict(root, &key(0), 256 * MIB).unwrap();
    assert!(
        has(root, &format!("{}.git", key(0))),
        "fetching key evicted"
    );
    for n in 1..=15 {
        assert!(has(root, &format!("{}.git", key(n))), "key {n} evicted");
    }
    for n in 16..=21 {
        assert!(!has(root, &format!("{}.git", key(n))), "key {n} kept");
        assert!(
            !has(root, &format!("{}.used", key(n))),
            "key {n} .used kept"
        );
    }
    assert_eq!(report.evicted, 6);
    assert_eq!(report.bytes, 16 * MIB);
}

#[test]
fn keeps_the_total_within_two_gib_minus_max_bytes() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    entry(root, &key(0), 600 * MIB, None);
    entry(root, &key(1), 600 * MIB, Some(10));
    entry(root, &key(2), 600 * MIB, Some(20));
    entry(root, &key(3), MIB, Some(30));
    let report = evict(root, &key(0), 256 * MIB).unwrap();
    assert!(has(root, &format!("{}.git", key(0))));
    assert!(has(root, &format!("{}.git", key(1))));
    assert!(!has(root, &format!("{}.git", key(2))), "over budget kept");
    assert!(
        !has(root, &format!("{}.git", key(3))),
        "older than an evicted key kept"
    );
    assert!(report.bytes <= 2 * GIB - 256 * MIB);
    assert_eq!(report.bytes, 1200 * MIB);
}

#[test]
fn never_deletes_a_lock() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    entry(root, &key(0), MIB, None);
    entry(root, &key(1), 2 * GIB, Some(10));
    File::create(root.join(format!("{}.lock", key(1)))).unwrap();
    evict(root, &key(0), 256 * MIB).unwrap();
    assert!(!has(root, &format!("{}.git", key(1))), "oversized key kept");
    assert!(has(root, &format!("{}.lock", key(1))), "lock deleted");
}

#[test]
fn skips_a_key_whose_lock_another_handle_holds() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    entry(root, &key(0), MIB, None);
    entry(root, &key(1), 2 * GIB, Some(10));
    let _held = hold(root, &key(1));
    let report = evict(root, &key(0), 256 * MIB).unwrap();
    assert!(has(root, &format!("{}.git", key(1))), "busy key evicted");
    assert!(has(root, &format!("{}.used", key(1))));
    assert_eq!(report.busy, 1);
    assert_eq!(report.evicted, 0);
}

#[test]
fn removes_staging_only_under_the_lock() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    entry(root, &key(0), MIB, None);
    entry(root, &key(1), MIB, Some(10));
    entry(root, &key(2), MIB, Some(20));
    let free = format!("{}.tmp-0123456789abcdef", key(1));
    let busy = format!("{}.tmp-0123456789abcdef", key(2));
    std::fs::create_dir(root.join(&free)).unwrap();
    std::fs::create_dir(root.join(&busy)).unwrap();
    let _held = hold(root, &key(2));
    evict(root, &key(0), 256 * MIB).unwrap();
    assert!(!has(root, &free), "staging of a free key kept");
    assert!(has(root, &busy), "staging of a busy key removed");
}

#[test]
fn removes_trash_without_a_lock() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    entry(root, &key(0), MIB, None);
    let trash = format!("{}.trash-fedcba9876543210", key(1));
    std::fs::create_dir_all(root.join(&trash).join("objects")).unwrap();
    let _held = hold(root, &key(1));
    evict(root, &key(0), 256 * MIB).unwrap();
    assert!(!has(root, &trash), "trash kept");
}

#[test]
fn leaves_foreign_names_untouched() {
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    entry(root, &key(0), MIB, None);
    std::fs::write(root.join("notes.txt"), "mine").unwrap();
    entry(root, "abc", 2 * GIB, None);
    let upper = key(0xabc).to_uppercase();
    entry(root, &upper, 2 * GIB, None);
    std::fs::create_dir(root.join(format!("{}.trash-xyz", key(1)))).unwrap();
    entry(root, &key(2), 2 * GIB, Some(10));
    let report = evict(root, &key(0), 256 * MIB).unwrap();
    assert_eq!(report.evicted, 1, "eviction did not run");
    assert!(has(root, "notes.txt"));
    assert!(has(root, "abc.git"));
    assert!(has(root, &format!("{upper}.git")));
    assert!(has(root, &format!("{}.trash-xyz", key(1))));
}

#[cfg(unix)]
#[test]
fn a_link_counts_zero_bytes_and_its_target_survives() {
    let dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let big = outside.path().join("big");
    File::create(&big).unwrap().set_len(3 * GIB).unwrap();
    let root = dir.path();
    entry(root, &key(0), MIB, None);
    entry(root, &key(1), MIB, Some(10));
    let linked = root.join(format!("{}.git", key(1)));
    std::os::unix::fs::symlink(&big, linked.join("big")).unwrap();
    std::os::unix::fs::symlink(outside.path(), linked.join("dir")).unwrap();
    let report = evict(root, &key(0), 256 * MIB).unwrap();
    assert!(has(root, &format!("{}.git", key(1))), "link was followed");
    assert_eq!(report.bytes, 2 * MIB);
    for n in 2..=17 {
        entry(root, &key(n), MIB, Some(1));
    }
    evict(root, &key(0), 256 * MIB).unwrap();
    assert!(!has(root, &format!("{}.git", key(1))), "oldest key kept");
    assert_eq!(big.metadata().unwrap().len(), 3 * GIB, "link target lost");
}

#[cfg(unix)]
#[test]
fn an_unreadable_key_is_kept_as_busy_and_others_still_go() {
    use std::os::unix::fs::PermissionsExt;
    let dir = TempDir::new().unwrap();
    let root = dir.path();
    entry(root, &key(0), MIB, None);
    for n in 1..=17 {
        entry(root, &key(n), MIB, Some(u64::from(n) * 10));
    }
    let locked = root.join(format!("{}.git/objects", key(1)));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let report = evict(root, &key(0), 256 * MIB);
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let report = report.unwrap();
    assert_eq!(report.busy, 1, "the unmeasured key counts as busy");
    assert!(
        has(root, &format!("{}.git", key(1))),
        "unmeasured key evicted"
    );
    for n in 2..=15 {
        assert!(has(root, &format!("{}.git", key(n))), "key {n} evicted");
    }
    for n in 16..=17 {
        assert!(!has(root, &format!("{}.git", key(n))), "key {n} kept");
    }
}
