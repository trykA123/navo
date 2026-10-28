use std::sync::Arc;

use rusty_core::retention::{RETAINED_DELETE_KIND, RetentionPolicy};

use super::*;

fn drive(identity: bool) -> Arc<FakeFs> {
    let fs = FakeFs::new();
    fs.add_file("home/a", 8);
    if !identity {
        fs.set_volume_serial("home", None);
    }
    Arc::new(fs)
}

fn latest_kind(dirs: &StoreDirs) -> String {
    let (db, _) = Db::open(&dirs.journal_file(), false).unwrap();
    db.connection()
        .query_row("SELECT kind FROM job ORDER BY id DESC LIMIT 1", [], |row| {
            row.get(0)
        })
        .unwrap()
}

fn logged(fs: &Arc<FakeFs>) -> (Box<dyn OpLog>, OperationPlan, StoreDirs, tempfile::TempDir) {
    let (profile, dirs, journal) = setup();
    let plan = delete(fs.as_ref(), &[fs.location("home/a")]);
    let ops: Arc<dyn ProviderOps> = fs.clone();
    let log = crate::jobs::retaining_factory(journal, dirs.clone(), ops)(&plan).unwrap();
    (log, plan, dirs, profile)
}

fn check(
    log: &mut dyn OpLog,
    plan: &OperationPlan,
    fs: &FakeFs,
    confirmed: Option<RetentionPolicy>,
) -> Result<(), String> {
    log.check(LoggedOperation {
        plan,
        ops: fs,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: confirmed,
    })
}

#[test]
fn a_drive_that_cannot_retain_journals_a_plain_permanent_delete() {
    let fs = drive(false);
    let (mut log, plan, dirs, _profile) = logged(&fs);
    assert_eq!(latest_kind(&dirs), "delete");
    check(log.as_mut(), &plan, &fs, None).unwrap();
    let cancel = CancelToken::new();
    let outcome = rusty_fs::run_job(
        &plan,
        &[],
        &mut rusty_fs::JobContext {
            ops: fs.as_ref(),
            log: log.as_mut(),
            cancel: &cancel,
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&outcome, fs.as_ref()).unwrap();
    assert!(fs.metadata(&fs.location("home/a")).is_err());
    assert!(entries(&dirs).is_empty());
}

#[test]
fn a_retained_confirmation_on_a_drive_that_cannot_retain_is_refused() {
    let fs = drive(false);
    let (mut log, plan, dirs, _profile) = logged(&fs);
    let confirmed = Some(RetentionPolicy::default());
    assert!(check(log.as_mut(), &plan, &fs, confirmed).is_err());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
    assert!(entries(&dirs).is_empty());
}

#[test]
fn a_drive_that_retains_still_journals_a_retained_delete() {
    let fs = drive(true);
    let (mut log, plan, dirs, _profile) = logged(&fs);
    assert_eq!(latest_kind(&dirs), RETAINED_DELETE_KIND);
    let confirmed = Some(RetentionPolicy::default());
    assert_eq!(check(log.as_mut(), &plan, &fs, confirmed), Ok(()));
}
