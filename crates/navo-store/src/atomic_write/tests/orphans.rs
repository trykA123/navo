use std::time::SystemTime;

use super::temp::{BACKUP_MARK, ORPHAN_AGE, TempFile, staged_owner, temp_owner};
use super::*;

fn other_pid() -> u32 {
    std::process::id().wrapping_add(1)
}

fn age(path: &Path, by: std::time::Duration) {
    let file = fs::OpenOptions::new().write(true).open(path).unwrap();
    file.set_modified(SystemTime::now() - by).unwrap();
}

fn age_folder(path: &Path) {
    let mut options = fs::OpenOptions::new();
    #[cfg(unix)]
    options.read(true);
    #[cfg(windows)]
    {
        use std::os::windows::fs::OpenOptionsExt;
        const FILE_WRITE_ATTRIBUTES: u32 = 0x100;
        const FILE_FLAG_BACKUP_SEMANTICS: u32 = 0x0200_0000;
        options
            .access_mode(FILE_WRITE_ATTRIBUTES)
            .custom_flags(FILE_FLAG_BACKUP_SEMANTICS);
    }
    let folder = options.open(path).unwrap();
    folder
        .set_modified(SystemTime::now() - ORPHAN_AGE * 2)
        .unwrap();
}

fn orphan(dir: &Path, name: &str) -> std::path::PathBuf {
    let path = dir.join(name);
    fs::write(&path, b"partial").unwrap();
    age(&path, ORPHAN_AGE * 2);
    path
}

#[test]
fn an_old_temp_of_another_process_is_removed() {
    let dir = tmp();
    let path = orphan(dir.path(), &format!(".navo.toml.tmp-{}-3", other_pid()));
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 1);
    assert!(!path.exists());
}

#[test]
fn a_temp_of_this_process_is_kept() {
    let dir = tmp();
    let path = orphan(
        dir.path(),
        &format!(".navo.toml.tmp-{}-3", std::process::id()),
    );
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 0);
    assert!(path.exists());
}

#[test]
fn a_recent_temp_of_another_process_is_kept() {
    let dir = tmp();
    let path = dir
        .path()
        .join(format!(".session.bin.tmp-{}-0", other_pid()));
    fs::write(&path, b"in flight").unwrap();
    age(&path, ORPHAN_AGE / 2);
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 0);
    assert!(path.exists());
}

#[test]
fn files_that_only_look_like_temps_are_kept() {
    let dir = tmp();
    let pid = other_pid();
    let names = [
        format!("navo.toml.tmp-{pid}-1"),
        format!(".navo.toml.tmp-{pid}-1.bak"),
        format!(".navo.toml.tmp-{pid}"),
        format!(".navo.toml.tmp-{pid}-"),
        format!(".navo.toml.tmp-+{pid}-1"),
        format!(".navo.toml.tmp-0{pid}-1"),
        format!(".navo.toml.tmp-{pid}-1-2"),
        format!(".tmp-{pid}-1"),
        ".navo.toml.tmp-abc-1".to_owned(),
        "notes.tmp".to_owned(),
        "navo.toml".to_owned(),
    ];
    let paths: Vec<_> = names.iter().map(|n| orphan(dir.path(), n)).collect();
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 0);
    assert!(paths.iter().all(|path| path.exists()));
}

#[test]
fn a_folder_with_a_temp_name_is_kept() {
    let dir = tmp();
    let folder = dir.path().join(format!(".navo.toml.tmp-{}-1", other_pid()));
    fs::create_dir(&folder).unwrap();
    age_folder(&folder);
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 0);
    assert!(folder.is_dir());
}

#[test]
fn temps_in_subfolders_are_kept() {
    let dir = tmp();
    let inner = dir.path().join("logs");
    fs::create_dir(&inner).unwrap();
    let path = orphan(&inner, &format!(".crash.json.tmp-{}-1", other_pid()));
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 0);
    assert!(path.exists());
}

#[test]
fn a_missing_folder_has_nothing_to_remove() {
    let dir = tmp();
    assert_eq!(remove_orphan_temps(&dir.path().join("gone")).unwrap(), 0);
}

