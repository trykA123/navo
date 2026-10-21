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

const DELETE: u32 = 0x0001_0000;
const FILE_DELETE_CHILD: u32 = 0x40;
const WRITE_DAC: u32 = 0x0004_0000;
const WRITE_OWNER: u32 = 0x0008_0000;
const GENERIC_ALL: u32 = 0x1000_0000;
const GENERIC_WRITE: u32 = 0x4000_0000;

pub(super) const PARENT_FORBIDDEN: u32 =
    DELETE | FILE_DELETE_CHILD | WRITE_DAC | WRITE_OWNER | GENERIC_ALL | GENERIC_WRITE;

pub(super) fn parent_ace_allowed(mask: u32, privileged: bool) -> bool {
    privileged || mask & PARENT_FORBIDDEN == 0
}
