use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusty_core::retention::RetentionPolicy;
use rusty_core::{OpKind, OperationPlan};
use rusty_fs::FakeFs;

use super::{Record, SharedLog, browser, exists, fake, home_beside_dest, settle};
use crate::browser::Browser;
use crate::browser::operate::{OpLog, Operations};

fn retaining(fs: &Arc<FakeFs>, retaining: &Arc<AtomicBool>) -> (Browser, Arc<Mutex<Record>>) {
    let record = Arc::new(Mutex::new(Record::default()));
    let shared = Arc::clone(&record);
    let setting = Arc::clone(retaining);
    let mut browser = browser(fs, home_beside_dest(fs));
    browser.set_operations(Operations {
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
    assert!(!pending.cannot_retain);
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
    assert!(pending.cannot_retain);
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
            .is_some_and(|pending| pending.cannot_retain)
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
            .is_some_and(|pending| !pending.cannot_retain)
    );
}
