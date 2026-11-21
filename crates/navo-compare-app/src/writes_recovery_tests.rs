//! Test of the start-up recovery toast (P1 A1): a copy killed before the target changed is
//! still rolled back at the next start, so the start-up note must say so.
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::fs;
use std::sync::Arc;

use sidewise_fs::durable::{FaultDisk, Plan, RealDisk};
use sidewise_journal::{Expect, JobKind, Journal};

use crate::compare_tab_tests::TempDir;
use crate::writes::open_writes;

/// Kills one overwrite at its `n`th disk op (the process dies, the job log stays open).
fn killed_copy(journal_dir: &std::path::Path, target: &std::path::Path, n: u64) {
    let disk = Arc::new(FaultDisk::new(RealDisk, Plan::CrashAt(n)));
    let Ok(journal) = Journal::open_raw(journal_dir, disk) else {
        return;
    };
    let Ok(mut job) = journal.begin(JobKind::Copy, "Copy", None) else {
        return;
    };
    let _ = job.overwrite(target, Expect::default(), None, &mut |out| {
        out.write_all(b"the new bytes, longer\n")
            .map_err(|error| error.to_string())
    });
}

/// True when the one job log has an `Intent` and no `Commit` or `Aborted`: the copy was open.
fn log_is_open(journal_dir: &std::path::Path) -> bool {
    let Ok(jobs) = fs::read_dir(journal_dir.join("v1")) else {
        return false;
    };
    jobs.filter_map(Result::ok)
        .filter_map(|job| fs::read_to_string(job.path().join("log")).ok())
        .any(|log| {
            log.contains("\"t\":\"Intent\"")
                && !log.contains("\"t\":\"Commit\"")
                && !log.contains("\"t\":\"Aborted\"")
        })
}

/// Whenever recovery closes an interrupted copy with `Aborted {recovered}`, the start-up note
/// names it, whatever stage the kill hit (not only after the target already held new bytes).
#[test]
fn every_recovered_copy_gets_a_start_up_note() {
    let mut silent = Vec::new();
    for n in 1..80 {
        let profile = TempDir::new("recover-note");
        let journal_dir = navo_compare_profile::paths::open_profile(Some(profile.path()))
            .expect("profile")
            .journal_dir();
        let target = profile.path().join("target.txt");
        fs::write(&target, b"old bytes\n").expect("target");
        killed_copy(&journal_dir, &target, n);
        if !log_is_open(&journal_dir) {
            continue;
        }
        let (_host, note) = open_writes(Some(profile.path())).expect("journal opens");
        if note.is_none() {
            silent.push(n);
        }
    }
    assert!(
        silent.is_empty(),
        "recovered with no note after a kill at op {silent:?}"
    );
}
