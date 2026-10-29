use std::fs;

use rusty_core::Location;

use super::{hooks, resolve};

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
    assert_eq!(
        resolve(&full, &found),
        Some(std::path::PathBuf::from(&full))
    );
    assert!(resolve("edit.exe", &found).is_some());
    assert!(resolve("missing.exe", &found).is_none());
    assert!(resolve("", &found).is_none());
}

#[test]
fn a_bare_name_finds_its_exe_through_pathext() {
    let dir = tempfile::tempdir().unwrap();
    let full = exe(dir.path(), "notepad.exe");
    let found = resolve("notepad", &search(dir.path(), ".COM;.EXE"));
    assert_eq!(found, Some(std::path::PathBuf::from(full)));
}

#[test]
fn quotes_from_copy_as_path_are_dropped() {
    assert_eq!(
        super::clean(r#"  "C:\Program Files\x.exe" "#),
        r"C:\Program Files\x.exe"
    );
}

#[test]
fn a_chosen_editor_resolves_to_its_full_path() {
    let dir = tempfile::tempdir().unwrap();
    let program = exe(dir.path(), "edit.exe");
    let editor = hooks(|_| None);
    assert_eq!((editor.resolve)(&format!("\"{program}\"")), Ok(program));
}

#[test]
fn a_missing_program_is_refused() {
    let editor = hooks(|_| None);
    assert!((editor.resolve)("no/such/editor").is_err());
}

#[test]
fn only_local_files_are_opened() {
    let editor = hooks(|_| None);
    let remote = Location::new("fake://home/a.txt").unwrap();
    assert!((editor.open)("edit", &remote).is_err());
}

#[cfg(unix)]
#[test]
fn the_editor_starts_with_the_file() {
    let editor = hooks(|_| None);
    let file = Location::new("/tmp").unwrap();
    assert_eq!((editor.open)("true", &file), Ok(()));
}

#[test]
fn editor_discovery_is_lazy_and_keeps_its_first_result() {
    let dir = tempfile::tempdir().unwrap();
    let discover = super::suggest(vec![dir.path().join("edit.exe")]);
    let program = exe(dir.path(), "edit.exe");
    assert_eq!(discover(), Some(program.clone()));
    fs::remove_file(dir.path().join("edit.exe")).unwrap();
    assert_eq!(discover(), Some(program));
}
