use std::sync::atomic::{AtomicBool, Ordering};

use rusty_core::{Location, ProviderPath};

const MAX_ENTRIES: usize = 100_000;
const EXTENSIONS: [&str; 3] = ["mdf", "map", "elf"];

pub(super) fn names(folder: &Location, cancel: &AtomicBool) -> Result<Vec<ProviderPath>, String> {
    let mut names = Vec::new();
    let mut counts = [0; 3];
    ensure_active(cancel)?;
    let entries = std::fs::read_dir(super::path_of(folder)?).map_err(|error| error.to_string())?;
    for (index, entry) in entries.enumerate() {
        ensure_active(cancel)?;
        if index >= MAX_ENTRIES {
            return Err("companion directory exceeds the entry limit".to_owned());
        }
        let entry = entry.map_err(|error| error.to_string())?;
        let Some((group, name)) = candidate(&entry)? else {
            continue;
        };
        if counts[group] < 2 {
            counts[group] += 1;
            names.push(name);
        }
    }
    ensure_active(cancel)?;
    Ok(names)
}

fn ensure_active(cancel: &AtomicBool) -> Result<(), String> {
    if cancel.load(Ordering::Relaxed) {
        return Err(std::io::Error::from(std::io::ErrorKind::Interrupted).to_string());
    }
    Ok(())
}

fn candidate(entry: &std::fs::DirEntry) -> Result<Option<(usize, ProviderPath)>, String> {
    let filename = entry.file_name();
    let extension = std::path::Path::new(&filename)
        .extension()
        .and_then(|extension| extension.to_str());
    let Some(group) = extension.and_then(|extension| {
        EXTENSIONS
            .iter()
            .position(|known| known.eq_ignore_ascii_case(extension))
    }) else {
        return Ok(None);
    };
    if entry
        .file_type()
        .map_err(|error| error.to_string())?
        .is_dir()
    {
        return Ok(None);
    }
    let name = crate::startup::path_location(&entry.path())
        .and_then(|location| location.name())
        .ok_or_else(|| "invalid companion file name".to_owned())?;
    Ok(Some((group, name)))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(path: &std::path::Path) -> Location {
        Location::new(path.to_string_lossy().into_owned()).unwrap()
    }

    #[test]
    fn companions_are_files_with_case_insensitive_known_extensions() {
        let dir = tempfile::tempdir().unwrap();
        for name in ["memory.MdF", "build.map", "app.elf", "ignored.txt"] {
            std::fs::write(dir.path().join(name), b"fixture").unwrap();
        }
        std::fs::create_dir(dir.path().join("folder.mdf")).unwrap();
        let hooks = crate::files::hooks(None);
        let mut found: Vec<_> = (hooks.companions)(&at(dir.path()), &AtomicBool::new(false))
            .unwrap()
            .into_iter()
            .map(|name| name.to_text())
            .collect();
        found.sort();
        assert_eq!(found, ["app.elf", "build.map", "memory.MdF"]);
    }

    #[test]
    fn several_files_still_make_a_type_ambiguous_without_retaining_every_name() {
        let dir = tempfile::tempdir().unwrap();
        for index in 0..5 {
            std::fs::write(dir.path().join(format!("{index}.mdf")), b"fixture").unwrap();
        }
        let found = names(&at(dir.path()), &AtomicBool::new(false)).unwrap();
        assert_eq!(found.len(), 2);
    }

    #[test]
    fn a_cancelled_scan_returns_no_partial_listing() {
        let dir = tempfile::tempdir().unwrap();
        assert!(names(&at(dir.path()), &AtomicBool::new(true)).is_err());
    }

    #[test]
    fn an_unreadable_folder_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        assert!(names(&at(&dir.path().join("missing")), &AtomicBool::new(false)).is_err());
    }
}
