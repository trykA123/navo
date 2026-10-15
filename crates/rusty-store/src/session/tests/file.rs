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
