use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use navo_core::retention::RetentionPolicy;
use navo_core::{OpKind, OperationPlan};
use navo_fs::FakeFs;

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
        undoes_overwrites: false,
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
    let sources = [fs.location("home/a.txt"), fs.location("work/b.txt")];
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
    crate::browser::operate::retain::delete_retention(fs, &plan, &sources, Some(&active))
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
