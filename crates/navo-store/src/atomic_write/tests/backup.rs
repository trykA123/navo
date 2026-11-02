use super::*;

#[test]
fn a_backup_that_cannot_be_made_fails_the_write() {
    let dir = tmp();
    let target = dir.path().join("keymap.toml");
    fs::write(&target, b"old").unwrap();
    fs::create_dir(backup_path(&target)).unwrap();
    fs::write(backup_path(&target).join("inside"), b"kept").unwrap();
    assert!(write_atomic(&target, b"new").is_err());
}

#[test]
fn a_backup_that_cannot_be_made_keeps_the_target_and_what_was_in_the_way() {
    let dir = tmp();
    let target = dir.path().join("keymap.toml");
    fs::write(&target, b"old").unwrap();
    fs::create_dir(backup_path(&target)).unwrap();
    fs::write(backup_path(&target).join("inside"), b"kept").unwrap();
    let _ = write_atomic(&target, b"new");
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert_eq!(
        fs::read(backup_path(&target).join("inside")).unwrap(),
        b"kept"
    );
    assert_eq!(temp_files(dir.path()), Vec::<String>::new());
}

#[test]
fn a_failed_backup_step_keeps_the_previous_backup() {
    for op in BACKUP_OPS
        .into_iter()
        .filter(|op| *op != Op::BackupFolderSync)
    {
        let dir = tmp();
        let target = dir.path().join("keymap.toml");
        fs::write(&target, b"old").unwrap();
        fs::write(backup_path(&target), b"older").unwrap();
        inject(op, INJECTED);
        let _ = write_atomic(&target, b"new");
        assert_eq!(fs::read(backup_path(&target)).unwrap(), b"older", "{op:?}");
    }
}

#[test]
fn a_backup_is_complete_before_the_target_is_replaced() {
    let dir = tmp();
    let target = dir.path().join("keymap.toml");
    fs::write(&target, b"old").unwrap();
    let backup = backup_path(&target);
    let mut seen = None;
    write_atomic_with(&target, b"new", |stage| {
        if stage == Stage::BeforeRename {
            seen = fs::read(&backup).ok();
        }
        Ok(())
    })
    .unwrap();
    assert_eq!(seen.as_deref(), Some(&b"old"[..]));
}

#[test]
fn a_backup_of_a_large_file_keeps_every_byte() {
    let dir = tmp();
    let target = dir.path().join("session.bin");
    let old: Vec<u8> = (0..2 * 1024 * 1024).map(|i: u32| (i % 253) as u8).collect();
    fs::write(&target, &old).unwrap();
    write_atomic(&target, b"new").unwrap();
    assert!(fs::read(backup_path(&target)).unwrap() == old);
}

#[cfg(unix)]
#[test]
fn a_backup_of_a_read_only_target_is_made() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tmp();
    let target = dir.path().join("session.bin");
    fs::write(&target, b"old").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o444)).unwrap();
    write_atomic(&target, b"new").unwrap();
    assert_eq!(fs::read(backup_path(&target)).unwrap(), b"old");
    assert_eq!(fs::read(&target).unwrap(), b"new");
}

fn copied_backup(target: &Path) {
    fs::write(target, b"old").unwrap();
    inject(Op::BackupLink, io::ErrorKind::Unsupported);
    write_atomic(target, b"new").unwrap();
}

#[test]
fn a_copied_backup_holds_the_old_bytes() {
    let dir = tmp();
    let target = dir.path().join("keymap.toml");
    copied_backup(&target);
    assert_eq!(fs::read(&target).unwrap(), b"new");
    assert_eq!(fs::read(backup_path(&target)).unwrap(), b"old");
    assert_eq!(temp_files(dir.path()), Vec::<String>::new());
}

#[cfg(unix)]
#[test]
fn a_copied_backup_keeps_the_file_mode() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tmp();
    let target = dir.path().join("keymap.toml");
    fs::write(&target, b"seed").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    copied_backup(&target);
    let mode = fs::metadata(backup_path(&target))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o600);
}

#[test]
fn an_unsupported_backup_folder_sync_does_not_refuse_the_save() {
    for kind in [io::ErrorKind::InvalidInput, io::ErrorKind::Unsupported] {
        let dir = tmp();
        let target = dir.path().join("session.bin");
        fs::write(&target, b"old").unwrap();
        inject(Op::BackupFolderSync, kind);
        write_atomic(&target, b"new").unwrap();
        assert_eq!(fs::read(&target).unwrap(), b"new", "{kind:?}");
        assert_eq!(fs::read(backup_path(&target)).unwrap(), b"old", "{kind:?}");
    }
}

#[test]
fn a_real_backup_folder_sync_failure_still_refuses_the_save() {
    let dir = tmp();
    let target = dir.path().join("session.bin");
    fs::write(&target, b"old").unwrap();
    inject(Op::BackupFolderSync, io::ErrorKind::StorageFull);
    assert!(write_atomic(&target, b"new").is_err());
    assert_eq!(fs::read(&target).unwrap(), b"old");
}

#[cfg(target_os = "linux")]
#[test]
fn enotsup_is_tolerated_and_eio_is_not() {
    use super::super::backup::best_effort;
    assert!(best_effort(Err(io::Error::from_raw_os_error(95))).is_ok());
    assert!(best_effort(Err(io::Error::from_raw_os_error(5))).is_err());
}

#[test]
fn a_hard_linked_backup_is_synced_before_the_replace() {
    let dir = tmp();
    let target = dir.path().join("session.bin");
    fs::write(&target, b"old").unwrap();
    inject(Op::LinkSync, io::ErrorKind::StorageFull);
    let got = write_atomic(&target, b"new").map_err(|e| e.kind());
    assert_eq!(got, Err(io::ErrorKind::StorageFull));
    assert_eq!(fs::read(&target).unwrap(), b"old");
    assert_eq!(temp_files(dir.path()), Vec::<String>::new());
}

#[test]
fn a_hard_linked_backup_sync_passes_on_a_writable_file() {
    let dir = tmp();
    let target = dir.path().join("session.bin");
    fs::write(&target, b"old").unwrap();
    write_atomic(&target, b"new").unwrap();
    assert_eq!(fs::read(backup_path(&target)).unwrap(), b"old");
}
