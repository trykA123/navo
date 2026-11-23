use std::sync::Arc;

use navo_core::retention::{RETAINED_DELETE_KIND, RetentionPolicy};
use navo_ui::browser::LogRefusal;

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
    let outcome = navo_fs::run_job(
        &plan,
        &[],
        &mut navo_fs::JobContext {
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

/// Runs a confirmed delete to its end and checks that it finished.
fn run(log: &mut dyn OpLog, plan: &OperationPlan, fs: &FakeFs) {
    let cancel = CancelToken::new();
    let outcome = navo_fs::run_job(
        plan,
        &[],
        &mut navo_fs::JobContext {
            ops: fs,
            log,
            cancel: &cancel,
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&outcome, fs).unwrap();
}

#[test]
fn a_delete_on_a_full_drive_runs_without_a_kept_copy() {
    let fs = drive(true);
    fs.set_available_space("home", Some(0));
    let (mut log, plan, dirs, _profile) = logged(&fs);
    assert_eq!(latest_kind(&dirs), "delete");
    check(log.as_mut(), &plan, &fs, None).unwrap();
    run(log.as_mut(), &plan, &fs);
    assert!(fs.metadata(&fs.location("home/a")).is_err());
    assert!(entries(&dirs).is_empty());
}

#[test]
fn a_kept_delete_confirmed_before_the_drive_filled_is_refused() {
    let fs = drive(true);
    fs.set_available_space("home", Some(0));
    let (mut log, plan, dirs, _profile) = logged(&fs);
    let confirmed = Some(RetentionPolicy::default());
    assert!(check(log.as_mut(), &plan, &fs, confirmed).is_err());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
    assert!(entries(&dirs).is_empty());
}

#[test]
fn a_delete_with_one_source_on_a_full_drive_keeps_nothing() {
    let fs = drive(true);
    fs.add_file("work/b", 8);
    fs.set_available_space("work", Some(0));
    let (_profile, dirs, journal) = setup();
    let plan = delete(fs.as_ref(), &[fs.location("home/a"), fs.location("work/b")]);
    let ops: Arc<dyn ProviderOps> = fs.clone();
    let mut log = crate::jobs::retaining_factory(journal, dirs.clone(), ops)(&plan).unwrap();
    check(log.as_mut(), &plan, &fs, None).unwrap();
    run(log.as_mut(), &plan, &fs);
    assert!(fs.metadata(&fs.location("work/b")).is_err());
    assert!(entries(&dirs).is_empty());
}

#[test]
fn a_full_drive_counts_even_after_a_large_selected_folder() {
    let fs = drive(true);
    fs.add_file("work/b.txt", 8);
    fs.set_available_space("work", Some(0));
    for n in 0..20 {
        fs.add_file(&format!("home/docs/f{n}"), 8);
    }
    let (_profile, dirs, journal) = setup();
    let sources = [fs.location("work/b.txt"), fs.location("home/docs")];
    let plan = delete(fs.as_ref(), &sources);
    let ops: Arc<dyn ProviderOps> = fs.clone();
    let mut log = crate::jobs::retaining_factory(journal, dirs.clone(), ops)(&plan).unwrap();
    check(log.as_mut(), &plan, &fs, None).unwrap();
    run(log.as_mut(), &plan, &fs);
    assert!(fs.metadata(&fs.location("home/docs")).is_err());
    assert!(entries(&dirs).is_empty());
}

/// Plans a delete of `files` files in each of `folders` folders under `home`, and asks the real
/// factory for its log; the temp profile is returned so it outlives the log.
fn factory_on_folders(folders: usize, files: usize) -> Factored {
    let fs = drive(true);
    let sources: Vec<_> = (0..folders)
        .flat_map(|d| (0..files).map(move |f| format!("home/d{d}/f{f}")))
        .map(|path| {
            fs.add_file(&path, 8);
            fs.location(&path)
        })
        .collect();
    let (profile, dirs, journal) = setup();
    let plan = delete(fs.as_ref(), &sources);
    let ops: Arc<dyn ProviderOps> = fs.clone();
    let made = crate::jobs::retaining_factory(journal, dirs.clone(), ops)(&plan);
    (fs, plan, dirs, made, profile)
}

type Factored = (
    Arc<FakeFs>,
    OperationPlan,
    StoreDirs,
    Result<Box<dyn OpLog>, LogRefusal>,
    tempfile::TempDir,
);

#[test]
fn a_delete_from_many_folders_on_a_healthy_drive_keeps_a_copy() {
    let (fs, plan, dirs, made, _profile) = factory_on_folders(17, 1);
    let mut log = made.unwrap();
    assert_eq!(latest_kind(&dirs), RETAINED_DELETE_KIND);
    let confirmed = Some(RetentionPolicy::default());
    assert_eq!(check(log.as_mut(), &plan, &fs, confirmed), Ok(()));
}

#[test]
fn many_selected_files_in_one_folder_are_checked_once() {
    let (_fs, _plan, _dirs, made, _profile) = factory_on_folders(1, 40);
    assert!(made.is_ok());
}
