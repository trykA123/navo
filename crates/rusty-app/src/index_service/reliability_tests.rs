use super::bind_retry::{bind_with_retry, retry_delay};
use super::config::require_user_sid;
use super::directory_policy::{DirectoryAction, Found, directory_action};
use super::install_plan::{CopyPlan, Step, clear_directory, data_directories, run_install};
use rusty_index::pipe::CancelToken;
use std::{io, path::Path, time::Duration};

fn record(exists: bool, failed: Option<Step>) -> (Vec<Step>, io::Result<()>) {
    let mut calls = Vec::new();
    let result = run_install(exists, true, |step| {
        calls.push(step);
        match failed == Some(step) && calls.iter().filter(|&&call| call == step).count() == 1 {
            true => Err(io::Error::other("original failure")),
            false => Ok(()),
        }
    });
    (calls, result)
}

#[test]
fn reinstall_cleans_leftovers_then_deletes_the_old_image_after_start() {
    use Step::*;
    let (calls, result) = record(true, None);
    result.unwrap();
    assert_eq!(
        calls,
        [
            RecoverOld,
            DeleteOld,
            Copy,
            RenameOld,
            Replace,
            Stop,
            BackupSettings,
            Configure,
            Start,
            DeleteOld
        ]
    );
}

#[test]
fn a_failed_reinstall_restores_the_image_and_starts_the_service_again() {
    use Step::*;
    let steps = [
        Copy,
        RenameOld,
        Replace,
        Stop,
        BackupSettings,
        Configure,
        Start,
    ];
    for (end, failed) in steps.into_iter().enumerate() {
        let (calls, result) = record(true, Some(failed));
        assert_eq!(result.unwrap_err().to_string(), "original failure");
        assert_eq!(&calls[..2], &[RecoverOld, DeleteOld]);
        assert_eq!(&calls[2..=end + 2], &steps[..=end]);
        let rollback: &[Step] = match failed {
            Copy => &[Start],
            RenameOld => &[DiscardCopy, Start],
            Replace => &[Restore, Start],
            Stop | BackupSettings => &[Stop, Restore, RestoreConfiguration, Start],
            _ => &[Stop, RestoreSettings, Restore, RestoreConfiguration, Start],
        };
        assert_eq!(&calls[end + 3..], rollback, "after {failed:?}");
    }
}

#[test]
fn a_failed_first_install_restores_but_never_starts() {
    let (calls, result) = record(false, Some(Step::Configure));
    assert!(result.is_err());
    assert_eq!(calls.last(), Some(&Step::RemoveFresh));
    assert!(!calls.contains(&Step::Start));
}

#[test]
fn replacement_failure_restores_the_previous_executable() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 123);
    std::fs::create_dir_all(&plan.directory).unwrap();
    std::fs::write(&plan.executable, b"old").unwrap();
    plan.rename_old().unwrap();
    assert_eq!(std::fs::read(&plan.previous).unwrap(), b"old");
    assert!(plan.replace().is_err());
    plan.restore().unwrap();
    assert_eq!(std::fs::read(&plan.executable).unwrap(), b"old");
}

#[test]
fn restore_puts_the_old_image_back_over_the_replacement() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 7);
    std::fs::create_dir_all(&plan.directory).unwrap();
    std::fs::write(&plan.executable, b"old").unwrap();
    std::fs::write(&plan.temporary, b"new").unwrap();
    plan.rename_old().unwrap();
    plan.replace().unwrap();
    plan.restore().unwrap();
    assert_eq!(std::fs::read(&plan.executable).unwrap(), b"old");
    assert!(!plan.previous.exists());
}

#[test]
fn every_failure_after_replace_restores_the_old_image_and_configuration_before_start() {
    use Step::*;
    for failed in [Stop, Configure, Start] {
        let root = tempfile::tempdir().unwrap();
        let plan = CopyPlan::new(root.path(), 7);
        std::fs::create_dir_all(&plan.directory).unwrap();
        std::fs::write(plan.directory.join("new.exe"), b"new").unwrap();
        std::fs::write(&plan.executable, b"old").unwrap();
        let previous = previous_service_configuration();
        let mut configuration = previous.clone();
        let (mut running, mut injected) = (true, false);
        let mut calls = Vec::new();
        let result = run_install(true, true, |step| {
            calls.push(step);
            if step == Start && injected {
                assert_eq!(std::fs::read(&plan.executable).unwrap(), b"old");
                assert_eq!(configuration, previous);
            }
            execute_install_step(&plan, &mut configuration, &mut running, step)?;
            if step == failed && !injected {
                injected = true;
                return Err(io::Error::other("injected failure"));
            }
            Ok(())
        });
        assert!(result.is_err(), "after {failed:?}");
        assert_eq!(
            std::fs::read(&plan.executable).unwrap(),
            b"old",
            "after {failed:?}"
        );
        assert_eq!(configuration, previous, "after {failed:?}");
        assert!(running, "after {failed:?}");
        assert!(calls.ends_with(&[Restore, RestoreConfiguration, Start]));
    }
}

