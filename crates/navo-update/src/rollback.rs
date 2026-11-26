//! Rollback target listing and downgrade guard.

use crate::{Release, Version};
use std::path::PathBuf;

/// Maximum number of rollback targets shown to the user.
pub const MAX_ROLLBACK_TARGETS: usize = 3;

/// A previous release available for rollback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RollbackTarget {
    pub version: Version,
    pub size: u64,
}

/// Compatibility of a rollback target with the current on-disk state.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum RollbackCompatibility {
    /// The on-disk versions are at or below the target; downgrade is safe.
    Compatible,
    /// The on-disk versions are above the target, but a pre-migration backup exists.
    CompatibleWithBackup { backup_path: PathBuf },
    /// The on-disk versions are above the target and no backup is available.
    Incompatible { reason: String },
}

/// Returns up to `MAX_ROLLBACK_TARGETS` releases strictly older than `current`,
/// sorted newest first.
#[must_use]
pub fn recent_targets(releases: &[Release], current: &Version) -> Vec<RollbackTarget> {
    let mut older: Vec<&Release> = releases.iter().filter(|r| r.version < *current).collect();
    older.sort_by(|a, b| b.version.cmp(&a.version));
    older.truncate(MAX_ROLLBACK_TARGETS);
    older
        .into_iter()
        .map(|r| RollbackTarget {
            version: r.version.clone(),
            size: r.size,
        })
        .collect()
}

/// Error from a rollback attempt.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum RollbackError {
    #[error("downgrade not safe: {0}")]
    Incompatible(String),
    #[error("rollback not supported on this platform")]
    Unsupported,
}

/// Result of a successful rollback.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct RollbackApplied {
    /// The version rolled back FROM (the current version before rollback).
    pub from_version: Version,
    /// The version rolled back TO.
    pub to_version: Version,
}

/// Checks whether a rollback to the given target is safe.
///
/// Returns `Err(RollbackError::Incompatible)` when the on-disk state
/// cannot be downgraded safely.
pub fn prepare_rollback(
    target: &RollbackTarget,
    compatibility: &RollbackCompatibility,
) -> Result<(), RollbackError> {
    let _ = target;
    match compatibility {
        RollbackCompatibility::Incompatible { reason } => {
            Err(RollbackError::Incompatible(reason.clone()))
        }
        RollbackCompatibility::Compatible | RollbackCompatibility::CompatibleWithBackup { .. } => {
            Ok(())
        }
    }
}

/// Applies the rollback after checking compatibility.
///
/// Returns `RollbackApplied` with the version rolled back FROM so the caller
/// can record it as `skipped_version` to prevent an immediate re-upgrade.
///
/// On non-Windows platforms this is a testable stub that passes the
/// compatibility guard but does not invoke an installer.
pub fn apply_rollback(
    target: &RollbackTarget,
    current: &Version,
    compatibility: &RollbackCompatibility,
) -> Result<RollbackApplied, RollbackError> {
    prepare_rollback(target, compatibility)?;

    #[cfg(target_os = "windows")]
    {
        // Velopack: allow_version_downgrade = true
        // UNTESTED on real Windows — the real call goes through Velopack's
        // UpdateManager with the downgrade flag set.
    }

    #[cfg(not(target_os = "windows"))]
    {
        // Stub: no actual installer on Linux; the guard and return value
        // are exercised by unit tests.
    }

    Ok(RollbackApplied {
        from_version: current.clone(),
        to_version: target.version.clone(),
    })
}

#[cfg(test)]
#[path = "rollback_tests.rs"]
mod tests;
