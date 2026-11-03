//! Cheap hints about where virtual files come from (D104).

use std::path::PathBuf;

/// Whether the clipboard owner is the system's own `rdpclip.exe`. Compared by full path: any
/// other program with that name is not a remote session.
pub(super) fn is_rdpclip(exe: &str, system_root: &str) -> bool {
    let root = system_root.trim_end_matches(['\\', '/']);
    !root.is_empty()
        && exe.replace('/', "\\").eq_ignore_ascii_case(&format!(
            "{}\\System32\\rdpclip.exe",
            root.replace('/', "\\")
        ))
}

/// Whether a shell folder name lies inside a .zip file.
pub(super) fn inside_zip(name: &str) -> bool {
    zip_end(name).is_some()
}

/// The .zip file that holds a shell folder, when the name is a drive or network path.
pub(super) fn zip_file(name: &str) -> Option<PathBuf> {
    let file = name.get(..zip_end(name)?)?;
    let bytes = file.as_bytes();
    let drive = bytes.get(1) == Some(&b':') && bytes.get(2) == Some(&b'\\');
    (drive || file.starts_with("\\\\")).then(|| PathBuf::from(file))
}

/// The end of the first `.zip` name part.
fn zip_end(name: &str) -> Option<usize> {
    let lower = name.to_ascii_lowercase();
    lower.match_indices(".zip").find_map(|(at, found)| {
        let end = at + found.len();
        (end == lower.len() || lower[end..].starts_with('\\')).then_some(end)
    })
}
