use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};

use navo_core::{OpKind, OperationPlan};
use navo_fs::FakeFs;

use super::{Record, SharedLog, browser, exists, fake, home_beside_dest, settle};
use crate::browser::Browser;
use crate::browser::operate::{JobStatus, KeepOld, LogFactory, LogRefusal, OpLog, Operations};

struct Rig {
    browser: Browser,
    asked: Arc<AtomicUsize>,
    runs: Arc<AtomicUsize>,
    record: Arc<Mutex<Record>>,
}

fn refusing(asked: &Arc<AtomicUsize>) -> LogFactory {
    let asked = Arc::clone(asked);
    Arc::new(move |_: &OperationPlan| {
        asked.fetch_add(1, Ordering::AcqRel);
        Err(LogRefusal::RetentionUnknown)
    })
}

fn rig(fs: &Arc<FakeFs>, without: Option<LogFactory>) -> Rig {
    let asked = Arc::new(AtomicUsize::new(0));
    let runs = Arc::new(AtomicUsize::new(0));
    let record = Arc::new(Mutex::new(Record::default()));
    let (shared, counted) = (Arc::clone(&record), Arc::clone(&runs));
    let without = without.unwrap_or_else(|| {
        Arc::new(move |_: &OperationPlan| {
            counted.fetch_add(1, Ordering::AcqRel);
            Ok(Box::new(SharedLog(Arc::clone(&shared))) as Box<dyn OpLog>)
        })
    });
    let mut browser = browser(fs, home_beside_dest(fs));
    browser.set_operations(Operations {
        log: refusing(&asked),
        log_without_retention: Some(without),
        ..Operations::unlogged()
    });
    Rig {
        browser,
        asked,
        runs,
        record,
    }
}

fn refused(rig: &mut Rig, kind: OpKind) {
    assert!(rig.browser.prepare(kind));
    settle(&mut rig.browser);
    assert!(rig.browser.confirm());
    settle(&mut rig.browser);
}

#[test]
fn unknown_retention_asks_before_anything_changes() {
    for (kind, path) in [(OpKind::Delete, "home/docs"), (OpKind::Copy, "dest/docs")] {
        let fs = fake();
        let mut rig = rig(&fs, None);
        refused(&mut rig, kind);
        let pending = rig.browser.pending().expect("the question is open");
        assert_eq!(pending.keep_old, KeepOld::Skip);
        assert_eq!(pending.plan.kind, kind);
        assert_eq!(pending.retain_deletes, None);
        assert!(rig.browser.jobs().is_empty(), "the refused job never ran");
        assert_eq!(rig.asked.load(Ordering::Acquire), 1);
        assert_eq!(rig.runs.load(Ordering::Acquire), 0);
        assert_eq!(exists(&fs, path), kind == OpKind::Delete);
    }
}

#[test]
fn yes_runs_the_job_once_without_retention() {
    let fs = fake();
    let mut rig = rig(&fs, None);
    refused(&mut rig, OpKind::Delete);
    assert!(rig.browser.confirm());
    settle(&mut rig.browser);
    assert!(rig.browser.pending().is_none());
    assert_eq!(rig.runs.load(Ordering::Acquire), 1);
    assert_eq!(rig.asked.load(Ordering::Acquire), 1);
    assert_eq!(rig.record.lock().unwrap().confirmed, vec![None]);
    assert_eq!(rig.browser.jobs()[0].status, JobStatus::Done);
    assert!(!exists(&fs, "home/docs"));

    assert!(rig.browser.prepare(OpKind::Copy));
    settle(&mut rig.browser);
    assert!(rig.browser.confirm());
    settle(&mut rig.browser);
    assert_eq!(
        rig.asked.load(Ordering::Acquire),
        2,
        "the choice was for one job"
    );
    assert_eq!(rig.runs.load(Ordering::Acquire), 1);
}

#[test]
fn no_leaves_everything_as_it_was() {
    let fs = fake();
    let mut rig = rig(&fs, None);
    refused(&mut rig, OpKind::Delete);
    assert!(rig.browser.dismiss());
    settle(&mut rig.browser);
    assert!(rig.browser.pending().is_none());
    assert!(rig.browser.jobs().is_empty());
    assert_eq!(rig.runs.load(Ordering::Acquire), 0);
    assert!(exists(&fs, "home/docs"));
}

#[test]
fn without_a_fallback_the_refusal_fails_the_job() {
    let fs = fake();
    let mut rig = rig(&fs, None);
    let asked = Arc::clone(&rig.asked);
    rig.browser.set_operations(Operations {
        log: refusing(&asked),
        ..Operations::unlogged()
    });
    refused(&mut rig, OpKind::Delete);
    assert!(rig.browser.pending().is_none());
    assert_eq!(
        rig.browser.jobs()[0].status,
        JobStatus::Failed(navo_l10n::tr("job-retention-probe-failed"))
    );
    assert!(exists(&fs, "home/docs"));
}

#[test]
fn a_job_already_without_retention_is_never_asked_again() {
    let fs = fake();
    let asked_again = Arc::new(AtomicUsize::new(0));
    let mut rig = rig(&fs, Some(refusing(&asked_again)));
    refused(&mut rig, OpKind::Delete);
    assert!(rig.browser.confirm());
    settle(&mut rig.browser);
    assert_eq!(asked_again.load(Ordering::Acquire), 1);
    assert!(rig.browser.pending().is_none());
    assert!(matches!(rig.browser.jobs()[0].status, JobStatus::Failed(_)));
    assert!(exists(&fs, "home/docs"));
}
