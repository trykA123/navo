use std::path::{Path, PathBuf};

use super::MenuError;

pub(super) fn common_parent(paths: &[PathBuf]) -> Result<(), MenuError> {
    let (first, rest) = paths.split_first().ok_or(MenuError::Empty)?;
    let parent = folder_key(first)?;
    for path in rest {
        if folder_key(path)? != parent {
            return Err(MenuError::DifferentFolders);
        }
    }
    Ok(())
}

fn folder_key(path: &Path) -> Result<String, MenuError> {
    let absolute = std::path::absolute(path).map_err(|error| MenuError::Path {
        path: path.to_path_buf(),
        message: error.to_string(),
    })?;
    let normal: PathBuf = absolute.components().collect();
    let parent = normal
        .parent()
        .map(|folder| folder.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(if cfg!(windows) {
        parent.to_lowercase()
    } else {
        parent
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn paths(items: &[&str]) -> Vec<PathBuf> {
        items.iter().map(PathBuf::from).collect()
    }

    #[test]
    fn empty_selection_is_rejected() {
        assert_eq!(common_parent(&[]), Err(MenuError::Empty));
    }

    #[test]
    fn items_in_one_folder_share_a_parent() {
        let items = paths(&["/data/a.txt", "/data/b.txt", "/data/sub"]);
        assert_eq!(common_parent(&items), Ok(()));
    }

    #[test]
    fn items_in_two_folders_are_rejected() {
        let items = paths(&["/data/a.txt", "/other/b.txt"]);
        assert_eq!(common_parent(&items), Err(MenuError::DifferentFolders));
    }

    #[test]
    fn relative_paths_resolve_against_the_current_folder() {
        let items = paths(&["a.txt", "./b.txt"]);
        assert_eq!(common_parent(&items), Ok(()));
    }
}
