//! Locked-source retry (NV6a): a source another program holds open is retried after 1, 2 and
//! 4 s on the fake clock, and only while nothing was written to the step's temporary file.

use std::time::Duration;

use super::*;

mod throttle;

const SOURCE: &str = "src/two.txt";
const WAITS: [Duration; 3] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(4),
];

/// Makes the first `times` copies of the second file fail with a sharing violation.
fn in_use(fs: &FakeFs, times: u32) {
    for call in 1..=times {
        let fault = Fault::Fail(FsErrorKind::SharingViolation);
        fs.fault(SOURCE, FakeOp::Copy, call, fault);
    }
}

#[test]
fn a_source_that_frees_on_the_second_try_copies() {
    for workers in MODES {
        let fs = three_files();
        let plan = copy_three(&fs);
        in_use(&fs, 1);
        let (outcome, _) = run_in(workers, &fs, &plan);
        assert_eq!(outcome, JobOutcome::Done, "{workers} workers");
        assert_eq!(size(&fs, "dst/two.txt"), Some(6_000), "{workers} workers");
        assert_eq!(fs.retry_waits(), WAITS[..1], "{workers} workers");
        assert!(leftovers(&fs).is_empty(), "{workers} workers");
    }
}

#[test]
fn a_source_that_stays_in_use_fails_after_three_retries() {
    for workers in MODES {
        let fs = three_files();
        let plan = copy_three(&fs);
        in_use(&fs, 5);
        let (outcome, harness) = run_in(workers, &fs, &plan);
        let run = (failure(&outcome), fs.calls(SOURCE, FakeOp::Copy));
        assert_eq!(
            run,
            (Some((1, FsErrorKind::SharingViolation)), 4),
            "{workers} workers"
        );
        assert_eq!(fs.retry_waits(), WAITS, "{workers} workers");
        assert!(!exists(&fs, "dst/two.txt"), "{workers} workers");
        assert!(leftovers(&fs).is_empty(), "{workers} workers");
        assert!(!harness.log.done().contains(&1), "{workers} workers");
    }
}

/// A strict (retaining) job's journal: every copy goes through a recorded temporary file.
#[derive(Default)]
struct Strict {
    log: Log,
    prepared: Vec<u32>,
    created: Vec<u32>,
}

