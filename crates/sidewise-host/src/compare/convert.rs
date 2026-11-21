//! Engine events and settings as the UI's plain compare types.

use navo_compare::model::{EntryKind, Row, SideEntry, Status};
use navo_compare::session::{ComparePhase as EnginePhase, RowBatch, StopBound, Totals};
use navo_ui::browser::{
    CompareKind, ComparePhase, CompareRow, CompareSide, CompareTotals, CompareUpdate, RowProgress,
    RowStatus, StopBound as UiBound,
};
use navo_ui::compare_diff::CompareOptions;

use super::handle::{CompareError, CompareEvent};
use super::settings::CompareSettings;

/// The compare settings in `options` as the compare backend reads them.
pub fn settings_from(options: &CompareOptions) -> CompareSettings {
    CompareSettings::from(&options.compare)
}

/// The UI updates for an event, in the order the UI applies them.
pub fn updates_of(event: CompareEvent) -> Vec<CompareUpdate> {
    match event {
        CompareEvent::Rows(batch) => rows_of(batch),
        CompareEvent::Totals { generation, totals } => vec![CompareUpdate::Totals {
            generation: generation.get(),
            totals: totals_of(totals),
        }],
        CompareEvent::Finished { generation } => vec![CompareUpdate::Finished {
            generation: generation.get(),
        }],
        CompareEvent::Failed(error) => {
            tracing::warn!(?error, "compare did not start");
            vec![CompareUpdate::Failed(failure_text(&error))]
        }
        CompareEvent::StoppedAtLimit {
            generation,
            entries,
            bound,
        } => vec![CompareUpdate::Stopped {
            generation: generation.get(),
            entries,
            bound: match bound {
                StopBound::Entries => UiBound::Entries,
                StopBound::Time => UiBound::Time,
            },
        }],
        CompareEvent::Continued { generation } => vec![CompareUpdate::Continued {
            generation: generation.get(),
        }],
    }
}

/// The text a failed start shows: the side's own problem when its opener named one, else
/// "The compare could not start."
fn failure_text(error: &CompareError) -> String {
    let named = match error {
        CompareError::Left(failure) | CompareError::Right(failure) => failure.text_id,
        CompareError::Rules(_) | CompareError::Spawn(_) => None,
    };
    navo_l10n::tr(named.unwrap_or("compare-could-not-start"))
}

/// A row batch as rows, then its phase progress, the running read, and one slow path per file.
fn rows_of(batch: RowBatch) -> Vec<CompareUpdate> {
    let generation = batch.generation.get();
    let mut updates = vec![CompareUpdate::Rows {
        generation,
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
    }];
    if let Some(phase) = batch.phase {
        updates.push(CompareUpdate::PhaseProgress {
            generation,
            phase: phase_of(phase.phase),
            done: phase.done,
            total: phase.total,
        });
    }
    if let Some(entry) = batch.reading {
        updates.push(CompareUpdate::Reading {
            generation,
            path: entry.rel.as_path().to_text(),
            elapsed_ms: entry.elapsed_ms,
        });
    }
    updates.extend(batch.slow.into_iter().map(|entry| CompareUpdate::SlowPath {
        generation,
        path: entry.rel.as_path().to_text(),
        elapsed_ms: entry.elapsed_ms,
    }));
    updates
}

/// The engine's walk stage as the UI's compare phase.
fn phase_of(phase: EnginePhase) -> ComparePhase {
    match phase {
        EnginePhase::Listing => ComparePhase::Listing,
        EnginePhase::QuickCheck => ComparePhase::QuickCheck,
        EnginePhase::ContentCheck => ComparePhase::ContentCheck,
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

pub fn status_of(status: Status) -> RowStatus {
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
