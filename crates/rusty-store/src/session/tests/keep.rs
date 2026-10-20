use super::*;

const TOO_BIG: usize = 9 * 1024 * 1024;

fn kept(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(suffix);
    PathBuf::from(name)
}

fn large(version: u16) -> Vec<u8> {
    framed(version, &vec![0; TOO_BIG])
}

#[test]
fn a_large_newer_file_reports_too_new() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, large(SESSION_VERSION + 1)).unwrap();
    assert!(matches!(
        read_session_file(&path),
        Err(SessionFileError::TooNew { .. })
    ));
}

#[test]
fn a_large_current_file_still_reports_decode() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, large(SESSION_VERSION)).unwrap();
    assert!(matches!(
        read_session_file(&path),
        Err(SessionFileError::Decode(_))
    ));
}

#[test]
fn a_large_newer_main_file_is_kept_not_set_corrupt() {
    let dir = tmp();
    let path = session_file(&dir);
    let newer = large(SESSION_VERSION + 1);
    fs::write(&path, &newer).unwrap();
    write_session(&path, &first()).unwrap();
    let copy = kept(&path, &format!(".v{}", SESSION_VERSION + 1));
    assert_eq!(fs::read(copy).unwrap(), newer);
    assert!(!kept(&path, ".corrupt").exists());
}

#[test]
fn a_second_corrupt_main_file_keeps_the_first_corrupt_copy() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, b"first garbage").unwrap();
    write_session(&path, &first()).unwrap();
    fs::write(&path, b"second garbage").unwrap();
    write_session(&path, &second()).unwrap();
    let copy = kept(&path, ".corrupt");
    assert_eq!(fs::read(&copy).unwrap(), b"first garbage");
    assert_eq!(fs::read(kept(&copy, ".2")).unwrap(), b"second garbage");
}

#[test]
fn a_corrupt_backup_file_is_kept_aside_when_writing() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(backup_path(&path), b"garbage").unwrap();
    write_session(&path, &first()).unwrap();
    assert_eq!(
        fs::read(kept(&backup_path(&path), ".corrupt")).unwrap(),
        b"garbage"
    );
}

#[test]
fn a_main_file_that_cannot_be_read_refuses_the_save() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::create_dir(&path).unwrap();
    let result = write_session(&path, &first());
    assert!(matches!(result, Err(SessionFileError::Io(_))), "{result:?}");
    assert!(path.is_dir());
}

#[test]
fn a_backup_file_that_cannot_be_read_refuses_the_save() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::create_dir(backup_path(&path)).unwrap();
    let result = write_session(&path, &first());
    assert!(matches!(result, Err(SessionFileError::Io(_))), "{result:?}");
    assert!(!path.exists());
}

#[test]
fn a_save_over_a_newer_main_file_leaves_the_backup_unchanged() {
    let dir = tmp();
    let path = session_file(&dir);
    let newer = framed(SESSION_VERSION + 1, &body(&first()));
    fs::write(&path, &newer).unwrap();
    put(&backup_path(&path), &second());
    let backup = fs::read(backup_path(&path)).unwrap();
    write_session(&path, &first()).unwrap();
    let copy = kept(&path, &format!(".v{}", SESSION_VERSION + 1));
    assert_eq!(fs::read(copy).unwrap(), newer);
    assert_eq!(fs::read(backup_path(&path)).unwrap(), backup);
}

#[cfg(windows)]
#[test]
fn a_newer_file_held_open_by_another_process_is_not_overwritten() {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ: u32 = 1;
    let dir = tmp();
    let path = session_file(&dir);
    let newer = framed(SESSION_VERSION + 1, &body(&first()));
    fs::write(&path, &newer).unwrap();
    let _held = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&path)
        .unwrap();
    assert!(write_session(&path, &second()).is_err());
    assert_eq!(fs::read(&path).unwrap(), newer);
}

#[test]
fn a_corrupt_main_file_that_cannot_be_kept_aside_refuses_the_save() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, b"garbage").unwrap();
    put(&backup_path(&path), &second());
    let backup = fs::read(backup_path(&path)).unwrap();
    let base = kept(&path, ".corrupt");
    fs::write(&base, b"").unwrap();
    for n in 2..1000 {
        fs::write(kept(&base, &format!(".{n}")), b"").unwrap();
    }
    assert!(write_session(&path, &first()).is_err());
    assert_eq!(fs::read(&path).unwrap(), b"garbage");
    assert_eq!(fs::read(backup_path(&path)).unwrap(), backup);
}

#[cfg(windows)]
#[test]
fn a_corrupt_main_file_held_open_leaves_the_backup_unchanged() {
    use std::os::windows::fs::OpenOptionsExt;

    const FILE_SHARE_READ: u32 = 1;
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, b"garbage").unwrap();
    put(&backup_path(&path), &second());
    let backup = fs::read(backup_path(&path)).unwrap();
    let _held = fs::OpenOptions::new()
        .read(true)
        .share_mode(FILE_SHARE_READ)
        .open(&path)
        .unwrap();
    assert!(write_session(&path, &first()).is_err());
    assert_eq!(fs::read(backup_path(&path)).unwrap(), backup);
}
