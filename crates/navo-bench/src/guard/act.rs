//! Verify-then-act: an existing item is re-opened by handle and checked before anything touches
//! it, and new items are created relative to held parent handles, never by path.

use std::ffi::OsStr;
use std::fs::File;
use std::path::{Component, Path, PathBuf};

use super::{CheckedPath, GuardError, PathRole, RecycleError, act_platform as platform, resolve};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) struct Identity {
    pub(super) volume: u64,
    pub(super) file: u128,
}

/// What an act may touch: a strict descendant of `container`, at `path`, with `identity`.
pub(super) struct Expected<'a> {
    pub(super) container: &'a Path,
    pub(super) path: &'a Path,
    pub(super) identity: Identity,
    pub(super) role: PathRole,
}

/// An item re-opened and verified by handle; the handle stays open until the act ends.
pub struct VerifiedItem {
    handle: File,
    path: PathBuf,
    identity: Identity,
}

impl VerifiedItem {
    /// The final path taken from the verified handle, never the caller's spelling.
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "used by the Windows recycler")
    )]
    pub(super) fn handle(&self) -> &File {
        &self.handle
    }

    #[cfg_attr(
        not(windows),
        allow(dead_code, reason = "used by the Windows recycler")
    )]
    pub(super) fn identity(&self) -> Identity {
        self.identity
    }
}

impl std::fmt::Debug for VerifiedItem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("VerifiedItem { scratch-root }")
    }
}

/// Moves one verified item to the Recycle Bin; never deletes permanently.
/// Sealed: only this crate implements it.
pub trait Recycler: sealed::Sealed {
    fn recycle(&self, item: &VerifiedItem) -> Result<(), RecycleError>;
}

pub(super) mod sealed {
    pub trait Sealed {}
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum Stage {
    BeforeAct,
    BeforeHashCheck,
    BeforeCreate,
    ParentHeld,
    AfterCreate,
    OutputHandoff,
}

/// What an act handle may do besides reading attributes.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Access {
    Inspect,
    Delete,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(super) enum Kind {
    File,
    Dir,
    /// A folder whose create handle also holds it: on Windows nobody else can rename or delete
    /// it while that handle lives.
    HeldDir,
}

/// A file or folder one create made, with the handle that create returned.
pub(super) struct NewItem {
    pub(super) handle: File,
    pub(super) identity: Identity,
}

/// Folders from a base handle down to an item's parent, each opened relative to the one above
/// without following links. On Windows none of them can be renamed or deleted while held.
struct Chain<'a> {
    base: &'a File,
    held: Vec<File>,
}

impl Chain<'_> {
    fn parent(&self) -> &File {
        self.held.last().unwrap_or(self.base)
    }
}

/// Holds each folder of `dirs` below `base`; a link or a non-folder anywhere is refused.
fn hold<'a>(base: &'a File, dirs: &[&OsStr], role: PathRole) -> Result<Chain<'a>, GuardError> {
    let mut chain = Chain {
        base,
        held: Vec::with_capacity(dirs.len()),
    };
    for name in dirs {
        let dir = platform::hold_dir_at(chain.parent(), name).map_err(|_| GuardError::Io(role))?;
        require_dir(&dir, role)?;
        chain.held.push(dir);
    }
    Ok(chain)
}

/// Holds `container`, opened by path, only when it is exactly that folder and not a link.
pub(super) fn hold_container(container: &Path, role: PathRole) -> Result<File, GuardError> {
    let handle = platform::hold_dir(container).map_err(|_| GuardError::Io(role))?;
    require_dir(&handle, role)?;
    if !resolve::equal(&platform::final_path(&handle, role)?, container) {
        return Err(GuardError::Changed(role));
    }
    Ok(handle)
}

fn require_dir(handle: &File, role: PathRole) -> Result<(), GuardError> {
    if platform::is_reparse(handle, role)? {
        return Err(GuardError::ReparsePoint(role));
    }
    if !handle
        .metadata()
        .map_err(|_| GuardError::Io(role))?
        .is_dir()
    {
        return Err(GuardError::Io(role));
    }
    Ok(())
}

/// The folder names from `container` down to `path`'s parent, and `path`'s own name.
fn below<'p>(
    container: &Path,
    path: &'p Path,
    role: PathRole,
) -> Result<(Vec<&'p OsStr>, &'p OsStr), GuardError> {
    if !strictly_inside(container, path) {
        return Err(GuardError::OutsideRoot(role));
    }
    plain_names(path.components().skip(container.components().count()), role)
}

