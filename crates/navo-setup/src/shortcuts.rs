//! IW11 shortcut removal: deletes only the exact unticked Navo links that point into the root.

#![cfg(windows)]
#![allow(
    unsafe_code,
    reason = "shell links, file dispositions and known folders require Win32 and COM"
)]

use std::io;
use std::path::{Component, Path, PathBuf};

use windows_sys::Win32::UI::Shell::{FOLDERID_Desktop, FOLDERID_StartMenu};
use windows_sys::core::GUID;

use crate::host::ShortcutReport;
use crate::placement::NameEq;
use crate::probe::chain::OrdinalNames;

#[path = "shortcuts_file.rs"]
mod files;
use files::{open_regular, read_link, set_disposition, target_inside};

#[cfg(test)]
#[path = "shortcuts_tests.rs"]
mod tests;

/// The report for an unticked-shortcut pass; failures are kept, never fatal.
pub type Report = ShortcutReport;

/// Removes the exact unticked Navo links that resolve inside `root`.
pub fn remove_unticked(root: &Path, desktop: bool, start_menu: bool) -> Report {
    remove_unticked_with(root, desktop, start_menu, crate::probe::chain::known_folder)
}

/// Removes the unticked links using an injectable known-folder lookup.
pub fn remove_unticked_with<F>(root: &Path, desktop: bool, start_menu: bool, known: F) -> Report
where
    F: Fn(&GUID) -> io::Result<Option<PathBuf>>,
{
    let mut report = Report::default();
    if !desktop {
        check_box(root, &known(&FOLDERID_Desktop), "Navo.lnk", &mut report);
    }
    if !start_menu {
        check_box(
            root,
            &known(&FOLDERID_StartMenu),
            "Programs\\Navo.lnk",
            &mut report,
        );
    }
    report
}

/// Resolves one known folder and checks exactly one named link below it.
fn check_box(root: &Path, base: &io::Result<Option<PathBuf>>, suffix: &str, report: &mut Report) {
    let Some(path) = base
        .as_ref()
        .ok()
        .and_then(|base| base.as_ref())
        .map(|base| base.join(suffix))
    else {
        return;
    };
    if remove_one(root, &path) {
        report.removed.push(path);
    } else {
        report.kept.push(path);
    }
}

/// Deletes one link only when it is a regular .lnk whose target is inside `root`.
fn remove_one(root: &Path, path: &Path) -> bool {
    if !path
        .extension()
        .is_some_and(|extension| extension.to_string_lossy().eq_ignore_ascii_case("lnk"))
    {
        return false;
    }
    let Ok(Some(file)) = open_regular(path) else {
        return false;
    };
    let Ok(bytes) = read_link(&file) else {
        return false;
    };
    if !target_inside(root, &bytes) {
        return false;
    }
    if set_disposition(&file).is_err() {
        return false;
    }
    drop(file);
    std::fs::symlink_metadata(path).is_err()
}

/// Whether `target` is the root itself or a path below it, compared by ordinal names.
fn inside(root: &Path, target: &Path) -> bool {
    if !root.is_absolute() || !target.is_absolute() {
        return false;
    }
    let root_parts: Vec<Component<'_>> = root.components().collect();
    let target_parts: Vec<Component<'_>> = target.components().collect();
    if root_parts
        .iter()
        .chain(&target_parts)
        .any(|part| matches!(part, Component::ParentDir | Component::CurDir))
    {
        return false;
    }
    if target_parts.len() < root_parts.len() {
        return false;
    }
    let names = OrdinalNames;
    root_parts
        .iter()
        .zip(&target_parts)
        .all(|(left, right)| component_same(left, right, &names))
}

/// Compares one root component with one target component using the ordinal name rule.
fn component_same(left: &Component<'_>, right: &Component<'_>, names: &OrdinalNames) -> bool {
    match (left, right) {
        (Component::Prefix(left), Component::Prefix(right)) => {
            names.same(left.as_os_str(), right.as_os_str())
        }
        (Component::RootDir, Component::RootDir) => true,
        (Component::Normal(left), Component::Normal(right)) => names.same(left, right),
        _ => false,
    }
}
