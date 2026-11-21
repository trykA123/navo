//! Tests for profile folder resolution and opening: NS4 and NS8 of the shell plan.

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use crate::brand;
use crate::paths::{Folders, PROFILE_VERSION, Profile, ProfileError, resolve};

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
            "{}-paths-{}-{name}-{count}",
            brand::SLUG,
            std::process::id()
        );
        let path = std::env::temp_dir().join(folder);
        std::fs::create_dir_all(&path).expect("temp folder");
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
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

/// An environment lookup that knows no variables.
fn no_vars(_name: &str) -> Option<String> {
    None
}

/// An environment lookup backed by a fixed table.
fn vars<'a>(pairs: &'a [(&'a str, &'a str)]) -> impl Fn(&str) -> Option<String> + 'a {
    move |name| {
        pairs
            .iter()
            .find(|(key, _)| *key == name)
            .map(|(_, value)| (*value).to_string())
    }
}

/// Profile folders under `config` and `data` inside `root`.
fn folders_in(root: &TempDir) -> Folders {
    Folders {
        config: root.join("config"),
        data: root.join("data"),
    }
}

/// Writes `text` to `data/profile.toml` under `root`.
fn write_marker(root: &TempDir, text: &str) {
    std::fs::create_dir_all(root.join("data")).expect("data folder");
    std::fs::write(root.join("data").join("profile.toml"), text).expect("profile file");
}

/// Reads the bytes of `data/profile.toml` under `root`.
fn read_marker(root: &TempDir) -> Vec<u8> {
    std::fs::read(root.join("data").join("profile.toml")).expect("profile file")
}

/// Lists the entry names in `path`, sorted.
fn names_in(path: &Path) -> Vec<String> {
    let mut names: Vec<String> = std::fs::read_dir(path)
        .expect("folder")
        .filter_map(|entry| entry.ok())
        .map(|entry| entry.file_name().to_string_lossy().into_owned())
        .collect();
    names.sort();
    names
}

#[test]
fn explicit_profile_puts_both_folders_there() {
    let dir = Path::new("/profiles/work");
    let other = resolve(Some(dir), false, &no_vars).expect("resolution");
    assert_eq!(other.config, dir);
    assert_eq!(other.data, dir);
    let windows = resolve(Some(dir), true, &no_vars).expect("resolution");
    assert_eq!(windows.config, dir);
    assert_eq!(windows.data, dir);
}

#[test]
fn windows_uses_appdata_and_localappdata() {
    let env = vars(&[
        ("APPDATA", "C:/Users/ana/AppData/Roaming"),
        ("LOCALAPPDATA", "C:/Users/ana/AppData/Local"),
    ]);
    let folders = resolve(None, true, &env).expect("resolution");
    assert_eq!(
        folders.config,
        PathBuf::from("C:/Users/ana/AppData/Roaming").join(brand::FOLDER)
    );
    assert_eq!(
        folders.data,
        PathBuf::from("C:/Users/ana/AppData/Local").join(brand::FOLDER)
    );
}

#[test]
fn windows_without_localappdata_is_no_home() {
    let env = vars(&[("APPDATA", "C:/Users/ana/AppData/Roaming")]);
    let result = resolve(None, true, &env);
    assert!(matches!(result, Err(ProfileError::NoHome)));
}

#[test]
fn other_hosts_use_xdg_variables() {
    let env = vars(&[
        ("HOME", "/home/ana"),
        ("XDG_CONFIG_HOME", "/cfg"),
        ("XDG_DATA_HOME", "/dat"),
    ]);
    let folders = resolve(None, false, &env).expect("resolution");
    assert_eq!(folders.config, PathBuf::from("/cfg").join(brand::SLUG));
    assert_eq!(folders.data, PathBuf::from("/dat").join(brand::SLUG));
}

#[test]
fn other_hosts_fall_back_to_home() {
    let env = vars(&[("HOME", "/home/ana")]);
    let folders = resolve(None, false, &env).expect("resolution");
    assert_eq!(
        folders.config,
        PathBuf::from("/home/ana/.config").join(brand::SLUG)
    );
    assert_eq!(
        folders.data,
        PathBuf::from("/home/ana/.local/share").join(brand::SLUG)
    );
}

