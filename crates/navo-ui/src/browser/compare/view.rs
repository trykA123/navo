//! What the panes draw in compare mode: aligned rows, the totals line and the empty states.
//!
//! Text and a tone per piece, no colours: the views map a `Tone` to theme tokens.

use std::sync::Arc;

use navo_l10n::{t, tr};

use super::{
    CompareFilter, CompareKind, CompareMode, CompareRow, CompareSide, CompareTotals, RowProgress,
    RowStatus,
};
use crate::compare_list::{GutterLabel, gutter_label};
use crate::format;

/// Which theme colour a row mark, status or total is drawn in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Tone {
    Muted,
    Change,
    Delete,
    Add,
    Conflict,
    Busy,
}

/// One side of a row, as the pane shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SideView {
    pub kind: CompareKind,
    pub size: String,
    pub time: String,
}

/// One pane's row. `side` is `None` where the file does not exist: the pane draws a gap.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RowView {
    pub path: String,
    pub name: String,
    pub side: Option<SideView>,
    pub tone: Tone,
    pub mark: &'static str,
    pub status: String,
    /// The pair's gutter words, set only for the rows of side 0 (the left side), which draws the
    /// gutter column. `None` where the pair has nothing to say there.
    pub gutter: Option<GutterLabel>,
}

/// A count in the totals line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Total {
    pub tone: Tone,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BarView {
    pub filter: CompareFilter,
    pub totals: Vec<Total>,
    /// "Comparing…" while the newest generation is still being checked.
    pub running: Option<String>,
    /// The notice of a walk that waits at the network limit; the bar offers Continue with it.
    pub stopped: Option<String>,
}

pub const FILTERS: [(CompareFilter, &str); 4] = [
    (CompareFilter::All, "compare-filter-all"),
    (CompareFilter::Differences, "compare-filter-differences"),
    (CompareFilter::Orphans, "compare-filter-orphans"),
    (CompareFilter::Same, "compare-filter-same"),
];

/// The tone and mark of a status; `Same` rows stay quiet.
#[must_use]
pub fn look_of(status: RowStatus) -> (Tone, &'static str) {
    match status {
        RowStatus::Same | RowStatus::SameAfterRules | RowStatus::SameSizeTime => (Tone::Muted, "="),
        RowStatus::Different => (Tone::Change, "≠"),
        RowStatus::OnlyLeft => (Tone::Delete, "←"),
        RowStatus::OnlyRight => (Tone::Add, "→"),
        RowStatus::TypeConflict | RowStatus::CaseConflict | RowStatus::Unavailable(_) => {
            (Tone::Conflict, "!")
        }
        RowStatus::Checking => (Tone::Busy, "…"),
        RowStatus::NotChecked(_) => (Tone::Muted, "–"),
    }
}

/// The words after the name; `progress` is the bytes checked of a row over 64 MiB.
#[must_use]
pub fn status_text(status: RowStatus, progress: Option<RowProgress>) -> String {
    match status {
        RowStatus::Same => String::new(),
        RowStatus::SameAfterRules => tr("compare-status-same-after-rules"),
        RowStatus::SameSizeTime => tr("compare-status-same-size-time"),
        RowStatus::Different => tr("compare-status-different"),
        RowStatus::OnlyLeft => tr("compare-status-only-left"),
        RowStatus::OnlyRight => tr("compare-status-only-right"),
        RowStatus::TypeConflict => tr("compare-status-type-conflict"),
        RowStatus::CaseConflict => tr("compare-status-case-conflict"),
        RowStatus::Checking => checking_text(progress),
        RowStatus::NotChecked(reason) => {
            t!("compare-status-not-checked", reason = tr(reason))
        }
        RowStatus::Unavailable(reason) => t!("compare-status-unavailable", reason = tr(reason)),
    }
}

