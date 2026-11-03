use navo_git::{SyncKind, SyncSummary};
use navo_l10n::{t, tr};

use super::pullpush::{Confirmation, SyncFailure};

const HEAD_SHORT: usize = 12;

/// The confirmation shown before a git-cli Pull or Push: branch, remote, commits, HEAD, guarded
/// keys. Confirmations that differ in any of these never read the same.
pub(super) fn confirm_text(confirmation: &Confirmation) -> String {
    let c = confirmation;
    let (branch, remote, remote_branch) = (&c.branch, &c.remote, &c.remote_branch);
    let head = match (c.kind, c.creates) {
        (SyncKind::Pull, _) => t!(
            "git-confirm-pull",
            count = c.commits,
            branch = branch.as_str(),
            remote = remote.as_str(),
            remote_branch = remote_branch.as_str()
        ),
        (SyncKind::Push, true) => t!(
            "git-confirm-push-creates",
            branch = branch.as_str(),
            remote = remote.as_str(),
            remote_branch = remote_branch.as_str()
        ),
        (SyncKind::Push, false) => t!(
            "git-confirm-push",
            count = c.commits,
            branch = branch.as_str(),
            remote = remote.as_str(),
            remote_branch = remote_branch.as_str()
        ),
    };
    let at = c.head.as_deref().map(|head| {
        let short = head.get(..HEAD_SHORT).unwrap_or(head);
        t!("git-confirm-head", branch = branch.as_str(), head = short)
    });
    let counted = match (c.creates, c.fetched_ms) {
        (true, _) => None,
        (false, Some(_)) => Some(tr("git-confirm-counted")),
        (false, None) => Some(tr("git-confirm-never-fetched")),
    };
    let keys = c.guarded.join(", ");
    let guarded = (!c.guarded.is_empty()).then(|| t!("git-confirm-guarded", keys = keys.as_str()));
    [Some(head), at, counted, guarded]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" ")
}

/// The status line after a finished Pull or Push.
pub(super) fn done_text(summary: &SyncSummary) -> String {
    let (branch, remote) = (summary.branch.as_str(), summary.remote.as_str());
    match (summary.kind, summary.commits) {
        (SyncKind::Pull, 0) => t!("git-done-up-to-date", branch = branch),
        (SyncKind::Pull, count) => t!(
            "git-done-pulled-count",
            count = count,
            branch = branch,
            remote = remote
        ),
        (SyncKind::Push, 0) => t!("git-done-push-nothing", branch = branch),
        (SyncKind::Push, _) => t!("git-done-pushed", branch = branch, remote = remote),
    }
}

/// The status line for a Pull or Push that did not finish. A stopped Pull always warns that the
/// working tree may have changed, whatever Git reported.
pub(super) fn failure_text(kind: SyncKind, failure: SyncFailure) -> String {
    match (kind, failure) {
        (SyncKind::Pull, SyncFailure::Cancelled | SyncFailure::TimedOut) => {
            interrupted(false, false)
        }
        (_, SyncFailure::Interrupted { lock, moved }) => interrupted(lock, moved),
        (_, SyncFailure::Unfinished) => {
            format!(
                "{} {}",
                tr("git-fail-unfinished"),
                tr("git-tree-may-have-changed")
            )
        }
        (_, other) => tr(refusal_id(other)),
    }
}

fn interrupted(lock: bool, moved: bool) -> String {
    let notes = [
        Some(tr("git-fail-interrupted")),
        lock.then(|| tr("git-fail-interrupted-lock")),
        moved.then(|| tr("git-fail-interrupted-moved")),
        Some(tr("git-tree-may-have-changed")),
    ];
    notes.into_iter().flatten().collect::<Vec<_>>().join(" ")
}

fn refusal_id(failure: SyncFailure) -> &'static str {
    match failure {
        SyncFailure::NotFastForward => "git-fail-not-ff",
        SyncFailure::LocalChanges => "git-fail-local-changes",
        SyncFailure::Rejected => "git-fail-rejected",
        SyncFailure::AuthFailed => "git-fail-auth",
        SyncFailure::NoUpstream => "git-fail-no-upstream",
        SyncFailure::UnsafeRemote => "git-fail-unsafe-remote",
        SyncFailure::UnsupportedRefspec => "git-fail-refspec",
        SyncFailure::RemoteBranchGone => "git-fail-remote-gone",
        SyncFailure::Detached => "git-fail-detached",
        SyncFailure::ConfigChanged => "git-fail-config-changed",
        SyncFailure::TargetChanged => "git-fail-target-changed",
        SyncFailure::Cancelled => "git-fail-cancelled",
        SyncFailure::TimedOut => "git-fail-timed-out",
        SyncFailure::NoGit => "git-fail-no-git",
        SyncFailure::NotTrusted => "git-fail-not-trusted",
        SyncFailure::Failed | SyncFailure::Interrupted { .. } | SyncFailure::Unfinished => {
            "git-fail-failed"
        }
    }
}
