use std::fs;
use std::path::PathBuf;
use std::sync::mpsc;
use std::time::Duration;

use rusty_ui::commands::Keymap;
use rusty_ui::window_view::KeymapNews;

use super::{FILE, load, watch};

const NEWS_WAIT: Duration = Duration::from_secs(10);

fn file_in(dir: &tempfile::TempDir) -> PathBuf {
    dir.path().join(FILE)
}

fn applied(news: KeymapNews) -> (Keymap, Vec<String>) {
    match news {
        KeymapNews::Applied { keymap, warnings } => (keymap, warnings),
        KeymapNews::Invalid(message) => panic!("expected a keymap, got: {message}"),
    }
}

fn invalid(news: KeymapNews) -> String {
    match news {
        KeymapNews::Invalid(message) => message,
        KeymapNews::Applied { .. } => panic!("expected an error"),
    }
}

fn wait_live(watching: Option<&rusty_fs::Watch>) {
    let watching = watching.expect("the settings folder is watched");
    let deadline = std::time::Instant::now() + NEWS_WAIT;
    while !watching.is_live() && std::time::Instant::now() < deadline {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(watching.is_live());
}

fn entries(dir: &tempfile::TempDir) -> Vec<String> {
    let mut names: Vec<String> = fs::read_dir(dir.path())
        .expect("dir")
        .filter_map(Result::ok)
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn a_missing_file_means_the_built_in_keymap() {
    let dir = tempfile::tempdir().expect("dir");
    let (keymap, warnings) = applied(load(&file_in(&dir)));
    assert_eq!(&keymap, Keymap::builtin());
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(entries(&dir), Vec::<String>::new());
}

#[test]
fn a_valid_file_overrides_the_built_in_keys() {
    let dir = tempfile::tempdir().expect("dir");
    let text = "[[bind]]\ncommand = \"files.copy\"\nkeys = [\"alt-f5\"]\n";
    fs::write(file_in(&dir), text).expect("write");
    let (keymap, warnings) = applied(load(&file_in(&dir)));
    assert!(keymap.has_key("files.copy", "alt-f5"));
    assert!(!keymap.has_key("files.copy", "f5"));
    assert_eq!(warnings, Vec::<String>::new());
    assert_eq!(fs::read_to_string(file_in(&dir)).expect("read"), text);
}

#[test]
fn a_conflict_is_applied_and_warned_with_the_file_and_line() {
    let dir = tempfile::tempdir().expect("dir");
    let text = "\n[[bind]]\ncommand = \"files.sort_by_size\"\nkeys = [\"ctrl-f3\"]\n";
    fs::write(file_in(&dir), text).expect("write");
    let (keymap, warnings) = applied(load(&file_in(&dir)));
    assert!(keymap.has_key("files.sort_by_size", "ctrl-f3"));
    assert_eq!(warnings.len(), 1);
    let warning = &warnings[0];
    for part in [FILE, "line 2", "files.sort_by_size", "files.sort_by_name"] {
        assert!(warning.contains(part), "{warning}");
    }
}

#[test]
fn an_invalid_file_is_reported_and_kept_byte_for_byte() {
    let dir = tempfile::tempdir().expect("dir");
    let bytes = b"# mine\r\n[[bind]]\r\ncommand = \"files.teleport\"\r\nkeys = [\"f5\"]\r\n";
    fs::write(file_in(&dir), bytes).expect("write");
    let message = invalid(load(&file_in(&dir)));
    assert_eq!(fs::read(file_in(&dir)).expect("read"), bytes);
    assert_eq!(entries(&dir), [FILE]);
    assert!(message.starts_with(FILE), "{message}");
    assert!(message.contains("line 2"), "{message}");
    assert!(message.contains("files.teleport"), "{message}");
}

#[test]
fn an_upper_case_key_is_reported_with_its_line() {
    let dir = tempfile::tempdir().expect("dir");
    let text = "[[bind]]\ncommand = \"files.copy\"\nkeys = [\"F5\"]\n";
    fs::write(file_in(&dir), text).expect("write");
    let message = invalid(load(&file_in(&dir)));
    assert!(
        message.contains("line 1") && message.contains("f5"),
        "{message}"
    );
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn a_change_on_disk_is_read_validated_and_sent() {
    let dir = tempfile::tempdir().expect("dir");
    let path = file_in(&dir);
    let (sender, news) = mpsc::channel();
    let start = load(&path);
    let watching = watch(&path, start, move |news| {
        let _ = sender.send(news);
    });
    wait_live(watching.as_ref());
    fs::write(dir.path().join("ferrite.toml"), "").expect("write settings");
    let text = "[[bind]]\ncommand = \"files.copy\"\nkeys = [\"alt-f5\"]\n";
    fs::write(&path, text).expect("write keymap");
    let (keymap, _) = applied(news.recv_timeout(NEWS_WAIT).expect("news"));
    assert!(keymap.has_key("files.copy", "alt-f5"));
    let broken = "[[bind]]\ncommand = \"files.copy\"\nkeys = [\"F5\"]\n";
    fs::write(&path, broken).expect("write keymap");
    let message = invalid(news.recv_timeout(NEWS_WAIT).expect("news"));
    assert!(message.starts_with(FILE), "{message}");
    assert_eq!(fs::read_to_string(&path).expect("read"), broken);
    fs::remove_file(&path).expect("remove");
    let (keymap, _) = applied(news.recv_timeout(NEWS_WAIT).expect("news"));
    assert_eq!(&keymap, Keymap::builtin());
}

#[cfg(any(windows, target_os = "linux"))]
#[test]
fn the_same_content_is_not_sent_twice() {
    let dir = tempfile::tempdir().expect("dir");
    let path = file_in(&dir);
    let text = "[[bind]]\ncommand = \"files.copy\"\nkeys = [\"alt-f5\"]\n";
    fs::write(&path, text).expect("write keymap");
    let (sender, news) = mpsc::channel();
    let watching = watch(&path, load(&path), move |news| {
        let _ = sender.send(news);
    });
    wait_live(watching.as_ref());
    fs::write(&path, text).expect("write keymap");
    assert!(news.recv_timeout(Duration::from_secs(2)).is_err());
}
