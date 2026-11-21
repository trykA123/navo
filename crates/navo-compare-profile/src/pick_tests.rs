//! Pick store tests: round trips, refusals, bad files and the size cap.

use std::fs;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::pick::{MAX_PICK_BYTES, PICK_VERSION, PickError, PickStore};

/// A unique temporary folder, removed when the test ends.
struct TempDir {
    path: PathBuf,
}

impl TempDir {
    /// Creates an empty folder named after the test and a counter.
    fn new(name: &str) -> TempDir {
        static COUNTER: AtomicU64 = AtomicU64::new(0);
        let count = COUNTER.fetch_add(1, Ordering::Relaxed);
        let folder = format!(
            "{}-pick-{}-{name}-{count}",
            crate::brand::SLUG,
            std::process::id()
        );
        let path = std::env::temp_dir().join(folder);
        fs::create_dir_all(&path).expect("temp folder");
        TempDir { path }
    }

    /// Returns the path of `name` inside the folder.
    fn join(&self, name: &str) -> PathBuf {
        self.path.join(name)
    }
}

impl Drop for TempDir {
    /// Removes the folder and everything in it, ignoring errors.
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.path);
    }
}

/// Returns a store whose pick folder is under `temp`.
fn store(temp: &TempDir) -> PickStore {
    PickStore::open(temp.join("pick"))
}

/// Returns an absolute path under the system temporary folder.
fn absolute(name: &str) -> PathBuf {
    std::env::temp_dir().join(name)
}

/// Writes `text` as the pick file of `store`, creating its folder.
fn write_pick(store: &PickStore, text: &str) {
    let file = store.file();
    fs::create_dir_all(file.parent().expect("pick folder")).expect("pick folder");
    fs::write(file, text).expect("write pick file");
}

#[test]
fn save_then_take_round_trips_and_moves_the_file_to_the_expected_name() {
    let temp = TempDir::new("round-trip");
    let store = store(&temp);
    let left = absolute("picked-left");
    assert_eq!(store.file(), temp.join("pick/explorer-pick.toml"));
    store.save(&left).expect("save");
    assert!(store.file().exists());
    assert_eq!(store.take().expect("take"), Some(left));
    assert!(!store.file().exists());
}

#[test]
fn save_replaces_the_previous_path() {
    let temp = TempDir::new("replace");
    let store = store(&temp);
    store.save(&absolute("first-left")).expect("first save");
    let second = absolute("second-left");
    store.save(&second).expect("second save");
    assert_eq!(store.take().expect("take"), Some(second));
}

#[test]
fn a_successful_take_leaves_nothing_for_a_second_take() {
    let temp = TempDir::new("twice");
    let store = store(&temp);
    store.save(&absolute("taken-left")).expect("save");
    assert!(store.take().expect("first take").is_some());
    assert!(store.take().expect("take").is_none());
}

#[test]
fn a_missing_folder_gives_none_and_is_not_created() {
    let temp = TempDir::new("missing");
    let store = store(&temp);
    assert!(store.take().expect("take").is_none());
    assert!(!temp.join("pick").exists());
}

#[test]
fn save_refuses_a_relative_path_and_writes_nothing() {
    let temp = TempDir::new("relative-save");
    let store = store(&temp);
    assert!(matches!(
        store.save(Path::new("relative/left")),
        Err(PickError::NotAbsolute)
    ));
    assert!(!temp.join("pick").exists());
    assert!(!store.file().exists());
}

#[cfg(unix)]
#[test]
fn save_refuses_a_non_unicode_path_and_writes_nothing() {
    use std::ffi::OsString;
    use std::os::unix::ffi::OsStringExt;

    let temp = TempDir::new("non-unicode");
    let store = store(&temp);
    let path = PathBuf::from("/").join(OsString::from_vec(vec![0x80]));
    assert!(matches!(store.save(&path), Err(PickError::NotUnicode)));
    assert!(!temp.join("pick").exists());
}

#[test]
fn garbage_text_is_removed_and_gives_none() {
    let temp = TempDir::new("garbage");
    let store = store(&temp);
    write_pick(&store, "this is not toml [");
    assert!(store.take().expect("take").is_none());
    assert!(!store.file().exists());
}

#[test]
fn a_document_without_version_is_removed_and_gives_none() {
    let temp = TempDir::new("no-version");
    let store = store(&temp);
    write_pick(&store, "left = \"/tmp/left\"\n");
    assert!(store.take().expect("take").is_none());
    assert!(!store.file().exists());
}

#[test]
fn a_newer_version_is_removed_and_gives_none() {
    let temp = TempDir::new("newer");
    let store = store(&temp);
    write_pick(&store, "version = 2\nleft = \"/tmp/left\"\n");
    assert!(store.take().expect("take").is_none());
    assert!(!store.file().exists());
}

#[test]
fn a_relative_left_is_removed_and_gives_none() {
    let temp = TempDir::new("relative-left");
    let store = store(&temp);
    write_pick(
        &store,
        &format!("version = {PICK_VERSION}\nleft = \"relative/left\"\n"),
    );
    assert!(store.take().expect("take").is_none());
    assert!(!store.file().exists());
}

#[test]
fn an_oversized_file_is_removed_without_being_parsed() {
    let temp = TempDir::new("oversized");
    let store = store(&temp);
    let size = usize::try_from(MAX_PICK_BYTES + 1).expect("fits in usize");
    write_pick(&store, &"x".repeat(size));
    assert!(store.take().expect("take").is_none());
    assert!(!store.file().exists());
}

#[test]
fn save_leaves_no_temp_file_behind() {
    let temp = TempDir::new("temp-clean");
    let store = store(&temp);
    store.save(&absolute("left")).expect("save");
    assert!(!temp.join("pick/explorer-pick.toml.tmp").exists());
}
