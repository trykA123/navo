use super::*;

#[test]
fn written_session_reads_back_equal() {
    let dir = tmp();
    let path = session_file(&dir);
    write_session(&path, &three_panes_of_100_rows()).unwrap();
    assert_eq!(read_session_file(&path).unwrap(), three_panes_of_100_rows());
}

#[test]
fn file_starts_with_magic_then_version_one() {
    let dir = tmp();
    let path = session_file(&dir);
    write_session(&path, &first()).unwrap();
    let bytes = fs::read(&path).unwrap();
    assert_eq!(&bytes[..18], b"FERRITE-SESSION\n\x01\x00");
}

#[test]
fn file_is_the_header_then_the_postcard_body() {
    let dir = tmp();
    let path = session_file(&dir);
    write_session(&path, &first()).unwrap();
    assert_eq!(fs::read(&path).unwrap(), framed(1, &body(&first())));
}

#[test]
fn second_write_keeps_the_first_as_backup() {
    let dir = tmp();
    let path = session_file(&dir);
    write_session(&path, &first()).unwrap();
    write_session(&path, &second()).unwrap();
    assert_eq!(read_session_file(&backup_path(&path)).unwrap(), first());
}

#[test]
fn hand_framed_file_reads() {
    let dir = tmp();
    let path = session_file(&dir);
    put(&path, &first());
    assert_eq!(read_session_file(&path).unwrap(), first());
}

#[test]
fn wrong_magic_is_bad_magic() {
    let dir = tmp();
    let path = session_file(&dir);
    let mut bytes = framed(SESSION_VERSION, &body(&first()));
    bytes[0] = b'R';
    fs::write(&path, bytes).unwrap();
    assert!(matches!(
        read_session_file(&path),
        Err(SessionFileError::BadMagic)
    ));
}

#[test]
fn newer_version_is_too_new() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, framed(2, &body(&first()))).unwrap();
    assert!(matches!(
        read_session_file(&path),
        Err(SessionFileError::TooNew {
            found: 2,
            supported: 1
        })
    ));
}

#[test]
fn flipped_body_byte_is_bad_checksum() {
    let dir = tmp();
    let path = session_file(&dir);
    put_corrupt(&path, &first());
    assert!(matches!(
        read_session_file(&path),
        Err(SessionFileError::BadChecksum)
    ));
}

#[test]
fn garbage_with_a_valid_checksum_is_decode() {
    let dir = tmp();
    let path = session_file(&dir);
    fs::write(&path, framed(SESSION_VERSION, &[0xFF; 8])).unwrap();
    assert!(matches!(
        read_session_file(&path),
        Err(SessionFileError::Decode(_))
    ));
}

#[test]
fn missing_file_is_io_not_found() {
    let dir = tmp();
    let result = read_session_file(&session_file(&dir));
    assert!(result.as_ref().is_err_and(not_found), "{result:?}");
}

fn nested(depth: usize) -> rusty_core::LayoutState {
    (0..depth).fold(rusty_core::LayoutState::Pane(1), |inner, _| {
        rusty_core::LayoutState::Split {
            side_by_side: true,
            ratio: 500,
            first: Box::new(inner),
            second: Box::new(rusty_core::LayoutState::Pane(2)),
        }
    })
}

#[test]
fn a_layout_nested_past_the_limit_is_a_decode_error() {
    let dir = tmp();
    let path = dir.path().join("session.bin");
    let mut session =
        rusty_core::AppState::new(rusty_core::Location::new("/home").unwrap()).to_session(10);
    session.windows[0].layout = nested(rusty_core::MAX_LAYOUT_DEPTH + 5);
    write_session(&path, &session).unwrap();
    assert!(matches!(
        read_session_file(&path),
        Err(SessionFileError::Decode(_))
    ));
}

#[test]
fn a_layout_at_the_depth_limit_still_decodes() {
    let dir = tmp();
    let path = dir.path().join("session.bin");
    let mut session =
        rusty_core::AppState::new(rusty_core::Location::new("/home").unwrap()).to_session(10);
    session.windows[0].layout = nested(rusty_core::MAX_LAYOUT_DEPTH - 1);
    write_session(&path, &session).unwrap();
    assert!(read_session_file(&path).is_ok());
}

#[test]
fn writing_after_a_backup_load_keeps_the_good_backup() {
    let dir = tmp();
    let path = dir.path().join("session.bin");
    let session =
        rusty_core::AppState::new(rusty_core::Location::new("/home").unwrap()).to_session(10);
    write_session(&path, &session).unwrap();
    write_session(&path, &session).unwrap();
    std::fs::write(&path, b"garbage").unwrap();
    write_session(&path, &session).unwrap();
    assert!(read_session_file(&crate::backup_path(&path)).is_ok());
}

#[test]
fn a_newer_main_file_is_not_deleted_by_a_write() {
    let dir = tmp();
    let path = dir.path().join("session.bin");
    let session =
        rusty_core::AppState::new(rusty_core::Location::new("/home").unwrap()).to_session(10);
    write_session(&path, &session).unwrap();
    let mut bytes = std::fs::read(&path).unwrap();
    bytes[16] = 2;
    std::fs::write(&path, &bytes).unwrap();
    write_session(&path, &session).unwrap();
    assert_eq!(std::fs::read(crate::backup_path(&path)).unwrap(), bytes);
}

#[test]
fn a_corrupt_main_file_is_kept_aside_when_writing() {
    let dir = tmp();
    let path = dir.path().join("session.bin");
    let session =
        rusty_core::AppState::new(rusty_core::Location::new("/home").unwrap()).to_session(10);
    std::fs::write(&path, b"garbage").unwrap();
    write_session(&path, &session).unwrap();
    assert_eq!(
        std::fs::read(dir.path().join("session.bin.corrupt")).unwrap(),
        b"garbage"
    );
}
