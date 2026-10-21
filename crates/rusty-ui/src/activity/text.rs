use gpui_kit::SharedString;
use rusty_core::OpKind;
use rusty_l10n::{t, tr};

use crate::format;
use crate::job_card::{JobInfo, Pause, speed_text, time_text};

fn target_of(job: &JobInfo) -> String {
    job.target.as_deref().unwrap_or_default().to_owned()
}

fn leaf(path: &str) -> &str {
    let trimmed = path.trim_end_matches(['/', '\\']);
    trimmed
        .rsplit(['/', '\\'])
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or(path)
}

fn items_of(job: &JobInfo) -> u64 {
    u64::try_from(job.items).unwrap_or(u64::MAX)
}

pub(super) fn job_title(job: &JobInfo) -> String {
    let (count, files) = (job.files_total, format::bytes(job.files_total));
    if job.rename.undo {
        return t!("activity-undoing", count = count, items = files);
    }
    match job.kind {
        OpKind::Copy => t!(
            "activity-copying",
            count = count,
            files = files,
            target = target_of(job)
        ),
        OpKind::Move => t!(
            "activity-moving",
            count = count,
            files = files,
            target = target_of(job)
        ),
        OpKind::Recycle => t!("activity-recycling", count = count, files = files),
        OpKind::Delete => t!("activity-deleting", count = count, files = files),
        OpKind::Rename => t!("activity-renaming", count = count, files = files),
        OpKind::Link => t!(
            "activity-linking",
            count = count,
            files = files,
            target = target_of(job)
        ),
    }
}

pub(super) fn job_detail(job: &JobInfo) -> String {
    let mut parts = vec![t!(
        "activity-progress",
        done = format::bytes(job.files_done),
        total = format::bytes(job.files_total)
    )];
    match job.pause {
        Pause::Off => {
            parts.extend(job.speed.map(speed_text));
            parts.extend(
                job.left()
                    .map(|left| t!("activity-left", time = time_text(left))),
            );
        }
        Pause::Asked => parts.push(tr("activity-pausing")),
        Pause::On => parts.push(tr("activity-paused")),
    }
    parts.join(" · ")
}

pub(super) fn failure_title(job: &JobInfo) -> String {
    let count = job.files_total.saturating_sub(job.files_done).max(1);
    let files = format::bytes(count);
    if job.rename.undo {
        return t!("activity-undo-failed", count = count, items = files);
    }
    let id = match job.kind {
        OpKind::Copy => "activity-copy-failed",
        OpKind::Move => "activity-move-failed",
        OpKind::Recycle => "activity-recycle-failed",
        OpKind::Delete => "activity-delete-failed",
        OpKind::Rename => "activity-rename-failed",
        OpKind::Link => "activity-link-failed",
    };
    t!(id, count = count, files = files)
}

pub(super) fn done_title(job: &JobInfo) -> String {
    let (count, items) = (items_of(job), format::bytes(items_of(job)));
    if job.rename.undo {
        return t!("activity-undone", count = count, items = items);
    }
    let target = leaf(&target_of(job)).to_owned();
    match job.kind {
        OpKind::Copy => t!(
            "activity-copied",
            count = count,
            items = items,
            target = target
        ),
        OpKind::Move => t!(
            "activity-moved",
            count = count,
            items = items,
            target = target
        ),
        OpKind::Recycle => t!("activity-recycled", count = count, items = items),
        OpKind::Delete => t!("activity-deleted", count = count, items = items),
        OpKind::Rename => t!("activity-renamed", count = count, items = items),
        OpKind::Link => t!(
            "activity-linked",
            count = count,
            items = items,
            target = target
        ),
    }
}

pub(super) fn drag_title(items: usize) -> String {
    let count = u64::try_from(items).unwrap_or(u64::MAX);
    t!(
        "activity-dragging",
        count = count,
        items = format::bytes(count)
    )
}

pub(super) fn drag_keys() -> String {
    tr("activity-drag-keys")
}

#[must_use]
pub fn percent(fraction: f32) -> String {
    let percent = (fraction.clamp(0., 1.) * 100.).floor();
    t!("activity-percent", percent = format!("{percent:.0}"))
}

pub(super) fn job_chip(job: &JobInfo) -> String {
    let id = match job.kind {
        OpKind::Rename if job.rename.undo => "activity-chip-undo",
        OpKind::Copy => "activity-chip-copy",
        OpKind::Move => "activity-chip-move",
        OpKind::Recycle => "activity-chip-recycle",
        OpKind::Delete => "activity-chip-delete",
        OpKind::Rename => "activity-chip-rename",
        OpKind::Link => "activity-chip-link",
    };
    let chip = t!(id, percent = percent(job.fraction()));
    match job.pause {
        Pause::Off => chip,
        Pause::Asked | Pause::On => t!("activity-chip-paused", chip = chip),
    }
}

pub(super) fn failure_chip(job: &JobInfo) -> String {
    tr(match job.kind {
        OpKind::Rename if job.rename.undo => "activity-chip-undo-failed",
        OpKind::Copy => "activity-chip-copy-failed",
        OpKind::Move => "activity-chip-move-failed",
        OpKind::Recycle => "activity-chip-recycle-failed",
        OpKind::Delete => "activity-chip-delete-failed",
        OpKind::Rename => "activity-chip-rename-failed",
        OpKind::Link => "activity-chip-link-failed",
    })
}

pub(super) fn done_chip(job: &JobInfo) -> String {
    let (count, items) = (items_of(job), format::bytes(items_of(job)));
    let id = match job.kind {
        OpKind::Rename if job.rename.undo => "activity-chip-undone",
        OpKind::Copy => "activity-chip-copied",
        OpKind::Move => "activity-chip-moved",
        OpKind::Recycle => "activity-chip-recycled",
        OpKind::Delete => "activity-chip-deleted",
        OpKind::Rename => "activity-chip-renamed",
        OpKind::Link => "activity-chip-linked",
    };
    t!(id, count = count, items = items)
}

const NAMES_SHOWN: usize = 3;

fn names_of(left: &[SharedString]) -> String {
    let shown = left.iter().take(NAMES_SHOWN).map(SharedString::as_ref);
    let mut names = shown.collect::<Vec<&str>>().join(", ");
    if left.len() > NAMES_SHOWN {
        names.push_str(", …");
    }
    names
}

fn count_of(left: &[SharedString]) -> u64 {
    u64::try_from(left.len()).unwrap_or(u64::MAX)
}

pub(super) fn done_detail(job: &JobInfo, undo_key: Option<&SharedString>) -> Option<String> {
    if job.kind != OpKind::Rename {
        return None;
    }
    if !job.rename.undo {
        return undo_key.map(|key| t!("activity-undo-hint", key = key.to_string()));
    }
    let skipped = u64::try_from(job.rename.skipped).unwrap_or(u64::MAX);
    let parts: Vec<String> = [
        (skipped > 0).then(|| {
            t!(
                "activity-undo-skipped",
                count = skipped,
                items = format::bytes(skipped)
            )
        }),
        left_text("activity-undo-left", &job.rename.left),
    ]
    .into_iter()
    .flatten()
    .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

pub(super) fn failure_detail(job: &JobInfo, message: &SharedString) -> String {
    let id = if job.rename.undo {
        "activity-undo-left"
    } else {
        "activity-rename-left"
    };
    match left_text(id, &job.rename.left) {
        Some(left) => format!("{message} · {left}"),
        None => message.to_string(),
    }
}

fn left_text(id: &str, left: &[SharedString]) -> Option<String> {
    (!left.is_empty()).then(|| t!(id, count = count_of(left), names = names_of(left)))
}