fn plain_names<'p>(
    parts: impl Iterator<Item = Component<'p>>,
    role: PathRole,
) -> Result<(Vec<&'p OsStr>, &'p OsStr), GuardError> {
    let mut names = Vec::new();
    for part in parts {
        let Component::Normal(name) = part else {
            return Err(GuardError::InvalidPath(role));
        };
        names.push(name);
    }
    let name = names.pop().ok_or(GuardError::InvalidPath(role))?;
    Ok((names, name))
}

/// Creates `rel` below the held `base` through handles only: each parent is opened relative to
/// the one above and held, and the item is made relative to its parent. An item that is not at
/// `path` afterwards is removed through its own handle, or left and reported when that fails.
pub(super) fn create_at(
    base: &File,
    rel: &Path,
    path: &Path,
    kind: Kind,
) -> Result<NewItem, GuardError> {
    let role = PathRole::ScratchRoot;
    let (dirs, name) = plain_names(rel.components(), role)?;
    hook(Stage::BeforeCreate);
    let chain = hold(base, &dirs, role)?;
    hook(Stage::ParentHeld);
    let made = match kind {
        Kind::File => platform::create_file_at(chain.parent(), name),
        Kind::Dir => platform::create_dir_at(chain.parent(), name),
        Kind::HeldDir => platform::create_held_dir_at(chain.parent(), name),
    };
    let handle = made.map_err(|error| create_error(&error, role))?;
    hook(Stage::AfterCreate);
    match confirm_handle(&handle, path, role) {
        Ok(identity) => Ok(NewItem { handle, identity }),
        Err(error) => match platform::remove_stray_at(chain.parent(), name, &handle, role) {
            Ok(()) => Err(error),
            Err(_) => Err(GuardError::Left(role)),
        },
    }
}

fn create_error(error: &std::io::Error, role: PathRole) -> GuardError {
    if error.kind() == std::io::ErrorKind::AlreadyExists {
        GuardError::Exists(role)
    } else {
        GuardError::Io(role)
    }
}

/// Identity of an existing checked target, read from the handle the guard pinned.
pub(super) fn capture(checked: &CheckedPath<'_>, role: PathRole) -> Result<Identity, GuardError> {
    let handle = checked
        .resolved
        ._handles
        .last()
        .ok_or(GuardError::Missing(role))?;
    if !resolve::equal(&platform::final_path(handle, role)?, &checked.resolved.path) {
        return Err(GuardError::Missing(role));
    }
    platform::identity(handle, role)
}

/// Confirms a file created through `handle`; a stray made elsewhere is removed through it, or
/// left and reported when that fails.
pub(super) fn confirm_new_file(
    handle: &File,
    path: &Path,
    role: PathRole,
) -> Result<Identity, GuardError> {
    hook(Stage::AfterCreate);
    confirm_handle(handle, path, role).map_err(|error| match platform::remove_stray(handle, role) {
        Ok(()) => error,
        Err(_) => GuardError::Left(role),
    })
}

/// Removes a file this run just created, through the handle its create returned.
pub(super) fn discard_new_file(handle: &File, role: PathRole) -> Result<(), GuardError> {
    platform::remove_stray(handle, role)
}

/// Checks a handle is not a link and its final path is exactly `path`.
pub(super) fn confirm_handle(
    handle: &File,
    path: &Path,
    role: PathRole,
) -> Result<Identity, GuardError> {
    if platform::is_reparse(handle, role)? {
        return Err(GuardError::ReparsePoint(role));
    }
    if !resolve::equal(&platform::final_path(handle, role)?, path) {
        return Err(GuardError::Changed(role));
    }
    platform::identity(handle, role)
}

/// Re-opens the expected item relative to its held parent and checks it is still that item.
fn verify(
    expected: &Expected<'_>,
    parent: &File,
    name: &OsStr,
    access: Access,
) -> Result<VerifiedItem, GuardError> {
    let role = expected.role;
    let handle = platform::open_act_at(parent, name, access).map_err(|_| GuardError::Io(role))?;
    if platform::is_reparse(&handle, role)? {
        return Err(GuardError::ReparsePoint(role));
    }
    let path = platform::final_path(&handle, role)?;
    if !strictly_inside(expected.container, &path) {
        return Err(GuardError::OutsideRoot(role));
    }
    let identity = platform::identity(&handle, role)?;
    if !resolve::equal(&path, expected.path) || identity != expected.identity {
        return Err(GuardError::Changed(role));
    }
    Ok(VerifiedItem {
        handle,
        path,
        identity,
    })
}