impl JobLog for Strict {
    fn preserve_overwrites(&self) -> bool {
        true
    }
    fn temporary_namespace(&self) -> Option<String> {
        Some("ab12".to_owned())
    }
    fn prepare_temporary(
        &mut self,
        temp: &navo_core::retention::CopyTemporary,
    ) -> Result<(), FsError> {
        self.prepared.push(temp.seq);
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
    fn retire_temporary(&mut self, _: u32, _: &dyn ProviderOps) -> Result<(), FsError> {
        Ok(())
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

#[test]
fn a_strict_job_retries_before_its_temporary_exists() {
    for workers in MODES {
        let fs = three_files();
        let plan = copy_three(&fs);
        in_use(&fs, 2);
        let (cancel, mut log) = (CancelToken::new(), Strict::default());
        let mut ctx = JobContext {
            ops: &fs,
            log: &mut log,
            cancel: &cancel,
            progress: &mut |_: &Progress| {},
        };
        let outcome = match workers {
            0 => run_job(&plan, &[], &mut ctx),
            n => super::super::parallel::run(&plan, &[], &mut ctx, n),
        };
        assert_eq!(outcome, JobOutcome::Done, "{workers} workers");
        assert_eq!(size(&fs, "dst/two.txt"), Some(6_000), "{workers} workers");
        assert_eq!(fs.retry_waits(), WAITS[..2], "{workers} workers");
        let one_each = |seqs: &[u32]| seqs.iter().filter(|seq| **seq == 1).count() == 1;
        assert!(
            one_each(&log.prepared) && one_each(&log.created),
            "{workers} workers"
        );
        assert!(leftovers(&fs).is_empty(), "{workers} workers");
    }
}

#[test]
fn a_violation_after_bytes_reached_the_temporary_is_not_retried() {
    for workers in MODES {
        let fs = three_files();
        let plan = copy_three(&fs);
        let kind = FsErrorKind::SharingViolation;
        fs.fault(
            SOURCE,
            FakeOp::Copy,
            1,
            Fault::FailAfterBytes { kind, bytes: 5_000 },
        );
        let (outcome, _) = run_in(workers, &fs, &plan);
        assert_eq!(failure(&outcome), Some((1, kind)), "{workers} workers");
        assert!(fs.retry_waits().is_empty(), "{workers} workers");
        assert!(leftovers(&fs).is_empty(), "{workers} workers");
    }
}

#[test]
fn other_errors_on_the_source_are_not_retried() {
    for workers in MODES {
        let fs = three_files();
        let plan = copy_three(&fs);
        fs.fault(SOURCE, FakeOp::Copy, 1, Fault::Fail(FsErrorKind::Locked));
        let (outcome, _) = run_in(workers, &fs, &plan);
        assert_eq!(
            failure(&outcome),
            Some((1, FsErrorKind::Locked)),
            "{workers} workers"
        );
        assert!(fs.retry_waits().is_empty(), "{workers} workers");
    }
}

#[test]
fn cancel_during_a_retry_wait_stops_the_job_without_another_attempt() {
    for workers in MODES {
        let fs = three_files();
        let plan = copy_three(&fs);
        in_use(&fs, 5);
        let mut harness = Harness {
            workers,
            ..Harness::default()
        };
        fs.cancel_during_next_wait(harness.cancel.clone());
        let outcome = harness.run(&fs, &plan, &[]);
        assert_eq!(outcome, JobOutcome::Cancelled, "{workers} workers");
        assert_eq!(fs.calls(SOURCE, FakeOp::Copy), 1, "{workers} workers");
        assert_eq!(fs.retry_waits(), WAITS[..1], "{workers} workers");
        assert!(!exists(&fs, "dst/two.txt"), "{workers} workers");
        assert!(leftovers(&fs).is_empty(), "{workers} workers");
    }
}

/// A source on another provider that is always in use; it counts how often it was opened.
#[derive(Default)]
struct Busy {
    opens: std::sync::atomic::AtomicU32,
}

impl SourceOps for Busy {
    fn id(&self) -> navo_core::ProviderId {
        navo_core::ProviderId::new("mem").unwrap()
    }

    fn metadata(&self, location: &navo_core::Location) -> Result<SourceMetadata, FsError> {
        Ok(FsEntry {
            name: location.name().unwrap(),
            kind: EntryKind::File,
            size: 6_000,
            modified_ms: Some(1_000),
            attributes: Attributes::default(),
        }
        .into())
    }

    fn open_read(
        &self,
        location: &navo_core::Location,
    ) -> Result<Box<dyn std::io::Read + Send>, FsError> {
        self.opens.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
        Err(super::super::copy::failure(
            FsErrorKind::SharingViolation,
            location,
        ))
    }
}

#[test]
fn a_stream_from_another_provider_is_never_retried() {
    for workers in MODES {
        let fs = fixture(&[]);
        let source = navo_core::Location::new("mem:///far/a.bin").unwrap();
        let mut plan = copy_plan(&fs, &[], "dst");
        plan.steps = vec![Step::CopyFile {
            source,
            target: fs.location("dst/a.bin"),
            size: 6_000,
        }];
        plan.totals.files = 1;
        plan.totals.bytes = 6_000;
        let busy = std::sync::Arc::new(Busy::default());
        let sources = Sources::new().with(busy.clone());
        let mut harness = Harness::default();
        let outcome = harness.with(&fs, |ctx| match workers {
            0 => run_job_with_sources(&plan, &[], ctx, &sources),
            n => run_job_parallel_with_sources((&plan, &sources), &[], ctx, n),
        });
        let opens = busy.opens.load(std::sync::atomic::Ordering::SeqCst);
        let run = (failure(&outcome), opens);
        assert_eq!(
            run,
            (Some((0, FsErrorKind::SharingViolation)), 1),
            "{workers} workers"
        );
        assert!(fs.retry_waits().is_empty(), "{workers} workers");
        assert!(leftovers(&fs).is_empty(), "{workers} workers");
    }
}
