use rusty_core::{FsError, FsErrorKind};
use rusty_fs::{FileIdentity, FileStamp, HeldFile, JobLog, MoveProof, PlacedFile, StepOutcome};
use rusty_ui::browser::OpLog;

use super::super::{identity, still_pending};
use super::*;

struct LostDone {
    inner: Box<dyn OpLog>,
    at: u32,
}

impl JobLog for LostDone {
    fn preserve_overwrites(&self) -> bool {
        self.inner.preserve_overwrites()
    }

    fn prepare_move(&mut self, seq: u32, proof: MoveProof) -> Result<(), FsError> {
        self.inner.prepare_move(seq, proof)
    }

    fn rename_identity(&self, seq: u32) -> Result<Option<FileIdentity>, FsError> {
        self.inner.rename_identity(seq)
    }

    fn rename_stamp(&self, seq: u32) -> Result<Option<FileStamp>, FsError> {
        self.inner.rename_stamp(seq)
    }

    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if seq == self.at && outcome == StepOutcome::Done {
            return Err(FsError::new(FsErrorKind::Unknown));
        }
        self.inner.step(seq, outcome)
    }

    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.inner.undone(seq, outcome)
    }

    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.inner.journal_held(held)
    }

    fn journal_placed(&mut self, placed: &PlacedFile) -> Result<(), FsError> {
        self.inner.journal_placed(placed)
    }
}

