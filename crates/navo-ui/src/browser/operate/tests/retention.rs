use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use navo_core::retention::RetentionPolicy;
use navo_core::{ConflictPolicy, OpKind, OperationPlan};
use navo_fs::FakeFs;

use super::super::OpRequest;
use super::{Record, SharedLog, browser, exists, fake, home_beside_dest, settle};
use crate::browser::Browser;
use crate::browser::operate::{NoKeep, OpLog, Operations};

fn retaining(fs: &Arc<FakeFs>, retaining: &Arc<AtomicBool>) -> (Browser, Arc<Mutex<Record>>) {
    let record = Arc::new(Mutex::new(Record::default()));
    let shared = Arc::clone(&record);
    let setting = Arc::clone(retaining);
    let mut browser = browser(fs, home_beside_dest(fs));
    browser.set_operations(Operations {
        log_without_retention: None,
        retain_deletes: Some(Arc::new(move || {
            setting
                .load(Ordering::Acquire)
                .then(RetentionPolicy::default)
        })),
        write: None,
        log: Arc::new(move |_: &OperationPlan| {
            Ok(Box::new(SharedLog(Arc::clone(&shared))) as Box<dyn OpLog>)
        }),
        protected: Vec::new(),
        retain_overwrites: None,
    });
    (browser, record)
}

#[test]
fn a_delete_runs_with_the_retention_policy_the_user_confirmed() {
    let fs = fake();
    let setting = Arc::new(AtomicBool::new(true));
    let (mut browser, record) = retaining(&fs, &setting);
    browser.prepare(OpKind::Delete);
    settle(&mut browser);
    let pending = browser.pending().unwrap();
    let confirmed = pending.retain_deletes;
    assert_eq!(confirmed, Some(RetentionPolicy::default()));
    assert_eq!(pending.cannot_retain, None);
    setting.store(false, Ordering::Release);
    browser.confirm();
    settle(&mut browser);
    assert_eq!(record.lock().unwrap().confirmed, vec![confirmed]);
    assert!(!exists(&fs, "home/docs"));
}

#[test]
fn a_drive_without_file_identity_plans_a_permanent_delete() {
    let fs = fake();
    fs.set_volume_serial("home", None);
    let (mut browser, record) = retaining(&fs, &Arc::new(AtomicBool::new(true)));
    browser.prepare(OpKind::Delete);
    settle(&mut browser);
    let pending = browser.pending().unwrap();
    assert_eq!(pending.cannot_retain, Some(NoKeep::Drive));
    assert_eq!(pending.retain_deletes, None);
    browser.confirm();
    settle(&mut browser);
    assert_eq!(record.lock().unwrap().confirmed, vec![None]);
    assert!(!exists(&fs, "home/docs"));
}

#[test]
fn cancelling_a_delete_that_cannot_retain_creates_no_job() {
    let fs = fake();
    fs.set_volume_serial("home", None);
    let (mut browser, record) = retaining(&fs, &Arc::new(AtomicBool::new(true)));
    browser.prepare(OpKind::Delete);
    settle(&mut browser);
    assert!(
        browser
            .pending()
            .is_some_and(|pending| pending.cannot_retain == Some(NoKeep::Drive))
    );
    assert!(browser.dismiss());
    settle(&mut browser);
    assert!(browser.jobs().is_empty());
    assert!(record.lock().unwrap().confirmed.is_empty());
    assert!(exists(&fs, "home/docs"));
}

#[test]
fn retention_turned_off_is_not_reported_as_a_drive_limit() {
    let fs = fake();
    fs.set_volume_serial("home", None);
    let (mut browser, _) = retaining(&fs, &Arc::new(AtomicBool::new(false)));
    browser.prepare(OpKind::Delete);
    settle(&mut browser);
    assert!(
        browser
            .pending()
            .is_some_and(|pending| pending.cannot_retain.is_none())
    );
}

#[test]
fn a_delete_on_a_full_drive_runs_without_keeping_and_warns() {
    let fs = fake();
    fs.set_available_space("home", Some(0));
    let (mut browser, record) = retaining(&fs, &Arc::new(AtomicBool::new(true)));
    browser.prepare(OpKind::Delete);
    settle(&mut browser);
    let pending = browser.pending().unwrap();
    assert_eq!(pending.cannot_retain, Some(NoKeep::LowSpace));
    assert_eq!(pending.retain_deletes, None);
    browser.confirm();
    settle(&mut browser);
    assert_eq!(record.lock().unwrap().confirmed, vec![None]);
    assert!(!exists(&fs, "home/docs"));
}

/// Plans a delete of `home/a.txt` and `work/b.txt` with the safety net on.
fn two_drive_delete(fs: &FakeFs) -> (Option<RetentionPolicy>, Option<NoKeep>) {
    let state = navo_core::AppState::new(fs.location("home"));
    let request = OpRequest {
        id: 1,
        kind: OpKind::Delete,
        sources: vec![fs.location("home/a.txt"), fs.location("work/b.txt")],
        target: None,
        same_volume: false,
        protected: Vec::new(),
        source_tab: state.active_tab().id(),
        asks: true,
        structure: None,
        stream: None,
        policy: ConflictPolicy::Ask,
    };
    let plan = OperationPlan {
        kind: OpKind::Delete,
        steps: Vec::new(),
        conflicts: Vec::new(),
        risks: Vec::new(),
        totals: navo_core::Totals {
            files: 0,
            folders: 0,
            bytes: 0,
        },
    };
    let active: crate::browser::operate::RetainDeletes =
        Arc::new(|| Some(RetentionPolicy::default()));
    crate::browser::operate::retain::drive_retention(fs, &plan, &request, Some(&active))
}

#[test]
fn a_delete_with_one_source_on_a_full_or_unsupported_drive_keeps_nothing() {
    let fs = fake();
    fs.add_file("work/b.txt", 2);
    let kept = Some(RetentionPolicy::default());
    assert_eq!(two_drive_delete(&fs), (kept, None));
    fs.set_available_space("work", Some(0));
    assert_eq!(two_drive_delete(&fs), (None, Some(NoKeep::LowSpace)));
    fs.set_volume_serial("work", None);
    assert_eq!(two_drive_delete(&fs), (None, Some(NoKeep::Drive)));
}

/// Opens a copy of `home/docs` into `dest`, where `report.txt` is already there, with retention
/// on; returns why, if at all, the dialog was told the target drive cannot keep the files it replaces.
fn copy_over_twin(fs: &Arc<FakeFs>) -> Option<NoKeep> {
    fs.add_folder("dest/docs");
    fs.add_file("dest/docs/report.txt", 1);
    let (mut browser, _) = retaining(fs, &Arc::new(AtomicBool::new(true)));
    browser.prepare(OpKind::Copy);
    settle(&mut browser);
    let pending = browser.pending().expect("a pending copy");
    assert_eq!(pending.plan.conflicts.len(), 1);
    pending.cannot_retain
}

#[test]
fn a_copy_into_a_drive_without_file_identity_is_told_it_cannot_keep_replaced_files() {
    let fs = fake();
    fs.set_volume_serial("dest", None);
    assert_eq!(copy_over_twin(&fs), Some(NoKeep::Drive));
}

#[test]
fn a_copy_into_a_retaining_drive_is_not_warned_even_when_the_source_cannot_retain() {
    let fs = fake();
    fs.set_volume_serial("home", None);
    assert_eq!(copy_over_twin(&fs), None);
}
