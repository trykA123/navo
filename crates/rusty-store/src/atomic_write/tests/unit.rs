use super::*;

#[test]
fn new_file_gets_the_bytes() {
    let dir = tmp();
    let target = dir.path().join("settings.toml");
    write_atomic(&target, b"new").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
}

#[test]
fn new_file_makes_no_backup() {
    let dir = tmp();
    let target = dir.path().join("settings.toml");
    write_atomic(&target, b"new").unwrap();
    assert!(!backup_path(&target).exists());
}

#[test]
fn replace_writes_the_new_bytes() {
    let dir = tmp();
    let target = dir.path().join("settings.toml");
    fs::write(&target, b"old").unwrap();
    write_atomic(&target, b"new").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"new");
}

#[test]
fn replace_keeps_the_old_bytes_in_backup() {
    let dir = tmp();
    let target = dir.path().join("settings.toml");
    fs::write(&target, b"old").unwrap();
    write_atomic(&target, b"new").unwrap();
    assert_eq!(fs::read(backup_path(&target)).unwrap(), b"old");
}

#[test]
fn second_replace_moves_backup_forward() {
    let dir = tmp();
    let target = dir.path().join("settings.toml");
    fs::write(&target, b"first").unwrap();
    write_atomic(&target, b"second").unwrap();
    write_atomic(&target, b"third").unwrap();
    assert_eq!(fs::read(backup_path(&target)).unwrap(), b"second");
}

#[test]
fn empty_bytes_give_an_empty_file() {
    let dir = tmp();
    let target = dir.path().join("empty");
    fs::write(&target, b"old").unwrap();
    write_atomic(&target, b"").unwrap();
    assert_eq!(fs::read(&target).unwrap(), b"");
}

#[test]
fn large_write_keeps_every_byte() {
    let dir = tmp();
    let target = dir.path().join("large.bin");
    let bytes: Vec<u8> = (0..8 * 1024 * 1024).map(|i: u32| (i % 251) as u8).collect();
    write_atomic(&target, &bytes).unwrap();
    assert!(fs::read(&target).unwrap() == bytes);
}

#[test]
fn missing_parent_folder_is_not_found() {
    let dir = tmp();
    let target = dir.path().join("missing").join("settings.toml");
    let err = write_atomic(&target, b"new").unwrap_err();
    assert_eq!(err.kind(), io::ErrorKind::NotFound);
}

#[test]
fn missing_parent_folder_is_not_created() {
    let dir = tmp();
    let parent = dir.path().join("missing");
    let _ = write_atomic(&parent.join("settings.toml"), b"new");
    assert!(!parent.exists());
}

#[test]
fn directory_as_target_fails() {
    let dir = tmp();
    let target = dir.path().join("folder");
    fs::create_dir(&target).unwrap();
    assert!(write_atomic(&target, b"new").is_err());
}

#[test]
fn directory_as_target_is_left_alone() {
    let dir = tmp();
    let target = dir.path().join("folder");
    fs::create_dir(&target).unwrap();
    fs::write(target.join("inside"), b"kept").unwrap();
    let _ = write_atomic(&target, b"new");
    assert_eq!(fs::read(target.join("inside")).unwrap(), b"kept");
}

#[test]
fn backup_path_appends_bak_after_the_extension() {
    assert_eq!(
        backup_path(Path::new("/cfg/settings.toml")),
        Path::new("/cfg/settings.toml.bak")
    );
}

#[test]
fn backup_path_appends_bak_without_an_extension() {
    assert_eq!(
        backup_path(Path::new("/cfg/settings")),
        Path::new("/cfg/settings.bak")
    );
}

#[test]
fn backup_path_keeps_every_dot_in_the_name() {
    assert_eq!(
        backup_path(Path::new("/cfg/session.v2.bin")),
        Path::new("/cfg/session.v2.bin.bak")
    );
}
