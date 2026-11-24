//! Whether this install can update itself without rights it lacks (IW35). Velopack keeps its
//! packages in the install root only when the user can write it; a fallback folder means "no".

use std::path::Path;

/// True when the Velopack locator of this copy keeps its packages inside the install root. A copy
/// Velopack cannot locate (unpackaged, broken manifest) is true: `detect` reports it as before.
#[cfg(windows)]
pub(crate) fn current() -> bool {
    use velopack::locator::{LocationContext, auto_locate_app_manifest};
    match auto_locate_app_manifest(LocationContext::FromCurrentExe) {
        Ok(locator) => updates_allowed(&locator.get_root_dir(), &locator.get_packages_dir()),
        Err(_) => true,
    }
}

/// Always true off Windows: Velopack's root-writability fallback exists only there.
#[cfg(not(windows))]
pub(crate) fn current() -> bool {
    true
}

/// True when `packages` lies inside `root`, compared component by component without case, after
/// stripping a `\\?\` prefix from both. Works on Windows path text on any OS.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only Windows reads the Velopack locator")
)]
pub(crate) fn updates_allowed(root: &Path, packages: &Path) -> bool {
    let root = components(root);
    let packages = components(packages);
    root.len() <= packages.len() && root.iter().zip(&packages).all(|(r, p)| r == p)
}

/// Lower-cased, non-empty parts of `path` split on `\` and `/`, without a leading `\\?\`.
#[cfg_attr(
    all(not(windows), not(test)),
    expect(dead_code, reason = "only Windows reads the Velopack locator")
)]
fn components(path: &Path) -> Vec<String> {
    let text = path.to_string_lossy();
    let text = text.strip_prefix(r"\\?\").unwrap_or(&text);
    text.split(['\\', '/'])
        .filter(|part| !part.is_empty())
        .map(str::to_lowercase)
        .collect()
}

#[cfg(test)]
#[path = "root_access_tests.rs"]
mod tests;
