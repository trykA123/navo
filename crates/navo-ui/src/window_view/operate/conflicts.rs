//! The copy and move dialog's conflict row: "N already there" on its own summary line, one
//! choice for all of them (Skip, Overwrite older, Keep both, Overwrite all) whose pick rebuilds the
//! plan, and "Show conflicts", which lists both sides of the first conflicts.

use navo_core::{ConflictPolicy, OpKind, OperationPlan, Resolution};
use navo_l10n::{t, tr};

use super::super::NavoWindow;
use crate::browser::{Clash, Facts, KeepOld, PendingOp};
use crate::confirm_dialog::{ConfirmDialog, DetailRow, Details, OptionItem, Options};
use crate::format;
use crate::prelude::*;

/// One choice of the row: the policy it plans with, its element id and its label key.
struct ConflictChoice {
    policy: ConflictPolicy,
    id: &'static str,
    label: &'static str,
}

/// Every choice, in the order the row shows them.
const CHOICES: [ConflictChoice; 4] = [
    ConflictChoice {
        policy: ConflictPolicy::Skip,
        id: "conflict-skip",
        label: "conflict-skip",
    },
    ConflictChoice {
        policy: ConflictPolicy::OverwriteOlder,
        id: "conflict-overwrite-older",
        label: "conflict-overwrite-older",
    },
    ConflictChoice {
        policy: ConflictPolicy::Rename,
        id: "conflict-keep-both",
        label: "conflict-keep-both",
    },
    ConflictChoice {
        policy: ConflictPolicy::OverwriteAll,
        id: "conflict-overwrite-all",
        label: "conflict-overwrite-all",
    },
];

/// The choices shown: all four, or only Skip and Keep both when Undo could not restore an
/// overwritten file (`undoes_overwrites` false).
fn shown(undoes_overwrites: bool) -> Vec<&'static ConflictChoice> {
    CHOICES
        .iter()
        .filter(|choice| {
            undoes_overwrites
                || matches!(choice.policy, ConflictPolicy::Skip | ConflictPolicy::Rename)
        })
        .collect()
}

/// The index among `choices` of the choice `shown_policy` highlights.
fn picked(choices: &[&ConflictChoice], policy: &ConflictPolicy, plan: &OperationPlan) -> usize {
    let policy = shown_policy(policy, plan);
    choices
        .iter()
        .position(|choice| choice.policy == policy)
        .unwrap_or(0)
}

/// The choice the row highlights for `policy` on `plan`. A plan that asks shows what it really
/// does: Keep both when it renames the clashes (a paste into the same folder), else Skip.
pub(super) fn shown_policy(policy: &ConflictPolicy, plan: &OperationPlan) -> ConflictPolicy {
    let renames = || {
        plan.conflicts
            .iter()
            .any(|c| matches!(c.resolution, Resolution::Rename(_)))
    };
    match policy {
        ConflictPolicy::Ask if renames() => ConflictPolicy::Rename,
        ConflictPolicy::Ask => ConflictPolicy::Skip,
        other => other.clone(),
    }
}

/// True when the dialog for `pending` offers the row: a copy or move whose plan found names
/// already at the target, outside an undo offer and the "run without old versions" question.
fn offers_row(pending: &PendingOp) -> bool {
    matches!(pending.plan.kind, OpKind::Copy | OpKind::Move)
        && !pending.plan.conflicts.is_empty()
        && pending.undo.is_none()
        && pending.keep_old == KeepOld::AsSet
}

/// True when `pending` replaces at least one file already at the target. A shown plan says so
/// itself; while a new pick is planned, the pick and the conflict facts decide, so the warning
/// follows the pick at once. Conflicts without facts count as replaced (the warning fails closed).
pub(super) fn replaces_files(pending: &PendingOp) -> bool {
    let conflicts = &pending.plan.conflicts;
    if !matches!(pending.plan.kind, OpKind::Copy | OpKind::Move) || conflicts.is_empty() {
        return false;
    }
    if !pending.is_planning() {
        return conflicts
            .iter()
            .any(|c| matches!(c.resolution, Resolution::Overwrite));
    }
    match pending.replan.policy {
        ConflictPolicy::OverwriteAll => true,
        ConflictPolicy::OverwriteOlder => {
            conflicts.len() > pending.clashes.len() || pending.clashes.iter().any(replaces_older)
        }
        _ => false,
    }
}

/// True when Overwrite older may replace `clash`'s target: the source is newer, or a time is
/// unknown on a side that is a file.
fn replaces_older(clash: &Clash) -> bool {
    match (clash.source.modified_ms, clash.target.modified_ms) {
        (Some(source), Some(target)) => source > target,
        _ => !clash.source.is_folder && !clash.target.is_folder,
    }
}

