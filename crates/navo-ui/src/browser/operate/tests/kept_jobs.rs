//! Files a job kept in place reach the jobs panel model, whatever the job's outcome.

use navo_core::FsError;
use navo_fs::{HeldFile, KeptCopy, KeptFile, KeptReason, KeptSource};

use super::super::LoggedOperation;
use super::*;

/// A log whose job ends with `outcome` after it kept `kept`.
struct KeptLog {
    outcome: JobOutcome,
    kept: Vec<KeptFile>,
}

impl JobLog for KeptLog {
    fn step(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn undone(&mut self, _: u32, _: StepOutcome) -> Result<(), FsError> {
        Ok(())
    }

    fn journal_held(&mut self, _: &HeldFile) -> Result<bool, FsError> {
        Ok(false)
    }
}

impl OpLog for KeptLog {
    fn execute(&mut self, _: LoggedOperation<'_>) -> Option<JobOutcome> {
        Some(self.outcome.clone())
    }

    fn finish(&mut self, _: &JobOutcome, _: &dyn ProviderOps) -> Result<(), String> {
        Ok(())
    }

    fn take_kept(&mut self) -> Vec<KeptFile> {
        std::mem::take(&mut self.kept)
    }
}

fn kept_source(fs: &FakeFs) -> KeptSource {
    KeptSource {
        seq: 1,
        location: fs.location("home/a.txt"),
        reason: KeptReason::Changed,
    }
}

fn kept(fs: &FakeFs) -> Vec<KeptFile> {
    vec![
        KeptFile::Source(kept_source(fs)),
        KeptFile::Copy(KeptCopy {
            seq: 0,
            location: fs.location("dest/a.txt"),
        }),
    ]
}

/// Runs a copy whose log ends with `outcome` after keeping `kept(fs)`.
fn ended(outcome: JobOutcome) -> (Arc<FakeFs>, Browser) {
    ended_keeping(outcome, kept)
}

/// Runs a copy whose log ends with `outcome` after keeping `keep(fs)`.
fn ended_keeping(
    outcome: JobOutcome,
    keep: fn(&FakeFs) -> Vec<KeptFile>,
) -> (Arc<FakeFs>, Browser) {
    let fs = fake();
    let mut browser = browser(&fs, home_beside_dest(&fs));
    let (outcome, kept) = (Mutex::new(Some(outcome)), keep(&fs));
    browser.set_operations(Operations {
        log_without_retention: None,
        retain_deletes: None,
        write: None,
        log: Arc::new(move |_: &OperationPlan| {
            let outcome = outcome.lock().unwrap().take().unwrap_or(JobOutcome::Done);
            let kept = kept.clone();
            Ok(Box::new(KeptLog { outcome, kept }) as Box<dyn OpLog>)
        }),
        protected: Vec::new(),
        retain_overwrites: None,
    });
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    (fs, browser)
}

#[test]
fn a_move_that_kept_sources_ends_kept_and_lists_every_kept_file() {
    let fs = fake();
    let sources = vec![kept_source(&fs)];
    let (fs, mut browser) = ended(JobOutcome::Kept { sources });
    let job = &browser.jobs()[0];
    let text = kept_source(&fs).error().message();
    assert_eq!(job.status, JobStatus::Kept(text));
    assert_eq!(job.report.kept, kept(&fs));
    assert!(
        browser.take_ended().is_empty(),
        "a kept job stays on the card"
    );
}

#[test]
fn a_cancelled_job_that_kept_files_ends_kept_not_cancelled() {
    let (fs, mut browser) = ended(JobOutcome::Cancelled);
    let job = &browser.jobs()[0];
    assert_eq!(job.status, JobStatus::Kept(kept(&fs)[0].error().message()));
    assert_eq!(job.report.kept, kept(&fs));
    assert!(browser.take_ended().is_empty());
}

#[test]
fn a_failed_job_keeps_its_error_and_lists_its_kept_files() {
    let error = FsError::new(FsErrorKind::DiskFull);
    let (fs, browser) = ended(JobOutcome::Failed { seq: 2, error });
    let job = &browser.jobs()[0];
    assert!(
        matches!(job.status, JobStatus::Failed(_)),
        "{:?}",
        job.status
    );
    assert_eq!(job.report.kept, kept(&fs));
}

#[test]
fn a_kept_job_survives_the_next_job() {
    let (_, mut browser) = ended(JobOutcome::Cancelled);
    let first = browser.jobs()[0].id;
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    browser.confirm();
    settle(&mut browser);
    assert!(browser.jobs().iter().any(|job| job.id == first));
}

#[test]
fn a_kept_outcome_without_kept_files_still_explains_itself() {
    let sources = Vec::new();
    let (_, browser) = ended_keeping(JobOutcome::Kept { sources }, |_| Vec::new());
    let status = &browser.jobs()[0].status;
    assert_eq!(
        *status,
        JobStatus::Kept(navo_l10n::tr("job-kept-unlisted")),
        "{status:?}"
    );
    assert!(!navo_l10n::tr("job-kept-unlisted").is_empty());
}
