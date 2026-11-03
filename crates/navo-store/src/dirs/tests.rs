use std::collections::HashMap;
use std::path::{Path, PathBuf};

use super::*;

fn env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|&(k, v)| (k.to_owned(), v.to_owned()))
        .collect();
    move |name| map.get(name).cloned()
}

#[test]
fn windows_uses_roaming_for_config_and_local_for_data() {
    let dirs = StoreDirs::resolve(
        true,
        env(&[
            ("APPDATA", r"C:\Users\admin\AppData\Roaming"),
            ("LOCALAPPDATA", r"C:\Users\admin\AppData\Local"),
        ]),
    )
    .unwrap();
    assert_eq!(
        dirs.config,
        PathBuf::from(r"C:\Users\admin\AppData\Roaming").join("Navo")
    );
    assert_eq!(
        dirs.data,
        PathBuf::from(r"C:\Users\admin\AppData\Local").join("Navo")
    );
}

#[test]
fn windows_without_appdata_has_no_dirs() {
    assert!(StoreDirs::resolve(true, env(&[("LOCALAPPDATA", r"C:\x")])).is_none());
}

#[test]
fn unix_uses_xdg_dirs() {
    let dirs = StoreDirs::resolve(
        false,
        env(&[
            ("XDG_CONFIG_HOME", "/cfg"),
            ("XDG_DATA_HOME", "/data"),
            ("HOME", "/home/admin"),
        ]),
    )
    .unwrap();
    assert_eq!(dirs.config, PathBuf::from("/cfg/navo"));
    assert_eq!(dirs.data, PathBuf::from("/data/navo"));
}

#[test]
fn unix_falls_back_to_home() {
    let dirs = StoreDirs::resolve(false, env(&[("HOME", "/home/admin")])).unwrap();
    assert_eq!(dirs.config, PathBuf::from("/home/admin/.config/navo"));
    assert_eq!(dirs.data, PathBuf::from("/home/admin/.local/share/navo"));
}

#[test]
fn empty_variables_count_as_missing() {
    assert!(StoreDirs::resolve(false, env(&[("HOME", "")])).is_none());
}

#[test]
fn files_have_fixed_names() {
    let dirs = StoreDirs {
        config: PathBuf::from("/c"),
        data: PathBuf::from("/d"),
    };
    assert_eq!(dirs.settings_file(), PathBuf::from("/c/navo.toml"));
    assert_eq!(dirs.session_file(), PathBuf::from("/d/session.bin"));
}

#[test]
fn the_user_keymap_sits_beside_the_settings_file() {
    let dirs = StoreDirs {
        config: PathBuf::from("/c"),
        data: PathBuf::from("/d"),
    };
    assert_eq!(dirs.keymap_file(), PathBuf::from("/c/keymap.toml"));
    assert_eq!(dirs.keymap_file().parent(), dirs.settings_file().parent());
}

#[test]
fn ensure_creates_both_dirs() {
    let root = tempfile::tempdir().unwrap();
    let dirs = StoreDirs {
        config: root.path().join("c/navo"),
        data: root.path().join("d/navo"),
    };
    dirs.ensure().unwrap();
    assert!(dirs.config.is_dir() && dirs.data.is_dir());
}

#[test]
fn a_profile_folder_holds_both_dirs() {
    let root = PathBuf::from("/profile");
    let dirs = StoreDirs::in_profile(&root);
    assert_eq!(dirs.config, root.join("config"));
    assert_eq!(dirs.data, root.join("data"));
    assert!(dirs.settings_file().starts_with(&root));
    assert!(dirs.session_file().starts_with(&root));
}

fn sample() -> StoreDirs {
    StoreDirs {
        config: PathBuf::from("/c"),
        data: PathBuf::from("/d"),
    }
}

#[test]
fn journal_and_lock_files_use_the_navo_names() {
    assert_eq!(sample().journal_file(), PathBuf::from("/d/navo.db"));
    assert_eq!(sample().lock_file(), PathBuf::from("/d/navo.lock"));
}

#[test]
fn legacy_files_keep_the_legacy_names_in_the_same_dirs() {
    let dirs = sample();
    assert_eq!(
        dirs.legacy_settings_file(),
        PathBuf::from("/c/ferrite.toml")
    );
    assert_eq!(dirs.legacy_journal_file(), PathBuf::from("/d/ferrite.db"));
    assert_eq!(dirs.legacy_lock_file(), PathBuf::from("/d/ferrite.lock"));
}

#[test]
fn legacy_windows_roots_are_the_legacy_folders() {
    let dirs = StoreDirs::legacy(
        true,
        env(&[
            ("APPDATA", r"C:\Users\admin\AppData\Roaming"),
            ("LOCALAPPDATA", r"C:\Users\admin\AppData\Local"),
        ]),
    )
    .unwrap();
    assert_eq!(
        dirs.config,
        PathBuf::from(r"C:\Users\admin\AppData\Roaming").join("Ferrite")
    );
    assert_eq!(
        dirs.data,
        PathBuf::from(r"C:\Users\admin\AppData\Local").join("Ferrite")
    );
}