/// Counts `n` and formats it for a label.
fn counted(key: &str, n: usize) -> Option<String> {
    let count = u64::try_from(n).unwrap_or(u64::MAX);
    (n > 0).then(|| t!(key, count = count, shown = format::bytes(count)))
}

/// Puts the conflict line for `pending` on its own line below `summary`, so it is never cut; it
/// ends with the drive warning when the job replaces files on a drive that cannot keep them.
pub(super) fn below(summary: String, pending: &PendingOp) -> String {
    let Some(conflicts) = self::summary(&pending.plan) else {
        return summary;
    };
    if pending.cannot_retain.is_some() && replaces_files(pending) {
        let warning = tr("overwrite-cannot-retain");
        return format!("{summary}\n{conflicts} · {warning}");
    }
    format!("{summary}\n{conflicts}")
}

/// The summary part for `plan`'s conflicts: "3 items already there will be skipped", or what
/// happens to them ("…: 1 overwritten, 2 skipped"); `None` when nothing is already there.
fn summary(plan: &OperationPlan) -> Option<String> {
    let conflicts = &plan.conflicts;
    if conflicts.is_empty() {
        return None;
    }
    let items = super::items(conflicts.len());
    let count = |wanted: fn(&Resolution) -> bool| {
        conflicts.iter().filter(|c| wanted(&c.resolution)).count()
    };
    let overwritten = count(|r| matches!(r, Resolution::Overwrite));
    let renamed = count(|r| matches!(r, Resolution::Rename(_)));
    let skipped = conflicts.len() - overwritten - renamed;
    if skipped == conflicts.len() {
        return Some(t!("conflict-all-skipped", items = items.as_str()));
    }
    let outcome = [
        counted("conflict-overwritten", overwritten),
        counted("conflict-renamed", renamed),
        counted("conflict-skipped", skipped),
    ];
    let outcome = outcome.into_iter().flatten().collect::<Vec<_>>().join(", ");
    Some(t!(
        "conflict-outcome",
        items = items.as_str(),
        outcome = outcome
    ))
}

/// One side's facts as list text: "12.0 KB · 20.11.2026 14:02", "Folder · …" or "Not readable".
fn side(facts: &Facts) -> String {
    let what = match (facts.is_folder, facts.size) {
        (true, _) => tr("conflict-folder"),
        (false, Some(size)) => format::size(size),
        (false, None) => return tr("conflict-unreadable"),
    };
    match facts.modified_ms.map(format::date) {
        Some(date) if !date.is_empty() => format!("{what} · {date}"),
        _ => what,
    }
}

/// The conflict list behind "Show conflicts": the first conflicts with both sides' size and time,
/// and a line counting the ones not listed.
fn conflict_list(pending: &PendingOp) -> Details {
    let conflicts = pending.plan.conflicts.len();
    let rows = pending
        .clashes
        .iter()
        .map(|clash| DetailRow {
            name: clash.name.clone().into(),
            left: side(&clash.source).into(),
            right: side(&clash.target).into(),
        })
        .collect::<Vec<_>>();
    let incoming = match pending.plan.kind {
        OpKind::Move => "conflict-column-moved",
        _ => "conflict-column-copied",
    };
    let items = super::items(conflicts);
    let heading = t!("conflict-list-heading", items = items.as_str());
    Details {
        link: tr("conflict-show").into(),
        heading: heading.into(),
        columns: [tr(incoming).into(), tr("conflict-column-there").into()],
        more: counted("conflict-list-more", conflicts.saturating_sub(rows.len())).map(Into::into),
        rows,
        back: tr("conflict-back").into(),
    }
}

impl NavoWindow {
    /// Adds the conflict row to `dialog` when `pending` offers it; a pick goes to the browser,
    /// which rebuilds the plan with that choice.
    pub(super) fn conflict_row(
        &self,
        dialog: ConfirmDialog,
        pending: &PendingOp,
        cx: &Context<Self>,
    ) -> ConfirmDialog {
        if !offers_row(pending) {
            return dialog;
        }
        let choices = shown(self.browser.undoes_overwrites());
        let options = Options {
            label: tr("conflict-choice-label").into(),
            items: choices
                .iter()
                .map(|choice| OptionItem {
                    id: choice.id.into(),
                    label: tr(choice.label).into(),
                })
                .collect(),
            picked: picked(&choices, &pending.replan.policy, &pending.plan),
        };
        let policies: Vec<ConflictPolicy> = choices.iter().map(|c| c.policy.clone()).collect();
        let view = cx.weak_entity();
        let dialog = dialog.details(conflict_list(pending));
        dialog.options(options, move |index, _, cx| {
            let Some(policy) = policies.get(index).cloned() else {
                return;
            };
            let _ = view.update(cx, |view, cx| {
                view.browser.pick_policy(policy);
                cx.notify();
            });
        })
    }
}