type ServiceConfiguration = (&'static str, u32, Vec<(u32, u32)>, &'static str, bool, bool);

fn previous_service_configuration() -> ServiceConfiguration {
    (
        "old path",
        3,
        vec![(1, 7000)],
        "old description",
        false,
        true,
    )
}

fn execute_install_step(
    plan: &CopyPlan,
    configuration: &mut ServiceConfiguration,
    running: &mut bool,
    step: Step,
) -> io::Result<()> {
    use Step::*;
    match step {
        RecoverOld => plan.recover_old()?,
        Copy => plan.copy_from(&plan.directory.join("new.exe"))?,
        RenameOld => plan.rename_old()?,
        Replace => plan.replace()?,
        Stop => *running = false,
        Configure => {
            *configuration = (
                "new path",
                2,
                vec![(1, 5000)],
                "new description",
                true,
                false,
            )
        }
        Start => *running = true,
        Restore => {
            assert!(!*running);
            plan.restore()?;
        }
        RestoreConfiguration => *configuration = previous_service_configuration(),
        DiscardCopy => plan.discard_copy()?,
        BackupSettings | RestoreSettings | RemoveFresh => {}
        DeleteOld => plan.delete_old()?,
    }
    Ok(())
}

#[test]
fn old_images_are_deleted_and_other_files_are_kept() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 123);
    std::fs::create_dir_all(&plan.directory).unwrap();
    for name in [
        "navo.exe.122.old",
        "navo.exe",
        "navo.exe.x.old",
        "notes.old",
    ] {
        std::fs::write(plan.directory.join(name), b"x").unwrap();
    }
    plan.delete_old().unwrap();
    let mut left: Vec<_> = std::fs::read_dir(&plan.directory)
        .unwrap()
        .map(|entry| entry.unwrap().file_name().into_string().unwrap())
        .collect();
    left.sort();
    assert_eq!(left, ["navo.exe", "navo.exe.x.old", "notes.old"]);
}

#[test]
fn bind_failures_retry_on_schedule_and_stop_cancels_the_wait() {
    let schedule: Vec<_> = (0..6).map(retry_delay).collect();
    assert_eq!(schedule, [1, 5, 30, 60, 60, 60].map(Duration::from_secs));
    let cancel = CancelToken::new();
    let (mut attempts, mut errors, mut delays) = (0, Vec::new(), Vec::new());
    let bound = bind_with_retry(
        &cancel,
        || {
            attempts += 1;
            Err::<(), _>("pipe occupied")
        },
        |error| errors.push(error),
        |delay| {
            delays.push(delay);
            if delays.len() == 4 {
                cancel.cancel();
            }
        },
    );
    assert!(bound.is_none());
    assert_eq!(attempts, 4);
    assert!(errors.iter().all(|error| error.contains("pipe occupied")));
    assert_eq!(errors.len(), 4);
    assert_eq!(delays, [1, 5, 30, 60].map(Duration::from_secs));
}

#[test]
fn bind_recovers_after_a_failure_and_never_binds_after_stop() {
    let cancel = CancelToken::new();
    let mut attempts = 0;
    let bound = bind_with_retry(
        &cancel,
        || {
            attempts += 1;
            if attempts == 1 {
                Err("occupied")
            } else {
                Ok(42)
            }
        },
        |_| {},
        |_| {},
    );
    assert_eq!(bound, Some(42));
    cancel.cancel();
    let bound = bind_with_retry(
        &cancel,
        || -> Result<(), &str> { panic!("bound after stop") },
        |_| {},
        |_| {},
    );
    assert!(bound.is_none());
}

