//! Startup recovery of a cross-volume move whose copy was journaled with a no-ID source proof:
//! the rollback learns from the journal which removals may have run, so a copy-phase crash
//! compares no bytes (D165).

use std::sync::Arc;

use navo_core::{ConflictPolicy, OpKind, OperationPlan, PlanRequest, Step, plan};
use navo_fs::{
    CancelToken, CopiedSource, FakeFs, FakeOp, FileStamp, JobContext, JobOutcome, PlacedFile,
    Provider, run_job, scan,
};

use super::held_tests::{add_timed, journal};
use super::{SharedJournal, held, lock, log_factory, recover};

const NAMES: [&str; 2] = ["a.txt", "b.txt"];

fn sources() -> FakeFs {
    let fs = FakeFs::new();
    for name in NAMES {
        add_timed(&fs, "src", name, 7);
    }
    fs.add_folder("dst");
    fs
}

/// Turns the source volume into one without file IDs and journals a newer proof row for the copy
/// of `a.txt` that matches on size, times and attributes, as the Windows read of FAT does.
fn proved_without_id(fs: &FakeFs, journal: &SharedJournal) {
    fs.set_without_ids("src");
    fs.set_path_details(500, 0x20);
    let job = lock(journal).unfinished().unwrap()[0].id;
    let copy = fs.metadata(&fs.location("dst/a.txt")).unwrap();
    let source = fs.metadata(&fs.location("src/a.txt")).unwrap();
    let placed = PlacedFile {
        target: fs.location("dst/a.txt"),
        stamp: FileStamp {
            size: copy.size,
            modified_ms: copy.modified_ms,
        },
        source: Some(CopiedSource {
            identity: None,
            size: source.size,
            modified_ms: source.modified_ms,
            created_ms: Some(500),
            attributes: Some(0x20),
        }),
    };
    held::record_placed(journal, job, &placed).unwrap();
}

fn cross_volume_move(fs: &FakeFs) -> OperationPlan {
    let cancel = CancelToken::new();
    let sources: Vec<_> = NAMES
        .iter()
        .map(|name| fs.location(&format!("src/{name}")))
        .collect();
    plan(&PlanRequest {
        kind: OpKind::Move,
        items: scan(fs, &sources, &cancel).unwrap(),
        target: Some(fs.location("dst")),
        existing: Vec::new(),
        policy: ConflictPolicy::Ask,
        same_volume: false,
        protected: Vec::new(),
    })
}

/// Journals the whole move but runs it only up to the copy of `b.txt`, as if the app died then.
fn crashed_copying_b(fs: &FakeFs, journal: &SharedJournal, plan: &OperationPlan) {
    let b = fs.location("dst/b.txt");
    let in_flight = plan
        .steps
        .iter()
        .position(|step| matches!(step, Step::CopyFile { target, .. } if target.same_place(&b)))
        .unwrap();
    let mut head = plan.clone();
    head.steps.truncate(in_flight);
    let mut log = log_factory(Arc::clone(journal))(plan).unwrap();
    let outcome = run_job(
        &head,
        &[],
        &mut JobContext {
            ops: fs,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
}

fn content_reads(fs: &FakeFs) -> u32 {
    ["src/a.txt", "dst/a.txt"]
        .iter()
        .map(|path| fs.calls(path, FakeOp::Read))
        .sum()
}

#[test]
fn startup_recovery_of_a_copy_phase_crash_removes_a_no_id_copy_without_reading_it() {
    let fs = sources();
    let journal = journal();
    let plan = cross_volume_move(&fs);
    crashed_copying_b(&fs, &journal, &plan);
    proved_without_id(&fs, &journal);
    let before = content_reads(&fs);
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    assert!(
        fs.metadata(&fs.location("dst/a.txt")).is_err(),
        "the duplicate copy of a stayed"
    );
    assert!(fs.metadata(&fs.location("src/a.txt")).is_ok());
    assert_eq!(
        content_reads(&fs),
        before,
        "recovery read the copy-phase journal as unknown and compared bytes"
    );
}