#[test]
fn legacy_unix_roots_follow_xdg_then_home() {
    let xdg = StoreDirs::legacy(
        false,
        env(&[("XDG_CONFIG_HOME", "/cfg"), ("XDG_DATA_HOME", "/data")]),
    )
    .unwrap();
    assert_eq!(xdg.config, PathBuf::from("/cfg/ferrite"));
    assert_eq!(xdg.data, PathBuf::from("/data/ferrite"));
    let home = StoreDirs::legacy(false, env(&[("HOME", "/home/admin")])).unwrap();
    assert_eq!(home.config, PathBuf::from("/home/admin/.config/ferrite"));
    assert_eq!(home.data, PathBuf::from("/home/admin/.local/share/ferrite"));
}

#[test]
fn legacy_with_empty_or_missing_variables_has_no_dirs() {
    assert!(StoreDirs::legacy(false, env(&[("HOME", "")])).is_none());
    assert!(StoreDirs::legacy(true, env(&[("LOCALAPPDATA", r"C:\x")])).is_none());
}

fn windows_vars(root: &Path) -> impl Fn(&str) -> Option<String> {
    let vars = HashMap::from([
        (
            "APPDATA".to_owned(),
            root.join("Roaming").to_string_lossy().into_owned(),
        ),
        (
            "LOCALAPPDATA".to_owned(),
            root.join("Local").to_string_lossy().into_owned(),
        ),
    ]);
    move |name| vars.get(name).cloned()
}

fn user_dirs(root: &Path) -> StoreDirs {
    StoreDirs::resolve(true, windows_vars(root)).unwrap()
}

fn install_exe(install: &Path) -> PathBuf {
    std::fs::create_dir_all(install.join("current")).unwrap();
    std::fs::write(install.join("Update.exe"), b"").unwrap();
    install.join("current/navo.exe")
}

#[test]
fn an_installed_exe_uses_the_user_folders() {
    let root = tempfile::tempdir().unwrap();
    let exe = install_exe(&root.path().join("Local/NavoApp"));
    assert_eq!(
        StoreDirs::detect_for(&exe, true, windows_vars(root.path())),
        Some(user_dirs(root.path()))
    );
}

#[test]
fn a_plain_exe_from_any_folder_uses_the_same_user_folders() {
    let root = tempfile::tempdir().unwrap();
    let exe = root.path().join("Downloads/navo.exe");
    assert_eq!(
        StoreDirs::detect_for(&exe, true, windows_vars(root.path())),
        Some(user_dirs(root.path()))
    );
}

#[test]
fn an_installed_exe_ignores_a_settings_file_next_to_it() {
    let root = tempfile::tempdir().unwrap();
    let exe = install_exe(&root.path().join("Local/NavoApp"));
    std::fs::write(exe.with_file_name("navo.toml"), b"").unwrap();
    assert_eq!(
        StoreDirs::detect_for(&exe, true, windows_vars(root.path())),
        Some(user_dirs(root.path()))
    );
}

#[test]
fn a_settings_file_beside_a_plain_exe_keeps_the_profile_there() {
    let root = tempfile::tempdir().unwrap();
    let portable = root.path().join("Portable");
    std::fs::create_dir_all(&portable).unwrap();
    std::fs::write(portable.join("navo.toml"), b"").unwrap();
    let dirs = StoreDirs::detect_for(&portable.join("navo.exe"), true, |_| None).unwrap();
    assert_eq!(dirs.config, portable);
    assert_eq!(dirs.data, dirs.config);
    assert_eq!(dirs.settings_file(), dirs.data.join("navo.toml"));
}

#[test]
fn a_marked_velopack_portable_root_keeps_one_profile_at_the_root() {
    let root = tempfile::tempdir().unwrap();
    let package = root.path().join("package");
    let exe = install_exe(&package);
    std::fs::write(package.join(".portable"), b"").unwrap();
    std::fs::write(exe.with_file_name("navo.toml"), b"").unwrap();
    let dirs = StoreDirs::detect_for(&exe, true, |_| None).unwrap();
    assert_eq!(dirs.config, package);
    assert_eq!(dirs.data, dirs.config);
}

#[test]
fn a_velopack_portable_copy_without_a_settings_marker_keeps_the_user_folders() {
    let root = tempfile::tempdir().unwrap();
    let package = root.path().join("package");
    let exe = install_exe(&package);
    std::fs::write(package.join(".portable"), b"").unwrap();
    assert_eq!(
        StoreDirs::detect_for(&exe, true, windows_vars(root.path())),
        Some(user_dirs(root.path()))
    );
}
