use std::fs;
use std::path::PathBuf;

use tempfile::TempDir;

use super::*;
use crate::backup_path;

const USER: &str = "\
# My keys. Ctrl+F5 belongs to sorting by size.

[[bind]]
command = \"files.copy\"
keys = [\"alt-f5\"] # like the old manager

# Viewer
[[bind]]
command = \"viewer.back\"
keys = [\"backspace\"]
context = \"Tree\"

[[bind]]
command = \"files.sort_by_size\"
keys = [\"ctrl-f6\"]
";

fn set(command: &str, keys: &[&str]) -> KeymapEdit {
    KeymapEdit::Set {
        command: command.to_owned(),
        context: None,
        keys: keys.iter().map(|&key| key.to_owned()).collect(),
    }
}

fn reset(command: &str) -> KeymapEdit {
    KeymapEdit::Reset {
        command: command.to_owned(),
    }
}

fn patched(text: &str, edits: &[KeymapEdit]) -> String {
    patch_keymap(text, edits).expect("the keymap patches")
}

fn keymap_in(dir: &TempDir) -> PathBuf {
    dir.path().join("keymap.toml")
}

fn accept(text: &str) -> Result<String, String> {
    Ok(text.to_owned())
}

#[test]
fn new_keys_replace_only_the_keys_of_that_entry() {
    let edited = patched(USER, &[set("files.copy", &["ctrl-f5"])]);
    let expected = USER.replace(
        "keys = [\"alt-f5\"] # like the old manager",
        "keys = [\"ctrl-f5\"] # like the old manager",
    );
    assert_eq!(edited, expected);
}

#[test]
fn several_keys_are_written_as_one_array() {
    let edited = patched(USER, &[set("files.sort_by_size", &["ctrl-f6", "alt-s"])]);
    let expected = USER.replace("keys = [\"ctrl-f6\"]", "keys = [\"ctrl-f6\", \"alt-s\"]");
    assert_eq!(edited, expected);
}

#[test]
fn unbinding_writes_an_empty_key_list() {
    let edited = patched(USER, &[set("files.copy", &[])]);
    assert!(edited.contains("command = \"files.copy\"\nkeys = [] # like the old manager\n"));
}

#[test]
fn a_command_without_an_entry_gets_a_new_one_at_the_end() {
    let edited = patched(USER, &[set("files.move", &["alt-f6"])]);
    let added = "\n[[bind]]\ncommand = \"files.move\"\nkeys = [\"alt-f6\"]\n";
    assert_eq!(edited, format!("{USER}{added}"));
}

#[test]
fn a_new_entry_outside_the_home_context_names_its_context() {
    let edit = KeymapEdit::Set {
        command: "viewer.go_to_start".to_owned(),
        context: Some("Archive".to_owned()),
        keys: vec!["ctrl-home".to_owned()],
    };
    let edited = patched(USER, &[edit]);
    assert!(edited.ends_with(
        "[[bind]]\ncommand = \"viewer.go_to_start\"\nkeys = [\"ctrl-home\"]\ncontext = \"Archive\"\n"
    ));
}

#[test]
fn a_set_matches_only_the_entry_of_the_same_context() {
    let edited = patched(USER, &[set("viewer.back", &["escape"])]);
    assert!(edited.starts_with(USER), "{edited}");
    assert!(edited.ends_with("command = \"viewer.back\"\nkeys = [\"escape\"]\n"));
}

#[test]
fn reset_removes_the_entry_and_keeps_the_rest_byte_for_byte() {
    let edited = patched(USER, &[reset("files.sort_by_size")]);
    let gone = "\n[[bind]]\ncommand = \"files.sort_by_size\"\nkeys = [\"ctrl-f6\"]\n";
    assert_eq!(edited, USER.replace(gone, ""));
}

#[test]
fn reset_keeps_the_comments_above_the_removed_entry() {
    let edited = patched(USER, &[reset("files.copy")]);
    assert!(
        edited.starts_with("# My keys. Ctrl+F5 belongs to sorting by size.\n"),
        "{edited}"
    );
    assert!(!edited.contains("files.copy"), "{edited}");
    assert!(edited.contains("# Viewer\n[[bind]]\ncommand = \"viewer.back\""));
}

#[test]
fn reset_removes_every_entry_of_that_command() {
    let text = format!("{USER}\n[[bind]]\ncommand = \"viewer.back\"\nkeys = [\"q\"]\n");
    let edited = patched(&text, &[reset("viewer.back")]);
    assert!(!edited.contains("viewer.back"), "{edited}");
    assert!(edited.contains("files.sort_by_size"));
}

#[test]
fn reset_of_an_unknown_entry_changes_nothing() {
    assert_eq!(patched(USER, &[reset("files.move")]), USER);
    assert_eq!(patched("", &[reset("files.move")]), "");
}

#[test]
fn crlf_line_ends_are_kept() {
    let text = USER.replace('\n', "\r\n");
    let edited = patched(&text, &[set("files.move", &["alt-f6"])]);
    assert!(!edited.replace("\r\n", "").contains('\n'), "{edited:?}");
    assert!(edited.starts_with(&text));
}

#[test]
fn a_byte_order_mark_is_kept() {
    let text = format!("\u{feff}{USER}");
    let edited = patched(&text, &[set("files.copy", &["ctrl-f5"])]);
    assert!(edited.starts_with("\u{feff}# My keys."), "{edited:?}");
    assert!(edited.contains("keys = [\"ctrl-f5\"]"));
}

#[test]
fn edits_apply_in_order() {
    let edits = [
        set("files.sort_by_size", &["ctrl-f5"]),
        set("files.sort_by_date", &[]),
    ];
    let edited = patched(USER, &edits);
    assert!(edited.contains("command = \"files.sort_by_size\"\nkeys = [\"ctrl-f5\"]"));
    assert!(edited.ends_with("command = \"files.sort_by_date\"\nkeys = []\n"));
}

#[test]
fn keys_written_inline_are_refused() {
    let text = "bind = [{ command = \"files.copy\", keys = [\"alt-f5\"] }]\n";
    let error = patch_keymap(text, &[set("files.move", &["alt-f6"])]);
    assert_eq!(error, Err(KeymapFileError::Layout));
}

#[test]
fn a_missing_file_is_created_with_the_change() {
    let dir = TempDir::new().unwrap();
    let path = keymap_in(&dir);
    let mut file = KeymapFile::new(&path);
    let saved = file
        .save(&[set("files.copy", &["alt-f5"])], accept)
        .unwrap();
    let written = fs::read_to_string(&path).unwrap();
    assert_eq!(saved, written);
    assert!(
        written.ends_with("[[bind]]\ncommand = \"files.copy\"\nkeys = [\"alt-f5\"]\n"),
        "{written}"
    );
    assert!(!backup_path(&path).exists());
}

#[test]
fn a_reset_with_no_file_creates_nothing() {
    let dir = TempDir::new().unwrap();
    let path = keymap_in(&dir);
    let mut file = KeymapFile::new(&path);
    file.save(&[reset("files.copy")], accept).unwrap();
    assert!(!path.exists());
}

#[test]
fn the_first_save_of_a_session_keeps_one_backup() {
    let dir = TempDir::new().unwrap();
    let path = keymap_in(&dir);
    fs::write(&path, USER).unwrap();
    let mut file = KeymapFile::new(&path);
    file.save(&[set("files.copy", &["ctrl-f5"])], accept)
        .unwrap();
    file.save(&[set("files.copy", &["ctrl-f7"])], accept)
        .unwrap();
    assert_eq!(fs::read_to_string(backup_path(&path)).unwrap(), USER);
    assert!(fs::read_to_string(&path).unwrap().contains("ctrl-f7"));
}

#[test]
fn an_invalid_file_is_never_written() {
    let dir = TempDir::new().unwrap();
    let path = keymap_in(&dir);
    fs::write(&path, USER).unwrap();
    let mut file = KeymapFile::new(&path);
    let refuse = |_: &str| Err::<(), _>("line 3: unknown command".to_owned());
    let error = file.save(&[set("files.copy", &["ctrl-f5"])], refuse);
    assert_eq!(
        error,
        Err(KeymapFileError::FileInvalid(
            "line 3: unknown command".to_owned()
        ))
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), USER);
    assert!(!backup_path(&path).exists());
}

#[test]
fn a_change_the_check_rejects_is_not_written() {
    let dir = TempDir::new().unwrap();
    let path = keymap_in(&dir);
    fs::write(&path, USER).unwrap();
    let mut file = KeymapFile::new(&path);
    let only_the_old = |text: &str| {
        if text == USER {
            Ok(())
        } else {
            Err("`ctrl-f6` is given twice".to_owned())
        }
    };
    let error = file.save(&[set("files.copy", &["ctrl-f6"])], only_the_old);
    assert_eq!(
        error,
        Err(KeymapFileError::Rejected(
            "`ctrl-f6` is given twice".to_owned()
        ))
    );
    assert_eq!(fs::read_to_string(&path).unwrap(), USER);
}

#[test]
fn the_check_gets_the_text_that_is_written() {
    let dir = TempDir::new().unwrap();
    let path = keymap_in(&dir);
    fs::write(&path, USER).unwrap();
    let mut file = KeymapFile::new(&path);
    let length = file
        .save(&[reset("files.copy")], |text| Ok(text.len()))
        .unwrap();
    assert_eq!(length, fs::read_to_string(&path).unwrap().len());
}
