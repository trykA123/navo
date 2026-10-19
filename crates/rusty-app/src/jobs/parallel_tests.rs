use rusty_core::{FsErrorKind, OperationPlan, ProviderPath, Step};
use rusty_fs::{
    CancelToken, FakeFs, FakeOp, Fault, JobContext, JobOutcome, Provider, run_job_parallel,
};
use rusty_store::StepState;

use super::held_tests::{add_timed, held_rows, holding, old_report, overwrite_plan, placed_rows};
use super::{SharedJournal, lock, log_factory, open, recover};

const FILES: usize = 6;
const FAILING: usize = 2;
const NEW_SIZE: u64 = 7;

fn name(index: usize) -> String {
    format!("f{index}.txt")
}

fn source_tree(old_files: &[usize]) -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("dest");
    for index in 0..FILES {
        add_timed(&fs, "home/docs", &name(index), NEW_SIZE);
    }
    if !old_files.is_empty() {
        fs.add_folder("dest/docs");
        for &index in old_files {
            let entry = rusty_fs::FsEntry {
                name: ProviderPath::from_text(&name(index)),
                ..old_report()
            };
            fs.add_entry("dest/docs", entry);
        }
    }
    fs
}

fn sizes_at_target(fs: &FakeFs) -> Vec<Option<u64>> {
    (0..FILES)
        .map(|index| {
            let path = format!("dest/docs/{}", name(index));
            fs.metadata(&fs.location(&path)).ok().map(|e| e.size)
        })
        .collect()
}

fn done_copies(journal: &SharedJournal, plan: &OperationPlan) -> usize {
    let job = lock(journal).unfinished().unwrap().remove(0);
    job.steps
        .iter()
        .filter(|step| step.state == StepState::Done)
        .filter(|step| matches!(plan.steps[step.seq as usize], Step::CopyFile { .. }))
        .count()
}

fn crash_midway(fs: &FakeFs, journal: &SharedJournal) -> OperationPlan {
    let plan = overwrite_plan(fs);
    let failing = format!("home/docs/{}", name(FAILING));
    fs.fault(&failing, FakeOp::Copy, 1, Fault::Fail(FsErrorKind::Locked));
    let mut log = log_factory(journal.clone())(&plan).unwrap();
    let outcome = run_job_parallel(
        &plan,
        &[],
        &mut JobContext {
            ops: fs,
            log: log.as_mut(),
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
        4,
    );
    assert!(matches!(outcome, JobOutcome::Failed { .. }), "{outcome:?}");
    plan
}

fn temp_journal() -> (tempfile::TempDir, SharedJournal) {
    let dir = tempfile::tempdir().unwrap();
    let journal = open(&dir.path().join("ferrite.db")).unwrap();
    (dir, journal)
}

#[test]
fn a_four_worker_copy_cut_midway_is_rolled_back_to_the_empty_target() {
    let (_dir, journal) = temp_journal();
    let fs = source_tree(&[]);
    let plan = crash_midway(&fs, &journal);
    let landed = sizes_at_target(&fs);
    let done = done_copies(&journal, &plan);
    assert_eq!(landed.iter().flatten().count(), done);
    assert!((FAILING..FILES).contains(&done), "{done} copies done");
    assert!(landed[..FAILING].iter().all(Option::is_some));
    assert_eq!(landed[FAILING], None);
    assert_eq!((held_rows(&journal), placed_rows(&journal)), (0, done));
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    assert_eq!(
        (sizes_at_target(&fs), holding(&fs), placed_rows(&journal)),
        (vec![None; FILES], Vec::new(), 0)
    );
    assert!(fs.metadata(&fs.location("dest/docs")).is_err());
    assert!(lock(&journal).unfinished().unwrap().is_empty());
    let kept = |index| {
        let path = format!("home/docs/{}", name(index));
        fs.metadata(&fs.location(&path)).is_ok()
    };
    assert!((0..FILES).all(kept));
}

const OVERWRITTEN: [usize; 3] = [0, 2, 4];

#[test]
fn a_four_worker_overwrite_cut_midway_gets_every_old_file_back() {
    let (_dir, journal) = temp_journal();
    let fs = source_tree(&OVERWRITTEN);
    let plan = crash_midway(&fs, &journal);
    let landed = sizes_at_target(&fs);
    let done = done_copies(&journal, &plan);
    let new_at = |wanted: bool| {
        (0..FILES)
            .filter(|index| OVERWRITTEN.contains(index) == wanted)
            .filter(|&index| landed[index] == Some(NEW_SIZE))
            .count()
    };
    assert_eq!(new_at(true) + new_at(false), done);
    assert!((FAILING..FILES).contains(&done), "{done} copies done");
    assert_eq!(
        (held_rows(&journal), placed_rows(&journal)),
        (new_at(true), new_at(false))
    );
    assert_eq!(holding(&fs).len(), usize::from(new_at(true) > 0));
    assert_eq!(recover(&journal, &fs).unwrap(), 1);
    let old = Some(old_report().size);
    let restored = |index| {
        if OVERWRITTEN.contains(&index) {
            old
        } else {
            None
        }
    };
    assert_eq!(
        (sizes_at_target(&fs), holding(&fs)),
        ((0..FILES).map(restored).collect::<Vec<_>>(), Vec::new())
    );
    assert_eq!((held_rows(&journal), placed_rows(&journal)), (0, 0));
    assert!(lock(&journal).unfinished().unwrap().is_empty());
}
