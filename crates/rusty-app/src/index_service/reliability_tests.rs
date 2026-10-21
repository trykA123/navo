use super::bind_retry::{bind_with_retry, retry_delay};
use super::config::require_user_sid;
use super::directory_policy::{DirectoryAction, Found, directory_action};
use super::install_plan::{CopyPlan, Step, clear_directory, data_directories, run_install};
use rusty_index::pipe::CancelToken;
use std::{io, path::Path, time::Duration};

fn record(exists: bool, failed: Option<Step>) -> (Vec<Step>, io::Result<()>) {
    let mut calls = Vec::new();
    let result = run_install(exists, |step| {
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
            DeleteOld, Copy, RenameOld, Replace, Stop, Configure, Start, DeleteOld
        ]
    );
}

#[test]
fn a_failed_reinstall_restores_the_image_and_starts_the_service_again() {
    use Step::*;
    let steps = [Copy, RenameOld, Replace, Stop, Configure, Start];
    for (end, failed) in steps.into_iter().enumerate() {
        let (calls, result) = record(true, Some(failed));
        assert_eq!(result.unwrap_err().to_string(), "original failure");
        assert_eq!(calls[0], DeleteOld);
        assert_eq!(&calls[1..=end + 1], &steps[..=end]);
        assert_eq!(&calls[end + 2..], &[Restore, Start], "after {failed:?}");
    }
}

#[test]
fn a_failed_first_install_restores_but_never_starts() {
    let (calls, result) = record(false, Some(Step::Configure));
    assert!(result.is_err());
    assert_eq!(calls.last(), Some(&Step::Restore));
    assert!(!calls.contains(&Step::Start));
}

#[test]
fn a_failed_restart_keeps_the_original_error_first() {
    let error = run_install(true, |step| match step {
        Step::Replace => Err(io::Error::new(
            io::ErrorKind::PermissionDenied,
            "original failure",
        )),
        Step::Start => Err(io::Error::other("start refused")),
        _ => Ok(()),
    })
    .unwrap_err();
    assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
    assert!(error.to_string().starts_with("original failure; "));
    assert!(error.to_string().contains("start refused"));
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
fn restore_keeps_a_new_image_that_is_already_in_place() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 7);
    std::fs::create_dir_all(&plan.directory).unwrap();
    std::fs::write(&plan.executable, b"old").unwrap();
    std::fs::write(&plan.temporary, b"new").unwrap();
    plan.rename_old().unwrap();
    plan.replace().unwrap();
    plan.restore().unwrap();
    assert_eq!(std::fs::read(&plan.executable).unwrap(), b"new");
}

#[test]
fn old_images_are_deleted_and_other_files_are_kept() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 123);
    std::fs::create_dir_all(&plan.directory).unwrap();
    for name in [
        "ferrite.exe.122.old",
        "ferrite.exe",
        "ferrite.exe.x.old",
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
    assert_eq!(left, ["ferrite.exe", "ferrite.exe.x.old", "notes.old"]);
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
fn only_install_repairs_a_planted_directory_and_a_reparse_point_is_refused() {
    use DirectoryAction::{Accept, Refuse, Repair};
    assert_eq!(directory_action(Found::Directory, false, true), Repair);
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
    assert_eq!(parent, Path::new("program-data/Ferrite"));
    assert_eq!(index, Path::new("program-data/Ferrite/index"));
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
fn a_repaired_folder_is_emptied_and_a_trusted_folder_is_left_as_is() {
    use super::directory_policy::apply;
    let root = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(root.path().join("planted")).unwrap();
    std::fs::write(root.path().join("service.json"), b"planted").unwrap();
    assert!(!apply(DirectoryAction::Accept, root.path(), || panic!("no repair")).unwrap());
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 2);
    let mut owned = false;
    let repaired = apply(DirectoryAction::Repair, root.path(), || {
        owned = true;
        Ok(())
    });
    assert!(repaired.unwrap() && owned);
    assert_eq!(std::fs::read_dir(root.path()).unwrap().count(), 0);
    let refused = apply(
        DirectoryAction::Refuse("is a reparse point"),
        root.path(),
        || Ok(()),
    );
    assert_eq!(refused.unwrap_err().kind(), io::ErrorKind::PermissionDenied);
}
