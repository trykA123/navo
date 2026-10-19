use std::sync::{Arc, Mutex};

use rusty_core::{ConflictPolicy, FsError, FsErrorKind, OpKind, OperationPlan, PlanRequest, plan};
use rusty_diag::{EventRing, Identity, Redactor, Salt};
use rusty_fs::{CancelToken, FakeFs, JobOutcome, scan};
use rusty_store::{Db, JobJournal};
use tracing_subscriber::Registry;
use tracing_subscriber::layer::SubscriberExt;

use super::log_factory;
use crate::logging::{log_config, open};

fn fake() -> FakeFs {
    let fs = FakeFs::new();
    fs.add_folder("home/secret-docs");
    fs.add_file("home/secret-docs/payroll.txt", 7);
    fs.add_folder("dest");
    fs
}

fn copy_plan(fs: &FakeFs) -> OperationPlan {
    let items = scan(fs, &[fs.location("home/secret-docs")], &CancelToken::new()).unwrap();
    plan(&PlanRequest {
        kind: OpKind::Copy,
        items,
        target: Some(fs.location("dest")),
        existing: Vec::new(),
        policy: ConflictPolicy::Ask,
        same_volume: true,
        protected: Vec::new(),
    })
}

fn finish_and_capture(outcome: &JobOutcome) -> String {
    let fs = fake();
    let journal = Arc::new(Mutex::new(
        JobJournal::new(Db::open_in_memory().unwrap()).unwrap(),
    ));
    let redactor = Arc::new(Redactor::new(
        Salt::from_bytes([3; 16]),
        Identity::default(),
    ));
    let ring = Arc::new(EventRing::new(EventRing::DEFAULT_BUDGET));
    let opened = open(log_config(None), &redactor, &ring);
    let subscriber = Registry::default().with(opened.layer);
    tracing::subscriber::with_default(subscriber, || {
        let mut log = log_factory(journal)(&copy_plan(&fs)).unwrap();
        log.finish(outcome, &fs);
    });
    ring.snapshot().join("\n")
}

#[test]
fn a_finished_job_is_logged_with_its_kind_and_state() {
    let line = finish_and_capture(&JobOutcome::Done);
    assert!(line.contains(r#""msg":"job finished""#), "{line}");
    assert!(line.contains(r#""kind":"copy""#), "{line}");
    assert!(line.contains(r#""state":"Done""#), "{line}");
}

#[test]
fn a_failed_job_is_logged_without_its_paths() {
    let fs = fake();
    let error = FsError {
        location: Some(fs.location("home/secret-docs/payroll.txt")),
        os_code: Some(2),
        ..FsError::new(FsErrorKind::NotFound)
    };
    let line = finish_and_capture(&JobOutcome::Failed { seq: 1, error });
    assert!(line.contains(r#""msg":"job failed""#), "{line}");
    assert!(line.contains("NotFound"), "{line}");
    assert!(
        !line.contains("payroll") && !line.contains("secret"),
        "{line}"
    );
}
