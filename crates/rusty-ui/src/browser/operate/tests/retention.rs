use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use rusty_core::retention::RetentionPolicy;
use rusty_core::{OpKind, OperationPlan};

use super::{Record, SharedLog, browser, exists, fake, home_beside_dest, settle};
use crate::browser::operate::{OpLog, Operations};

#[test]
fn a_delete_runs_with_the_retention_policy_the_user_confirmed() {
    let fs = fake();
    let record = Arc::new(Mutex::new(Record::default()));
    let shared = Arc::clone(&record);
    let retaining = Arc::new(AtomicBool::new(true));
    let setting = Arc::clone(&retaining);
    let mut browser = browser(&fs, home_beside_dest(&fs));
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
    browser.prepare(OpKind::Delete);
    settle(&mut browser);
    let confirmed = browser.pending().and_then(|pending| pending.retain_deletes);
    assert_eq!(confirmed, Some(RetentionPolicy::default()));
    retaining.store(false, Ordering::Release);
    browser.confirm();
    settle(&mut browser);
    assert_eq!(record.lock().unwrap().confirmed, vec![confirmed]);
    assert!(!exists(&fs, "home/docs"));
}
