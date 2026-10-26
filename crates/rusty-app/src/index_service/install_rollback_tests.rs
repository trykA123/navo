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
    let top = root.path().join("Navo");
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

#[test]
fn interrupted_install_restores_the_only_old_image_before_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 10);
    std::fs::create_dir_all(&plan.directory).unwrap();
    let old = plan.directory.join("navo.exe.9.old");
    std::fs::write(&old, b"only good image").unwrap();
    plan.recover_old().unwrap();
    plan.delete_old().unwrap();
    assert_eq!(std::fs::read(&plan.executable).unwrap(), b"only good image");
}

#[test]
fn settings_rollback_replaces_a_partial_write_without_mutating_open_readers() {
    use std::io::Read;
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("service.json");
    let original = br#"{"user_sid":"old"}"#;
    std::fs::write(&path, original).unwrap();
    let backup = SettingsBackup::capture(&path).unwrap();
    std::fs::write(&path, b"{").unwrap();
    let mut reader = std::fs::File::open(&path).unwrap();
    backup.restore().unwrap();
    assert_eq!(std::fs::read(&path).unwrap(), original);
    let mut previous = Vec::new();
    reader.read_to_end(&mut previous).unwrap();
    assert_eq!(previous, b"{");
}

#[test]
fn a_planted_image_moved_aside_does_not_count_as_a_trusted_previous_image() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 1);
    std::fs::create_dir_all(&plan.directory).unwrap();
    std::fs::write(&plan.executable, b"planted").unwrap();
    let mut fresh = FreshInstall::observe(&plan, &[]);
    let preserved = root.path().join("untrusted");
    std::fs::rename(&plan.directory, &preserved).unwrap();
    std::fs::create_dir_all(&plan.directory).unwrap();
    fresh.observe_executable(&plan);
    std::fs::write(&plan.executable, b"new trusted copy").unwrap();
    fresh.remove_executable(&plan).unwrap();
    assert!(!plan.executable.exists());
    assert_eq!(
        std::fs::read(preserved.join("navo.exe")).unwrap(),
        b"planted"
    );
}

#[test]
fn install_preflight_recovers_the_only_image_before_a_new_copy_fails() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 10);
    std::fs::create_dir_all(&plan.directory).unwrap();
    std::fs::write(plan.directory.join("navo.exe.9.old"), b"good image").unwrap();
    let result = run_install(true, false, |step| match step {
        Step::RecoverOld => plan.recover_old(),
        Step::DeleteOld => plan.delete_old(),
        Step::Copy => Err(io::Error::other("copy failed")),
        _ => Ok(()),
    });
    assert!(result.is_err());
    assert_eq!(std::fs::read(&plan.executable).unwrap(), b"good image");
}

#[test]
fn ambiguous_crash_images_are_preserved_without_starting_cleanup() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 10);
    std::fs::create_dir_all(&plan.directory).unwrap();
    for nonce in [8, 9] {
        std::fs::write(
            plan.directory.join(format!("navo.exe.{nonce}.old")),
            b"preserved",
        )
        .unwrap();
    }
    let cleaned = Cell::new(false);
    let result = run_install(true, false, |step| match step {
        Step::RecoverOld => plan.recover_old(),
        Step::DeleteOld => {
            cleaned.set(true);
            plan.delete_old()
        }
        _ => Ok(()),
    });
    assert!(result.is_err());
    assert!(!cleaned.get());
    assert_eq!(std::fs::read_dir(&plan.directory).unwrap().count(), 2);
}

#[test]
fn a_recovered_trusted_image_is_not_removed_as_a_fresh_copy() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 10);
    std::fs::create_dir_all(&plan.directory).unwrap();
    std::fs::write(plan.directory.join("navo.exe.9.old"), b"good image").unwrap();
    let mut fresh = FreshInstall::observe(&plan, &[]);
    fresh.recover_executable(&plan).unwrap();
    fresh.remove_executable(&plan).unwrap();
    assert_eq!(std::fs::read(&plan.executable).unwrap(), b"good image");
}

mod upgrade {
    use super::super::upgrade::{Legacy, UpgradeStep, run_upgrade};
    use std::io;

    #[derive(Default)]
    struct World {
        navo: bool,
        ferrite_service: bool,
        ferrite_files: bool,
        steps: Vec<UpgradeStep>,
    }

    impl World {
        fn with_ferrite() -> Self {
            Self {
                ferrite_service: true,
                ferrite_files: true,
                ..Self::default()
            }
        }

