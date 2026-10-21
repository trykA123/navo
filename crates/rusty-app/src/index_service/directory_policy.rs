#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum Found {
    Directory,
    ReparsePoint,
    Other,
}

#[derive(Debug, PartialEq, Eq)]
pub(super) enum DirectoryAction {
    Accept,
    Recreate,
    Refuse(&'static str),
}

pub(super) fn directory_action(found: Found, trusted: bool, install: bool) -> DirectoryAction {
    match found {
        Found::ReparsePoint => DirectoryAction::Refuse("is a reparse point"),
        Found::Other => DirectoryAction::Refuse("is not a directory"),
        Found::Directory if trusted => DirectoryAction::Accept,
        Found::Directory if install => DirectoryAction::Recreate,
        Found::Directory => {
            DirectoryAction::Refuse("directory owner or protected ACL failed verification")
        }
    }
}

pub(super) fn apply(
    action: DirectoryAction,
    move_aside: impl FnOnce() -> std::io::Result<()>,
    create_fresh: impl FnOnce() -> std::io::Result<()>,
) -> std::io::Result<bool> {
    match action {
        DirectoryAction::Accept => Ok(false),
        DirectoryAction::Refuse(reason) => Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            reason,
        )),
        DirectoryAction::Recreate => {
            move_aside()?;
            create_fresh()?;
            Ok(true)
        }
    }
}
