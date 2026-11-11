//! The default fresh install root: `<per-user Programs folder>\Navo` and how to recognise it.

use std::path::{Path, PathBuf};

use crate::placement::{NameEq, same_path};

#[cfg(test)]
#[path = "default_root_tests.rs"]
mod tests;

/// The folder name Setup adds below the per-user Programs folder.
pub const DEFAULT_ROOT_NAME: &str = "Navo";

/// The default root below a known parent, which need not exist yet.
pub fn under(parent: &Path) -> PathBuf {
    parent.join(DEFAULT_ROOT_NAME)
}

/// Whether `root` is exactly the default root below `parent`, compared by `names`.
pub fn is_default_root(root: &Path, parent: &Path, names: &impl NameEq) -> bool {
    same_path(root, &under(parent), names)
}