/// Holds the container and every folder down to the item's parent, verifies the item and acts
/// on it; all of them stay held until `run` returns.
fn with_item<T>(
    expected: &Expected<'_>,
    access: Access,
    pin: Option<File>,
    run: impl FnOnce(&VerifiedItem) -> Result<T, GuardError>,
) -> Result<T, GuardError> {
    let role = expected.role;
    let container = hold_container(expected.container, role)?;
    let (dirs, name) = below(expected.container, expected.path, role)?;
    let chain = hold(&container, &dirs, role)?;
    let item = verify(expected, chain.parent(), name, access)?;
    act(&item, role, pin, run)
}

/// Runs `act` only while the handle still names the verified path and that path the same item.
/// `pin`, the caller's own hold on the item, is released only then.
fn act<T>(
    item: &VerifiedItem,
    role: PathRole,
    pin: Option<File>,
    run: impl FnOnce(&VerifiedItem) -> Result<T, GuardError>,
) -> Result<T, GuardError> {
    hook(Stage::BeforeAct);
    let now = platform::final_path(&item.handle, role)?;
    if !resolve::equal(&now, &item.path) {
        return Err(GuardError::Changed(role));
    }
    match platform::identity_at(&item.path) {
        Ok(Some(identity)) if identity == item.identity => {
            drop(pin);
            run(item)
        }
        Ok(_) => Err(GuardError::Changed(role)),
        Err(_) => Err(GuardError::Io(role)),
    }
}

/// Recycles the expected item, then requires that it is gone from its path.
pub(super) fn recycle(expected: &Expected<'_>, recycler: &dyn Recycler) -> Result<(), GuardError> {
    recycle_pinned(expected, None, recycler)
}

/// As [`recycle`]; `pin`, the caller's hold on the item itself, is released only after every
/// check, just before the recycler acts.
pub(super) fn recycle_pinned(
    expected: &Expected<'_>,
    pin: Option<File>,
    recycler: &dyn Recycler,
) -> Result<(), GuardError> {
    let role = expected.role;
    with_item(expected, Access::Inspect, pin, |item| {
        recycler
            .recycle(item)
            .map_err(|error| GuardError::Recycle(role, error))?;
        match platform::identity_at(&item.path) {
            Ok(Some(identity)) if identity == item.identity => {
                Err(GuardError::Recycle(role, RecycleError::StillPresent))
            }
            Ok(_) => Ok(()),
            Err(_) => Err(GuardError::Recycle(role, RecycleError::Io)),
        }
    })
}

/// Removes a file this run created itself; used only to discard a failed copy.
pub(super) fn discard(expected: &Expected<'_>) -> Result<(), GuardError> {
    with_item(expected, Access::Delete, None, |item| {
        platform::delete(item, expected.role)
    })
}

fn strictly_inside(container: &Path, path: &Path) -> bool {
    resolve::contains(container, path) && !resolve::equal(container, path)
}

pub(super) fn hook(stage: Stage) {
    #[cfg(test)]
    seam::run(stage);
    #[cfg(not(test))]
    let _ = stage;
}

#[cfg(test)]
pub(crate) mod seam {
    use std::cell::RefCell;

    use super::Stage;

    type Hook = Box<dyn FnOnce()>;

    thread_local! {
        static HOOKS: RefCell<Vec<(Stage, Hook)>> = const { RefCell::new(Vec::new()) };
    }

    /// Runs `hook` once, the next time this thread reaches `stage`.
    pub(crate) fn set(stage: Stage, hook: impl FnOnce() + 'static) {
        HOOKS.with(|hooks| hooks.borrow_mut().push((stage, Box::new(hook))));
    }

    pub(super) fn run(stage: Stage) {
        let found = HOOKS.with(|hooks| {
            let mut hooks = hooks.borrow_mut();
            let index = hooks.iter().position(|(at, _)| *at == stage)?;
            Some(hooks.remove(index).1)
        });
        if let Some(hook) = found {
            hook();
        }
    }
}
