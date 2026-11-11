//! The words in the folder compare's centre gutter (D33): one optional label per row.
//!
//! A pure mapping from a compare row to a label; `gutter_cell` draws it.

use navo_l10n::{t, tr};

use crate::browser::{CompareRow, RowProgress, RowStatus, Tone, status_text};

/// What a gutter label says; each variant is one `compare-gutter-*` l10n id.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum GutterText {
    Changed,
    OnlyLeft,
    OnlyRight,
    Type,
    /// A folder with this many differing files or conflicts below it.
    Inside(u32),
    /// Same after the compare rules.
    Rules,
    /// A big file being checked, in whole percent.
    Checking(u8),
    /// A checking row whose bytes are not counted yet.
    Pending,
    /// A row the compare stopped before checking.
    NotChecked,
    /// A row one side cannot be read.
    Unavailable,
}

impl GutterText {
    /// The l10n id of the label.
    #[must_use]
    pub fn id(self) -> &'static str {
        match self {
            Self::Changed => "compare-gutter-changed",
            Self::OnlyLeft => "compare-gutter-only-left",
            Self::OnlyRight => "compare-gutter-only-right",
            Self::Type => "compare-gutter-type",
            Self::Inside(_) => "compare-gutter-inside",
            Self::Rules => "compare-gutter-rules",
            Self::Checking(_) => "compare-gutter-checking",
            Self::Pending => "compare-gutter-pending",
            Self::NotChecked => "compare-gutter-not-checked",
            Self::Unavailable => "compare-gutter-unavailable",
        }
    }

    /// The label in the current language.
    #[must_use]
    pub fn words(self) -> String {
        match self {
            Self::Inside(count) => t!(self.id(), count = count),
            Self::Checking(percent) => t!(self.id(), percent = percent),
            _ => tr(self.id()),
        }
    }
}

/// One gutter pill: its words, its tone and the status words shown on hover.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GutterLabel {
    pub text: GutterText,
    pub tone: Tone,
    pub hover: String,
}

/// The label of a row, or `None` for rows that say nothing in the gutter.
///
/// A folder pair with differences inside always says "{n} inside", even when the folder is itself
/// changed. Identical rows have no label; rows the compare did not reach say what they are in the
/// muted tone, and so does a checking row whose byte count is missing (small files finish at once).
#[must_use]
pub fn gutter_label(row: &CompareRow, progress: Option<RowProgress>) -> Option<GutterLabel> {
    let inside = row.is_folder_pair()
        && row.differences_inside > 0
        && (row.status.is_same() || row.status == RowStatus::Different);
    if inside {
        let hover = tr("compare-status-contains-differences");
        return Some(label(
            GutterText::Inside(row.differences_inside),
            Tone::Change,
            hover,
        ));
    }
    let (text, tone) = match row.status {
        RowStatus::Same | RowStatus::SameSizeTime => return folder_changed(row),
        RowStatus::SameAfterRules => (GutterText::Rules, Tone::Muted),
        RowStatus::Different => (GutterText::Changed, Tone::Change),
        RowStatus::OnlyLeft => (GutterText::OnlyLeft, Tone::Delete),
        RowStatus::OnlyRight => (GutterText::OnlyRight, Tone::Add),
        RowStatus::TypeConflict | RowStatus::CaseConflict => (GutterText::Type, Tone::Conflict),
        RowStatus::Checking => match progress.and_then(percent) {
            Some(done) => (GutterText::Checking(done), Tone::Busy),
            None => (GutterText::Pending, Tone::Muted),
        },
        RowStatus::NotChecked(_) => (GutterText::NotChecked, Tone::Muted),
        RowStatus::Unavailable(_) => (GutterText::Unavailable, Tone::Muted),
    };
    Some(label(text, tone, status_text(row.status, progress)))
}

/// A same folder whose count has not arrived yet but is known to hold differences.
fn folder_changed(row: &CompareRow) -> Option<GutterLabel> {
    let waiting = row.is_folder_pair() && row.contains_differences;
    let hover = tr("compare-status-contains-differences");
    waiting.then(|| label(GutterText::Changed, Tone::Change, hover))
}

/// A gutter label built from its words, its tone and the text shown on hover.
fn label(text: GutterText, tone: Tone, hover: String) -> GutterLabel {
    GutterLabel { text, tone, hover }
}

/// Whole percent checked; `None` when the total is unknown.
fn percent(progress: RowProgress) -> Option<u8> {
    let done = progress.bytes_done.min(progress.bytes_total);
    let whole = done.checked_mul(100)?.checked_div(progress.bytes_total)?;
    u8::try_from(whole).ok()
}

#[cfg(test)]
mod tests;
