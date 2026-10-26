use super::*;

struct LostWrite {
    inner: Box<dyn OpLog>,
    at: u32,
}

impl JobLog for LostWrite {
    fn prepare_rename(
        &mut self,
        seq: u32,
        identity: rusty_fs::FileIdentity,
    ) -> Result<(), FsError> {
        self.inner.prepare_rename(seq, identity)
    }

    fn rename_identity(&self, seq: u32) -> Result<Option<rusty_fs::FileIdentity>, FsError> {
        self.inner.rename_identity(seq)
    }

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

pub(super) fn crash_before_the_journal_write(
    fs: &FakeFs,
    plan: &OperationPlan,
    journal: &SharedJournal,
) {
    let mut log = LostWrite {
        inner: log_factory(Arc::clone(journal))(plan).unwrap(),
        at: 1,
    };
    let outcome = run_job(
        plan,
        &[],
        &mut JobContext {
            ops: fs,
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
    );
    assert!(matches!(outcome, JobOutcome::Failed { seq: 1, .. }));
}
