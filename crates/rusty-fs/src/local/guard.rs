use std::io;
use std::path::{Path, PathBuf};

use rusty_core::{FsError, FsErrorKind, Location};

use super::{error, io_error, os_path, platform};

pub fn mutation_location(location: &Location) -> Result<Location, FsError> {
    refuse_reserved(location)?;
    if !location.is_local() {
        return Ok(location.clone());
    }
    let path = os_path(location)?;
    let resolved = match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => resolve(parent).map(|parent| parent.join(name)),
        _ => std::fs::canonicalize(&path),
    }
    .map_err(|failure| io_error(&failure, location))?;
    let resolved = Location::local(platform::entry_name(resolved.as_os_str()))
        .map_err(|_| error(FsErrorKind::NameInvalid, location))?;
    refuse_reserved(&resolved)?;
    Ok(resolved)
}

pub fn save_location(location: &Location) -> Result<Location, FsError> {
    refuse_reserved(location)?;
    if !location.is_local() {
        return Ok(location.clone());
    }
    let resolved = resolve(&os_path(location)?).map_err(|failure| io_error(&failure, location))?;
    let resolved = Location::local(platform::entry_name(resolved.as_os_str()))
        .map_err(|_| error(FsErrorKind::NameInvalid, location))?;
    refuse_reserved(&resolved)?;
    Ok(resolved)
}

fn resolve(path: &Path) -> io::Result<PathBuf> {
    let mut missing = Vec::new();
    let mut parent = path;
    loop {
        match std::fs::canonicalize(parent) {
            Ok(mut resolved) => {
                for part in missing.iter().rev() {
                    resolved.push(part);
                }
                return Ok(resolved);
            }
            Err(failure) if failure.kind() == io::ErrorKind::NotFound => {
                if std::fs::symlink_metadata(parent).is_ok() {
                    return Err(failure);
                }
                missing.push(parent.file_name().ok_or(failure)?);
                parent = parent
                    .parent()
                    .ok_or_else(|| io::Error::other("no existing mutation parent"))?;
            }
            Err(failure) => return Err(failure),
        }
    }
}

pub(crate) fn refuse_reserved(location: &Location) -> Result<(), FsError> {
    if std::iter::successors(Some(location.clone()), Location::parent)
        .filter_map(|path| path.name())
        .any(|name| rusty_core::brand::is_reserved_name(&name.to_text()))
    {
        return Err(error(FsErrorKind::AccessDenied, location));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(path: &Path) -> Location {
        Location::new(path.to_string_lossy().into_owned()).unwrap()
    }

    #[test]
    fn reserved_retention_folders_refuse_mutations_and_saves() {
        let dir = tempfile::tempdir().unwrap();
        for folder in [
            ".navo-retained-ab",
            ".navo-held-1-2",
            ".NAVO-RETAINED-ab",
            ".ferrite-retained-ab",
            ".ferrite-held-1-2",
            ".navo-mutations-runtime",
            ".ferrite-mutations-runtime",
        ] {
            let held = dir.path().join(folder).join("payload");
            std::fs::create_dir_all(&held).unwrap();
            std::fs::write(held.join("a"), b"original").unwrap();
            for path in [
                dir.path().join(folder),
                held.join("a"),
                held.join("new"),
                held.join("new/folder/file"),
            ] {
                assert!(mutation_location(&at(&path)).is_err(), "{}", path.display());
                assert!(save_location(&at(&path)).is_err(), "{}", path.display());
            }
            assert_eq!(std::fs::read(held.join("a")).unwrap(), b"original");
        }
    }

    #[test]
    fn ordinary_folders_and_lookalike_names_stay_writable() {
        let dir = tempfile::tempdir().unwrap();
        for folder in ["docs", "navo-retained-ab", "notes.navo-held-1"] {
            std::fs::create_dir_all(dir.path().join(folder)).unwrap();
            let path = dir.path().join(folder).join("a");
            assert!(mutation_location(&at(&path)).is_ok(), "{folder}");
            assert!(save_location(&at(&path)).is_ok(), "{folder}");
        }
    }

    #[cfg(unix)]
    #[test]
    fn aliases_to_private_payloads_refuse_new_and_existing_mutations() {
        let dir = tempfile::tempdir().unwrap();
        let held = dir.path().join(".navo-retained-ab/payload");
        std::fs::create_dir_all(&held).unwrap();
        std::fs::write(held.join("a"), b"original").unwrap();
        std::os::unix::fs::symlink(&held, dir.path().join("alias")).unwrap();
        for name in ["a", "new", "new/folder/file"] {
            let location = Location::new(
                dir.path()
                    .join("alias")
                    .join(name)
                    .to_string_lossy()
                    .into_owned(),
            )
            .unwrap();
            assert!(mutation_location(&location).is_err(), "{name}");
        }
        assert_eq!(std::fs::read(held.join("a")).unwrap(), b"original");
    }
}
