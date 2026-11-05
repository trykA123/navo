//! The real probe. On Windows it also offers what a kept status needs: a subtree watch, the root's
//! volume class and its file identity.

use std::path::Path;

use navo_core::Location;
use navo_fs::DriveClass;

use super::Probe;

#[derive(Debug)]
pub(crate) struct SystemProbe;

impl Probe for SystemProbe {
    fn has_git(&self, folder: &Path) -> bool {
        std::fs::metadata(folder.join(".git")).is_ok_and(|m| m.is_file() || m.is_dir())
    }

    fn drive_class(&self, listing: &Location) -> DriveClass {
        navo_fs::drive_class(listing)
    }

    #[cfg(windows)]
    fn fingerprint(
        &self,
        root: &Path,
        upstream: Option<&str>,
    ) -> Option<navo_git::status::Fingerprint> {
        navo_git::status::fingerprint(root, upstream, std::time::SystemTime::now())
    }

    #[cfg(windows)]
    fn watch_tree(&self, root: &Path) -> Option<Box<dyn super::RepoWatch>> {
        use navo_fs::{LocalFs, Provider};

        let folder = Location::new(root.to_str()?).ok()?;
        let watch = LocalFs::new()
            .watch_tree(&folder, Box::new(|_: navo_fs::WatchEvent| {}))
            .ok()?;
        Some(Box::new(watch))
    }

    #[cfg(windows)]
    fn root_class(&self, root: &Path, listing: &Location) -> DriveClass {
        root_class_with(
            root,
            listing,
            navo_fs::volume_mount_point,
            navo_fs::drive_class,
        )
    }

    #[cfg(windows)]
    fn identity(&self, root: &Path) -> Option<navo_core::FileIdentity> {
        navo_fs::native_identity(root).ok().flatten()
    }
}

/// The listing's class, or `Unknown` unless every folder from the listing to `root` is plain
/// (no symlink or junction) and `root` lies on the volume of its own drive letter.
#[cfg(windows)]
fn root_class_with(
    root: &Path,
    listing: &Location,
    mount_point: impl FnOnce(&Path) -> std::io::Result<std::path::PathBuf>,
    class_of: impl FnOnce(&Location) -> DriveClass,
) -> DriveClass {
    let Ok(folder) = navo_fs::os_path(listing) else {
        return DriveClass::Unknown;
    };
    let plain = between(&folder, root).is_some_and(|levels| levels.into_iter().all(is_plain));
    if !plain || !mount_point(root).is_ok_and(|mount| is_drive_root_of(&mount, root)) {
        return DriveClass::Unknown;
    }
    class_of(listing)
}

/// The folders from the deeper of `a` and `b` up to the other, both included; `None` when
/// neither contains the other.
#[cfg(windows)]
fn between<'a>(a: &'a Path, b: &'a Path) -> Option<Vec<&'a Path>> {
    let (top, deep) = if b.starts_with(a) {
        (a, b)
    } else if a.starts_with(b) {
        (b, a)
    } else {
        return None;
    };
    Some(
        deep.ancestors()
            .take_while(|level| level.starts_with(top))
            .collect(),
    )
}

#[cfg(windows)]
fn is_plain(level: &Path) -> bool {
    std::fs::symlink_metadata(level).is_ok_and(|m| !m.file_type().is_symlink())
}

/// Whether `mount` is exactly `X:\`, where `X` is the drive letter of `root`.
#[cfg(windows)]
fn is_drive_root_of(mount: &Path, root: &Path) -> bool {
    use std::path::Component;

    let mut parts = mount.components();
    let shape = (parts.next(), parts.next(), parts.next());
    let bare = matches!(
        shape,
        (Some(Component::Prefix(_)), Some(Component::RootDir), None)
    );
    let letter = drive_letter(mount);
    bare && letter.is_some() && letter == drive_letter(root)
}

#[cfg(windows)]
fn drive_letter(path: &Path) -> Option<u8> {
    use std::path::{Component, Prefix};

    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return None;
    };
    match prefix.kind() {
        Prefix::Disk(letter) | Prefix::VerbatimDisk(letter) => Some(letter.to_ascii_uppercase()),
        _ => None,
    }
}

// Two attributes: clippy only treats a plain `cfg(test)` module as test code.
#[cfg(test)]
#[cfg(windows)]
#[path = "scan_tests/system_windows.rs"]
mod tests;