#[test]
fn only_orphans_go_when_mixed_with_real_files() {
    let dir = tmp();
    fs::write(dir.path().join("session.bin"), b"live").unwrap();
    let stale = orphan(dir.path(), &format!(".session.bin.tmp-{}-9", other_pid()));
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 1);
    assert!(!stale.exists());
    assert_eq!(fs::read(dir.path().join("session.bin")).unwrap(), b"live");
}

#[test]
fn store_dirs_clean_both_config_and_data() {
    let dir = tmp();
    let dirs = crate::StoreDirs::in_profile(dir.path());
    dirs.ensure().unwrap();
    let pid = other_pid();
    let config = orphan(&dirs.config, &format!(".keymap.toml.tmp-{pid}-1"));
    let data = orphan(&dirs.data, &format!(".session.bin.tmp-{pid}-2"));
    assert_eq!(dirs.remove_orphan_temps().unwrap(), 2);
    assert!(!config.exists() && !data.exists());
}

#[test]
fn every_temp_name_this_module_makes_is_recognised_as_its_own() {
    let dir = tmp();
    for name in ["navo.toml", "session.v2.bin", "a.tmp-1-2", "noext"] {
        let temp = TempFile::reserve(&dir.path().join(name), ".tmp-");
        let file_name = temp.path().file_name().unwrap().to_str().unwrap();
        assert_eq!(temp_owner(file_name), Some(std::process::id()), "{name}");
    }
}

#[cfg(unix)]
#[test]
fn a_symlink_with_a_temp_name_is_kept_with_its_target() {
    let dir = tmp();
    let real = dir.path().join("real.toml");
    fs::write(&real, b"mine").unwrap();
    age(&real, ORPHAN_AGE * 2);
    let link = dir.path().join(format!(".navo.toml.tmp-{}-1", other_pid()));
    std::os::unix::fs::symlink(&real, &link).unwrap();
    let aged = std::process::Command::new("touch")
        .args(["-h", "-t", "200001010000"])
        .arg(&link)
        .status()
        .unwrap();
    assert!(aged.success());
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 0);
    assert!(fs::symlink_metadata(&link).is_ok());
    assert_eq!(fs::read(&real).unwrap(), b"mine");
}

#[test]
fn a_staged_backup_is_never_taken_for_a_temp() {
    let dir = tmp();
    let staged = TempFile::reserve(&dir.path().join("session.bin"), BACKUP_MARK);
    let name = staged.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(temp_owner(name), None);
}

#[test]
fn a_staged_backup_name_carries_its_process_id() {
    let dir = tmp();
    let staged = TempFile::reserve(&dir.path().join("session.bin"), BACKUP_MARK);
    let name = staged.path().file_name().unwrap().to_str().unwrap();
    assert_eq!(staged_owner(name), Some(std::process::id()));
}

#[test]
fn a_fresh_staged_backup_of_another_process_is_removed() {
    let dir = tmp();
    let path = dir
        .path()
        .join(format!(".session.bin.bak-tmp-{}-4", other_pid()));
    fs::write(&path, b"old").unwrap();
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 1);
    assert!(!path.exists());
}

#[test]
fn a_staged_backup_of_this_process_is_kept() {
    let dir = tmp();
    let path = orphan(
        dir.path(),
        &format!(".session.bin.bak-tmp-{}-4", std::process::id()),
    );
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 0);
    assert!(path.exists());
}

#[test]
fn files_that_only_look_like_staged_backups_are_kept() {
    let dir = tmp();
    let pid = other_pid();
    let names = [
        format!("session.bin.bak-tmp-{pid}-1"),
        format!(".session.bin.bak-tmp-{pid}-1.old"),
        format!(".session.bin.bak-tmp-0{pid}-1"),
        format!(".bak-tmp-{pid}-1"),
        ".session.bin.bak-tmp-abc-1".to_owned(),
        "session.bin.bak".to_owned(),
    ];
    let paths: Vec<_> = names.iter().map(|n| orphan(dir.path(), n)).collect();
    let folder = dir.path().join(format!(".session.bin.bak-tmp-{pid}-2"));
    fs::create_dir(&folder).unwrap();
    assert_eq!(remove_orphan_temps(dir.path()).unwrap(), 0);
    assert!(paths.iter().all(|path| path.exists()) && folder.is_dir());
}
