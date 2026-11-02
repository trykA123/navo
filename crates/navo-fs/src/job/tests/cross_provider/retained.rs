use super::*;

#[derive(Default)]
struct Retaining {
    log: Log,
    created: Vec<u32>,
    retired: Vec<u32>,
}

impl JobLog for Retaining {
    fn preserve_overwrites(&self) -> bool {
        true
    }
    fn temporary_namespace(&self) -> Option<String> {
        Some("ab12".to_owned())
    }
    fn prepare_temporary(
        &mut self,
        _: &navo_core::retention::CopyTemporary,
    ) -> Result<(), FsError> {
        Ok(())
    }
    fn created_temporary(
        &mut self,
        (seq, _): (u32, crate::FileIdentity),
        _: &dyn ProviderOps,
    ) -> Result<(), FsError> {
        self.created.push(seq);
        Ok(())
    }
    fn retire_temporary(&mut self, seq: u32, _: &dyn ProviderOps) -> Result<(), FsError> {
        self.retired.push(seq);
        Ok(())
    }
    fn journal_proved_held(
        &mut self,
        held: &HeldFile,
        _: &navo_core::retention::OverwriteProof,
    ) -> Result<bool, FsError> {
        self.log.journal_held(held)
    }
    fn step(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.log.step(seq, outcome)
    }
    fn undone(&mut self, seq: u32, outcome: StepOutcome) -> Result<(), FsError> {
        self.log.undone(seq, outcome)
    }
    fn journal_held(&mut self, held: &HeldFile) -> Result<bool, FsError> {
        self.log.journal_held(held)
    }
}

#[cfg(unix)]
#[test]
fn a_retaining_overwrite_from_another_provider_reports_its_temporary_and_holds_the_old_file() {
    let scene = Scene::new(Feed::Whole, Some(OLD));
    let mut log = Retaining::default();
    let outcome = run_job_with_sources(
        &scene.plan,
        &[],
        &mut JobContext {
            ops: &LocalFs::new(),
            log: &mut log,
            cancel: &CancelToken::new(),
            progress: &mut |_| {},
        },
        &scene.sources,
    );
    assert_eq!(outcome, JobOutcome::Done);
    assert_eq!(scene.target(), pattern());
    assert_eq!((log.created, log.retired), (vec![0], vec![0]));
    let [held] = log.log.held.as_slice() else {
        panic!("one held file");
    };
    assert_eq!(
        std::fs::read(crate::os_path(&held.held).unwrap()).unwrap(),
        OLD
    );
}
