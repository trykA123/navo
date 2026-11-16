use super::names::name_of;
use crate::Location;

const PROGRAM_FOLDERS: [&str; 3] = ["windows", "program files", "program files (x86)"];

#[must_use]
pub fn is_protected(location: &Location, extra: &[Location]) -> bool {
    if extra.iter().any(|folder| folder.same_place(location)) {
        return true;
    }
    if !location.is_local() {
        return false;
    }
    let Some(parent) = location.parent() else {
        return true;
    };
    let under_drive_root = parent.parent().is_none() && parent.as_str() != "/";
    under_drive_root
        && name_of(location).is_some_and(|name| {
            let name = name.display().to_lowercase();
            PROGRAM_FOLDERS.contains(&name.as_str())
        })
}
