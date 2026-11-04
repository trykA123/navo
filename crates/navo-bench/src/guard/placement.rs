//! Where a run root may be made. Drive roots are refused as parents: v1 makes no same-drive
//! copies of the owner's files, so a run never needs a folder at a drive root.

use std::ffi::OsStr;
use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use super::act::{Kind, NewItem, Stage};
use super::{
    Guard, GuardError, MARKER_FILE, OutputFolder, PathRole, ScratchRoot, act, act_platform, ledger,
    platform, resolve, write,
};

const ROLE: PathRole = PathRole::ScratchRoot;

pub(super) fn create(parent: &Path, owner: Option<&Path>) -> Result<ScratchRoot, GuardError> {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|_| GuardError::Clock(ROLE))?;
    let mut nonce = [0_u8; 16];
    getrandom::fill(&mut nonce).map_err(|_| GuardError::Entropy(ROLE))?;
    let name = format!("run-{}-{:032x}", now.as_nanos(), u128::from_le_bytes(nonce));
    create_named(parent, owner, &name, &super::forbidden_roots(None)?)
}

fn create_named(
    parent: &Path,
    owner: Option<&Path>,
    name: &str,
    forbidden: &[PathBuf],
) -> Result<ScratchRoot, GuardError> {
    let parent = resolve::resolve(parent, ROLE)?;
    check_parent(&parent.path, forbidden)?;
    let proposed = resolve::resolve(&parent.path.join(name), ROLE)?;
    let _owner = check_owner(&proposed.path, owner)?;
    let mut token = [0_u8; 32];
    getrandom::fill(&mut token).map_err(|_| GuardError::Entropy(ROLE))?;
    ledger::add(&parent.path, &token, ROLE)?;
    if write::make_dir(&proposed.path).is_err() {
        let _ = ledger::remove(&parent.path, &token, ROLE);
        return Err(GuardError::Io(ROLE));
    }
    let root = resolve::resolve(&proposed.path, ROLE)?;
    if !resolve::equal(&root.path, &proposed.path) {
        return Err(GuardError::Changed(ROLE));
    }
    let guard = Guard { root };
    write_marker(&guard, &token)?;
    let handle = platform::pin(&parent.path).map_err(|_| GuardError::Io(ROLE))?;
    platform::sync_directory(&handle).map_err(|_| GuardError::Io(ROLE))?;
    let pin = act_platform::pin_root(&guard.root.path).map_err(|_| GuardError::Io(ROLE))?;
    act::confirm_handle(&pin, &guard.root.path, ROLE)?;
    Ok(ScratchRoot { guard, token, pin })
}

pub(super) fn create_output(path: &Path, owner: Option<&Path>) -> Result<OutputFolder, GuardError> {
    create_output_with(path, owner, &super::forbidden_roots(None)?)
}

fn create_output_with(
    path: &Path,
    owner: Option<&Path>,
    forbidden: &[PathBuf],
) -> Result<OutputFolder, GuardError> {
    let role = PathRole::OutputFolder;
    let (Some(parent), Some(name)) = (path.parent(), path.file_name()) else {
        return Err(GuardError::InvalidPath(role));
    };
    let parent = resolve::resolve(parent, role)?;
    check_parent(&parent.path, forbidden)?;
    let proposed = resolve::resolve(&parent.path.join(name), role)?;
    let _owner = check_owner(&proposed.path, owner)?;
    let held = act::hold_container(&parent.path, role)?;
    let made =
        act::create_at(&held, Path::new(name), &proposed.path, Kind::HeldDir).map_err(|error| {
            match error {
                GuardError::Exists(_) => GuardError::Exists(role),
                other => other,
            }
        })?;
    match confirm_output(&made, &proposed.path) {
        Ok(root) => Ok(OutputFolder {
            guard: Guard { root },
            pin: made.handle,
        }),
        Err(error) => discard_output(&held, name, &made).and(Err(error)),
    }
}