#[test]
fn only_a_user_sid_type_is_accepted() {
    for kind in 0..=11 {
        assert_eq!(require_user_sid(kind).is_ok(), kind == 1, "SID type {kind}");
    }
}

#[test]
fn only_install_recreates_an_untrusted_directory_and_a_reparse_point_is_refused() {
    use DirectoryAction::{Accept, Recreate, Refuse};
    assert_eq!(directory_action(Found::Directory, false, true), Recreate);
    assert!(matches!(
        directory_action(Found::Directory, false, false),
        Refuse(_)
    ));
    for install in [false, true] {
        assert_eq!(directory_action(Found::Directory, true, install), Accept);
        for trusted in [false, true] {
            assert!(matches!(
                directory_action(Found::ReparsePoint, trusted, install),
                Refuse(_)
            ));
            assert!(matches!(
                directory_action(Found::Other, trusted, install),
                Refuse(_)
            ));
        }
    }
}

#[test]
fn remove_includes_the_program_data_index_and_its_parent() {
    let [parent, index] = data_directories(Path::new("program-data"));
    assert_eq!(parent, Path::new("program-data/Navo"));
    assert_eq!(index, Path::new("program-data/Navo/index"));
}

#[test]
fn clearing_the_index_deletes_files_and_folders_inside_it() {
    let root = tempfile::tempdir().unwrap();
    let index = root.path().join("index");
    std::fs::create_dir_all(index.join("nested")).unwrap();
    std::fs::write(index.join("service.json"), b"{}").unwrap();
    std::fs::write(index.join("nested/c.idx"), b"x").unwrap();
    clear_directory(&index).unwrap();
    assert_eq!(std::fs::read_dir(&index).unwrap().count(), 0);
}

#[cfg(unix)]
#[test]
fn clearing_the_index_never_follows_a_link_out_of_it() {
    let root = tempfile::tempdir().unwrap();
    let (index, outside) = (root.path().join("index"), root.path().join("outside"));
    std::fs::create_dir_all(&index).unwrap();
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("keep.txt"), b"keep").unwrap();
    std::os::unix::fs::symlink(&outside, index.join("planted")).unwrap();
    std::os::unix::fs::symlink(outside.join("keep.txt"), index.join("file-link")).unwrap();
    clear_directory(&index).unwrap();
    assert_eq!(std::fs::read_dir(&index).unwrap().count(), 0);
    assert_eq!(std::fs::read(outside.join("keep.txt")).unwrap(), b"keep");
}

#[test]
fn an_untrusted_directory_is_moved_aside_before_a_fresh_directory_is_created() {
    use super::directory_policy::apply;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("index");
    let moved = root.path().join("index.untrusted.1");
    std::fs::create_dir(&path).unwrap();
    std::fs::write(path.join("service.json"), b"planted").unwrap();
    assert!(
        !apply(
            DirectoryAction::Accept,
            || panic!("no move"),
            || panic!("no create")
        )
        .unwrap()
    );
    let recreated = apply(
        directory_action(Found::Directory, false, true),
        || std::fs::rename(&path, &moved),
        || std::fs::create_dir(&path),
    );
    assert!(recreated.unwrap());
    assert_eq!(std::fs::read_dir(&path).unwrap().count(), 0);
    assert_eq!(
        std::fs::read(moved.join("service.json")).unwrap(),
        b"planted"
    );
    let refused = apply(
        DirectoryAction::Refuse("is a reparse point"),
        || panic!("no move"),
        || panic!("no create"),
    );
    assert_eq!(refused.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
}

#[test]
fn untrusted_directory_recovery_moves_aside_then_creates_fresh() {
    let calls = std::cell::RefCell::new(Vec::new());
    super::directory_policy::apply(
        directory_action(Found::Directory, false, true),
        || {
            calls.borrow_mut().push("move aside");
            Ok(())
        },
        || {
            calls.borrow_mut().push("create fresh");
            Ok(())
        },
    )
    .unwrap();
    assert_eq!(calls.into_inner(), ["move aside", "create fresh"]);
}

#[test]
fn a_failed_move_refuses_install_without_creating_a_directory() {
    let result = super::directory_policy::apply(
        DirectoryAction::Recreate,
        || Err(io::Error::new(io::ErrorKind::PermissionDenied, "held open")),
        || panic!("must not create after a failed move"),
    );
    assert_eq!(result.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
}
