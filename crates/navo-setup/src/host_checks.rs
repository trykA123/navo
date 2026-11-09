//! Windows placement rechecks shared by the fresh and existing pin paths.

use std::io;

use crate::placement::{self, Chain, Protected, Refused};
use crate::probe::chain;

/// Refuses a fresh chain with any existing reparse point or a root that exists.
pub(super) fn refuse_fresh(chain: &Chain) -> io::Result<()> {
    if chain.missing.is_empty() {
        return Err(io::Error::from(io::ErrorKind::AlreadyExists));
    }
    refuse_existing(chain)
}

/// Refuses a chain with a reparse point or a root that cannot be the install root.
pub(super) fn refuse_existing(chain: &Chain) -> io::Result<()> {
    if chain.existing.is_empty() || chain.existing.iter().any(|link| link.reparse) {
        return Err(io::Error::other(
            "install chain has a reparse point or no parent",
        ));
    }
    Ok(())
}

/// Refuses a root inside a never-inside folder or equal to a never-equal one.
/// `fresh` also refuses fresh-only folders: an upgrade of the legacy root is allowed (IW8a).
pub(super) fn refuse_existing_folders(
    chain: &Chain,
    refused: &Refused,
    fresh: bool,
) -> io::Result<()> {
    let names = chain::OrdinalNames;
    let fresh_only: &[Protected] = if fresh { &refused.fresh_only } else { &[] };
    let inside = refused
        .never_inside
        .iter()
        .chain(fresh_only)
        .any(|folder| placement::inside(chain, folder, &names));
    let equal = refused
        .never_equal
        .iter()
        .any(|folder| equal_root(chain, folder));
    if inside || equal {
        return Err(io::Error::other("install root is a protected folder"));
    }
    Ok(())
}

/// Whether the existing root is exactly one protected never-equal folder.
fn equal_root(chain: &Chain, protected: &Protected) -> bool {
    chain
        .existing
        .last()
        .is_some_and(|link| link.id == protected.anchor && protected.tail.is_empty())
}

/// Maps a placement refusal to an I/O error that keeps its exact reason.
pub(super) fn folder_error(problem: crate::syntax::FolderProblem) -> io::Error {
    crate::host::folder_error(problem)
}
