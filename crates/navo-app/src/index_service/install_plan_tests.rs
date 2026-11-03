use super::*;

#[test]
fn binary_is_copied_to_a_temporary_name_then_renamed_in_program_files() {
    let plan = CopyPlan::new(Path::new("program-files"), 123);
    assert_eq!(plan.directory, Path::new("program-files/Navo/Index"));
    assert_eq!(plan.executable, plan.directory.join("navo.exe"));
    assert_eq!(plan.temporary.parent(), Some(plan.directory.as_path()));
    assert_ne!(plan.temporary, plan.executable);
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("portable.exe");
    let plan = CopyPlan::new(root.path(), 123);
    std::fs::create_dir_all(&plan.directory).unwrap();
    std::fs::write(&source, b"new binary").unwrap();
    std::fs::write(&plan.executable, b"old binary").unwrap();
    plan.copy_from(&source).unwrap();
    plan.rename_old().unwrap();
    plan.replace().unwrap();
    assert_eq!(std::fs::read(&plan.executable).unwrap(), b"new binary");
    assert!(!plan.temporary.exists());
}

#[test]
fn reinstall_moves_the_new_image_in_before_it_stops_the_service() {
    use Step::*;
    assert_eq!(
        install_steps(false),
        &[
            Copy,
            RenameOld,
            Replace,
            BackupSettings,
            Configure,
            Start,
            Verify
        ]
    );
    assert_eq!(
        install_steps(true),
        &[
            Copy,
            RenameOld,
            Replace,
            Stop,
            BackupSettings,
            Configure,
            Start,
            Verify
        ]
    );
}

#[test]
fn copy_refuses_an_existing_temporary_file() {
    let root = tempfile::tempdir().unwrap();
    let plan = CopyPlan::new(root.path(), 1);
    std::fs::create_dir_all(&plan.directory).unwrap();
    std::fs::write(&plan.temporary, b"untouched").unwrap();
    assert!(plan.copy_from(&plan.temporary).is_err());
    assert_eq!(std::fs::read(&plan.temporary).unwrap(), b"untouched");
}
