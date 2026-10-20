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
        OpKind::Copy => "activity-chip-copied",
        OpKind::Move => "activity-chip-moved",
        OpKind::Recycle => "activity-chip-recycled",
        OpKind::Delete => "activity-chip-deleted",
        OpKind::Rename => "activity-chip-renamed",
        OpKind::Link => "activity-chip-linked",
    };
    t!(id, count = count, items = items)
}
