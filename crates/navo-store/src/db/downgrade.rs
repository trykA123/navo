use std::path::{Path, PathBuf};

use super::Db;
use crate::session::SESSION_VERSION;

/// Result of checking whether the on-disk state can safely downgrade to a
/// target minimum schema/session version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DowngradeCheck {
    /// The on-disk versions are at or below the target; downgrade is safe.
    Compatible,
    /// The on-disk versions are above the target, but a pre-migration backup
    /// file exists that can restore the data.
    CompatibleWithBackup { backup_path: PathBuf },
    /// The on-disk versions are above the target and no backup is available;
    /// downgrade is not safe.
    Incompatible { reason: String },
}

/// Checks whether the on-disk state (DB schema version, session format version,
/// and presence of a pre-migration backup) can safely downgrade to the given
/// target minimum versions.
///
/// `session_path` is the path to the session file; the function checks for a
/// sibling backup file (same name with `.bak` extension).
#[must_use]
pub fn check_downgrade(
    db_version: u32,
    session_version: u16,
    target_db_version: u32,
    target_session_version: u16,
    session_path: &Path,
) -> DowngradeCheck {
    let db_ok = db_version <= target_db_version;
    let session_ok = session_version <= target_session_version;

    if db_ok && session_ok {
        return DowngradeCheck::Compatible;
    }

    let backup = backup_path_for(session_path);
    if backup.exists() {
        return DowngradeCheck::CompatibleWithBackup {
            backup_path: backup,
        };
    }

    let mut reasons = Vec::new();
    if !db_ok {
        reasons.push(format!(
            "database schema version {db_version} is newer than target {target_db_version}"
        ));
    }
    if !session_ok {
        reasons.push(format!(
            "session format version {session_version} is newer than target {target_session_version}"
        ));
    }
    DowngradeCheck::Incompatible {
        reason: reasons.join("; "),
    }
}

/// Convenience: checks the current binary's DB and session versions against
/// target minimums, looking for a backup beside the given session path.
#[must_use]
pub fn check_downgrade_current(
    target_db_version: u32,
    target_session_version: u16,
    session_path: &Path,
) -> DowngradeCheck {
    check_downgrade(
        Db::VERSION,
        SESSION_VERSION,
        target_db_version,
        target_session_version,
        session_path,
    )
}

fn backup_path_for(session_path: &Path) -> PathBuf {
    let mut name = session_path.as_os_str().to_owned();
    name.push(".bak");
    PathBuf::from(name)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn compatible_when_versions_at_or_below_target() {
        let dir = tempfile::tempdir().expect("tempdir");
        let session = dir.path().join("session.bin");
        assert_eq!(
            check_downgrade(5, 3, 5, 3, &session),
            DowngradeCheck::Compatible
        );
        assert_eq!(
            check_downgrade(4, 2, 5, 3, &session),
            DowngradeCheck::Compatible
        );
    }

    #[test]
    fn compatible_with_backup_when_db_too_new() {
        let dir = tempfile::tempdir().expect("tempdir");
        let session = dir.path().join("session.bin");
        std::fs::write(&session, b"placeholder").expect("write");
        let backup = dir.path().join("session.bin.bak");
        std::fs::write(&backup, b"backup").expect("write backup");

        let result = check_downgrade(10, 3, 5, 3, &session);
        assert_eq!(
            result,
            DowngradeCheck::CompatibleWithBackup {
                backup_path: backup
            }
        );
    }

    #[test]
    fn compatible_with_backup_when_session_too_new() {
        let dir = tempfile::tempdir().expect("tempdir");
        let session = dir.path().join("session.bin");
        std::fs::write(&session, b"placeholder").expect("write");
        let backup = dir.path().join("session.bin.bak");
        std::fs::write(&backup, b"backup").expect("write backup");

        let result = check_downgrade(5, 10, 5, 3, &session);
        assert_eq!(
            result,
            DowngradeCheck::CompatibleWithBackup {
                backup_path: backup
            }
        );
    }

    #[test]
    fn incompatible_when_no_backup_and_versions_too_new() {
        let dir = tempfile::tempdir().expect("tempdir");
        let session = dir.path().join("session.bin");

        let result = check_downgrade(10, 7, 5, 3, &session);
        match result {
            DowngradeCheck::Incompatible { reason } => {
                assert!(reason.contains("database schema version 10"));
                assert!(reason.contains("session format version 7"));
            }
            other => panic!("expected Incompatible, got {other:?}"),
        }
    }

    #[test]
    fn incompatible_mentions_only_db_when_session_ok() {
        let dir = tempfile::tempdir().expect("tempdir");
        let session = dir.path().join("session.bin");

        let result = check_downgrade(10, 3, 5, 3, &session);
        match result {
            DowngradeCheck::Incompatible { reason } => {
                assert!(reason.contains("database schema version 10"));
                assert!(!reason.contains("session format"));
            }
            other => panic!("expected Incompatible, got {other:?}"),
        }
    }

    #[test]
    fn current_check_uses_binary_versions() {
        let dir = tempfile::tempdir().expect("tempdir");
        let session = dir.path().join("session.bin");
        let result = check_downgrade_current(Db::VERSION, SESSION_VERSION, &session);
        assert_eq!(result, DowngradeCheck::Compatible);
    }
}
