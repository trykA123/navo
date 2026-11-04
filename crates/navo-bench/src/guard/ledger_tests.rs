#![allow(
    clippy::disallowed_methods,
    reason = "ledger tests damage and remove the ledger in their own temp trees"
)]

use std::fs;
use std::path::PathBuf;

use super::ledger::{self, LEDGER_FILE, MAGIC};
use super::placement::create_test_root;
use super::stale::read_marker;
use super::test_support::{TestRecycler, tree};
use super::{MARKER_FILE, PathRole, ScratchRoot, StaleCleanup};

const HEX: &str = "0123456789abcdef0123456789abcdef";

fn run_name(stamp: u64) -> String {
    format!("run-{stamp}-{HEX}")
}

/// An older run left behind, plus the current run, in one fresh parent.
fn old_and_current() -> (tempfile::TempDir, PathBuf, ScratchRoot) {
    let parent = tempfile::tempdir().unwrap();
    let old = create_test_root(parent.path(), None, &run_name(1), &[]).unwrap();
    let old_path = old.path().to_path_buf();
    drop(old);
    let current = create_test_root(parent.path(), None, &run_name(2), &[]).unwrap();
    (parent, old_path, current)
}

fn resolved_parent(scratch: &ScratchRoot) -> PathBuf {
    scratch.path().parent().unwrap().to_path_buf()
}

fn listed(scratch: &ScratchRoot) -> Vec<[u8; 32]> {
    ledger::tokens_in(&resolved_parent(scratch)).unwrap()
}

fn clean(current: &ScratchRoot) -> StaleCleanup {
    let parent = resolved_parent(current);
    current
        .clean_stale_runs(&parent, &TestRecycler::default())
        .unwrap()
}

#[test]
fn small_create_lists_the_token_and_remove_root_unlists_it() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let (token, folder) = (*scratch.token(), resolved_parent(&scratch));
    assert_eq!(listed(&scratch), vec![token]);
    let text = fs::read_to_string(folder.join(LEDGER_FILE)).unwrap();
    let hex: String = token.iter().map(|byte| format!("{byte:02x}")).collect();
    assert_eq!(text, format!("{MAGIC}{hex}\n"));
    scratch.remove_root(&TestRecycler::default()).unwrap();
    assert!(ledger::tokens_in(&folder).unwrap().is_empty());
}

#[test]
fn medium_a_listed_older_run_is_recycled_and_unlisted() {
    let (_parent, old, current) = old_and_current();
    let old_token = read_marker(&old, PathRole::StaleRun).unwrap();
    assert_eq!(
        clean(&current),
        StaleCleanup {
            recycled: 1,
            refused: 0
        }
    );
    assert!(!old.exists());
    assert_eq!(listed(&current), vec![*current.token()]);
    assert!(!listed(&current).contains(&old_token));
}

#[test]
fn medium_a_marked_run_missing_from_the_ledger_is_untouched() {
    let (_parent, old, current) = old_and_current();
    let old_token = read_marker(&old, PathRole::StaleRun).unwrap();
    ledger::remove(&resolved_parent(&current), &old_token, PathRole::StaleRun).unwrap();
    let before = tree(&old);
    assert_eq!(
        clean(&current),
        StaleCleanup {
            recycled: 0,
            refused: 1
        }
    );
    assert_eq!(tree(&old), before);
    assert!(old.join(MARKER_FILE).exists());
}

#[test]
fn medium_a_missing_ledger_recycles_nothing() {
    let (_parent, old, current) = old_and_current();
    fs::remove_file(resolved_parent(&current).join(LEDGER_FILE)).unwrap();
    let before = tree(&old);
    assert_eq!(clean(&current).recycled, 0);
    assert_eq!(tree(&old), before);
}

fn corrupt_ledgers(token: &[u8; 32]) -> Vec<Vec<u8>> {
    let hex: String = token.iter().map(|byte| format!("{byte:02x}")).collect();
    vec![
        b"not a ledger".to_vec(),
        format!("{hex}\n").into_bytes(),
        format!("{MAGIC}{hex}").into_bytes(),
        format!("{MAGIC}{}\n", hex.to_uppercase()).into_bytes(),
        format!("{MAGIC}{hex}\n{}\n", &hex[..63]).into_bytes(),
        format!("{MAGIC}{hex}\r\n").into_bytes(),
        vec![b'0'; 70 * 1024],
    ]
}

#[test]
fn medium_a_corrupt_ledger_recycles_nothing() {
    for index in 0..corrupt_ledgers(&[0; 32]).len() {
        let (_parent, old, current) = old_and_current();
        let old_token = read_marker(&old, PathRole::StaleRun).unwrap();
        let damaged = corrupt_ledgers(&old_token).swap_remove(index);
        fs::write(resolved_parent(&current).join(LEDGER_FILE), damaged).unwrap();
        let before = tree(&old);
        assert_eq!(clean(&current).recycled, 0, "case {index}");
        assert_eq!(tree(&old), before, "case {index}");
    }
}

#[test]
fn small_a_file_that_is_not_our_ledger_is_never_replaced() {
    for index in 0..corrupt_ledgers(&[0xab; 32]).len() {
        let parent = tempfile::tempdir().unwrap();
        let first = ScratchRoot::create(parent.path(), None).unwrap();
        let folder = resolved_parent(&first);
        let damaged = corrupt_ledgers(&[0xab; 32]).swap_remove(index);
        fs::write(folder.join(LEDGER_FILE), &damaged).unwrap();
        let second = ScratchRoot::create(parent.path(), None);
        assert!(second.is_err(), "case {index}");
        assert_eq!(fs::read(folder.join(LEDGER_FILE)).unwrap(), damaged);
        assert_eq!(ledger_names(&folder), vec![LEDGER_FILE.to_string()]);
    }
}

#[test]
fn small_a_ledger_path_that_cannot_be_read_is_an_error_not_an_empty_ledger() {
    let parent = tempfile::tempdir().unwrap();
    let first = ScratchRoot::create(parent.path(), None).unwrap();
    let folder = resolved_parent(&first);
    fs::remove_file(folder.join(LEDGER_FILE)).unwrap();
    fs::create_dir(folder.join(LEDGER_FILE)).unwrap();
    assert!(ScratchRoot::create(parent.path(), None).is_err());
    assert!(folder.join(LEDGER_FILE).is_dir());
    assert_eq!(ledger_names(&folder), vec![LEDGER_FILE.to_string()]);
}

fn ledger_names(folder: &std::path::Path) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(LEDGER_FILE))
        .collect();
    names.sort();
    names
}

#[test]
fn small_a_ledger_write_leaves_no_temporary_file() {
    let parent = tempfile::tempdir().unwrap();
    let scratch = ScratchRoot::create(parent.path(), None).unwrap();
    let folder = resolved_parent(&scratch);
    scratch.remove_root(&TestRecycler::default()).unwrap();
    let names: Vec<String> = fs::read_dir(&folder)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|name| name.starts_with(LEDGER_FILE))
        .collect();
    assert_eq!(names, vec![LEDGER_FILE.to_string()]);
}

#[test]
fn medium_a_linked_ledger_recycles_nothing() {
    let (_parent, old, current) = old_and_current();
    let outside = tempfile::tempdir().unwrap();
    let ledger_path = resolved_parent(&current).join(LEDGER_FILE);
    fs::rename(&ledger_path, outside.path().join("ledger")).unwrap();
    super::test_support::link(&outside.path().join("ledger"), &ledger_path);
    assert_eq!(clean(&current).recycled, 0);
    assert!(old.join(MARKER_FILE).exists());
}
