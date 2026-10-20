use rusty_core::rename::FieldProvider;

use super::super::FileFields;
use super::assert_all_safe;

const ALL: [&str; 5] = ["size", "size_kb", "created", "accessed", "attrs"];

fn dated(text: &str) -> bool {
    let shape = "0000-00-00 00.00.00";
    text.len() == shape.len()
        && text.chars().zip(shape.chars()).all(|(ch, s)| match s {
            '0' => ch.is_ascii_digit(),
            _ => ch == s,
        })
}

#[test]
fn size_fields_come_from_the_file_length() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("notes.txt");
    std::fs::write(&path, vec![b'x'; 2049]).unwrap();
    let values = FileFields.read(&path);
    assert_eq!(values.get("size"), Some("2049"));
    assert_eq!(values.get("size_kb"), Some("3"));
}

#[test]
fn an_empty_file_has_size_zero_and_no_attrs() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("empty.txt");
    std::fs::write(&path, b"").unwrap();
    let values = FileFields.read(&path);
    assert_eq!(values.get("size"), Some("0"));
    assert_eq!(values.get("size_kb"), Some("0"));
    let attrs = if cfg!(windows) { Some("A") } else { None };
    assert_eq!(values.get("attrs"), attrs);
}

#[test]
fn dates_have_no_colons_and_all_values_are_safe() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("a.bin");
    std::fs::write(&path, b"abc").unwrap();
    let values = FileFields.read(&path);
    if let Some(created) = values.get("created") {
        assert!(dated(created), "{created}");
    }
    assert!(dated(
        values.get("accessed").unwrap_or("0000-00-00 00.00.00")
    ));
    assert_all_safe(&values, &ALL);
}

#[cfg(unix)]
#[test]
fn attrs_report_read_only_and_hidden() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join(".secret");
    std::fs::write(&path, b"abc").unwrap();
    let mut permissions = std::fs::metadata(&path).unwrap().permissions();
    permissions.set_readonly(true);
    std::fs::set_permissions(&path, permissions).unwrap();
    assert_eq!(FileFields.read(&path).get("attrs"), Some("RH"));
}

#[test]
fn a_missing_file_gives_no_values() {
    let dir = tempfile::tempdir().unwrap();
    let values = FileFields.read(&dir.path().join("gone.txt"));
    for field in ALL {
        assert_eq!(values.get(field), None);
    }
}
