use std::fs;

use rusty_core::Location;
use rusty_ui::window_view::{Save, SaveError};

use super::{line_endings, mostly_crlf, read, write};

fn at(path: &std::path::Path) -> Location {
    Location::new(path.to_string_lossy().into_owned()).unwrap()
}

fn save(text: &str, crlf: bool, expected: Option<rusty_ui::window_view::FileStamp>) -> Save<'_> {
    Save {
        text,
        crlf,
        expected,
    }
}

#[test]
fn text_reads_back_and_saves_without_a_backup() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("notes.md");
    fs::write(&file, "# hi\n").unwrap();
    fs::write(dir.path().join("notes.md.bak"), "mine").unwrap();
    let text = read(&at(&file)).unwrap();
    assert_eq!(
        (text.text.as_str(), text.read_only, text.crlf),
        ("# hi\n", false, false)
    );
    write(&at(&file), &save("# bye\n", text.crlf, text.stamp)).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "# bye\n");
    assert_eq!(
        fs::read_to_string(dir.path().join("notes.md.bak")).unwrap(),
        "mine"
    );
}

#[test]
fn crlf_files_stay_crlf_even_for_new_lines() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "one\r\ntwo\r\n").unwrap();
    let text = read(&at(&file)).unwrap();
    assert!(text.crlf);
    let edited = format!("{}three\n", text.text);
    write(&at(&file), &save(&edited, true, text.stamp)).unwrap();
    assert_eq!(fs::read(&file).unwrap(), b"one\r\ntwo\r\nthree\r\n");
}

#[test]
fn pasted_crlf_in_an_lf_file_becomes_lf() {
    assert_eq!(line_endings("a\r\nb\nc", false), "a\nb\nc");
    assert_eq!(line_endings("a\r\nb\nc", true), "a\r\nb\r\nc");
}

#[test]
fn a_file_changed_on_disk_is_not_overwritten_without_asking() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("a.txt");
    fs::write(&file, "old").unwrap();
    let text = read(&at(&file)).unwrap();
    fs::write(&file, "changed elsewhere, longer").unwrap();
    let refused = write(&at(&file), &save("mine", false, text.stamp));
    assert_eq!(refused, Err(SaveError::Changed));
    assert_eq!(
        fs::read_to_string(&file).unwrap(),
        "changed elsewhere, longer"
    );
    write(&at(&file), &save("mine", false, None)).unwrap();
    assert_eq!(fs::read_to_string(&file).unwrap(), "mine");
}

#[test]
fn binary_utf16_and_huge_files_are_refused() {
    let dir = tempfile::tempdir().unwrap();
    let binary = dir.path().join("a.bin");
    fs::write(&binary, [1_u8, 0, 2]).unwrap();
    assert!(read(&at(&binary)).unwrap_err().contains("binary"));
    let wide = dir.path().join("wide.txt");
    fs::write(&wide, [0xFF_u8, 0xFE, b'a', 0]).unwrap();
    assert!(read(&at(&wide)).unwrap_err().contains("UTF-16"));
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
    assert!(read(&at(&file)).unwrap().read_only);
}

#[test]
fn a_missing_or_remote_file_explains_itself() {
    assert!(read(&Location::new("/no/such/file.txt").unwrap()).is_err());
    assert!(read(&Location::new("fake://home/a.txt").unwrap()).is_err());
}

#[test]
fn the_majority_line_ending_wins() {
    assert!(mostly_crlf(b"a\r\nb\r\nc\n"));
    assert!(!mostly_crlf(b"a\nb\nc\r\n"));
    assert!(!mostly_crlf(b"no newline"));
}

#[cfg(unix)]
#[test]
fn saving_keeps_the_file_permissions() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join(".env");
    fs::write(&file, "KEY=1\n").unwrap();
    fs::set_permissions(&file, fs::Permissions::from_mode(0o600)).unwrap();
    let text = read(&at(&file)).unwrap();
    write(&at(&file), &save("KEY=2\n", false, text.stamp)).unwrap();
    let mode = fs::metadata(&file).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode, 0o600);
}
