use std::sync::Arc;

use rusty_core::{FsError, FsErrorKind, OpKind, OperationPlan, ProviderPath, Step, Totals};
use rusty_fs::{
    Attributes, CancelToken, EntryKind, FakeFs, FsEntry, HeldFile, JobContext, JobLog, JobOutcome,
    Provider, StepOutcome, run_job,
};
use rusty_ui::browser::OpLog;

use super::held_tests::journal;
use super::{SharedJournal, lock, log_factory, recover};

fn sources() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("dst");
    fs.add_file("src/a.txt", 8);
    fs.add_file("src/b.txt", 5);
    fs
}

fn link_plan(fs: &FakeFs) -> OperationPlan {
    OperationPlan {
        kind: OpKind::Link,
        steps: ["a.txt", "b.txt"]
            .iter()
            .map(|name| Step::MakeLink {
                target_of: fs.location(&format!("src/{name}")),
                at: fs.location(&format!("dst/{name}")),
            })
            .collect(),
        totals: Totals {
            files: 2,
            folders: 0,
            bytes: 0,
        },
        conflicts: Vec::new(),
        risks: Vec::new(),
    }
}

fn kind_of(fs: &FakeFs, path: &str) -> Option<EntryKind> {
    fs.metadata(&fs.location(path)).ok().map(|entry| entry.kind)
}

struct LostWrite {
    inner: Box<dyn OpLog>,
    at: u32,
}

impl JobLog for LostWrite {
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        if seq == self.at {
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
}

fn run_losing(fs: &FakeFs, plan: &OperationPlan, journal: &SharedJournal, at: u32) -> JobOutcome {
    let mut log = LostWrite {
        inner: log_factory(Arc::clone(journal))(plan).unwrap(),
        at,
    };
    run_job(
        plan,
        &[],
        &mut JobContext {
            ops: fs,
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    )
}

#[test]
fn a_crash_after_a_link_before_its_journal_write_is_removed_at_start() {
    let fs = sources();
    let plan = link_plan(&fs);
    let journal = journal();
    let outcome = run_losing(&fs, &plan, &journal, 1);
    assert!(matches!(outcome, JobOutcome::Failed { seq: 1, .. }));
    assert_eq!(kind_of(&fs, "dst/b.txt"), Some(EntryKind::Link));
    recover(&journal, &fs).unwrap();
    assert_eq!(
        (kind_of(&fs, "dst/a.txt"), kind_of(&fs, "dst/b.txt")),
        (None, None)
    );
    assert_eq!(
        (kind_of(&fs, "src/a.txt"), kind_of(&fs, "src/b.txt")),
        (Some(EntryKind::File), Some(EntryKind::File))
    );
    assert!(lock(&journal).unfinished().unwrap().is_empty());
}

#[test]
fn recovery_never_removes_a_link_that_beat_the_job_to_its_name() {
    let fs = sources();
    let plan = link_plan(&fs);
    fs.add_entry(
        "dst",
        FsEntry {
            name: ProviderPath::from_text("b.txt"),
            kind: EntryKind::Link,
            size: 0,
            modified_ms: None,
            attributes: Attributes::default(),
        },
    );
    let journal = journal();
    let outcome = run_losing(&fs, &plan, &journal, u32::MAX);
    assert!(matches!(outcome, JobOutcome::Failed { seq: 1, .. }));
    recover(&journal, &fs).unwrap();
    assert_eq!(kind_of(&fs, "dst/a.txt"), None);
    assert_eq!(kind_of(&fs, "dst/b.txt"), Some(EntryKind::Link));
}
