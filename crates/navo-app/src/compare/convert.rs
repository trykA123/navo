//! Engine events and settings as the UI's plain compare types.

use navo_compare::model::{EntryKind, Row, SideEntry, Status};
use navo_compare::session::{RowBatch, StopBound, Totals};
use navo_store::{Compare, SettingValue};
use navo_ui::browser::{
    CompareKind, CompareRow, CompareSide, CompareTotals, CompareUpdate, RowProgress, RowStatus,
    StopBound as UiBound,
};
use navo_ui::settings::{Change, FIELDS, Section, Values};

use super::handle::CompareEvent;
use super::settings::CompareSettings;

/// The compare settings the user sees in `values`, read through the stored `[compare]` rules.
pub(super) fn settings_of(values: &Values) -> CompareSettings {
    let mut table = serde_json::Map::new();
    for spec in FIELDS
        .iter()
        .filter(|spec| spec.section == Section::Compare)
    {
        let Some(key) = spec.key.strip_prefix("compare.") else {
            continue;
        };
        let change = Change {
            field: spec.field,
            value: values.get(spec.field).clone(),
        };
        if let Some(value) = crate::settings::stored_value(&change) {
            table.insert(key.to_owned(), json_of(value));
        }
    }
    let stored: Compare = serde_json::from_value(serde_json::Value::Object(table))
        .inspect_err(|error| tracing::warn!(%error, "compare settings fall back to defaults"))
        .unwrap_or_default();
    CompareSettings::from(&stored)
}

fn json_of(value: SettingValue) -> serde_json::Value {
    match value {
        SettingValue::Bool(flag) => flag.into(),
        SettingValue::Integer(number) => number.into(),
        SettingValue::Text(text) => text.into(),
        SettingValue::List(items) => items.into(),
    }
}

/// The UI update for an event.
pub(super) fn update_of(event: CompareEvent) -> CompareUpdate {
    match event {
        CompareEvent::Rows(batch) => rows_of(batch),
        CompareEvent::Totals { generation, totals } => CompareUpdate::Totals {
            generation: generation.get(),
            totals: totals_of(totals),
        },
        CompareEvent::Finished { generation } => CompareUpdate::Finished {
            generation: generation.get(),
        },
        CompareEvent::Failed(error) => {
            tracing::warn!(?error, "compare did not start");
            CompareUpdate::Failed(navo_l10n::tr("compare-could-not-start"))
        }
        CompareEvent::StoppedAtLimit {
            generation,
            entries,
            bound,
        } => CompareUpdate::Stopped {
            generation: generation.get(),
            entries,
            bound: match bound {
                StopBound::Entries => UiBound::Entries,
                StopBound::Time => UiBound::Time,
            },
        },
        CompareEvent::Continued { generation } => CompareUpdate::Continued {
            generation: generation.get(),
        },
    }
}

fn rows_of(batch: RowBatch) -> CompareUpdate {
    CompareUpdate::Rows {
        generation: batch.generation.get(),
        rows: batch.rows.iter().map(row_of).collect(),
        progress: batch
            .progress
            .iter()
            .map(|step| RowProgress {
                id: step.id,
                bytes_done: step.bytes_done,
                bytes_total: step.bytes_total,
            })
            .collect(),
    }
}

fn row_of(row: &Row) -> CompareRow {
    let entry = row.left.as_ref().or(row.right.as_ref());
    CompareRow {
        id: row.id,
        path: entry
            .map(|entry| entry.rel.as_path().to_text())
            .unwrap_or_default(),
        left: row.left.as_deref().map(side_of),
        right: row.right.as_deref().map(side_of),
        status: status_of(row.display_status),
        contains_differences: row.contains_differences,
        differences_inside: row.differences_inside,
    }
}

fn side_of(entry: &SideEntry) -> CompareSide {
    CompareSide {
        kind: match entry.kind {
            EntryKind::File => CompareKind::File,
            EntryKind::Folder => CompareKind::Folder,
            EntryKind::Link => CompareKind::Link,
            EntryKind::Submodule => CompareKind::Submodule,
        },
        size: entry.size,
        modified_ms: entry.modified,
    }
}

pub(super) fn status_of(status: Status) -> RowStatus {
    match status {
        Status::Same => RowStatus::Same,
        Status::SameAfterRules => RowStatus::SameAfterRules,
        Status::Different { .. } => RowStatus::Different,
        Status::OnlyLeft => RowStatus::OnlyLeft,
        Status::OnlyRight => RowStatus::OnlyRight,
        Status::TypeConflict => RowStatus::TypeConflict,
        Status::CaseConflict => RowStatus::CaseConflict,
        Status::Checking => RowStatus::Checking,
        Status::SameSizeTime => RowStatus::SameSizeTime,
        Status::NotChecked(reason) => RowStatus::NotChecked(reason.fluent_id().as_str()),
        Status::Unavailable(reason) => RowStatus::Unavailable(reason.fluent_id().as_str()),
    }
}

fn totals_of(totals: Totals) -> CompareTotals {
    CompareTotals {
        rows: totals.rows,
        same: totals.same,
        different: totals.different,
        only_left: totals.only_left,
        only_right: totals.only_right,
        conflicts: totals.conflicts,
        checking: totals.checking,
        same_size_time: totals.same_size_time,
        unavailable: totals.unavailable,
        not_checked: totals.not_checked,
    }
}
