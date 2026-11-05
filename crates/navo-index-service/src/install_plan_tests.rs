use super::*;

#[path = "install_order_tests.rs"]
mod order;

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
fn an_existing_service_is_stopped_before_its_image_is_moved_and_answer_follows_verify() {
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
            Verify,
            Answer
        ]
    );
    assert_eq!(
        install_steps(true),
        &[
            Copy,
            Stop,
            RenameOld,
            Replace,
            BackupSettings,
            Configure,
            Start,
            Verify,
            Answer
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

#[test]
fn remove_deletes_both_navo_images_while_install_keeps_the_old_image() {
    assert_eq!(NAVO.image, "navo.exe");
    assert_eq!(NAVO.removable, ["navo-index.exe", "navo.exe"]);
    assert_eq!(LEGACY_PRODUCT.removable, ["ferrite.exe"]);
}
