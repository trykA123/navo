use super::directory_policy::parent_ace_allowed;
use super::install_plan::{CopyPlan, FreshInstall, SettingsBackup, Step, retry, run_install};
use std::cell::Cell;
use std::io;
use std::time::Duration;

const FILE_ADD_FILE: u32 = 0x2;
const FILE_ADD_SUBDIRECTORY: u32 = 0x4;
const FILE_WRITE_EA: u32 = 0x10;
const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
const FILE_DELETE_CHILD: u32 = 0x40;
const WRITE_DAC: u32 = 0x0004_0000;
const READ_EXECUTE: u32 = 0x0012_00A9;
const FULL_CONTROL: u32 = 0x001F_01FF;

#[test]
fn stock_program_data_and_program_files_acls_are_trusted_as_a_parent() {
    let users_program_data = READ_EXECUTE
        | FILE_ADD_FILE
        | FILE_ADD_SUBDIRECTORY
        | FILE_WRITE_EA
        | FILE_WRITE_ATTRIBUTES;
    let program_data = [(FULL_CONTROL, true), (users_program_data, false)];
    let program_files = [(FULL_CONTROL, true), (READ_EXECUTE, false)];
    for acl in [program_data, program_files] {
        assert!(
            acl.iter()
                .all(|&(mask, privileged)| parent_ace_allowed(mask, privileged))
        );
    }
}

#[test]
fn a_parent_where_users_can_delete_children_or_change_the_acl_is_not_trusted() {
    for right in [FILE_DELETE_CHILD, WRITE_DAC] {
        assert!(!parent_ace_allowed(
            READ_EXECUTE | FILE_ADD_FILE | right,
            false
        ));
    }
}

#[test]
fn rollback_restores_the_previous_service_json_or_removes_a_new_one() {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("service.json");
    std::fs::write(&path, b"old").unwrap();
    let backup = SettingsBackup::capture(&path).unwrap();
    std::fs::write(&path, b"new").unwrap();
    backup.restore().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), b"old");
    std::fs::remove_file(&path).unwrap();
    let backup = SettingsBackup::capture(&path).unwrap();
    std::fs::write(&path, b"new").unwrap();
    backup.restore().unwrap();
    assert!(!path.exists());
}

#[test]
fn a_failed_configure_rolls_back_service_json_in_both_install_kinds() {
    for exists in [true, false] {
        let mut calls = Vec::new();
        let result = run_install(exists, true, |step| {
            calls.push(step);
            match step {
                Step::Configure => Err(io::Error::other("configure failed")),
                _ => Ok(()),
            }
        });
        assert!(result.is_err());
        assert!(calls.contains(&Step::RestoreSettings), "exists {exists}");
    }
}

#[test]
fn a_failed_fresh_install_removes_the_copied_exe_and_the_folders_it_created() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 1);
    let top = root.path().join("Ferrite");
    let fresh = FreshInstall::observe(&plan, &[top.clone(), plan.directory.clone()]);
    std::fs::create_dir_all(&plan.directory).unwrap();
    std::fs::write(&plan.executable, b"copy").unwrap();
    fresh.remove_executable(&plan).unwrap();
    fresh.remove_directories().unwrap();
    assert!(!top.exists());
}

#[test]
fn rollback_leaves_existing_folders_and_a_preexisting_exe_alone() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 1);
    std::fs::create_dir_all(&plan.directory).unwrap();
    std::fs::write(&plan.executable, b"leftover").unwrap();
    let fresh = FreshInstall::observe(&plan, std::slice::from_ref(&plan.directory));
    fresh.remove_executable(&plan).unwrap();
    fresh.remove_directories().unwrap();
    assert_eq!(std::fs::read(&plan.executable).unwrap(), b"leftover");
}

#[test]
fn a_fresh_install_failure_removes_the_exe_after_restoring() {
    let mut calls = Vec::new();
    let result = run_install(false, false, |step| {
        calls.push(step);
        match step {
            Step::Start => Err(io::Error::other("start failed")),
            _ => Ok(()),
        }
    });
    assert!(result.is_err());
    assert_eq!(
        &calls[calls.len() - 3..],
        &[Step::RestoreSettings, Step::Restore, Step::RemoveFresh]
    );
}

#[test]
fn rollback_starts_the_old_service_only_if_it_was_running() {
    for was_running in [true, false] {
        let mut starts = 0;
        let _ = run_install(true, was_running, |step| match step {
            Step::Configure => Err(io::Error::other("configure failed")),
            Step::Start => {
                starts += 1;
                Ok(())
            }
            _ => Ok(()),
        });
        assert_eq!(starts, usize::from(was_running));
    }
}

#[test]
fn the_restore_rename_retries_while_the_image_is_still_mapped() {
    let attempts = Cell::new(0);
    let result = retry(5, Duration::ZERO, || {
        attempts.set(attempts.get() + 1);
        match attempts.get() < 4 {
            true => Err(io::Error::other("mapped")),
            false => Ok(()),
        }
    });
    assert!(result.is_ok());
    assert_eq!(attempts.get(), 4);
    attempts.set(0);
    let failed = retry(5, Duration::ZERO, || -> io::Result<()> {
        attempts.set(attempts.get() + 1);
        Err(io::Error::other("mapped"))
    });
    assert!(failed.is_err());
    assert_eq!(attempts.get(), 5);
}

#[test]
fn a_failed_restart_keeps_the_original_error_first() {
    let error = run_install(true, true, |step| match step {
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
