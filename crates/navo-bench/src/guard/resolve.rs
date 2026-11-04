use std::fs::{self, File};
use std::path::{Component, Path, PathBuf};

use super::{GuardError, PathRole, platform};

pub(super) struct Resolved {
    pub(super) path: PathBuf,
    pub(super) _handles: Vec<File>,
}

pub(super) fn resolve(path: &Path, role: PathRole) -> Result<Resolved, GuardError> {
    validate(path, role)?;
    let mut current = PathBuf::new();
    let mut canonical = PathBuf::new();
    let mut handles: Vec<File> = Vec::new();
    let mut missing = false;
    for component in path.components() {
        current.push(component);
        if matches!(component, Component::Prefix(_)) {
            continue;
        }
        if missing {
            canonical.push(component);
            continue;
        }
        match fs::symlink_metadata(&current) {
            Ok(metadata) => {
                if platform::is_reparse(&metadata) {
                    return Err(GuardError::ReparsePoint(role));
                }
                let handle = platform::pin(&current).map_err(|_| GuardError::Io(role))?;
                if platform::is_reparse(&handle.metadata().map_err(|_| GuardError::Io(role))?) {
                    return Err(GuardError::ReparsePoint(role));
                }
                canonical = platform::final_path(&handle, &current, role)?;
                handles.push(handle);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                missing = true;
                canonical.push(component);
            }
            Err(_) => return Err(GuardError::Io(role)),
        }
    }
    Ok(Resolved {
        path: canonical,
        _handles: handles,
    })
}

fn validate(path: &Path, role: PathRole) -> Result<(), GuardError> {
    if !path.is_absolute() {
        return Err(GuardError::InvalidPath(role));
    }
    for component in path.components() {
        if matches!(component, Component::ParentDir) || !platform::valid_component(component) {
            return Err(GuardError::InvalidPath(role));
        }
    }
    Ok(())
}

pub(super) fn contains(root: &Path, path: &Path) -> bool {
    let mut candidate = path.components();
    root.components().all(|part| {
        candidate
            .next()
            .is_some_and(|other| platform::equal(part.as_os_str(), other.as_os_str()))
    })
}

pub(super) fn equal(left: &Path, right: &Path) -> bool {
    contains(left, right) && contains(right, left)
}
