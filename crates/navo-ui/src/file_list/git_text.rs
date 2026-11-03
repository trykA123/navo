use navo_l10n::{t, tr};

use crate::format;
use crate::git::{BadgeKind, BadgeReason, RepoBadge};

pub(crate) fn state_id(kind: BadgeKind) -> &'static str {
    match kind {
        BadgeKind::Conflict => "git-state-conflict",
        BadgeKind::Modified => "git-state-modified",
        BadgeKind::Ahead => "git-state-ahead",
        BadgeKind::Behind => "git-state-behind",
        BadgeKind::Diverged => "git-state-diverged",
        BadgeKind::Clean => "git-state-clean",
        BadgeKind::Unknown => "git-state-unknown",
        BadgeKind::Checking => "git-state-checking",
    }
}

pub(crate) fn is_settled(badge: &RepoBadge) -> bool {
    badge.is_root && !matches!(badge.kind, BadgeKind::Unknown | BadgeKind::Checking)
}

/// The `N↑ M↓` text after the name. Zero counts are left out.
pub(crate) fn counts(badge: &RepoBadge) -> Option<String> {
    if !is_settled(badge) {
        return None;
    }
    let parts = [(badge.ahead, '↑'), (badge.behind, '↓')]
        .into_iter()
        .filter(|(count, _)| *count > 0)
        .map(|(count, arrow)| format!("{count}{arrow}"))
        .collect::<Vec<_>>();
    (!parts.is_empty()).then(|| parts.join(" "))
}

fn ahead_behind(badge: &RepoBadge) -> Vec<String> {
    let ahead = (badge.ahead > 0).then(|| t!("git-spoken-ahead", count = badge.ahead));
    let behind = (badge.behind > 0).then(|| t!("git-spoken-behind", count = badge.behind));
    ahead.into_iter().chain(behind).collect()
}

/// The accessible name: `Git: <state>, <n> ahead, <n> behind`.
pub(super) fn spoken(badge: &RepoBadge) -> String {
    let state = t!("git-spoken", state = tr(state_id(badge.kind)));
    let counts = if is_settled(badge) {
        ahead_behind(badge)
    } else {
        Vec::new()
    };
    std::iter::once(state)
        .chain(counts)
        .collect::<Vec<_>>()
        .join(", ")
}

fn unknown_detail(reason: Option<BadgeReason>) -> String {
    tr(match reason {
        Some(BadgeReason::TimedOut) => "git-detail-timed-out",
        Some(BadgeReason::NoGit) => "git-detail-no-git",
        Some(BadgeReason::NotTrusted) => "git-detail-not-trusted",
        Some(BadgeReason::Failed) | None => "git-detail-failed",
    })
}

/// A folder inside a repository has no counts: the scan only knows its worst state.
fn modified_detail(badge: &RepoBadge) -> Option<String> {
    if !badge.is_root {
        return None;
    }
    let changed = t!(
        "git-detail-modified",
        count = badge.changed + badge.untracked
    );
    if badge.untracked == 0 {
        return Some(changed);
    }
    Some(format!(
        "{changed}, {}",
        t!("git-detail-new", count = badge.untracked)
    ))
}

pub(crate) fn detail(badge: &RepoBadge) -> Option<String> {
    match badge.kind {
        BadgeKind::Conflict => {
            (badge.conflicted > 0).then(|| t!("git-detail-conflict", count = badge.conflicted))
        }
        BadgeKind::Modified => modified_detail(badge),
        BadgeKind::Ahead | BadgeKind::Behind | BadgeKind::Diverged => {
            Some(ahead_behind(badge).join(", "))
        }
        BadgeKind::Clean => Some(tr(if badge.is_root {
            "git-detail-clean"
        } else {
            "git-detail-clean-below"
        })),
        BadgeKind::Unknown => Some(unknown_detail(badge.reason)),
        BadgeKind::Checking => Some(tr("git-detail-checking")),
    }
}

fn head_line(badge: &RepoBadge) -> String {
    match (&badge.branch, &badge.upstream) {
        _ if !badge.is_root => tr("git-inside"),
        (Some(branch), Some(upstream)) => {
            t!(
                "git-branch-upstream",
                branch = branch.as_str(),
                upstream = upstream.as_str()
            )
        }
        (Some(branch), None) => branch.clone(),
        (None, _) => tr(state_id(badge.kind)),
    }
}

fn fetch_line(badge: &RepoBadge) -> String {
    let (ahead, behind) = (badge.ahead, badge.behind);
    match badge.fetched_ms {
        Some(at) => t!(
            "git-tip-counts",
            ahead = ahead,
            behind = behind,
            time = format::read_time(at)
        ),
        None => t!("git-tip-never-fetched", ahead = ahead, behind = behind),
    }
}

/// The tooltip: head line, `State: detail`, and for a root the fetch line.
pub(super) fn tooltip(badge: &RepoBadge) -> String {
    let state = tr(state_id(badge.kind));
    let second = match detail(badge) {
        Some(detail) => t!("git-detail-state", state = state, detail = detail),
        None => state,
    };
    let mut lines = vec![head_line(badge), second];
    if is_settled(badge) {
        lines.push(fetch_line(badge));
    }
    lines.join("\n")
}
