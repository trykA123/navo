//! E4: the elevated helper's own checks. It trusts nothing the wizard measured: it re-resolves the
//! root folder by folder, judges every folder by its file id (never by path text) and refuses a
//! `--for` SID that is not the token's or the shell's user.

use crate::elevate::{HelperArgs, HelperExit};
use crate::payload::platform::security;
use crate::placement::{Chain, inside, link_problem, refused_problem};

use super::chain::{OrdinalNames, refused, resolve_path};
use super::facts::{drive_kind, subst_drive};
use super::shell::{Found, Shell};

#[cfg(test)]
#[path = "helper_checks_tests.rs"]
mod tests;

/// Resolves `args.root` and runs the helper's checks; the chain is what the helper may create.
///
/// In order: a SID that is not the caller's or an unresolvable root is `Refused`; a root with
/// nothing missing is `Exists`; a nearest folder whose id is not `args.parent` is `Changed`; a
/// reparse link, a drive that is not local, a `subst` drive, a refused folder or a root inside a
/// user profile is `Refused`. Every error while checking refuses.
pub fn helper_chain(args: &HelperArgs) -> Result<Chain, HelperExit> {
    if !sid_is_caller(&args.user_sid) {
        return Err(HelperExit::Refused);
    }
    let chain = resolve_path(&args.root).map_err(|_| HelperExit::Refused)?;
    if chain.missing.is_empty() {
        return Err(HelperExit::Exists);
    }
    let parent = chain.existing.last().ok_or(HelperExit::Refused)?;
    if parent.id != args.parent {
        return Err(HelperExit::Changed);
    }
    if !drive_is_safe(&chain) {
        return Err(HelperExit::Refused);
    }
    let folders = refused().map_err(|_| HelperExit::Refused)?;
    let names = OrdinalNames;
    if refused_problem(&chain, &folders, &names).is_some()
        || folders
            .profiles
            .iter()
            .any(|folder| inside(&chain, folder, &names))
    {
        return Err(HelperExit::Refused);
    }
    Ok(chain)
}

/// Whether `placement::link_problem` passes: no reparse link, no `subst`, a Fixed or Removable drive.
/// A drive whose kind cannot be read is unsafe.
fn drive_is_safe(chain: &Chain) -> bool {
    let Some(root) = chain.existing.first() else {
        return false;
    };
    let Ok(drive) = drive_kind(&root.path) else {
        return false;
    };
    link_problem(chain, subst_drive(&root.path), drive).is_none()
}

/// Whether `sid` is this process's token user or the desktop shell's user (E1 review carry).
/// A well-formed group SID is neither; an error reading either user counts as no match.
fn sid_is_caller(sid: &str) -> bool {
    if security::user_sid().is_ok_and(|own| own == sid) {
        return true;
    }
    matches!(Shell::find(), Ok(Found::Shell(shell)) if shell.facts().sid == sid)
}
