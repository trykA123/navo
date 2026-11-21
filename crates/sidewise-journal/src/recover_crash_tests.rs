//! Crash loops of recovery at open (J3): a power loss at every op of a two-item job (committed
//! or aborted), then `open`; a power loss at every op of that recovery, then `open` again. Each loop keeps
//! several subsets of the unsynced changes and tails (`crash_with`). After the last `open`
//! every target is old with its old time, or new under a durable `Commit` (NC162).
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::fs;
use std::sync::Arc;

use sidewise_fs::durable::{FaultDisk, Plan, RealDisk};

use crate::job::SKIP_BACKED_SYNC;
use crate::job_crash_tests::{PATTERNS, Pattern};
use crate::job_tests::{Fixture, NEW, OLD, OLD_MS, mtime_ms, run};
use crate::{Journal, Record, decode};

/// Opens the journal with `RealDisk`, so recovery runs, then checks what it left.
pub(crate) fn check_open(fixture: &Fixture, case: &str) -> Result<(), String> {
    let (_journal, recovered) = Journal::open(&fixture.journal_dir(), Arc::new(RealDisk))
        .map_err(|error| format!("{case}: open failed: {error}"))?;
    if recovered.failed != 0 || recovered.damaged != 0 {
        return Err(format!("{case}: not settled: {recovered:?}"));
    }
    check_settled(fixture, case)
}

/// Checks a settled journal: every log with records ends in `Commit` or `Aborted`, no log
/// rewrite or target temp is left, and each target is new after a `Commit`, else old with
/// its old modified time.
pub(crate) fn check_settled(fixture: &Fixture, case: &str) -> Result<(), String> {
    let mut committed = false;
    for job in fixture.jobs() {
        let leftover = fs::read_dir(&job)
            .map_err(|error| format!("{case}: {error}"))?
            .filter_map(Result::ok)
            .any(|entry| entry.file_name().to_string_lossy().starts_with("log.sw-"));
        if leftover {
            return Err(format!("{case}: log rewrite left in {job:?}"));
        }
        let log = decode(&fs::read(job.join("log")).unwrap_or_default());
        committed |= log.records.contains(&Record::Commit);
        let closed = matches!(
            log.records.last(),
            Some(Record::Commit | Record::Aborted { .. })
        );
        if !log.records.is_empty() && !closed {
            return Err(format!("{case}: job left open: {:?}", log.records.last()));
        }
    }
    if !fixture.temps().is_empty() {
        return Err(format!("{case}: temps left: {:?}", fixture.temps()));
    }
    for target in &fixture.targets {
        let bytes = fs::read(target).map_err(|error| format!("{case}: {target:?} {error}"))?;
        if committed && bytes != NEW {
            return Err(format!("{case}: committed but not new"));
        }
        if !committed && (bytes.as_slice(), mtime_ms(target)) != (OLD, OLD_MS) {
            return Err(format!("{case}: not old with its old time"));
        }
    }
    Ok(())
}

/// Runs a two-item job (committed when `commit`, else aborted) that crashes at op `n`, then
/// loses power keeping the unsynced changes and tails `pattern` names.
fn crashed_job(n: u64, pattern: Pattern, commit: bool) -> Fixture {
    let fixture = Fixture::new(2);
    let disk = Arc::new(FaultDisk::new(RealDisk, Plan::CrashAt(n)));
    let _ = run(disk.clone(), &fixture, commit);
    disk.crash_with(pattern.1, pattern.2).unwrap();
    fixture
}

/// The op count of the whole two-item job with no fault.
fn job_ops(commit: bool) -> u64 {
    let disk = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
    let _ = run(disk.clone(), &Fixture::new(2), commit);
    disk.ops()
}

/// Crashes the job at every op with every pattern, then recovers once with `RealDisk`.
fn job_crash_loop(commit: bool) -> Result<(), String> {
    let ops = job_ops(commit);
    assert!(ops > 40, "the loop covers both items ({ops} ops)");
    for n in 1..=ops + 1 {
        for pattern in PATTERNS {
            let fixture = crashed_job(n, pattern, commit);
            check_open(
                &fixture,
                &format!("job crash at op {n} of {ops}, {}", pattern.0),
            )?;
        }
    }
    Ok(())
}

#[test]
fn recover_after_a_crash_at_every_op_of_a_committed_job() {
    job_crash_loop(true).unwrap();
}

#[test]
fn recover_after_a_crash_at_every_op_of_an_aborted_job() {
    job_crash_loop(false).unwrap();
}

/// Crashes the job (committed when `commit`, else aborted) at op `n` (pattern `first`), then
/// crashes recovery at every op with every pattern, then recovers again with `RealDisk`.
/// Returns the op count of a clean recovery.
fn recovery_crash_loop_at(n: u64, first: Pattern, commit: bool) -> Result<u64, String> {
    let probe = crashed_job(n, first, commit);
    let counter = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
    let _ = Journal::open(&probe.journal_dir(), counter.clone());
    let ops = counter.ops();
    for m in 1..=ops + 1 {
        for second in PATTERNS {
            let fixture = crashed_job(n, first, commit);
            let disk = Arc::new(FaultDisk::new(RealDisk, Plan::CrashAt(m)));
            let _ = Journal::open(&fixture.journal_dir(), disk.clone());
            disk.crash_with(second.1, second.2).unwrap();
            let case = format!(
                "{} job crash at op {n}, {}; recovery crash at op {m} of {ops}, {}",
                if commit { "committed" } else { "aborted" },
                first.0,
                second.0
            );
            check_open(&fixture, &case)?;
        }
    }
    Ok(ops)
}

/// Runs `recovery_crash_loop_at` from every job crash; returns the most recovery ops seen.
fn recovery_crash_loop(commit: bool) -> Result<u64, String> {
    let mut most = 0;
    for n in 1..=job_ops(commit) + 1 {
        for first in PATTERNS {
            most = most.max(recovery_crash_loop_at(n, first, commit)?);
        }
    }
    Ok(most)
}

#[test]
fn recover_after_a_crash_at_every_op_of_recovery_itself() {
    let most = recovery_crash_loop(true).unwrap();
    assert!(most > 25, "a recovery rolls back two items ({most} ops)");
}

#[test]
fn recover_after_a_crash_at_every_op_of_recovery_of_an_aborted_job() {
    let most = recovery_crash_loop(false).unwrap();
    assert!(most > 25, "a recovery rolls back two items ({most} ops)");
}

#[test]
fn recover_crash_loop_fails_when_the_backed_line_is_not_synced() {
    SKIP_BACKED_SYNC.set(true);
    let result = job_crash_loop(true);
    SKIP_BACKED_SYNC.set(false);
    let caught = result.expect_err("an unsynced Backed line must fail a crash case");
    assert!(caught.contains("not old with its old time"), "{caught}");
}
