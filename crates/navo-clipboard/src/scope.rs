//! Folder identity shared by search and the notes store.

use std::path::Path;

/// The key two spellings of one folder share: `\` read as `/`, trailing `/` dropped,
/// Unicode lower case. A root made only of separators keys as `/`, never as empty.
pub fn folder_key(path: &Path) -> String {
    let lowered = path.to_string_lossy().replace('\\', "/").to_lowercase();
    let trimmed = lowered.trim_end_matches('/');
    if trimmed.is_empty() && !lowered.is_empty() {
        return "/".to_owned();
    }
    trimmed.to_owned()
}

/// Whether two paths name the same folder under [`folder_key`].
pub fn same_folder(left: &Path, right: &Path) -> bool {
    folder_key(left) == folder_key(right)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn slashes_trailing_separators_and_case_give_one_key() {
        let key = folder_key(Path::new(r"C:\Übung\a"));
        for spelling in [r"C:/übung/a", r"c:\ÜBUNG\A\", "C:/Übung/a//"] {
            assert_eq!(folder_key(Path::new(spelling)), key, "{spelling}");
        }
    }

    #[test]
    fn a_root_never_keys_as_empty() {
        assert_eq!(folder_key(Path::new("/")), "/");
        assert_eq!(folder_key(Path::new(r"\\")), "/");
        assert!(!same_folder(Path::new("/"), Path::new("")));
    }
}
