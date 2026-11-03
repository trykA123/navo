use navo_git::Tracking;
use navo_git::trust::Guard;

use super::*;

pub(crate) fn confirmation(kind: SyncKind) -> Confirmation {
    Confirmation {
        kind,
        branch: "main".into(),
        remote: "origin".into(),
        remote_branch: "main".into(),
        commits: 2,
        fetched_ms: Some(1),
        creates: false,
        head: Some("1".repeat(40)),
        guarded: vec!["core.hookspath".into()],
        target: SyncTarget {
            guard: Guard::default(),
            tracking: Tracking {
                branch: "main".into(),
                remote: "origin".into(),
                remote_branch: "main".into(),
                tracking_ref: "refs/remotes/origin/main".into(),
            },
            remote: Default::default(),
            head: Some("1".repeat(40)),
            commits: 2,
        },
    }
}

/// `confirmation(kind)` whose target fetches from `url` under a config that sets
/// `core.hooksPath` to `hooks`; the shown fields stay the same.
pub(crate) fn confirmation_of(kind: SyncKind, url: &str, hooks: &str) -> Confirmation {
    let config = tempfile::tempdir().unwrap();
    let text = format!("[core]\n\thooksPath = {hooks}\n");
    std::fs::write(config.path().join("config"), text).unwrap();
    let mut confirmed = confirmation(kind);
    confirmed.target.guard = Guard::read_with_home(config.path(), None).unwrap();
    confirmed.target.remote.urls = vec![url.to_owned()];
    confirmed
}

#[test]
fn git_errors_map_to_the_reasons_the_menu_shows() {
    let cases = [
        (GitError::NotFastForward, SyncFailure::NotFastForward),
        (GitError::LocalChanges, SyncFailure::LocalChanges),
        (GitError::Rejected, SyncFailure::Rejected),
        (GitError::GuardChanged, SyncFailure::ConfigChanged),
        (GitError::TargetChanged, SyncFailure::TargetChanged),
        (GitError::PullUnfinished, SyncFailure::Unfinished),
        (
            GitError::UnsafeRemote { name: "-x".into() },
            SyncFailure::UnsafeRemote,
        ),
        (
            GitError::UnsupportedRefspec,
            SyncFailure::UnsupportedRefspec,
        ),
        (GitError::RemoteBranchGone, SyncFailure::RemoteBranchGone),
        (GitError::NoUpstream, SyncFailure::NoUpstream),
        (
            GitError::Interrupted {
                lock: true,
                moved: false,
            },
            SyncFailure::Interrupted {
                lock: true,
                moved: false,
            },
        ),
        (GitError::NoCommit, SyncFailure::Failed),
        (GitError::GitNotFound, SyncFailure::NoGit),
        (
            GitError::NotTrusted {
                path: "repo".into(),
            },
            SyncFailure::NotTrusted,
        ),
    ];
    for (error, expected) in cases {
        assert_eq!(failure(&error), expected, "{error:?}");
    }
}

#[test]
fn the_command_line_port_reports_a_missing_repository_without_a_panic() {
    let missing = std::env::temp_dir().join(format!("navo-no-repo-{}", std::process::id()));
    let result = CliSync.confirm(&missing, SyncKind::Pull);
    assert!(
        matches!(result, Err(SyncFailure::Failed | SyncFailure::NoGit)),
        "{result:?}"
    );
}
