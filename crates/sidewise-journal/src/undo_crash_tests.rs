//! Crash and fault loops of journal undo (J5): a power loss (every `crash_with` pattern) or a
//! single fault at every op of an undo of a committed two-item copy, then recovery at open.
//! Every target ends whole: old with its old time under a durable undo `Commit`, else new
//! with the copy's time; a second recovery changes nothing.
// Linux only: `RealDisk` is built on Linux until D3.
#![cfg(target_os = "linux")]

use std::fs;
use std::io;
use std::sync::Arc;

use sidewise_fs::durable::{FaultDisk, Plan, RealDisk};

use super::UndoError;
use super::tests::copied;
use crate::job_crash_tests::{PATTERNS, Pattern};
use crate::job_tests::{Fixture, NEW, OLD, OLD_MS, mtime_ms};
use crate::{JobKind, Journal, JournalError, Record, Recovered, Swept, decode};

/// What an undo run left: the fixture and each target's time after the copy.
struct Undone {
    fixture: Fixture,
    copy_ms: Vec<i64>,
}

/// Opens a journal on a `FaultDisk` with `plan`, copies `NEW` over two targets, undoes the
/// copy, and returns the disk and the undo's result. `plan` must break nothing before the
/// copy ends; the journal is dropped, so recovery may claim its owner.
fn undo_run(plan: Plan) -> (Undone, Arc<FaultDisk>, Result<(), UndoError>) {
    let fixture = Fixture::new(2);
    let disk = Arc::new(FaultDisk::new(RealDisk, plan));
    let journal = Journal::open_raw(&fixture.journal_dir(), disk.clone()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    let copy_ms = fixture.targets.iter().map(|t| mtime_ms(t)).collect();
    let result = journal.undo(&id).map(|_| ());
    drop(journal);
    (Undone { fixture, copy_ms }, disk, result)
}

/// The ops of opening the journal and the copy, and the ops of the undo, with no fault.
fn op_counts() -> (u64, u64) {
    let fixture = Fixture::new(2);
    let disk = Arc::new(FaultDisk::new(RealDisk, Plan::Pass));
    let journal = Journal::open_raw(&fixture.journal_dir(), disk.clone()).unwrap();
    let id = copied(&journal, &fixture).unwrap();
    let base = disk.ops();
    journal.undo(&id).unwrap();
    (base, disk.ops() - base)
}

/// True when an `Undo` job's log ends in a durable `Commit`. Fails on a damaged log.
fn undo_committed(fixture: &Fixture, case: &str) -> Result<bool, String> {
    let mut committed = false;
    for job in fixture.jobs() {
        let log = decode(&fs::read(job.join("log")).unwrap_or_default());
        if log.damaged {
            return Err(format!("{case}: damaged log in {job:?}"));
        }
        let closed = matches!(
            log.records.last(),
            Some(Record::Commit | Record::Aborted { .. })
        );
        if !log.records.is_empty() && !closed {
            return Err(format!("{case}: job left open in {job:?}"));
        }
        let is_undo = matches!(
            log.records.first(),
            Some(Record::Begin {
                kind: JobKind::Undo,
                ..
            })
        );
        committed |= is_undo && log.records.contains(&Record::Commit);
    }
    Ok(committed)
}

/// Checks the disk after recovery: no temp or log rewrite left, every log closed, and every
/// target whole: old at `OLD_MS` after an undo `Commit`, else new at the copy's time.
fn check_settled(undone: &Undone, case: &str) -> Result<bool, String> {
    let fixture = &undone.fixture;
    let committed = undo_committed(fixture, case)?;
    if !fixture.temps().is_empty() {
        return Err(format!("{case}: temps left: {:?}", fixture.temps()));
    }
    for (target, copy_ms) in fixture.targets.iter().zip(&undone.copy_ms) {
        let bytes = fs::read(target).map_err(|e| format!("{case}: {target:?} {e}"))?;
        let want: (&[u8], i64) = if committed {
            (OLD, OLD_MS)
        } else {
            (NEW, *copy_ms)
        };
        if (bytes.as_slice(), mtime_ms(target)) != want {
            return Err(format!(
                "{case}: {target:?} is {:?}, undo committed: {committed}",
                String::from_utf8_lossy(&bytes)
            ));
        }
    }
    Ok(committed)
}

/// Recovers with `RealDisk`, checks the result, then recovers again: nothing more to do.
fn recover_and_check(undone: &Undone, case: &str) -> Result<bool, String> {
    let dir = undone.fixture.journal_dir();
    let (journal, recovered) =
        Journal::open(&dir, Arc::new(RealDisk)).map_err(|e| format!("{case}: open: {e}"))?;
    drop(journal);
    if recovered.failed != 0 || recovered.damaged != 0 || recovered.kept != 0 {
        return Err(format!("{case}: not settled: {recovered:?}"));
    }
    // The copy committed before the undo began: every job rolled back here is the undo.
    let undos = (recovered.rolled_back_undos, recovered.restored_undos);
    if undos != (recovered.rolled_back, recovered.restored) {
        return Err(format!("{case}: undo counts wrong: {recovered:?}"));
    }
    let committed = check_settled(undone, case)?;
    let (_journal, again) =
        Journal::open(&dir, Arc::new(RealDisk)).map_err(|e| format!("{case}: reopen: {e}"))?;
    // Only the bytes of the jobs left may be non-zero: nothing restored, swept or failed.
    let idle = Swept {
        bytes_left: again.swept.bytes_left,
        ..Swept::default()
    };
    if again
        != (Recovered {
            swept: idle,
            ..Recovered::default()
        })
    {
        return Err(format!("{case}: second recovery did work: {again:?}"));
    }
    check_settled(undone, &format!("{case}, second recovery"))?;
    Ok(committed)
}

/// Crashes the undo at op `n` after the copy, keeping the unsynced changes `pattern` names.
fn crashed_undo(base: u64, n: u64, pattern: Pattern) -> Undone {
    let (undone, disk, _) = undo_run(Plan::CrashAt(base + n));
    disk.crash_with(pattern.1, pattern.2).unwrap();
    undone
}

#[test]
fn undo_crash_at_every_op_leaves_each_target_whole_after_recovery() {
    let (base, ops) = op_counts();
    assert!(ops > 40, "the loop covers both undo items ({ops} ops)");
    let (mut rolled, mut kept) = (0, 0);
    for n in 1..=ops + 1 {
        for pattern in PATTERNS {
            let undone = crashed_undo(base, n, pattern);
            let case = format!("undo crash at op {n} of {ops}, {}", pattern.0);
            match recover_and_check(&undone, &case) {
                Ok(true) => kept += 1,
                Ok(false) => rolled += 1,
                Err(error) => panic!("{error}"),
            }
        }
    }
    assert!(
        rolled > 0 && kept > 0,
        "both ends seen: {rolled} rolled, {kept} kept"
    );
}

#[test]
fn undo_fault_at_every_op_is_rolled_back_or_whole() {
    let (base, ops) = op_counts();
    for n in 1..=ops {
        let plan = Plan::FailAt(base + n, io::ErrorKind::Other);
        let (undone, _, result) = undo_run(plan);
        let case = format!("undo fault at op {n} of {ops}: {result:?}");
        let committed = recover_and_check(&undone, &case).unwrap_or_else(|e| panic!("{e}"));
        let in_doubt = matches!(result, Err(UndoError::Journal(JournalError::InDoubt(_))));
        assert_eq!(
            committed,
            result.is_ok() || in_doubt,
            "{case}: the result says what the disk holds"
        );
    }
}

/// Recovery names a rolled-back interrupted `Undo` as an undo, and counts the files it put
/// back as the copy left them in `restored_undos`.
#[test]
fn interrupted_undo_is_counted_as_an_undo() {
    let (base, ops) = op_counts();
    let (mut rolled, mut restored) = (0, 0);
    for n in 1..=ops {
        let (undone, _, _) = undo_run(Plan::CrashAt(base + n));
        let dir = undone.fixture.journal_dir();
        let (_journal, recovered) = Journal::open(&dir, Arc::new(RealDisk)).unwrap();
        assert_eq!(recovered.rolled_back_undos, recovered.rolled_back, "op {n}");
        assert_eq!(recovered.restored_undos, recovered.restored, "op {n}");
        rolled += recovered.rolled_back_undos;
        restored += recovered.restored_undos;
    }
    assert!(
        rolled > 0 && restored > 0,
        "{rolled} undos, {restored} files"
    );
}
