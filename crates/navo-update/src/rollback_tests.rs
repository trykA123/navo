use super::*;
use crate::{Release, Version};
use std::path::PathBuf;

fn v(text: &str) -> Version {
    crate::parse_version(text).expect("valid version")
}

fn release(text: &str, size: u64) -> Release {
    Release {
        version: v(text),
        size,
    }
}

fn target(text: &str, size: u64) -> RollbackTarget {
    RollbackTarget {
        version: v(text),
        size,
    }
}

// ── recent_targets ──────────────────────────────────────────────────

#[test]
fn recent_targets_filters_out_current_and_newer() {
    let releases = [
        release("0.9.0", 10),
        release("0.9.1", 10),
        release("0.9.3", 10), // current
        release("0.9.4", 10), // newer
        release("1.0.0", 10), // newer
    ];
    let targets = recent_targets(&releases, &v("0.9.3"));
    assert_eq!(targets.len(), 2);
    assert_eq!(targets[0].version, v("0.9.1"));
    assert_eq!(targets[1].version, v("0.9.0"));
}

#[test]
fn recent_targets_caps_at_three_newest_first() {
    let releases = [
        release("0.8.0", 10),
        release("0.8.1", 10),
        release("0.9.0", 10),
        release("0.9.1", 10),
        release("0.9.2", 10),
    ];
    let targets = recent_targets(&releases, &v("0.9.3"));
    assert_eq!(targets.len(), MAX_ROLLBACK_TARGETS);
    assert_eq!(targets[0].version, v("0.9.2"));
    assert_eq!(targets[1].version, v("0.9.1"));
    assert_eq!(targets[2].version, v("0.9.0"));
}

#[test]
fn recent_targets_empty_when_no_older_releases() {
    let releases = [release("0.9.3", 10), release("1.0.0", 10)];
    let targets = recent_targets(&releases, &v("0.9.3"));
    assert!(targets.is_empty());
}

#[test]
fn recent_targets_empty_input() {
    let targets = recent_targets(&[], &v("0.9.3"));
    assert!(targets.is_empty());
}

#[test]
fn recent_targets_preserves_size() {
    let releases = [release("0.9.0", 42)];
    let targets = recent_targets(&releases, &v("0.9.3"));
    assert_eq!(targets[0].size, 42);
}

// ── prepare_rollback ────────────────────────────────────────────────

#[test]
fn prepare_refuses_incompatible() {
    let t = target("0.9.0", 10);
    let compat = RollbackCompatibility::Incompatible {
        reason: "database schema version 10 is newer than target 5".into(),
    };
    let result = prepare_rollback(&t, &compat);
    assert_eq!(
        result,
        Err(RollbackError::Incompatible(
            "database schema version 10 is newer than target 5".into()
        ))
    );
}

#[test]
fn prepare_accepts_compatible() {
    let t = target("0.9.0", 10);
    assert_eq!(
        prepare_rollback(&t, &RollbackCompatibility::Compatible),
        Ok(())
    );
}

#[test]
fn prepare_accepts_compatible_with_backup() {
    let t = target("0.9.0", 10);
    let compat = RollbackCompatibility::CompatibleWithBackup {
        backup_path: PathBuf::from("/tmp/session.bin.bak"),
    };
    assert_eq!(prepare_rollback(&t, &compat), Ok(()));
}

// ── apply_rollback ──────────────────────────────────────────────────

#[test]
fn apply_refuses_incompatible() {
    let t = target("0.9.0", 10);
    let compat = RollbackCompatibility::Incompatible {
        reason: "schema too new".into(),
    };
    let result = apply_rollback(&t, &v("0.9.3"), &compat);
    assert!(matches!(result, Err(RollbackError::Incompatible(_))));
}

#[test]
fn apply_success_returns_from_version_for_skip() {
    let t = target("0.9.0", 10);
    let compat = RollbackCompatibility::Compatible;
    let applied = apply_rollback(&t, &v("0.9.3"), &compat).expect("compatible succeeds");
    assert_eq!(applied.from_version, v("0.9.3"));
    assert_eq!(applied.to_version, v("0.9.0"));
}

#[test]
fn apply_success_with_backup_also_returns_versions() {
    let t = target("0.9.0", 10);
    let compat = RollbackCompatibility::CompatibleWithBackup {
        backup_path: PathBuf::from("/tmp/session.bin.bak"),
    };
    let applied = apply_rollback(&t, &v("0.9.3"), &compat).expect("backup succeeds");
    assert_eq!(applied.from_version, v("0.9.3"));
    assert_eq!(applied.to_version, v("0.9.0"));
}
