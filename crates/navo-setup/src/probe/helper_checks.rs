//! E4: the elevated helper's own checks. It trusts nothing the wizard measured: it re-resolves the
//! root folder by folder, judges every folder by its file id (never by path text) and refuses a
//! `--for` SID that is not the token's or the shell's user.

use std::io;
use std::path::Path;

use windows_sys::Win32::Foundation::ERROR_SHARING_VIOLATION;

use crate::elevate::{HelperArgs, HelperExit};
use crate::payload::platform::security;
use crate::placement::{Chain, DriveKind, inside, link_problem, refused_problem};

use super::chain::{HeldChain, OrdinalNames, refused, resolve_held};
use super::facts::{drive_kind, subst_drive};
use super::shell::{Found, Shell};

#[cfg(test)]
#[path = "helper_checks_tests.rs"]
mod tests;

/// Resolves `args.root` and runs the helper's checks; the chain is what the helper may create.
///
/// In order: a SID that is not the caller's or an unresolvable root is `Refused`; a root with
/// nothing missing is `Exists`; a reparse link is `Refused`; a nearest folder whose id is not
/// `args.parent` is `Changed`; a drive that is not local, a `subst` drive, a refused folder or a
/// root inside a user profile is `Refused`. A held folder that became a link since the walk is
/// `Refused`. A sharing violation is `Busy`, which a retry may pass; every other error refuses.
///
/// Every existing ancestor stays open, without delete sharing, for as long as the result lives.
pub fn helper_chain(args: &HelperArgs) -> Result<HeldChain, HelperExit> {
    checked_chain(args, drive_kind)
}

/// `helper_chain` with the drive-kind probe passed in, so tests can stand in a remote drive.
fn checked_chain(
    args: &HelperArgs,
    drive: fn(&Path) -> io::Result<DriveKind>,
) -> Result<HeldChain, HelperExit> {
    if !sid_is_caller(&args.user_sid) {
        return Err(HelperExit::Refused);
    }
    let held = resolve_held(&args.root).map_err(|error| exit_for(&error))?;
    let chain = &held.chain;
    if chain.missing.is_empty() {
        return Err(HelperExit::Exists);
    }
    // A walk that stopped at a link ends there, so its last folder is never the true parent.
    if chain.existing.iter().any(|link| link.reparse) {
        return Err(HelperExit::Refused);
    }
    let parent = chain.existing.last().ok_or(HelperExit::Refused)?;
    if parent.id != args.parent {
        return Err(HelperExit::Changed);
    }
    if !drive_is_safe(chain, drive) {
        return Err(HelperExit::Refused);
    }
    let folders = refused().map_err(|_| HelperExit::Refused)?;
    let names = OrdinalNames;
    if refused_problem(chain, &folders, &names).is_some()
        || folders
            .profiles
            .iter()
            .any(|folder| inside(chain, folder, &names))
    {
        return Err(HelperExit::Refused);
    }
    held.recheck().map_err(|error| exit_for(&error))?;
    Ok(held)
}

/// Maps a chain error to the helper's exit: another process holding a folder without sharing is
/// `Busy` (try again); anything else is `Refused`. Both fail closed.
fn exit_for(error: &io::Error) -> HelperExit {
    let busy =
        i32::try_from(ERROR_SHARING_VIOLATION).is_ok_and(|code| error.raw_os_error() == Some(code));
    if busy {
        HelperExit::Busy
    } else {
        HelperExit::Refused
    }
}

/// Whether `placement::link_problem` passes: no reparse link, no `subst`, a Fixed or Removable drive.
/// A drive whose kind cannot be read is unsafe.
fn drive_is_safe(chain: &Chain, drive: fn(&Path) -> io::Result<DriveKind>) -> bool {
    let Some(root) = chain.existing.first() else {
        return false;
    };
    let Ok(kind) = drive(&root.path) else {
        return false;
    };
    link_problem(chain, subst_drive(&root.path), kind).is_none()
}

/// Whether `sid` is this process's token user or the desktop shell's user (E1 review carry).
/// A well-formed group SID is neither; an error reading either user counts as no match.
fn sid_is_caller(sid: &str) -> bool {
    if security::user_sid().is_ok_and(|own| own == sid) {
        return true;
    }
    matches!(Shell::find(), Ok(Found::Shell(shell)) if shell.facts().sid == sid)
}