#[test]
fn empty_variables_count_as_missing() {
    let env = vars(&[
        ("HOME", "/home/ana"),
        ("XDG_CONFIG_HOME", ""),
        ("XDG_DATA_HOME", ""),
    ]);
    let folders = resolve(None, false, &env).expect("resolution");
    assert_eq!(
        folders.config,
        PathBuf::from("/home/ana/.config").join(brand::SLUG)
    );
    let windows = vars(&[("APPDATA", ""), ("LOCALAPPDATA", "C:/local")]);
    assert!(matches!(
        resolve(None, true, &windows),
        Err(ProfileError::NoHome)
    ));
}

#[test]
fn no_variable_at_all_is_no_home() {
    assert!(matches!(
        resolve(None, false, &no_vars),
        Err(ProfileError::NoHome)
    ));
}

#[test]
fn missing_profile_creates_folders_and_version_file() {
    let root = TempDir::new("missing");
    Profile::open(folders_in(&root)).expect("open");
    assert!(root.join("config").is_dir());
    assert_eq!(
        names_in(&root.join("data")),
        [
            "cache",
            "crashes",
            "journal",
            "logs",
            "profile.toml",
            "sessions"
        ]
    );
    assert_eq!(
        read_marker(&root),
        format!("version = {PROFILE_VERSION}\n").into_bytes()
    );
}

#[test]
fn paths_point_into_the_folders() {
    let root = TempDir::new("paths");
    let profile = Profile::open(folders_in(&root)).expect("open");
    let config = root.join("config");
    let data = root.join("data");
    assert_eq!(profile.settings_file(), config.join("settings.toml"));
    assert_eq!(profile.sessions_dir(), data.join("sessions"));
    assert_eq!(profile.cache_dir(), data.join("cache"));
    assert_eq!(profile.journal_dir(), data.join("journal"));
    assert_eq!(profile.logs_dir(), data.join("logs"));
    assert_eq!(profile.crashes_dir(), data.join("crashes"));
}

#[test]
fn second_open_recreates_a_missing_data_folder() {
    let root = TempDir::new("second");
    let config = root.join("config");
    Profile::open(folders_in(&root)).expect("first open");
    std::fs::remove_dir_all(root.join("data").join("cache")).expect("remove cache");
    let before = read_marker(&root);
    Profile::open(Folders {
        config: config.clone(),
        data: root.join("data"),
    })
    .expect("second open");
    assert!(config.is_dir());
    assert!(root.join("data").join("cache").is_dir());
    assert_eq!(read_marker(&root), before);
}

#[test]
fn same_version_open_recreates_a_missing_config_folder() {
    let root = TempDir::new("config-missing");
    Profile::open(folders_in(&root)).expect("first open");
    std::fs::remove_dir_all(root.join("config")).expect("remove config");
    let before = read_marker(&root);
    Profile::open(folders_in(&root)).expect("second open");
    assert!(root.join("config").is_dir());
    assert_eq!(read_marker(&root), before);
}

#[test]
fn newer_profile_is_refused_and_left_unchanged() {
    let root = TempDir::new("newer");
    write_marker(&root, "version = 2\n");
    let before = read_marker(&root);
    let result = Profile::open(folders_in(&root));
    match result {
        Err(ProfileError::Newer { found, supported }) => {
            assert_eq!(found, 2);
            assert_eq!(supported, PROFILE_VERSION);
        }
        other => panic!("expected Newer, got {other:?}"),
    }
    assert_eq!(read_marker(&root), before);
    assert!(!root.join("config").exists());
    assert_eq!(names_in(&root.join("data")), ["profile.toml"]);
}

#[test]
fn broken_profile_is_refused_and_left_unchanged() {
    for (name, text) in [
        ("not-toml", "this is not toml [[\n"),
        ("no-version", "other = 1\n"),
        ("empty", ""),
        ("zero", "version = 0\n"),
    ] {
        let root = TempDir::new(name);
        write_marker(&root, text);
        let before = read_marker(&root);
        let marker = root.join("data").join("profile.toml");
        match Profile::open(folders_in(&root)) {
            Err(ProfileError::Broken { path }) => assert_eq!(path, marker),
            other => panic!("expected Broken, got {other:?}"),
        }
        assert_eq!(read_marker(&root), before);
        assert!(!root.join("config").exists());
        assert_eq!(names_in(&root.join("data")), ["profile.toml"]);
    }
}
