#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Found {
    Directory,
    ReparsePoint,
    Other,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum DirectoryAction {
    Accept,
    Repair,
    Refuse(&'static str),
}

pub(super) fn directory_action(found: Found, trusted: bool, install: bool) -> DirectoryAction {
    match found {
        Found::ReparsePoint => DirectoryAction::Refuse("is a reparse point"),
        Found::Other => DirectoryAction::Refuse("is not a directory"),
        Found::Directory if trusted => DirectoryAction::Accept,
        Found::Directory if install => DirectoryAction::Repair,
        Found::Directory => {
            DirectoryAction::Refuse("directory owner or protected ACL failed verification")
        }
    }
}

pub(super) fn apply(
    action: DirectoryAction,
    path: &std::path::Path,
    take_ownership: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<bool> {
    match action {
        DirectoryAction::Accept => Ok(false),
        DirectoryAction::Refuse(reason) => Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            reason,
        )),
        DirectoryAction::Repair => {
            take_ownership()?;
            super::install_plan::clear_directory(path)?;
            Ok(true)
        }
    }
}
