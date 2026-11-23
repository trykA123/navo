//! Decides before a delete whether Navo keeps a copy: a drive below "Stop below free space"
//! keeps no new deleted files (AD14), so the delete runs without one and the user is told.

use std::collections::HashSet;

use navo_core::retention::RetentionPolicy;
use navo_core::{FsError, Location};
use navo_fs::ProviderOps;

/// What a delete of one item can expect from the safety net.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum KeepCheck {
    /// Navo keeps a copy, as before.
    Keep,
    /// The drive cannot keep deleted files, or the item is gone.
    Unsupported,
    /// The drive has less free space than the policy minimum; no copy is kept.
    DriveFull,
}

/// Checks whether deleting `at` keeps a copy under `policy`: measures free space on its folder.
/// Unknown free space keeps as before; a missing item is unsupported; a failed query is an error.
pub fn check_keep(
    ops: &dyn ProviderOps,
    at: &Location,
    policy: RetentionPolicy,
) -> Result<KeepCheck, FsError> {
    Ok(keep_if_present(ops, at, policy)?.unwrap_or(KeepCheck::Unsupported))
}

/// Checks a delete of several items: each distinct folder is measured once, and the first one
/// that cannot keep decides for the whole delete. A vanished item holds nothing and is passed
/// over, as the job's own probe does; an empty list keeps; a failed query is an error.
pub fn check_keep_all<'a>(
    ops: &dyn ProviderOps,
    items: impl IntoIterator<Item = &'a Location>,
    policy: RetentionPolicy,
) -> Result<KeepCheck, FsError> {
    let mut measured = HashSet::new();
    for at in items {
        if at.parent().is_some_and(|parent| measured.contains(&parent)) {
            continue;
        }
        match keep_if_present(ops, at, policy)? {
            None => continue,
            Some(KeepCheck::Keep) => measured.extend(at.parent()),
            Some(check) => return Ok(check),
        }
    }
    Ok(KeepCheck::Keep)
}

/// Like `check_keep`, but `None` when `at` itself does not exist.
fn keep_if_present(
    ops: &dyn ProviderOps,
    at: &Location,
    policy: RetentionPolicy,
) -> Result<Option<KeepCheck>, FsError> {
    match crate::check_target_support(ops, at)? {
        None => return Ok(None),
        Some(false) => return Ok(Some(KeepCheck::Unsupported)),
        Some(true) => {}
    }
    let Some(parent) = at.parent() else {
        return Ok(Some(KeepCheck::Unsupported));
    };
    Ok(Some(match ops.available_space(&parent)? {
        Some(free) if free < policy.min_free_bytes() => {
            tracing::info!("drive below the free-space minimum; this delete keeps no copy");
            KeepCheck::DriveFull
        }
        _ => KeepCheck::Keep,
    }))
}
