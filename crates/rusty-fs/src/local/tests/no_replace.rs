use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use super::*;

const EPERM: i32 = 1;
const EIO: i32 = 5;
const ENOSYS: i32 = 38;
const EOPNOTSUPP: i32 = 95;

fn path(dir: &TempDir, name: &str) -> PathBuf {
    dir.path().join(name)
}

fn refusing(code: i32) -> impl Fn(&Path, &Path) -> io::Result<()> {
    move |_, _| Err(io::Error::from_raw_os_error(code))
}

#[test]
fn a_mount_without_hard_links_still_renames() {
    for code in [EPERM, EOPNOTSUPP, ENOSYS] {
        let (dir, _) = temp();
        fs::write(path(&dir, "a.txt"), b"abc").unwrap();
        platform::move_without_replace(&path(&dir, "a.txt"), &path(&dir, "c.txt"), refusing(code))
            .unwrap();
        assert_eq!(
            (
                path(&dir, "a.txt").exists(),
                fs::read(path(&dir, "c.txt")).ok()
            ),
            (false, Some(b"abc".to_vec())),
            "errno {code}"
        );
    }
}

#[test]
fn a_mount_without_hard_links_never_replaces_an_existing_name() {
    for code in [EPERM, EOPNOTSUPP, ENOSYS] {
        let (dir, _) = temp();
        fs::write(path(&dir, "a.txt"), b"abc").unwrap();
        fs::write(path(&dir, "b.txt"), b"keep").unwrap();
        let refused = platform::move_without_replace(
            &path(&dir, "a.txt"),
            &path(&dir, "b.txt"),
            refusing(code),
        );
        assert_eq!(
            (
                refused.map_err(|error| error.kind()),
                fs::read(path(&dir, "a.txt")).ok(),
                fs::read(path(&dir, "b.txt")).ok()
            ),
            (
                Err(io::ErrorKind::AlreadyExists),
                Some(b"abc".to_vec()),
                Some(b"keep".to_vec())
            ),
            "errno {code}"
        );
    }
}

#[test]
fn any_other_link_error_is_returned_without_moving() {
    let (dir, _) = temp();
    fs::write(path(&dir, "a.txt"), b"abc").unwrap();
    let failed =
        platform::move_without_replace(&path(&dir, "a.txt"), &path(&dir, "c.txt"), refusing(EIO));
    assert_eq!(
        (
            failed.map_err(|error| error.raw_os_error()),
            path(&dir, "a.txt").exists(),
            path(&dir, "c.txt").exists()
        ),
        (Err(Some(EIO)), true, false)
    );
}

#[test]
fn a_move_whose_old_name_cannot_be_removed_leaves_no_second_name() {
    let (dir, _) = temp();
    fs::create_dir(path(&dir, "locked")).unwrap();
    fs::create_dir(path(&dir, "open")).unwrap();
    fs::write(path(&dir, "locked/a.txt"), b"abc").unwrap();
    let readonly = fs::Permissions::from_mode(0o555);
    fs::set_permissions(path(&dir, "locked"), readonly).unwrap();
    let moved = platform::move_without_replace(
        &path(&dir, "locked/a.txt"),
        &path(&dir, "open/b.txt"),
        |old, new| fs::hard_link(old, new),
    );
    let found = (
        path(&dir, "locked/a.txt").exists(),
        path(&dir, "open/b.txt").exists(),
    );
    fs::set_permissions(path(&dir, "locked"), fs::Permissions::from_mode(0o755)).unwrap();
    if moved.is_ok() {
        return;
    }
    assert_eq!(found, (true, false));
}