/// Checks the folder the held create handle names is still exactly `path`; that handle stays
/// the pin, so there is no handoff in which the folder could move.
fn confirm_output(made: &NewItem, path: &Path) -> Result<resolve::Resolved, GuardError> {
    let role = PathRole::OutputFolder;
    act::hook(Stage::OutputHandoff);
    if act::confirm_handle(&made.handle, path, role)? != made.identity {
        return Err(GuardError::Changed(role));
    }
    let root = resolve::resolve(path, role)?;
    if !resolve::equal(&root.path, path) {
        return Err(GuardError::Changed(role));
    }
    Ok(root)
}

/// Removes the folder this call made through its create handle, only while `name` in the held
/// parent still is that folder; otherwise it is left and reported.
fn discard_output(held: &File, name: &OsStr, made: &NewItem) -> Result<(), GuardError> {
    let role = PathRole::OutputFolder;
    act_platform::remove_stray_at(held, name, &made.handle, role)
        .map_err(|_| GuardError::Left(role))
}

fn check_parent(parent: &Path, forbidden: &[PathBuf]) -> Result<(), GuardError> {
    if parent.parent().is_none() || forbidden.iter().any(|root| resolve::equal(root, parent)) {
        return Err(GuardError::ForbiddenParent(ROLE));
    }
    if !parent.is_dir() {
        return Err(GuardError::Io(ROLE));
    }
    Ok(())
}

fn check_owner(root: &Path, owner: Option<&Path>) -> Result<Option<resolve::Resolved>, GuardError> {
    let Some(owner) = owner else { return Ok(None) };
    let owner = resolve::resolve(owner, PathRole::OwnerFolder)?;
    if resolve::contains(&owner.path, root) || resolve::contains(root, &owner.path) {
        return Err(GuardError::OwnerOverlap(PathRole::OwnerFolder));
    }
    Ok(Some(owner))
}

#[allow(
    clippy::disallowed_methods,
    reason = "creates the marker exclusively after guard validation"
)]
fn write_marker(guard: &Guard, token: &[u8; 32]) -> Result<(), GuardError> {
    let marker = guard.check(&guard.root.path.join(MARKER_FILE))?;
    let mut file = File::create_new(&marker.resolved.path).map_err(|_| GuardError::Io(ROLE))?;
    file.write_all(token).map_err(|_| GuardError::Io(ROLE))?;
    file.sync_all().map_err(|_| GuardError::Io(ROLE))?;
    let directory = platform::pin(&guard.root.path).map_err(|_| GuardError::Io(ROLE))?;
    platform::sync_directory(&directory).map_err(|_| GuardError::Io(ROLE))
}

pub(crate) fn forbidden_roots(injected: Option<&[PathBuf]>) -> Result<Vec<PathBuf>, GuardError> {
    let roots = match injected {
        Some(roots) => roots.to_vec(),
        None => environment_roots()?,
    };
    roots
        .into_iter()
        .map(|root| resolve::resolve(&root, ROLE).map(|root| root.path))
        .collect()
}

#[cfg(windows)]
fn environment_roots() -> Result<Vec<PathBuf>, GuardError> {
    let mut roots = Vec::new();
    for name in ["SystemRoot", "ProgramFiles", "USERPROFILE"] {
        let value = std::env::var_os(name).ok_or(GuardError::Configuration(ROLE))?;
        roots.push(PathBuf::from(value));
    }
    for name in ["ProgramFiles(x86)", "ProgramW6432"] {
        if let Some(value) = std::env::var_os(name) {
            roots.push(PathBuf::from(value));
        }
    }
    Ok(roots)
}

#[cfg(not(windows))]
fn environment_roots() -> Result<Vec<PathBuf>, GuardError> {
    Ok(Vec::new())
}

#[cfg(test)]
pub(super) fn create_test_output(
    path: &Path,
    owner: Option<&Path>,
    forbidden: &[PathBuf],
) -> Result<OutputFolder, GuardError> {
    create_output_with(path, owner, forbidden)
}

#[cfg(test)]
pub(super) fn create_test_root(
    parent: &Path,
    owner: Option<&Path>,
    name: &str,
    forbidden: &[PathBuf],
) -> Result<ScratchRoot, GuardError> {
    create_named(parent, owner, name, forbidden)
}
