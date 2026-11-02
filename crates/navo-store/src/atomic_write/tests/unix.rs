use std::os::unix::fs::PermissionsExt;

use super::*;

struct ReadOnly<'a>(&'a Path);

impl<'a> ReadOnly<'a> {
    fn new(dir: &'a Path) -> Self {
        fs::set_permissions(dir, fs::Permissions::from_mode(0o555)).unwrap();
        Self(dir)
    }
}

impl Drop for ReadOnly<'_> {
    fn drop(&mut self) {
        let _ = fs::set_permissions(self.0, fs::Permissions::from_mode(0o755));
    }
}

fn read_only_folder_with_target() -> (TempDir, std::path::PathBuf) {
    let dir = tmp();
    let folder = dir.path().join("locked");
    fs::create_dir(&folder).unwrap();
    let target = folder.join("settings.toml");
    fs::write(&target, b"old").unwrap();
    (dir, target)
}

#[test]
fn read_only_folder_makes_the_write_fail() {
    let (_dir, target) = read_only_folder_with_target();
    let folder = target.parent().unwrap();
    let _lock = ReadOnly::new(folder);
    assert!(write_atomic(&target, b"new").is_err());
}

#[test]
fn read_only_folder_keeps_the_target_unchanged() {
    let (_dir, target) = read_only_folder_with_target();
    let folder = target.parent().unwrap();
    let _lock = ReadOnly::new(folder);
    let _ = write_atomic(&target, b"new");
    assert_eq!(fs::read(&target).unwrap(), b"old");
}

#[test]
fn every_write_keeps_the_file_mode() {
    let dir = tmp();
    let target = dir.path().join("settings.toml");
    fs::write(&target, b"old").unwrap();
    fs::set_permissions(&target, fs::Permissions::from_mode(0o600)).unwrap();
    write_atomic(&target, b"new").unwrap();
    let mode = |path: &Path| fs::metadata(path).unwrap().permissions().mode() & 0o777;
    assert_eq!(mode(&target), 0o600);
    replace_atomic(&target, b"newer").unwrap();
    assert_eq!(mode(&target), 0o600);
}
