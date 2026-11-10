//! The pure parts of the plan sheet (V31): which matched changes go to the app, which files the
//! UI leaves out, how rows are ordered and counted, and the dialog text.

use navo_l10n::{t, tr};

use super::super::stream::StreamScreen;
use crate::compare_diff::{CopyChange, CopyDirection, PairKind, SheetRow};
use crate::confirm_dialog::ConfirmProps;
use crate::format;
use crate::prelude::*;

/// The Fluent id of the short reason for a refusal id from the app or from the UI.
fn short_why(id: &str) -> String {
    tr(match id {
        "compare-copy-changed" => "compare-sheet-why-changed",
        "compare-copy-read-only" => "compare-sheet-why-read-only",
        "compare-copy-not-copyable" => "compare-sheet-why-not-copyable",
        "compare-copy-failed" => "compare-sheet-why-failed",
        other => other,
    })
}

/// A row for a file the UI leaves out.
fn refused(path: &str, why: &'static str) -> SheetRow {
    SheetRow {
        path: path.to_owned(),
        hunks: 0,
        bytes: 0,
        refusal: Some(why),
    }
}

/// The changes the search matched in one file, or the reason the whole file is left out.
fn file_changes(
    stream: &StreamScreen,
    id: u64,
    direction: CopyDirection,
) -> Result<Vec<CopyChange>, &'static str> {
    let card = stream
        .model
        .card(id)
        .ok_or("compare-sheet-why-not-loaded")?;
    let pair = stream
        .requests
        .get(&id)
        .ok_or("compare-copy-not-copyable")?;
    if card.kind != PairKind::Text {
        return Err("compare-copy-not-copyable");
    }
    let hits = stream.model.matched_hunks(id).unwrap_or_default();
    hits.iter()
        .map(|hit| {
            let hunk = card.hunks.get(*hit as usize);
            let hunk = hunk.ok_or("compare-copy-not-copyable")?;
            Ok(CopyChange {
                pair: pair.clone(),
                left_bytes: hunk.left_bytes.clone(),
                right_bytes: hunk.right_bytes.clone(),
                digest: hunk.digest.ok_or("compare-sheet-why-no-digest")?,
                direction,
                lines: None,
            })
        })
        .collect()
}

/// Every matched change the app can be asked to copy, and a row for each file left out.
pub(super) fn gather(
    stream: &StreamScreen,
    direction: CopyDirection,
) -> (Vec<CopyChange>, Vec<SheetRow>) {
    let (mut changes, mut local) = (Vec::new(), Vec::new());
    for at in stream.model.visible() {
        let item = &stream.model.items()[at];
        if stream.model.matched_hunks(item.id).is_none() {
            continue;
        }
        match file_changes(stream, item.id, direction) {
            Ok(found) => changes.extend(found),
            Err(why) => local.push(refused(&item.path, why)),
        }
    }
    (changes, local)
}

/// The rows with the refused ones first, so the first lines of the sheet name what is left out.
pub(super) fn ordered(plan: Vec<SheetRow>, local: Vec<SheetRow>) -> Vec<SheetRow> {
    let (mut rows, mut kept): (Vec<_>, Vec<_>) = local
        .into_iter()
        .chain(plan)
        .partition(|row| row.refusal.is_some());
    rows.append(&mut kept);
    rows
}

/// The changes, files and bytes the job would write.
pub(super) fn totals(rows: &[SheetRow]) -> (usize, usize, u64) {
    rows.iter()
        .filter(|row| row.refusal.is_none())
        .fold((0, 0, 0), |(changes, files, bytes), row| {
            (changes + row.hunks, files + 1, bytes + row.bytes)
        })
}

/// The sheet's dialog text for `rows`.
pub(super) fn props_of(direction: CopyDirection, rows: &[SheetRow]) -> ConfirmProps {
    let (changes, files, bytes) = totals(rows);
    let lines = rows
        .iter()
        .map(|row| match row.refusal {
            Some(why) => t!(
                "compare-sheet-skipped",
                why = short_why(why),
                path = row.path.as_str()
            ),
            None => t!(
                "compare-sheet-line",
                path = row.path.as_str(),
                changes = row.hunks,
                size = format::size(row.bytes)
            ),
        })
        .map(SharedString::from)
        .collect();
    let title = match direction {
        CopyDirection::ToLeft => "compare-sheet-title-left",
        CopyDirection::ToRight => "compare-sheet-title-right",
    };
    ConfirmProps {
        title: tr(title).into(),
        lines,
        more: 0,
        summary: t!(
            "compare-sheet-summary",
            changes = changes,
            files = files,
            size = format::size(bytes)
        )
        .into(),
        confirm_label: tr("compare-sheet-confirm").into(),
        danger: false,
        typed: None,
        mode: None,
        choice: None,
    }
}
