use std::fs;

use rusty_core::Location;

use super::{read, write};

fn at(path: &std::path::Path) -> Location {
    Location::new(path.to_string_lossy().into_owned()).unwrap()
}

#[test]
fn text_reads_back_and_saves() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.md");
    fs::write(&file, "# hi\n").unwrap();
    let text = read(&at(&file)).unwrap();
    assert_eq!((text.text.as_str(), text.read_only), ("# hi\n", false));
    write(&at(&file), "# bye\n").unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "# bye\n");
}

#[test]
fn windows_line_endings_survive_a_save() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "one\r\ntwo\r\n").unwrap();
    write(&at(&file), "one\ntwo\nthree\n").unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"one\r\ntwo\r\nthree\r\n");
}

#[test]
fn binary_and_huge_files_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("a.bin");
    fs::write(&binary, [1_u8, 0, 2]).unwrap();
    assert!(read(&at(&binary)).unwrap_err().contains("binary"));
    let huge = dir.path().join("huge.log");
    let file = fs::File::create(&huge).unwrap();
    file.set_len(17 * 1024 * 1024).unwrap();
    assert!(read(&at(&huge)).unwrap_err().contains("Too large"));
}

#[test]
fn text_that_is_not_utf8_opens_read_only() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("latin1.txt");
    fs::write(&file, [b'c', 0xE9, b'\n']).unwrap();
    let text = read(&at(&file)).unwrap();
    assert!(text.read_only);
}

#[test]
fn a_missing_or_remote_file_explains_itself() {
    assert!(read(&Location::new("/no/such/file.txt").unwrap()).is_err());
    assert!(read(&Location::new("fake://home/a.txt").unwrap()).is_err());
}
