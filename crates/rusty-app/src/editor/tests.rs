use std::fs;

use rusty_core::Location;

use super::{hooks, resolves};

fn exe(dir: &std::path::Path, name: &str) -> String {
    let path = dir.join(name);
    fs::write(&path, b"").unwrap();
    path.to_string_lossy().into_owned()
}

fn search(dir: &std::path::Path, extensions: &str) -> super::Search {
    super::Search {
        path: dir.to_string_lossy().into_owned(),
        extensions: extensions.to_owned(),
    }
}

#[test]
fn a_full_path_or_a_name_on_the_search_path_resolves() {
    let dir = tempfile::tempdir().unwrap();
    let full = exe(dir.path(), "edit.exe");
    let found = search(dir.path(), "");
    assert!(resolves(&full, &found));
    assert!(resolves("edit.exe", &found));
    assert!(!resolves("missing.exe", &found));
    assert!(!resolves("", &found));
}

#[test]
fn a_bare_name_finds_its_exe_through_pathext() {
    let dir = tempfile::tempdir().unwrap();
    exe(dir.path(), "notepad.exe");
    assert!(resolves("notepad", &search(dir.path(), ".COM;.EXE")));
    assert!(!resolves("notepad", &search(dir.path(), "")));
}

#[test]
fn quotes_from_copy_as_path_are_dropped() {
    assert_eq!(
        super::clean(r#"  "C:\Program Files\x.exe" "#),
        r"C:\Program Files\x.exe"
    );
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
fn a_settings_file_edited_by_hand_does_not_block_the_choice() {
    let dir = tempfile::tempdir().unwrap();
    let settings = dir.path().join("ferrite.toml");
    fs::write(&settings, "[files]\nshow-hidden = true\n").unwrap();
    let program = exe(dir.path(), "edit.exe");
    let editor = hooks(Some(&settings), |_| None);
    fs::write(&settings, "[files]\nshow-hidden = false\n").unwrap();
    assert_eq!((editor.remember)(&program), Ok(()));
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