fn checking_text(progress: Option<RowProgress>) -> String {
    let base = tr("compare-status-checking");
    match progress.filter(|step| step.bytes_total > 0) {
        Some(step) => format!(
            "{base} · {}",
            t!(
                "compare-progress",
                done = format::size(step.bytes_done),
                total = format::size(step.bytes_total)
            )
        ),
        None => base,
    }
}

fn side_view(side: CompareSide) -> SideView {
    let size = match side.kind {
        CompareKind::File => format::size(side.size),
        CompareKind::Folder => "<DIR>".to_owned(),
        CompareKind::Link => "<LNK>".to_owned(),
        CompareKind::Submodule => "<SUB>".to_owned(),
    };
    SideView {
        kind: side.kind,
        size,
        time: side.modified_ms.map(format::date).unwrap_or_default(),
    }
}

/// One pane's row of a pair: its side, words, tone and (for side 0 only) the gutter label.
///
/// `side` is 0 for the pane that shows the compare's left side; `progress` is the bytes checked of
/// a row over 64 MiB. Only that pane draws the gutter, so the label describes the pair once.
fn row_view(row: &CompareRow, side: usize, progress: Option<RowProgress>) -> RowView {
    let contains_differences =
        row.is_folder_pair() && row.contains_differences && row.status.is_same();
    let (tone, mark) = if contains_differences {
        (Tone::Change, "⊃")
    } else {
        look_of(row.status)
    };
    let here = if side == 0 { row.left } else { row.right };
    RowView {
        path: row.path.clone(),
        name: row.name().to_owned(),
        side: here.map(side_view),
        tone,
        mark,
        gutter: (side == 0).then(|| gutter_label(row, progress)).flatten(),
        status: if here.is_none() {
            String::new()
        } else if contains_differences {
            tr("compare-status-contains-differences")
        } else {
            status_text(row.status, progress)
        },
    }
}

/// The rows of one pane (`side` 0 is the left) in the order of `mode.visible()`.
#[must_use]
pub fn pane_rows(mode: &CompareMode, side: usize) -> Arc<[RowView]> {
    mode.visible()
        .into_iter()
        .map(|row| row_view(row, side, mode.progress(row.id)))
        .collect()
}

fn total(count: u64, id: &str, tone: Tone) -> Total {
    Total {
        tone,
        text: t!(id, count = count),
    }
}

fn totals_line(totals: CompareTotals) -> Vec<Total> {
    let mut line = vec![
        total(
            totals.different + totals.conflicts,
            "compare-total-different",
            Tone::Change,
        ),
        total(totals.only_left, "compare-total-only-left", Tone::Delete),
        total(totals.only_right, "compare-total-only-right", Tone::Add),
        total(
            totals.same + totals.same_size_time,
            "compare-total-same",
            Tone::Muted,
        ),
    ];
    if totals.checking > 0 {
        line.push(total(totals.checking, "compare-total-checking", Tone::Busy));
    }
    line
}

#[must_use]
pub fn bar_view(mode: &CompareMode) -> BarView {
    BarView {
        filter: mode.filter(),
        totals: totals_line(mode.totals()),
        running: mode.checking().then(|| tr("compare-running")),
        stopped: mode
            .stopped()
            .map(|entries| t!("compare-stopped", count = entries)),
    }
}

/// The text for a list with no rows; `None` while rows are shown.
#[must_use]
pub fn empty_text(mode: &CompareMode) -> Option<String> {
    if !mode.visible().is_empty() {
        return None;
    }
    let totals = mode.totals();
    let id = match (mode.received(), mode.checking(), mode.filter()) {
        (0, true, _) => "compare-empty-waiting",
        (0, false, _) => "compare-empty-folders",
        (_, _, CompareFilter::All) => "compare-empty-folder",
        (_, false, _) if is_identical(totals) => "compare-empty-identical",
        _ => "compare-empty-filter",
    };
    Some(tr(id))
}

fn is_identical(totals: CompareTotals) -> bool {
    let differing = totals.different + totals.only_left + totals.only_right + totals.conflicts;
    differing + totals.checking + totals.unavailable + totals.not_checked == 0
}
