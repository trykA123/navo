use std::fs;

use rusty_core::Location;

use super::{hooks, resolves};

fn exe(dir: &std::path::Path, name: &str) -> String {
    let path = dir.join(name);
    fs::write(&path, b"").unwrap();
    path.to_string_lossy().into_owned()
}

#[test]
fn a_full_path_or_a_name_on_the_search_path_resolves() {
    let dir = tempfile::tempdir().unwrap();
    let full = exe(dir.path(), "edit.exe");
    let search = dir.path().to_string_lossy().into_owned();
    assert!(resolves(&full, ""));
    assert!(resolves("edit.exe", &search));
    assert!(!resolves("missing.exe", &search));
    assert!(!resolves("", &search));
}

#[test]
fn a_chosen_editor_is_saved_and_read_back() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("ferrite.toml");
    let program = exe(dir.path(), "edit.exe");
    let first = hooks(Some(&settings), |_| None);
    assert_eq!(first.chosen, None);
    assert_eq!((first.remember)(&program), Ok(()));
    let again = hooks(Some(&settings), |_| None);
    assert_eq!(again.chosen, Some(program));
}

#[test]
fn a_missing_program_is_refused_and_not_saved() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("ferrite.toml");
    let first = hooks(Some(&settings), |_| None);
    assert!((first.remember)("no/such/editor").is_err());
    assert!(!settings.exists());
}

#[test]
fn only_local_files_are_opened() {
    let editor = hooks(None, |_| None);
    let remote = Location::new("fake://home/a.txt").unwrap();
    assert!((editor.open)("edit", &remote).is_err());
}

#[cfg(unix)]
#[test]
fn the_editor_starts_with_the_file() {
    let editor = hooks(None, |_| None);
    let file = Location::new("/tmp").unwrap();
    assert_eq!((editor.open)("true", &file), Ok(()));
}
