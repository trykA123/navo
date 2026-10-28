use rusty_core::retention::RetentionPolicy;

use super::*;

const MIB: u64 = 1024 * 1024;
const SMALL: &str = "[retention]\nretain-deletes = true\nmax-volume-mib = 1\n";

fn checked(
    settings: &str,
    confirmed: Option<RetentionPolicy>,
) -> (FakeFs, Result<(), String>, StoreDirs, tempfile::TempDir) {
    checked_sized(settings, confirmed, 8)
}

fn policy_of(settings: &str) -> Option<RetentionPolicy> {
    let (_profile, dirs, _) = setup();
    std::fs::write(dirs.settings_file(), settings).unwrap();
    super::super::super::retains_deletes(&dirs)
}

fn checked_sized(
    settings: &str,
    confirmed: Option<RetentionPolicy>,
    size: u64,
) -> (FakeFs, Result<(), String>, StoreDirs, tempfile::TempDir) {
    let (profile, dirs, journal) = setup();
    std::fs::write(dirs.settings_file(), settings).unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", size);
    let plan = delete(&fs, &[fs.location("home/a")]);
    let mut log = super::super::super::experimental_factory(journal, dirs.clone())(&plan).unwrap();
    let result = log.check(LoggedOperation {
        plan: &plan,
        ops: &fs,
        cancel: &CancelToken::new(),
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: confirmed,
    });
    if result.is_err() {
        log.finish(
            &JobOutcome::Failed {
                seq: 0,
                error: rusty_core::FsError::new(rusty_core::FsErrorKind::AccessDenied),
            },
            &fs,
        )
        .unwrap();
    }
    (fs, result, dirs, profile)
}

#[test]
fn a_delete_confirmed_as_permanent_is_refused_when_retention_turned_on() {
    let (fs, result, dirs, _profile) = checked("[retention]\nretain-deletes = true\n", None);
    assert!(result.is_err());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
    assert!(entries(&dirs).is_empty());
}

#[test]
fn a_delete_confirmed_as_retained_is_refused_when_retention_turned_off() {
    let confirmed = Some(RetentionPolicy::default());
    let (fs, result, dirs, _profile) = checked("[retention]\nretain-deletes = false\n", confirmed);
    assert!(result.is_err());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
    assert!(entries(&dirs).is_empty());
}

#[test]
fn a_delete_whose_retention_matches_the_confirmation_runs() {
    let confirmed = Some(RetentionPolicy::default());
    let (_, result, _, _profile) = checked("[retention]\nretain-deletes = true\n", confirmed);
    assert_eq!(result, Ok(()));
    let (_, result, _, _profile) = checked("[retention]\nretain-deletes = false\n", None);
    assert_eq!(result, Ok(()));
}

#[test]
fn an_oversized_delete_confirmed_as_permanent_runs_and_deletes_permanently() {
    let (profile, dirs, journal) = setup();
    std::fs::write(dirs.settings_file(), SMALL).unwrap();
    let fs = FakeFs::new();
    fs.add_file("home/a", 2 * MIB);
    let plan = delete(&fs, &[fs.location("home/a")]);
    let confirmed = super::super::super::retains_deletes(&dirs);
    assert!(confirmed.is_some());
    let mut log = super::super::super::experimental_factory(journal, dirs.clone())(&plan).unwrap();
    let cancel = CancelToken::new();
    log.check(LoggedOperation {
        plan: &plan,
        ops: &fs,
        cancel: &cancel,
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: confirmed,
    })
    .unwrap();
    let retained = log.execute(LoggedOperation {
        plan: &plan,
        ops: &fs,
        cancel: &cancel,
        progress: &mut |_| {},
        waiting: &mut |_| {},
        retain_deletes: confirmed,
    });
    assert!(retained.is_none());
    let outcome = rusty_fs::run_job(
        &plan,
        &[],
        &mut rusty_fs::JobContext {
            ops: &fs,
            log: log.as_mut(),
            cancel: &cancel,
            progress: &mut |_| {},
        },
    );
    assert_eq!(outcome, JobOutcome::Done);
    log.finish(&outcome, &fs).unwrap();
    assert!(fs.metadata(&fs.location("home/a")).is_err());
    assert!(entries(&dirs).is_empty());
    drop(profile);
}

#[test]
fn an_oversized_delete_is_refused_when_the_capacity_grew_to_fit_it() {
    let confirmed = policy_of(SMALL);
    let larger = "[retention]\nretain-deletes = true\nmax-volume-mib = 4\n";
    let (fs, result, dirs, _profile) = checked_sized(larger, confirmed, 2 * MIB);
    assert!(result.is_err());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
    assert!(entries(&dirs).is_empty());
}

#[test]
fn an_oversized_delete_is_refused_when_retention_turned_off() {
    let confirmed = policy_of(SMALL);
    let off = "[retention]\nretain-deletes = false\nmax-volume-mib = 1\n";
    let (fs, result, dirs, _profile) = checked_sized(off, confirmed, 2 * MIB);
    assert!(result.is_err());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
    assert!(entries(&dirs).is_empty());
}

#[test]
fn a_retained_delete_is_refused_when_the_capacity_shrank_below_it() {
    let confirmed = policy_of("[retention]\nretain-deletes = true\nmax-volume-mib = 4\n");
    let (fs, result, dirs, _profile) = checked_sized(SMALL, confirmed, 2 * MIB);
    assert!(result.is_err());
    assert!(fs.metadata(&fs.location("home/a")).is_ok());
    assert!(entries(&dirs).is_empty());
}
