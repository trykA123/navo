use super::*;

#[test]
fn valid_main_file_is_used() {
    let dir = tmp();
    let path = session_file(&dir);
    put(&path, &first());
    put(&backup_path(&path), &second());
    let (session, source, _) = load_session(&path);
    assert_eq!((session, source), (Some(first()), SessionSource::Main));
}

#[test]
fn corrupt_main_file_falls_back_to_backup() {
    let dir = tmp();
    let path = session_file(&dir);
    put_corrupt(&path, &first());
    put(&backup_path(&path), &second());
    let (session, source, _) = load_session(&path);
    assert_eq!((session, source), (Some(second()), SessionSource::Backup));
}

#[test]
fn main_file_the_state_rejects_falls_back_to_backup() {
    let dir = tmp();
    let path = session_file(&dir);
    put(&path, &rejected());
    put(&backup_path(&path), &second());
    let (session, source, _) = load_session(&path);
    assert_eq!((session, source), (Some(second()), SessionSource::Backup));
}

#[test]
fn missing_main_file_falls_back_to_backup() {
    let dir = tmp();
    let path = session_file(&dir);
    put(&backup_path(&path), &second());
    let (session, source, _) = load_session(&path);
    assert_eq!((session, source), (Some(second()), SessionSource::Backup));
}

#[test]
fn both_files_bad_is_empty() {
    let dir = tmp();
    let path = session_file(&dir);
    put_corrupt(&path, &first());
    put(&backup_path(&path), &rejected());
    let (session, source, _) = load_session(&path);
    assert_eq!((session, source), (None, SessionSource::Empty));
}

#[test]
fn errors_are_listed_main_first_then_backup() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, b"not a session").unwrap();
    put_corrupt(&backup_path(&path), &second());
    let (_, _, errors) = load_session(&path);
    assert!(
        matches!(
            errors.as_slice(),
            [SessionFileError::BadMagic, SessionFileError::BadChecksum]
        ),
        "{errors:?}"
    );
}

#[test]
fn both_files_missing_is_empty_with_two_not_found_errors() {
    let dir = tmp();
    let (session, source, errors) = load_session(&session_file(&dir));
    let missing = errors.iter().filter(|e| not_found(e)).count();
    assert_eq!(
        (session, source, errors.len(), missing),
        (None, SessionSource::Empty, 2, 2)
    );
}

#[test]
fn backup_error_is_not_reported_when_main_is_used() {
    let dir = tmp();
    let path = session_file(&dir);
    put(&path, &first());
    let (_, _, errors) = load_session(&path);
    assert!(errors.is_empty(), "{errors:?}");
}

#[test]
fn newer_main_file_falls_back_to_the_backup_and_stays_on_disk() {
    let dir = tmp();
    let path = session_file(&dir);
    let newer = framed(SESSION_VERSION + 1, &body(&first()));
    fs::write(&path, &newer).unwrap();
    put(&backup_path(&path), &second());
    let (session, source, errors) = load_session(&path);
    assert_eq!((session, source), (Some(second()), SessionSource::Backup));
    assert!(matches!(
        errors.as_slice(),
        [SessionFileError::TooNew { .. }]
    ));
    assert_eq!(fs::read(&path).unwrap(), newer);
}

#[test]
fn newer_main_and_backup_files_start_empty() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, framed(SESSION_VERSION + 1, &body(&first()))).unwrap();
    fs::write(
        backup_path(&path),
        framed(SESSION_VERSION + 2, &body(&second())),
    )
    .unwrap();
    let (session, source, errors) = load_session(&path);
    assert_eq!((session, source), (None, SessionSource::Empty));
    assert!(matches!(
        errors.as_slice(),
        [
            SessionFileError::TooNew { .. },
            SessionFileError::TooNew { .. }
        ]
    ));
}