        fn legacy(&self) -> Legacy {
            Legacy {
                service: self.ferrite_service,
                files: self.ferrite_files,
            }
        }

        fn install(&mut self, fail: Option<UpgradeStep>) -> io::Result<Option<io::Error>> {
            let legacy = self.legacy();
            run_upgrade(legacy, |step| self.apply(step, fail))
        }

        fn apply(&mut self, step: UpgradeStep, fail: Option<UpgradeStep>) -> io::Result<()> {
            self.steps.push(step);
            if fail == Some(step) {
                return Err(io::Error::other("injected"));
            }
            match step {
                UpgradeStep::InstallNew => self.navo = true,
                UpgradeStep::AwaitNew => {}
                UpgradeStep::RemoveLegacyService if !self.ferrite_service => {
                    return Err(io::ErrorKind::NotFound.into());
                }
                UpgradeStep::RemoveLegacyService => self.ferrite_service = false,
                UpgradeStep::RemoveLegacyFiles => self.ferrite_files = false,
            }
            Ok(())
        }
    }

    #[test]
    fn an_old_service_is_removed_only_after_the_new_one_answers() {
        let mut world = World::with_ferrite();
        world.install(None).unwrap();
        assert_eq!(
            world.steps,
            [
                UpgradeStep::InstallNew,
                UpgradeStep::AwaitNew,
                UpgradeStep::RemoveLegacyService,
                UpgradeStep::RemoveLegacyFiles,
            ]
        );
        assert!(world.navo && !world.legacy().is_present());
    }

    #[test]
    fn without_an_old_service_install_does_not_wait_or_remove() {
        let mut world = World::default();
        world.install(None).unwrap();
        assert_eq!(world.steps, [UpgradeStep::InstallNew]);
    }

    #[test]
    fn a_failed_new_install_or_a_silent_new_pipe_keeps_the_old_service() {
        for fail in [UpgradeStep::InstallNew, UpgradeStep::AwaitNew] {
            let mut world = World::with_ferrite();
            let error = world.install(Some(fail)).unwrap_err();
            assert!(world.ferrite_service && world.ferrite_files, "{fail:?}");
            assert!(!world.steps.contains(&UpgradeStep::RemoveLegacyService));
            if fail == UpgradeStep::AwaitNew {
                assert!(error.to_string().contains("Ferrite one was kept"));
            }
        }
    }

    #[test]
    fn a_crash_after_the_new_install_leaves_the_old_service_detected_and_the_next_install_finishes()
    {
        let mut world = World::with_ferrite();
        let _ = world.install(Some(UpgradeStep::AwaitNew));
        assert!(world.navo && world.legacy().is_present());
        world.steps.clear();
        world.install(None).unwrap();
        assert!(world.navo && !world.legacy().is_present());
        assert!(world.steps.contains(&UpgradeStep::RemoveLegacyService));
    }

    #[test]
    fn leftover_old_files_are_a_warning_and_the_install_succeeds() {
        let mut world = World::with_ferrite();
        let warning = world
            .install(Some(UpgradeStep::RemoveLegacyFiles))
            .unwrap()
            .expect("the leftover files are reported");
        assert!(warning.to_string().contains("injected"));
        assert!(world.navo && !world.ferrite_service && world.ferrite_files);
        world.steps.clear();
        let again = world.install(Some(UpgradeStep::RemoveLegacyFiles)).unwrap();
        assert!(again.is_some(), "every later install still succeeds");
        assert!(world.navo);
    }

    #[test]
    fn a_crash_between_service_and_folder_removal_is_finished_by_the_next_install() {
        let mut world = World::with_ferrite();
        let _ = world.install(Some(UpgradeStep::RemoveLegacyFiles));
        assert!(!world.ferrite_service && world.ferrite_files);
        assert!(world.legacy().is_present());
        assert!(world.install(None).unwrap().is_none());
        assert!(world.navo && !world.legacy().is_present());
    }

    #[test]
    fn a_failed_old_service_delete_keeps_both_and_reports_the_state() {
        let mut world = World::with_ferrite();
        let error = world
            .install(Some(UpgradeStep::RemoveLegacyService))
            .unwrap_err();
        assert!(world.navo, "the new service stays installed");
        assert!(world.ferrite_service && world.ferrite_files);
        assert!(!world.steps.contains(&UpgradeStep::RemoveLegacyFiles));
        let text = error.to_string();
        assert!(text.contains("new index service runs") && text.contains("injected"));
    }
}
