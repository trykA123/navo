//! Pane entry and exit for archives: which local files open as an `ArchiveFs` folder, and the
//! local folder a pane returns to when it leaves an archive root.

use std::path::Path;

use navo_core::{Location, ProviderPath};

use super::diagnostics::UnsafeReason;
use super::index;
use super::location::{location, parse};

/// File-name endings of the archive formats Navo can list (compared lowercase).
const ARCHIVE_ENDINGS: [&str; 10] = [
    ".zip",
    ".7z",
    ".tar",
    ".tar.gz",
    ".tgz",
    ".tar.xz",
    ".txz",
    ".tar.zst",
    ".tar.zstd",
    ".tzst",
];

/// True when `name` ends like a supported archive; a name check only, no file is read.
#[must_use]
pub fn has_archive_name(name: &str) -> bool {
    let name = name.to_ascii_lowercase();
    ARCHIVE_ENDINGS.iter().any(|ending| name.ends_with(ending))
}

/// The archive root location for a local `file` named like a supported archive.
/// None for other names, non-local locations and paths that are not valid UTF-8.
#[must_use]
pub fn archive_root(file: &Location) -> Option<Location> {
    if !file.is_local() {
        return None;
    }
    let path = file.path().as_str()?;
    if !has_archive_name(path) {
        return None;
    }
    location(Path::new(path), "").ok()
}

/// The local folder holding the archive and the archive's file name, when `root` is an archive
/// root location. None for folders inside an archive and for every other location.
#[must_use]
pub fn leave_archive(root: &Location) -> Option<(Location, String)> {
    let found = parse(root).ok()?;
    if !found.inner.is_empty() {
        return None;
    }
    let folder = found.archive.parent()?.to_str()?;
    let name = found.archive.file_name()?.to_str()?.to_owned();
    let folder = Location::local(ProviderPath::from_text(folder)).ok()?;
    Some((folder, name))
}

/// What a pane shows for a location inside an archive: the local archive file and the inner
/// folder names, outermost first. None for every location outside an archive.
#[must_use]
pub fn shown_parts(location: &Location) -> Option<(Location, Vec<String>)> {
    let found = parse(location).ok()?;
    let file = Location::local(ProviderPath::from_text(found.archive.to_str()?)).ok()?;
    let inner = String::from_utf8_lossy(&found.inner);
    let names = inner
        .split('/')
        .filter(|name| !name.is_empty())
        .map(str::to_owned)
        .collect();
    Some((file, names))
}

/// Readable text for a location inside an archive, `C:\work\a.zip\docs` style: the holding path,
/// the archive name and the inner folders, joined with the holding path's separator.
/// None for every location outside an archive.
#[must_use]
pub fn shown_path(location: &Location) -> Option<String> {
    let (file, names) = shown_parts(location)?;
    let mut text = file.path().display().into_owned();
    let separator = if text.contains('\\') { '\\' } else { '/' };
    for name in names {
        text.push(separator);
        text.push_str(&name);
    }
    Some(text)
}

/// Why an entry name listed inside an archive is unsafe to unpack; None for a safe name.
#[must_use]
pub fn unsafe_name(name: &str) -> Option<UnsafeReason> {
    index::unsafe_reason(name)
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use navo_core::{Location, ProviderPath};

    use super::{UnsafeReason, location, shown_parts, shown_path, unsafe_name};

    #[test]
    fn a_folder_inside_an_archive_reads_as_a_path() {
        let inside = location(Path::new("/work/a.zip"), "docs/deep").unwrap();
        assert_eq!(
            shown_path(&inside).as_deref(),
            Some("/work/a.zip/docs/deep")
        );
        let (file, names) = shown_parts(&inside).unwrap();
        assert_eq!(file.as_str(), "/work/a.zip");
        assert_eq!(names, ["docs", "deep"]);
        let root = location(Path::new("/work/a.zip"), "").unwrap();
        assert_eq!(shown_path(&root).as_deref(), Some("/work/a.zip"));
    }

    #[test]
    fn a_windows_archive_path_keeps_backslashes() {
        let inside = location(Path::new(r"C:\work\a.zip"), "docs").unwrap();
        assert_eq!(shown_path(&inside).as_deref(), Some(r"C:\work\a.zip\docs"));
    }

    #[test]
    fn a_plain_folder_is_not_an_archive_path() {
        let folder = Location::local(ProviderPath::from_text("/work")).unwrap();
        assert_eq!(shown_path(&folder), None);
    }

    #[test]
    fn unsafe_names_carry_their_reason() {
        assert_eq!(unsafe_name("../evil"), Some(UnsafeReason::Parent));
        assert_eq!(unsafe_name("CON"), Some(UnsafeReason::WindowsName));
        assert_eq!(unsafe_name("fine.txt"), None);
    }
}