fn crash_before_done(tree: &Tree, plan: &OperationPlan, (at, workers): (u32, usize)) {
    let mut log = LostDone {
        inner: log_factory(Arc::clone(&tree.journal))(plan).unwrap(),
        at,
    };
    let outcome = run_job_parallel(
        plan,
        &[],
        &mut JobContext {
            ops: &tree.fs,
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
        workers,
    );
    assert!(matches!(outcome, JobOutcome::Failed { .. }), "{outcome:?}");
}

fn prepared(tree: &Tree, names: &[&str]) -> (JobId, OperationPlan) {
    let plan = tree.plan(names, true, ConflictPolicy::Ask);
    let steps = u32::try_from(plan.steps.len()).unwrap();
    let bytes = serde_json::to_vec(&plan).unwrap();
    let job = lock(&tree.journal)
        .create_job("move", &bytes, steps, 1)
        .unwrap();
    for (seq, name) in (0_u32..).zip(names) {
        let source = tree.location(&format!("src/{name}"));
        let found = rusty_fs::ProviderOps::file_identity(&tree.fs, &source)
            .unwrap()
            .unwrap();
        let entry = rusty_fs::Provider::metadata(&tree.fs, &source).unwrap();
        let stamp = (entry.kind == rusty_fs::EntryKind::File).then_some(FileStamp {
            size: entry.size,
            modified_ms: entry.modified_ms,
        });
        identity::record_stamped(&tree.journal, job, (seq, found, stamp)).unwrap();
    }
    (job, plan)
}

fn job_number(job: JobId) -> u64 {
    u64::try_from(job.0).unwrap()
}

#[test]
fn a_move_renamed_before_its_done_write_is_put_back_at_start_from_either_runner() {
    for workers in [1, 4] {
        let tree = Tree::new(&FILES);
        let plan = tree.plan(&["a.txt", "b.txt", "sub"], true, ConflictPolicy::Ask);
        crash_before_done(&tree, &plan, (1, workers));
        assert_eq!(tree.read("dst/b.txt").as_deref(), Some("bravo!"));
        let tree = tree.reopened();
        recover(&tree.journal, &tree.fs).unwrap();
        assert_eq!(
            present(&tree, &["src/a.txt", "src/b.txt", "src/sub/c.txt"]),
            some(&["alpha", "bravo!", "charlie"]),
            "workers {workers}"
        );
        assert!(
            std::fs::read_dir(tree.path("dst"))
                .unwrap()
                .next()
                .is_none()
        );
        assert!(still_pending(&tree.journal).unwrap().is_empty());
    }
}

#[test]
fn a_prepared_move_that_never_ran_is_left_alone() {
    let tree = Tree::new(&FILES);
    let (job, _) = prepared(&tree, &["a.txt"]);
    let tree = tree.reopened();
    recover(&tree.journal, &tree.fs).unwrap();
    assert_eq!(tree.read("src/a.txt").as_deref(), Some("alpha"));
    assert_eq!(tree.state(job_number(job)), JobState::RolledBack);
}

#[test]
fn a_prepared_move_with_a_stranger_at_its_destination_is_left_and_reported() {
    let tree = Tree::new(&FILES);
    let (job, _) = prepared(&tree, &["a.txt", "b.txt"]);
    std::fs::rename(tree.path("src/a.txt"), tree.path("aside/a.txt")).unwrap();
    tree.write("dst/a.txt", "a stranger");
    std::fs::rename(tree.path("src/b.txt"), tree.path("dst/b.txt")).unwrap();
    let tree = tree.reopened();
    recover(&tree.journal, &tree.fs).unwrap();
    assert_eq!(
        present(&tree, &["dst/a.txt", "aside/a.txt", "src/b.txt"]),
        some(&["a stranger", "alpha", "bravo!"])
    );
    assert_eq!(tree.state(job_number(job)), JobState::RestorePending);
    let reported: Vec<JobId> = still_pending(&tree.journal)
        .unwrap()
        .iter()
        .map(|pending| pending.job)
        .collect();
    assert_eq!(reported, vec![job]);
}

#[test]
fn an_unfinished_move_from_an_old_journal_is_left_in_place_and_reported() {
    let tree = Tree::new(&FILES);
    let plan = tree.plan(&["a.txt"], true, ConflictPolicy::Ask);
    let bytes = serde_json::to_vec(&plan).unwrap();
    let job = lock(&tree.journal)
        .create_job("move", &bytes, 1, 1)
        .unwrap();
    std::fs::rename(tree.path("src/a.txt"), tree.path("dst/a.txt")).unwrap();
    lock(&tree.journal)
        .mark_step(job, 0, StepState::Done, None)
        .unwrap();
    let tree = tree.reopened();
    recover(&tree.journal, &tree.fs).unwrap();
    assert_eq!(
        (tree.read("dst/a.txt"), tree.read("src/a.txt")),
        (Some("alpha".to_owned()), None)
    );
    assert_eq!(tree.state(job_number(job)), JobState::RestorePending);
    assert_eq!(still_pending(&tree.journal).unwrap().len(), 1);
    assert!(tree.offer().is_none());
}

fn detail(tree: &Tree, job: JobId, seq: u32) -> Option<FileIdentity> {
    identity::read(&tree.journal, job, seq).unwrap()
}

fn reported(tree: &Tree) -> Vec<JobId> {
    still_pending(&tree.journal)
        .unwrap()
        .iter()
        .map(|pending| pending.job)
        .collect()
}

#[test]
fn a_pending_move_without_identity_found_at_its_destination_stays_there_and_is_reported() {
    let tree = Tree::new(&FILES);
    let plan = tree.plan(&["a.txt", "b.txt"], true, ConflictPolicy::Ask);
    let bytes = serde_json::to_vec(&plan).unwrap();
    let job = lock(&tree.journal)
        .create_job("move", &bytes, 2, 1)
        .unwrap();
    std::fs::rename(tree.path("src/a.txt"), tree.path("dst/a.txt")).unwrap();
    let tree = tree.reopened();
    recover(&tree.journal, &tree.fs).unwrap();
    assert_eq!(
        present(&tree, &["dst/a.txt", "src/b.txt"]),
        some(&["alpha", "bravo!"])
    );
    assert!(!tree.path("src/a.txt").exists() && !tree.path("dst/b.txt").exists());
    assert_eq!(tree.state(job_number(job)), JobState::RestorePending);
    assert_eq!(reported(&tree), vec![job]);
}

#[test]
fn a_pending_move_without_identity_that_never_ran_is_left_alone() {
    let tree = Tree::new(&FILES);
    let plan = tree.plan(&["a.txt"], true, ConflictPolicy::Ask);
    let bytes = serde_json::to_vec(&plan).unwrap();
    let job = lock(&tree.journal)
        .create_job("move", &bytes, 1, 1)
        .unwrap();
    let tree = tree.reopened();
    recover(&tree.journal, &tree.fs).unwrap();
    assert_eq!(tree.read("src/a.txt").as_deref(), Some("alpha"));
    assert_eq!(tree.state(job_number(job)), JobState::RolledBack);
}

#[test]
fn a_changed_prepared_file_and_an_occupied_origin_stay_pending_with_their_identity() {
    let tree = Tree::new(&FILES);
    let (job, _) = prepared(&tree, &["a.txt", "b.txt"]);
    std::fs::rename(tree.path("src/a.txt"), tree.path("dst/a.txt")).unwrap();
    std::fs::write(tree.path("dst/a.txt"), "alpha, edited").unwrap();
    std::fs::rename(tree.path("src/b.txt"), tree.path("dst/b.txt")).unwrap();
    tree.write("src/b.txt", "made later");
    let tree = tree.reopened();
    for _ in 0..2 {
        recover(&tree.journal, &tree.fs).unwrap();
        assert_eq!(
            present(&tree, &["dst/a.txt", "dst/b.txt", "src/b.txt"]),
            some(&["alpha, edited", "bravo!", "made later"])
        );
        assert!(!tree.path("src/a.txt").exists());
        assert_eq!(tree.state(job_number(job)), JobState::RestorePending);
        assert!(detail(&tree, job, 0).is_some() && detail(&tree, job, 1).is_some());
        assert_eq!(reported(&tree), vec![job]);
    }
}

#[test]
fn a_refused_restore_after_an_undo_claim_keeps_its_row_and_identity() {
    let tree = Tree::new(&FILES);
    assert_eq!(tree.moved(&["a.txt", "b.txt"], 1), JobOutcome::Done);
    let offer = tree.offer().unwrap();
    drop((tree.hooks().undo_log)(&offer).unwrap());
    std::fs::write(tree.path("dst/a.txt"), "alpha, edited").unwrap();
    tree.write("src/b.txt", "made later");
    let tree = tree.reopened();
    recover(&tree.journal, &tree.fs).unwrap();
    let job = JobId(i64::try_from(offer.job).unwrap());
    assert_eq!(
        present(&tree, &["dst/a.txt", "dst/b.txt", "src/b.txt"]),
        some(&["alpha, edited", "bravo!", "made later"])
    );
    assert_eq!(tree.state(offer.job), JobState::RestorePending);
    assert!(detail(&tree, job, 0).is_some() && detail(&tree, job, 1).is_some());
}

#[test]
fn a_file_whose_stamp_was_removed_is_left_in_place_after_an_edit() {
    let tree = Tree::new(&FILES);
    assert_eq!(tree.moved(&["a.txt"], 1), JobOutcome::Done);
    let offer = tree.offer().unwrap();
    let job = JobId(i64::try_from(offer.job).unwrap());
    let bytes = lock(&tree.journal).step_detail(job, 0).unwrap().unwrap();
    let mut record: serde_json::Value = serde_json::from_slice(&bytes).unwrap();
    assert!(record.as_object_mut().unwrap().remove("stamp").is_some());
    let stripped = serde_json::to_vec(&record).unwrap();
    lock(&tree.journal)
        .mark_step(job, 0, StepState::Done, Some(&stripped))
        .unwrap();
    drop((tree.hooks().undo_log)(&offer).unwrap());
    std::fs::write(tree.path("dst/a.txt"), "alpha, edited").unwrap();
    let tree = tree.reopened();
    recover(&tree.journal, &tree.fs).unwrap();
    assert_eq!(tree.read("dst/a.txt").as_deref(), Some("alpha, edited"));
    assert!(!tree.path("src/a.txt").exists());
    assert_eq!(tree.state(offer.job), JobState::RestorePending);
}
