use super::*;

fn full(path: &Path) {
    let mut bytes = super::super::FORMAT.as_bytes().to_vec();
    bytes.resize(MAX_BYTES as usize, b'x');
    fs::write(path, bytes).unwrap();
}

#[test]
fn rotation_keeps_one_previous_log_and_uses_the_same_stable_lock() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rename.log");
    full(&path);
    let batch = format!("{}# rename batch current\n", super::super::FORMAT);
    append(&path, batch.as_bytes()).unwrap();
    assert_eq!(fs::read_to_string(&path).unwrap(), batch);
    assert_eq!(fs::metadata(related(&path, ".1")).unwrap().len(), MAX_BYTES);
    assert!(related(&path, ".lock").is_file());
    full(&path);
    append(&path, batch.as_bytes()).unwrap();
    assert_eq!(fs::metadata(related(&path, ".1")).unwrap().len(), MAX_BYTES);
    assert_eq!(fs::read_to_string(path).unwrap(), batch);
}

#[test]
fn rotation_preserves_an_unknown_backup_and_the_current_log() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rename.log");
    full(&path);
    fs::write(related(&path, ".1"), b"user notes").unwrap();
    let batch = format!("{}# rename batch current\n", super::super::FORMAT);
    assert!(append(&path, batch.as_bytes()).is_err());
    assert_eq!(fs::read(related(&path, ".1")).unwrap(), b"user notes");
    assert_eq!(fs::metadata(path).unwrap().len(), MAX_BYTES);
}

#[test]
fn backup_placement_never_replaces_a_late_collision() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("rename.log");
    let backup = related(&path, ".1");
    fs::write(&path, super::super::FORMAT).unwrap();
    fs::write(&backup, b"late user data").unwrap();
    assert!(place_backup(&path, &backup).is_err());
    assert_eq!(fs::read(backup).unwrap(), b"late user data");
    assert_eq!(fs::read_to_string(path).unwrap(), super::super::FORMAT);
}

#[cfg(unix)]
#[test]
fn rotation_preserves_raw_profile_directory_names() {
    use std::os::unix::ffi::OsStrExt;
    let dir = tempfile::tempdir().unwrap();
    let profile = dir
        .path()
        .join(std::ffi::OsStr::from_bytes(b"profile-\xff"));
    fs::create_dir(&profile).unwrap();
    let path = profile.join("rename.log");
    full(&path);
    let batch = format!("{}# rename batch current\n", super::super::FORMAT);
    append(&path, batch.as_bytes()).unwrap();
    assert_eq!(fs::read_to_string(path).unwrap(), batch);
}
