use rusty_core::Location;
use rusty_l10n::{t, tr};

use super::entries::Day;
use crate::browser::{RetainedKind, RetainedRow, RetainedState};
use crate::format;

const MINUTE_MS: i64 = 60_000;
const HOUR_MS: i64 = 60 * MINUTE_MS;
const DAY_MS: i64 = 24 * HOUR_MS;
pub const SOON_MS: i64 = 6 * HOUR_MS;

pub fn day_label(day: Day) -> String {
    match day {
        Day::Today => tr("recently-deleted-today"),
        Day::Yesterday => tr("recently-deleted-yesterday"),
        Day::On(date) => date.format("%a %-d %b").to_string(),
    }
}

pub fn summary(items: usize, bytes: u64) -> String {
    let count = u64::try_from(items).unwrap_or(u64::MAX);
    t!(
        "recently-deleted-summary",
        count = count,
        size = format::size(bytes)
    )
}

fn span(left_ms: i64) -> String {
    let days = left_ms / DAY_MS;
    let hours = left_ms % DAY_MS / HOUR_MS;
    let minutes = left_ms % HOUR_MS / MINUTE_MS;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else {
        format!("{}m", minutes.max(1))
    }
}

pub fn expires(left_ms: i64) -> String {
    if left_ms <= 0 {
        tr("recently-deleted-expiring")
    } else {
        t!("recently-deleted-expires", span = span(left_ms))
    }
}

pub fn state_label(state: RetainedState) -> Option<String> {
    match state {
        RetainedState::Ready => None,
        RetainedState::Busy => Some(tr("recently-deleted-in-use")),
        RetainedState::NeedsAttention => Some(tr("recently-deleted-attention")),
    }
}

pub fn kind_label(kind: RetainedKind) -> Option<String> {
    match kind {
        RetainedKind::Delete => None,
        RetainedKind::Overwrite => Some(tr("recently-deleted-replaced")),
    }
}

pub fn name_of(location: &Location) -> String {
    location
        .name()
        .map(|name| name.display().into_owned())
        .unwrap_or_else(|| location.as_str().to_owned())
}

pub fn folder_of(location: &Location) -> String {
    location
        .parent()
        .map(|folder| folder.path().display().into_owned())
        .unwrap_or_default()
}

pub fn spoken(row: &RetainedRow, now_ms: i64) -> String {
    let name = name_of(&row.original);
    let folder = folder_of(&row.original);
    let size = format::size(row.bytes);
    let left = expires(row.expires_at_ms.saturating_sub(now_ms));
    let state = state_label(row.state).or_else(|| kind_label(row.kind));
    match state {
        Some(state) => format!("{name}, {folder}, {size}, {left}, {state}"),
        None => format!("{name}, {folder}, {size}, {left}"),
    }
}
